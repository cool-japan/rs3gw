//! Advanced encryption module for rs3gw
//!
//! Provides envelope encryption, key management, and multiple encryption algorithms.
//!
//! ## Features
//! - Envelope encryption (encrypt data with DEK, encrypt DEK with KEK)
//! - Key rotation without re-encrypting data
//! - Multiple encryption algorithms (AES-256-GCM, ChaCha20-Poly1305)
//! - Pluggable key providers (local, future: HSM, KMS)

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

// Cryptographic dependencies
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use chacha20poly1305::{ChaCha20Poly1305, Key as ChaChaKey, Nonce as ChaChaNonce};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum EncryptionError {
    #[error("Encryption failed")]
    EncryptionFailed,

    #[error("Decryption failed")]
    DecryptionFailed,

    #[error("Invalid key length: expected {expected}, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Invalid algorithm: {0}")]
    InvalidAlgorithm(String),

    #[error("Nonce generation failed")]
    NonceGenerationFailed,

    #[error("Key provider error: {0}")]
    KeyProviderError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// ============================================================================
// Encryption Algorithm
// ============================================================================

/// Supported encryption algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM (Advanced Encryption Standard with Galois/Counter Mode)
    #[default]
    Aes256Gcm,
    /// ChaCha20-Poly1305 (Stream cipher with Poly1305 MAC)
    ChaCha20Poly1305,
}

impl EncryptionAlgorithm {
    /// Get the key length in bytes for this algorithm
    pub fn key_len(&self) -> usize {
        match self {
            EncryptionAlgorithm::Aes256Gcm => 32,        // 256 bits
            EncryptionAlgorithm::ChaCha20Poly1305 => 32, // 256 bits
        }
    }

    /// Get the nonce length in bytes for this algorithm
    pub fn nonce_len(&self) -> usize {
        match self {
            EncryptionAlgorithm::Aes256Gcm => 12, // 96 bits (recommended for GCM)
            EncryptionAlgorithm::ChaCha20Poly1305 => 12, // 96 bits
        }
    }
}

// ============================================================================
// Encrypted Data Structure
// ============================================================================

/// Per-chunk encryption metadata for v2 (chunked) format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub nonce: Vec<u8>,
    pub plaintext_len: u64,
}

/// GCM authentication tag length in bytes (fixed for AES-256-GCM).
const GCM_TAG_LEN: usize = 16;

/// Chunk size for v2 (chunked) SSE format: 5 MiB.
pub const SSE_CHUNK_SIZE: usize = 5 * 1024 * 1024;

/// Encrypted data with associated metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// Encryption algorithm used
    pub algorithm: EncryptionAlgorithm,
    /// Encrypted data encryption key (DEK encrypted with KEK)
    pub encrypted_dek: Vec<u8>,
    /// Key encryption key (KEK) identifier
    pub kek_id: String,
    /// Nonce/IV used for DEK encryption
    pub dek_nonce: Vec<u8>,
    /// Encrypted payload
    pub ciphertext: Vec<u8>,
    /// Nonce/IV used for payload encryption (empty for v2 chunked format)
    pub payload_nonce: Vec<u8>,
    /// Optional additional authenticated data (AAD)
    pub aad: Option<Vec<u8>>,
    /// Per-chunk nonces for v2 (chunked) format. Empty = v1 single-shot.
    #[serde(default)]
    pub chunks: Vec<ChunkInfo>,
    /// Chunk size used during encryption (0 = v1 single-shot).
    #[serde(default)]
    pub chunk_size: u64,
}

// ============================================================================
// Key Provider Trait
// ============================================================================

/// Trait for key providers (local, HSM, KMS, etc.)
#[async_trait]
pub trait KeyProvider: Send + Sync {
    /// Get a key encryption key (KEK) by ID
    async fn get_kek(&self, key_id: &str) -> Result<Vec<u8>, EncryptionError>;

    /// List available KEK IDs
    async fn list_kek_ids(&self) -> Result<Vec<String>, EncryptionError>;

    /// Get the default KEK ID
    async fn default_kek_id(&self) -> Result<String, EncryptionError>;

    /// Create a new KEK (optional, for key rotation)
    async fn create_kek(&self, key_id: String) -> Result<(), EncryptionError>;
}

// ============================================================================
// Local Key Provider (In-Memory)
// ============================================================================

/// Simple in-memory key provider for development/testing
pub struct LocalKeyProvider {
    keys: Arc<tokio::sync::RwLock<HashMap<String, Vec<u8>>>>,
    default_key_id: String,
}

impl LocalKeyProvider {
    /// Create a new local key provider with a default master key
    pub fn new() -> Result<Self, EncryptionError> {
        let mut keys = HashMap::new();
        let mut master_key = vec![0u8; 32];
        getrandom::fill(&mut master_key).map_err(|e| {
            EncryptionError::KeyProviderError(format!("Failed to generate random key: {}", e))
        })?;
        keys.insert("master-key-v1".to_string(), master_key);

        Ok(Self {
            keys: Arc::new(tokio::sync::RwLock::new(keys)),
            default_key_id: "master-key-v1".to_string(),
        })
    }

