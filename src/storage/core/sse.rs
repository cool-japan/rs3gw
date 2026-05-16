use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

use super::path_utils::sanitize_key_for_fs;
use super::types::{StorageEngine, StorageError};
use crate::storage::encryption::{ChunkInfo, EncryptedData, EncryptionAlgorithm};

/// Per-chunk metadata stored in the SSE sidecar for v2 (chunked) format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarChunk {
    pub nonce: Vec<u8>,
    pub plaintext_len: u64,
}

/// On-disk SSE sidecar for a single S3 object.
///
/// version=1 covers SSE-S3 (AES256). version=2 is the chunked format for
/// seekable range-GET. Reserved fields allow SSE-C and SSE-KMS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSseSidecar {
    pub version: u32,           // 1 = single-shot, 2 = chunked
    pub algorithm: String,      // "AES256" today
    pub encrypted_dek: Vec<u8>, // DEK encrypted with KEK (base64 in JSON)
    pub kek_id: String,         // KEK identifier used for DEK encryption
    pub dek_nonce: Vec<u8>,     // nonce used to encrypt the DEK
    pub payload_nonce: Vec<u8>, // nonce used to encrypt the payload (v1 only)
    pub aad_bucket: String,     // AAD bucket component (tamper-evidence)
    pub aad_key: String,        // AAD key component (tamper-evidence)
    pub ciphertext_len: u64,    // byte length of ciphertext on disk
    // Reserved for Session 6:
    pub kms_master_key_id: Option<String>, // SSE-KMS only
    pub customer_key_md5: Option<String>,  // SSE-C only
    /// Chunk size for v2 (chunked) format; 0 = v1 single-shot.
    #[serde(default)]
    pub chunk_size: u64,
    /// Per-chunk nonces + plaintext lengths for v2 format. Empty = v1.
    #[serde(default)]
    pub chunks: Vec<SidecarChunk>,
}

impl ObjectSseSidecar {
    /// Build a sidecar from a just-produced `EncryptedData` for a given object location.
    ///
    /// AAD = bucket + "/" + key (D4: tamper-evidence — sidecar cannot decrypt another object).
    /// v2 (chunked) objects get `version=2` and per-chunk metadata.
    pub fn from_encrypted(e: &EncryptedData, bucket: &str, key: &str) -> Self {
        let version = if e.chunks.is_empty() { 1 } else { 2 };
        Self {
            version,
            algorithm: match e.algorithm {
                EncryptionAlgorithm::Aes256Gcm => "AES256".to_string(),
                EncryptionAlgorithm::ChaCha20Poly1305 => "CHACHA20".to_string(),
            },
            encrypted_dek: e.encrypted_dek.clone(),
            kek_id: e.kek_id.clone(),
            dek_nonce: e.dek_nonce.clone(),
            payload_nonce: e.payload_nonce.clone(),
            aad_bucket: bucket.to_string(),
            aad_key: key.to_string(),
            ciphertext_len: e.ciphertext.len() as u64,
            kms_master_key_id: None,
            customer_key_md5: None,
            chunk_size: e.chunk_size,
            chunks: e
                .chunks
                .iter()
                .map(|c| SidecarChunk {
                    nonce: c.nonce.clone(),
                    plaintext_len: c.plaintext_len,
                })
                .collect(),
        }
    }

    /// Reconstruct an `EncryptedData` from this sidecar (for decryption).
    ///
    /// `ciphertext` is the raw bytes read from disk.
    pub fn into_encrypted_data(&self, ciphertext: Vec<u8>) -> EncryptedData {
        let aad = format!("{}/{}", self.aad_bucket, self.aad_key);
        EncryptedData {
            algorithm: EncryptionAlgorithm::Aes256Gcm, // SSE always uses AES256-GCM
            encrypted_dek: self.encrypted_dek.clone(),
            kek_id: self.kek_id.clone(),
            dek_nonce: self.dek_nonce.clone(),
            ciphertext,
            payload_nonce: self.payload_nonce.clone(),
            aad: Some(aad.into_bytes()),
            chunks: self
                .chunks
                .iter()
                .map(|c| ChunkInfo {
                    nonce: c.nonce.clone(),
                    plaintext_len: c.plaintext_len,
                })
                .collect(),
            chunk_size: self.chunk_size,
        }
    }
}

impl StorageEngine {
    fn sse_sidecar_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("sse")
            .join(format!("{}.json", sanitize_key_for_fs(key)))
    }

    /// Load the SSE sidecar for an object.
    ///
    /// Returns `Ok(None)` when the sidecar does not exist (object is not SSE-encrypted).
    /// Returns `Err(BucketNotFound)` when the bucket does not exist.
    pub async fn get_object_sse(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<ObjectSseSidecar>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.sse_sidecar_path(bucket, key);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map(Some)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Write the SSE sidecar for an object (atomic tmp + rename).
    pub async fn put_object_sse(
        &self,
        bucket: &str,
        key: &str,
        sidecar: &ObjectSseSidecar,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.sse_sidecar_path(bucket, key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_vec_pretty(sidecar).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        // Atomic write: write to a temp file first, then rename.
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, &data).await?;
        fs::rename(&tmp_path, &path).await?;
        Ok(())
    }

    /// Delete the SSE sidecar for an object (idempotent — not-found is not an error).
    pub async fn delete_object_sse(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        let path = self.sse_sidecar_path(bucket, key);
        match fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(StorageError::Io(e)),
        }
    }

    /// Overwrite an already-assembled object file with ciphertext (SSE post-processing).
    ///
    /// Used after `complete_multipart_upload` to replace the assembled plaintext with
    /// the encrypted ciphertext. Atomic write via tmp+rename.
    pub async fn overwrite_object_ciphertext(
        &self,
        bucket: &str,
        key: &str,
        ciphertext: &[u8],
    ) -> Result<(), StorageError> {
        // Use the canonical object_path so the path matches what complete_multipart_upload wrote.
        let obj_path = self.object_path(bucket, key);
        let tmp_path = obj_path.with_extension("ciphertext.tmp");
        fs::write(&tmp_path, ciphertext).await?;
        fs::rename(&tmp_path, &obj_path).await?;
        Ok(())
    }
}