    /// Create with a specific master key (for testing)
    pub fn with_master_key(key_id: String, master_key: Vec<u8>) -> Self {
        let mut keys = HashMap::new();
        keys.insert(key_id.clone(), master_key);

        Self {
            keys: Arc::new(tokio::sync::RwLock::new(keys)),
            default_key_id: key_id,
        }
    }

    /// Create a key provider that persists the KEK to disk.
    ///
    /// - If `kek_path` already exists, the key is loaded from the JSON file
    ///   `{"kek_id":"master-key-v1","key_base64":"<base64>"}`.
    /// - If `kek_path` does not exist, a fresh random 32-byte key is generated,
    ///   written to the file atomically (write-tmp + rename), and then returned.
    ///
    /// Parent directories are created automatically. All errors propagate as
    /// [`EncryptionError`] — no `unwrap()` is used.
    pub fn new_with_persistence(kek_path: std::path::PathBuf) -> Result<Self, EncryptionError> {
        /// On-disk representation of the KEK file.
        #[derive(serde::Serialize, serde::Deserialize)]
        struct KekFile {
            kek_id: String,
            key_base64: String,
        }

        const KEY_ID: &str = "master-key-v1";

        let master_key: Vec<u8> = if kek_path.exists() {
            // Load existing key.
            let raw = std::fs::read(&kek_path)?;
            let record: KekFile = serde_json::from_slice(&raw)
                .map_err(|e| EncryptionError::SerializationError(e.to_string()))?;
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(&record.key_base64)
                .map_err(|e| {
                    EncryptionError::SerializationError(format!("base64 decode error: {}", e))
                })?
        } else {
            // Generate a new random key and persist it atomically.
            let mut key = vec![0u8; 32];
            getrandom::fill(&mut key).map_err(|e| {
                EncryptionError::KeyProviderError(format!("Failed to generate random key: {}", e))
            })?;

            // Ensure parent directory exists.
            if let Some(parent) = kek_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            use base64::Engine as _;
            let record = KekFile {
                kek_id: KEY_ID.to_string(),
                key_base64: base64::engine::general_purpose::STANDARD.encode(&key),
            };
            let json = serde_json::to_vec(&record)
                .map_err(|e| EncryptionError::SerializationError(e.to_string()))?;

            // Write atomically: write to a sibling tmp file, then rename.
            let tmp_path = kek_path.with_extension("tmp");
            std::fs::write(&tmp_path, &json)?;
            std::fs::rename(&tmp_path, &kek_path)?;

            key
        };

        let mut keys = HashMap::new();
        keys.insert(KEY_ID.to_string(), master_key);

        Ok(Self {
            keys: Arc::new(tokio::sync::RwLock::new(keys)),
            default_key_id: KEY_ID.to_string(),
        })
    }
}

impl Default for LocalKeyProvider {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            // Fallback to a provider with a zero key if random generation fails
            // This should never happen in practice but provides a safe default
            Self::with_master_key("master-key-v1".to_string(), vec![0u8; 32])
        })
    }
}

#[async_trait]
impl KeyProvider for LocalKeyProvider {
    async fn get_kek(&self, key_id: &str) -> Result<Vec<u8>, EncryptionError> {
        let keys = self.keys.read().await;
        keys.get(key_id)
            .cloned()
            .ok_or_else(|| EncryptionError::KeyNotFound(key_id.to_string()))
    }

    async fn list_kek_ids(&self) -> Result<Vec<String>, EncryptionError> {
        let keys = self.keys.read().await;
        Ok(keys.keys().cloned().collect())
    }

    async fn default_kek_id(&self) -> Result<String, EncryptionError> {
        Ok(self.default_key_id.clone())
    }

    async fn create_kek(&self, key_id: String) -> Result<(), EncryptionError> {
        let mut keys = self.keys.write().await;
        if keys.contains_key(&key_id) {
            return Err(EncryptionError::KeyProviderError(format!(
                "Key {} already exists",
                key_id
            )));
        }

        let mut new_key = vec![0u8; 32];
        getrandom::fill(&mut new_key).map_err(|e| {
            EncryptionError::KeyProviderError(format!("Failed to generate random key: {}", e))
        })?;
        keys.insert(key_id, new_key);
        Ok(())
    }
}

// ============================================================================
// Encryption Service
// ============================================================================

/// Main encryption service using envelope encryption
pub struct EncryptionService {
    key_provider: Arc<dyn KeyProvider>,
    default_algorithm: EncryptionAlgorithm,
}

impl EncryptionService {
    /// Create a new encryption service
    pub fn new(key_provider: Arc<dyn KeyProvider>) -> Self {
        Self {
            key_provider,
            default_algorithm: EncryptionAlgorithm::Aes256Gcm,
        }
    }

    /// Create with a specific default algorithm
    pub fn with_algorithm(
        key_provider: Arc<dyn KeyProvider>,
        algorithm: EncryptionAlgorithm,
    ) -> Self {
        Self {
            key_provider,
            default_algorithm: algorithm,
        }
    }

    /// Generate a random data encryption key (DEK)
    fn generate_dek(&self, algorithm: EncryptionAlgorithm) -> Result<Vec<u8>, EncryptionError> {
        let mut dek = vec![0u8; algorithm.key_len()];
        getrandom::fill(&mut dek).map_err(|_| {
            EncryptionError::KeyProviderError("Failed to generate random DEK".to_string())
        })?;
        Ok(dek)
    }

    /// Generate a random nonce
    fn generate_nonce(&self, algorithm: EncryptionAlgorithm) -> Result<Vec<u8>, EncryptionError> {
        let mut nonce = vec![0u8; algorithm.nonce_len()];
        getrandom::fill(&mut nonce).map_err(|_| EncryptionError::NonceGenerationFailed)?;
        Ok(nonce)
    }

    /// Encrypt data using AES-256-GCM
    fn encrypt_aes256gcm(
        &self,
        plaintext: &[u8],
        key: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, EncryptionError> {
        let cipher =
            Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKeyLength {
                expected: 32,
                actual: key.len(),
            })?;

        let nonce_array = Nonce::from_slice(nonce);

        let ciphertext = if let Some(aad_data) = aad {
            cipher
                .encrypt(
                    nonce_array,
                    aes_gcm::aead::Payload {
                        msg: plaintext,
                        aad: aad_data,
                    },
                )
                .map_err(|_| EncryptionError::EncryptionFailed)?
        } else {
            cipher
                .encrypt(nonce_array, plaintext)
                .map_err(|_| EncryptionError::EncryptionFailed)?
        };

        Ok(ciphertext)
    }

    /// Decrypt data using AES-256-GCM
    fn decrypt_aes256gcm(
        &self,
        ciphertext: &[u8],
        key: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, EncryptionError> {
        let cipher =
            Aes256Gcm::new_from_slice(key).map_err(|_| EncryptionError::InvalidKeyLength {
                expected: 32,
                actual: key.len(),
            })?;

        let nonce_array = Nonce::from_slice(nonce);

        let plaintext = if let Some(aad_data) = aad {
            cipher
                .decrypt(
                    nonce_array,
                    aes_gcm::aead::Payload {
                        msg: ciphertext,
                        aad: aad_data,
                    },
                )
                .map_err(|_| EncryptionError::DecryptionFailed)?
        } else {
            cipher
                .decrypt(nonce_array, ciphertext)
                .map_err(|_| EncryptionError::DecryptionFailed)?
        };

        Ok(plaintext)
    }

    /// Encrypt data using ChaCha20-Poly1305
    fn encrypt_chacha20poly1305(
        &self,
        plaintext: &[u8],
        key: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, EncryptionError> {
        let key_array = ChaChaKey::from_slice(key);
        let cipher = ChaCha20Poly1305::new(key_array);
        let nonce_array = ChaChaNonce::from_slice(nonce);

        let ciphertext = if let Some(aad_data) = aad {
            cipher
                .encrypt(
                    nonce_array,
                    chacha20poly1305::aead::Payload {
                        msg: plaintext,
                        aad: aad_data,
                    },
                )
                .map_err(|_| EncryptionError::EncryptionFailed)?
        } else {
            cipher
                .encrypt(nonce_array, plaintext)
                .map_err(|_| EncryptionError::EncryptionFailed)?
        };

        Ok(ciphertext)
    }

    /// Decrypt data using ChaCha20-Poly1305
    fn decrypt_chacha20poly1305(
        &self,
        ciphertext: &[u8],
        key: &[u8],
        nonce: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, EncryptionError> {
        let key_array = ChaChaKey::from_slice(key);
        let cipher = ChaCha20Poly1305::new(key_array);
        let nonce_array = ChaChaNonce::from_slice(nonce);

        let plaintext = if let Some(aad_data) = aad {
            cipher
                .decrypt(
                    nonce_array,
                    chacha20poly1305::aead::Payload {
                        msg: ciphertext,
                        aad: aad_data,
                    },
                )
                .map_err(|_| EncryptionError::DecryptionFailed)?
        } else {
            cipher
                .decrypt(nonce_array, ciphertext)
                .map_err(|_| EncryptionError::DecryptionFailed)?
        };

        Ok(plaintext)
    }

    /// Encrypt data using envelope encryption
    ///
    /// 1. Generate a random data encryption key (DEK)
    /// 2. Encrypt the plaintext with the DEK
    /// 3. Encrypt the DEK with the key encryption key (KEK)
    /// 4. Return encrypted data with metadata
    pub async fn encrypt(
        &self,
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        self.encrypt_with_algorithm(plaintext, self.default_algorithm, aad)
            .await
    }

    /// Encrypt with a specific algorithm
    pub async fn encrypt_with_algorithm(
        &self,
        plaintext: &[u8],
        algorithm: EncryptionAlgorithm,
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        // Step 1: Generate DEK
        let dek = self.generate_dek(algorithm)?;

        // Step 2: Encrypt plaintext with DEK
        let payload_nonce = self.generate_nonce(algorithm)?;
        let ciphertext = match algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_aes256gcm(plaintext, &dek, &payload_nonce, aad)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20poly1305(plaintext, &dek, &payload_nonce, aad)?
            }
        };

        // Step 3: Get KEK and encrypt DEK
        let kek_id = self.key_provider.default_kek_id().await?;
        let kek = self.key_provider.get_kek(&kek_id).await?;

        let dek_nonce = self.generate_nonce(algorithm)?;
        let encrypted_dek = match algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_aes256gcm(&dek, &kek, &dek_nonce, None)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20poly1305(&dek, &kek, &dek_nonce, None)?
            }
        };

        Ok(EncryptedData {
            algorithm,
            encrypted_dek,
            kek_id,
            dek_nonce,
            ciphertext,
            payload_nonce,
            aad: aad.map(|a| a.to_vec()),
            chunks: vec![],
            chunk_size: 0,
        })
    }

    /// Decrypt data using envelope encryption.
    ///
    /// Dispatches to v1 (single-shot) or v2 (chunked) based on whether `chunks` is empty.
    pub async fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>, EncryptionError> {
        if encrypted.chunks.is_empty() {
            self.decrypt_single_shot(encrypted).await
        } else {
            self.decrypt_chunked_all(encrypted).await
        }
    }

    /// v1 single-shot decryption (existing path, now private).
    async fn decrypt_single_shot(
        &self,
        encrypted: &EncryptedData,
    ) -> Result<Vec<u8>, EncryptionError> {
        // Step 1: Get KEK and decrypt DEK
        let kek = self.key_provider.get_kek(&encrypted.kek_id).await?;

        let dek = match encrypted.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.decrypt_aes256gcm(&encrypted.encrypted_dek, &kek, &encrypted.dek_nonce, None)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => self.decrypt_chacha20poly1305(
                &encrypted.encrypted_dek,
                &kek,
                &encrypted.dek_nonce,
                None,
            )?,
        };

        // Step 2: Decrypt ciphertext with DEK
        let plaintext = match encrypted.algorithm {
            EncryptionAlgorithm::Aes256Gcm => self.decrypt_aes256gcm(
                &encrypted.ciphertext,
                &dek,
                &encrypted.payload_nonce,
                encrypted.aad.as_deref(),
            )?,
            EncryptionAlgorithm::ChaCha20Poly1305 => self.decrypt_chacha20poly1305(
                &encrypted.ciphertext,
                &dek,
                &encrypted.payload_nonce,
                encrypted.aad.as_deref(),
            )?,
        };

        Ok(plaintext)
    }

    /// v2 chunked decryption: decrypt all chunks in order and concatenate.
    async fn decrypt_chunked_all(
        &self,
        encrypted: &EncryptedData,
    ) -> Result<Vec<u8>, EncryptionError> {
        // Decrypt DEK with KEK.
        let kek = self.key_provider.get_kek(&encrypted.kek_id).await?;
        let dek =
            self.decrypt_aes256gcm(&encrypted.encrypted_dek, &kek, &encrypted.dek_nonce, None)?;

        let aad_prefix = encrypted.aad.as_deref().unwrap_or(&[]);
        let mut result: Vec<u8> = Vec::new();
        let mut offset: usize = 0;

        for (i, chunk_info) in encrypted.chunks.iter().enumerate() {
            let ct_len = chunk_info.plaintext_len as usize + GCM_TAG_LEN;
            let chunk_ct = encrypted
                .ciphertext
                .get(offset..offset + ct_len)
                .ok_or(EncryptionError::DecryptionFailed)?;
            let chunk_aad = build_chunk_aad(aad_prefix, i);
            let chunk_pt =
                self.decrypt_aes256gcm(chunk_ct, &dek, &chunk_info.nonce, Some(&chunk_aad))?;
            result.extend_from_slice(&chunk_pt);
            offset += ct_len;
        }

        Ok(result)
    }

    /// Re-encrypt data with a new KEK (for key rotation)
    ///
    /// This does NOT re-encrypt the actual data, only the DEK.
    /// This is the power of envelope encryption!
    pub async fn rotate_key(
        &self,
        encrypted: &EncryptedData,
        new_kek_id: &str,
    ) -> Result<EncryptedData, EncryptionError> {
        // Step 1: Decrypt DEK with old KEK
        let old_kek = self.key_provider.get_kek(&encrypted.kek_id).await?;
        let dek = match encrypted.algorithm {
            EncryptionAlgorithm::Aes256Gcm => self.decrypt_aes256gcm(
                &encrypted.encrypted_dek,
                &old_kek,
                &encrypted.dek_nonce,
                None,
            )?,
            EncryptionAlgorithm::ChaCha20Poly1305 => self.decrypt_chacha20poly1305(
                &encrypted.encrypted_dek,
                &old_kek,
                &encrypted.dek_nonce,
                None,
            )?,
        };

        // Step 2: Encrypt DEK with new KEK
        let new_kek = self.key_provider.get_kek(new_kek_id).await?;
        let new_dek_nonce = self.generate_nonce(encrypted.algorithm)?;
        let new_encrypted_dek = match encrypted.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_aes256gcm(&dek, &new_kek, &new_dek_nonce, None)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20poly1305(&dek, &new_kek, &new_dek_nonce, None)?
            }
        };

        // Step 3: Return new encrypted data with same ciphertext but new DEK encryption
        Ok(EncryptedData {
            algorithm: encrypted.algorithm,
            encrypted_dek: new_encrypted_dek,
            kek_id: new_kek_id.to_string(),
            dek_nonce: new_dek_nonce,
            ciphertext: encrypted.ciphertext.clone(),
            payload_nonce: encrypted.payload_nonce.clone(),
            aad: encrypted.aad.clone(),
            chunks: encrypted.chunks.clone(),
            chunk_size: encrypted.chunk_size,
        })
    }

    /// Encrypt with customer-provided key (SSE-C).
    ///
    /// Generates a random DEK, encrypts it with `customer_key` (AES-256-GCM),
    /// then encrypts `plaintext` with the DEK. The raw customer key is never
    /// stored — only the DEK encrypted with it is persisted in the sidecar.
    pub async fn encrypt_with_customer_key(
        &self,
        plaintext: &[u8],
        customer_key: &[u8; 32],
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        let algorithm = EncryptionAlgorithm::Aes256Gcm;

        // Step 1: Generate a random DEK.
        let dek = self.generate_dek(algorithm)?;

        // Step 2: Encrypt plaintext with the DEK.
        let payload_nonce = self.generate_nonce(algorithm)?;
        let ciphertext = self.encrypt_aes256gcm(plaintext, &dek, &payload_nonce, aad)?;

        // Step 3: Encrypt the DEK with the customer key (customer key acts as KEK).
        let dek_nonce = self.generate_nonce(algorithm)?;
        let encrypted_dek = self.encrypt_aes256gcm(&dek, customer_key, &dek_nonce, None)?;

        Ok(EncryptedData {
            algorithm,
            encrypted_dek,
            // No server KEK is involved — store an empty string to distinguish from SSE-S3.
            kek_id: String::new(),
            dek_nonce,
            ciphertext,
            payload_nonce,
            aad: aad.map(|a| a.to_vec()),
            chunks: vec![],
            chunk_size: 0,
        })
    }

    /// Decrypt data encrypted with a customer-provided key (SSE-C).
    ///
    /// Decrypts the DEK using `customer_key`, then decrypts the payload.
    /// If the customer key is wrong the DEK decryption will fail with
    /// `EncryptionError::DecryptionFailed`.
    pub async fn decrypt_with_customer_key(
        &self,
        encrypted: &EncryptedData,
        customer_key: &[u8; 32],
    ) -> Result<Vec<u8>, EncryptionError> {
        // Step 1: Decrypt the DEK using the customer key.
        let dek = self.decrypt_aes256gcm(
            &encrypted.encrypted_dek,
            customer_key,
            &encrypted.dek_nonce,
            None,
        )?;

        // Step 2: Decrypt the payload using the DEK.
        let plaintext = self.decrypt_aes256gcm(
            &encrypted.ciphertext,
            &dek,
            &encrypted.payload_nonce,
            encrypted.aad.as_deref(),
        )?;

        Ok(plaintext)
    }

    /// Encrypt using a specific named KEK (for SSE-KMS key selection).
    ///
    /// Identical to `encrypt_with_algorithm` but targets `kek_id` instead of
    /// calling `default_kek_id()`. The `EncryptedData.kek_id` field is set to
    /// `kek_id.to_string()`.
    pub async fn encrypt_with_kek_id(
        &self,
        plaintext: &[u8],
        kek_id: &str,
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        let algorithm = self.default_algorithm;

        // Step 1: Generate DEK.
        let dek = self.generate_dek(algorithm)?;

        // Step 2: Encrypt plaintext with DEK.
        let payload_nonce = self.generate_nonce(algorithm)?;
        let ciphertext = match algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_aes256gcm(plaintext, &dek, &payload_nonce, aad)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20poly1305(plaintext, &dek, &payload_nonce, aad)?
            }
        };

        // Step 3: Get the named KEK and encrypt DEK with it.
        let kek = self.key_provider.get_kek(kek_id).await?;
        let dek_nonce = self.generate_nonce(algorithm)?;
        let encrypted_dek = match algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.encrypt_aes256gcm(&dek, &kek, &dek_nonce, None)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20poly1305(&dek, &kek, &dek_nonce, None)?
            }
        };

        Ok(EncryptedData {
            algorithm,
            encrypted_dek,
            kek_id: kek_id.to_string(),
            dek_nonce,
            ciphertext,
            payload_nonce,
            aad: aad.map(|a| a.to_vec()),
            chunks: vec![],
            chunk_size: 0,
        })
    }

    /// Resolve a KMS key ID: if `requested` is `None`, returns `default_kek_id()`.
    ///
    /// If `requested` is `Some`, validates that the key exists in the key store
    /// and returns it.  Returns `Err(KeyNotFound)` if the requested ID is absent.
    pub async fn resolve_kms_key_id(
        &self,
        requested: Option<&str>,
    ) -> Result<String, EncryptionError> {
        match requested {
            None => self.key_provider.default_kek_id().await,
            Some(id) => {
                // Validate the key exists — get_kek returns Err(KeyNotFound) if absent.
                self.key_provider.get_kek(id).await?;
                Ok(id.to_string())
            }
        }
    }

    /// Encrypt using chunked AES-256-GCM (v2 format) with the default KEK.
    ///
    /// Each [`SSE_CHUNK_SIZE`]-byte block is independently encrypted.
    /// AAD per chunk: `{aad_prefix}/chunk/{i}` for tamper evidence.
    pub async fn encrypt_chunked(
        &self,
        plaintext: &[u8],
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        let kek_id = self.key_provider.default_kek_id().await?;
        self.encrypt_chunked_internal(plaintext, &kek_id, aad).await
    }

    /// Chunked encryption targeting a specific named KEK (for SSE-KMS).
    pub async fn encrypt_chunked_with_kek_id(
        &self,
        plaintext: &[u8],
        kek_id: &str,
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        // Validate the key exists.
        self.key_provider.get_kek(kek_id).await?;
        self.encrypt_chunked_internal(plaintext, kek_id, aad).await
    }

    /// Internal chunked encryption implementation.
    async fn encrypt_chunked_internal(
        &self,
        plaintext: &[u8],
        kek_id: &str,
        aad: Option<&[u8]>,
    ) -> Result<EncryptedData, EncryptionError> {
        let algorithm = EncryptionAlgorithm::Aes256Gcm;

        // Generate random DEK (32 bytes).
        let dek = self.generate_dek(algorithm)?;

        // Encrypt DEK with KEK.
        let kek = self.key_provider.get_kek(kek_id).await?;
        let dek_nonce = self.generate_nonce(algorithm)?;
        let encrypted_dek = self.encrypt_aes256gcm(&dek, &kek, &dek_nonce, None)?;

        let aad_prefix = aad.unwrap_or(&[]);
        let mut ciphertext: Vec<u8> = Vec::new();
        let mut chunks: Vec<ChunkInfo> = Vec::new();

        for (i, chunk_plain) in plaintext.chunks(SSE_CHUNK_SIZE).enumerate() {
            let nonce = self.generate_nonce(algorithm)?;
            let chunk_aad = build_chunk_aad(aad_prefix, i);
            let chunk_ct = self.encrypt_aes256gcm(chunk_plain, &dek, &nonce, Some(&chunk_aad))?;
            chunks.push(ChunkInfo {
                nonce,
                plaintext_len: chunk_plain.len() as u64,
            });
            ciphertext.extend_from_slice(&chunk_ct);
        }

        Ok(EncryptedData {
            algorithm,
            encrypted_dek,
            kek_id: kek_id.to_string(),
            dek_nonce,
            ciphertext,
            // v2: no single payload_nonce — per-chunk nonces are in `chunks`.
            payload_nonce: vec![],
            aad: aad.map(|a| a.to_vec()),
            chunks,
            chunk_size: SSE_CHUNK_SIZE as u64,
        })
    }

    /// Decrypt only the chunks covering `[range_start, range_end)` bytes (exclusive end).
    ///
    /// `ciphertext_bytes` is the full concatenated chunk ciphertext from disk.
    /// `aad_prefix` is the same prefix used during encryption (bucket/key path).
    pub async fn decrypt_chunked_range(
        &self,
        sidecar_kek_id: &str,
        sidecar_encrypted_dek: &[u8],
        sidecar_dek_nonce: &[u8],
        sidecar_chunk_size: u64,
        sidecar_chunks: &[crate::storage::SidecarChunk],
        ciphertext_bytes: &[u8],
        range_start: u64,
        range_end: u64, // exclusive
        aad_prefix: &[u8],
    ) -> Result<Vec<u8>, EncryptionError> {
        if sidecar_chunk_size == 0 || sidecar_chunks.is_empty() {
            return Err(EncryptionError::DecryptionFailed);
        }
        if range_start >= range_end {
            return Err(EncryptionError::DecryptionFailed);
        }

        // Decrypt DEK.
        let kek = self.key_provider.get_kek(sidecar_kek_id).await?;
        let dek = self.decrypt_aes256gcm(sidecar_encrypted_dek, &kek, sidecar_dek_nonce, None)?;

        let first_chunk = (range_start / sidecar_chunk_size) as usize;
        let last_chunk = ((range_end - 1) / sidecar_chunk_size) as usize;

        // Compute byte offsets of each chunk in the ciphertext file.
        // Each chunk occupies (plaintext_len + GCM_TAG_LEN) bytes on disk.
        let mut file_offsets: Vec<usize> = Vec::with_capacity(sidecar_chunks.len() + 1);
        let mut acc: usize = 0;
        for chunk in sidecar_chunks.iter() {
            file_offsets.push(acc);
            acc += chunk.plaintext_len as usize + GCM_TAG_LEN;
        }
        file_offsets.push(acc); // sentinel end

        // Decrypt only the required chunks.
        let mut plaintext_parts: Vec<u8> = Vec::new();
        for idx in first_chunk..=last_chunk {
            let chunk = sidecar_chunks
                .get(idx)
                .ok_or(EncryptionError::DecryptionFailed)?;
            let file_start = *file_offsets
                .get(idx)
                .ok_or(EncryptionError::DecryptionFailed)?;
            let file_end = file_start + chunk.plaintext_len as usize + GCM_TAG_LEN;
            let chunk_ct = ciphertext_bytes
                .get(file_start..file_end)
                .ok_or(EncryptionError::DecryptionFailed)?;
            let chunk_aad = build_chunk_aad(aad_prefix, idx);
            let chunk_pt =
                self.decrypt_aes256gcm(chunk_ct, &dek, &chunk.nonce, Some(&chunk_aad))?;
            plaintext_parts.extend_from_slice(&chunk_pt);
        }

        // Slice the concatenated plaintext to the exact requested byte range.
        let first_chunk_start_byte = first_chunk as u64 * sidecar_chunk_size;
        let local_start = (range_start - first_chunk_start_byte) as usize;
        let local_end = (range_end - first_chunk_start_byte) as usize;
        let result = plaintext_parts
            .get(local_start..local_end)
            .ok_or(EncryptionError::DecryptionFailed)?
            .to_vec();
        Ok(result)
    }

    /// Encrypt bytes and return as Bytes
    pub async fn encrypt_bytes(&self, data: &Bytes) -> Result<Bytes, EncryptionError> {
        let encrypted = self.encrypt(data, None).await?;
        let serialized = serde_json::to_vec(&encrypted)
            .map_err(|e| EncryptionError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(serialized))
    }

    /// Decrypt bytes
    pub async fn decrypt_bytes(&self, data: &Bytes) -> Result<Bytes, EncryptionError> {
        let encrypted: EncryptedData = serde_json::from_slice(data)
            .map_err(|e| EncryptionError::SerializationError(e.to_string()))?;
        let plaintext = self.decrypt(&encrypted).await?;
        Ok(Bytes::from(plaintext))
    }
}

/// Build the per-chunk AAD bytes: `{prefix}/chunk/{i}` (or `chunk/{i}` if prefix is empty).
fn build_chunk_aad(prefix: &[u8], chunk_index: usize) -> Vec<u8> {
    if prefix.is_empty() {
        format!("chunk/{}", chunk_index).into_bytes()
    } else {
        let mut aad = prefix.to_vec();
        aad.extend_from_slice(b"/chunk/");
        aad.extend_from_slice(chunk_index.to_string().as_bytes());
        aad
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_local_key_provider() {
        let provider = LocalKeyProvider::new().expect("Failed to create provider");

        // Get default key
        let default_id = provider
            .default_kek_id()
            .await
            .expect("Failed to get default key ID");
        assert_eq!(default_id, "master-key-v1");

        // Get key
        let key = provider
            .get_kek(&default_id)
            .await
            .expect("Failed to get key");
        assert_eq!(key.len(), 32);

        // List keys
        let keys = provider.list_kek_ids().await.expect("Failed to list keys");
        assert_eq!(keys.len(), 1);
        assert!(keys.contains(&"master-key-v1".to_string()));

        // Create new key
        provider
            .create_kek("master-key-v2".to_string())
            .await
            .expect("Failed to create key");

        let keys = provider.list_kek_ids().await.expect("Failed to list keys");
        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_encryption_aes256gcm() {
        let provider = Arc::new(LocalKeyProvider::new().expect("Failed to create provider"));
        let service = EncryptionService::new(provider);

        let plaintext = b"Hello, World! This is a secret message.";

        // Encrypt
        let encrypted = service
            .encrypt(plaintext, None)
            .await
            .expect("Failed to encrypt");

        assert_eq!(encrypted.algorithm, EncryptionAlgorithm::Aes256Gcm);
        assert!(!encrypted.ciphertext.is_empty());
        assert!(!encrypted.encrypted_dek.is_empty());

        // Decrypt
        let decrypted = service
            .decrypt(&encrypted)
            .await
            .expect("Failed to decrypt");

        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_encryption_chacha20poly1305() {
        let provider = Arc::new(LocalKeyProvider::new().expect("Failed to create provider"));
        let service =
            EncryptionService::with_algorithm(provider, EncryptionAlgorithm::ChaCha20Poly1305);

        let plaintext = b"Hello, ChaCha20-Poly1305!";

        // Encrypt
        let encrypted = service
            .encrypt(plaintext, None)
            .await
            .expect("Failed to encrypt");

        assert_eq!(encrypted.algorithm, EncryptionAlgorithm::ChaCha20Poly1305);

        // Decrypt
        let decrypted = service
            .decrypt(&encrypted)
            .await
            .expect("Failed to decrypt");

        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_encryption_with_aad() {
        let provider = Arc::new(LocalKeyProvider::new().expect("Failed to create provider"));
        let service = EncryptionService::new(provider);

        let plaintext = b"Secret data";
        let aad = b"bucket=test-bucket,key=test-key";

        // Encrypt with AAD
        let encrypted = service
            .encrypt(plaintext, Some(aad))
            .await
            .expect("Failed to encrypt");

        // Decrypt with correct AAD
        let decrypted = service
            .decrypt(&encrypted)
            .await
            .expect("Failed to decrypt");
        assert_eq!(decrypted, plaintext);

        // Tampering with AAD should fail decryption
        let mut tampered = encrypted.clone();
        tampered.aad = Some(b"tampered-aad".to_vec());

        let result = service.decrypt(&tampered).await;
        assert!(result.is_err(), "Decryption should fail with tampered AAD");
    }

    #[tokio::test]
    async fn test_key_rotation() {
        let provider = Arc::new(LocalKeyProvider::new().expect("Failed to create provider"));

        // Create a second key for rotation
        provider
            .create_kek("master-key-v2".to_string())
            .await
            .expect("Failed to create new key");

        let service = EncryptionService::new(provider);

        let plaintext = b"Data to be rotated";

        // Encrypt with first key
        let encrypted = service
            .encrypt(plaintext, None)
            .await
            .expect("Failed to encrypt");

        assert_eq!(encrypted.kek_id, "master-key-v1");

        // Rotate to second key
        let rotated = service
            .rotate_key(&encrypted, "master-key-v2")
            .await
            .expect("Failed to rotate key");

        assert_eq!(rotated.kek_id, "master-key-v2");
        // Ciphertext should be the same (only DEK encryption changed)
        assert_eq!(rotated.ciphertext, encrypted.ciphertext);
        // DEK encryption should be different
        assert_ne!(rotated.encrypted_dek, encrypted.encrypted_dek);

        // Decrypt with rotated key
        let decrypted = service.decrypt(&rotated).await.expect("Failed to decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_bytes() {
        let provider = Arc::new(LocalKeyProvider::new().expect("Failed to create provider"));
        let service = EncryptionService::new(provider);

        let plaintext = Bytes::from("Test data for bytes encryption");

        // Encrypt
        let encrypted = service
            .encrypt_bytes(&plaintext)
            .await
            .expect("Failed to encrypt bytes");

        assert!(!encrypted.is_empty());

        // Decrypt
        let decrypted = service
            .decrypt_bytes(&encrypted)
            .await
            .expect("Failed to decrypt bytes");

        assert_eq!(decrypted, plaintext);
    }

    #[tokio::test]
    async fn test_different_plaintexts_different_ciphertexts() {
        let provider = Arc::new(LocalKeyProvider::new().expect("Failed to create provider"));
        let service = EncryptionService::new(provider);

        let plaintext1 = b"Message 1";
        let plaintext2 = b"Message 2";

        let encrypted1 = service
            .encrypt(plaintext1, None)
            .await
            .expect("Failed to encrypt");
        let encrypted2 = service
            .encrypt(plaintext2, None)
            .await
            .expect("Failed to encrypt");

        // Different plaintexts should produce different ciphertexts
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);
        // Even with same plaintext, nonces should make ciphertext different
        let encrypted3 = service
            .encrypt(plaintext1, None)
            .await
            .expect("Failed to encrypt");
        assert_ne!(encrypted1.ciphertext, encrypted3.ciphertext);
    }
}
