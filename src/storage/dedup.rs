//! Data deduplication module for rs3gw
//!
//! Provides block-level deduplication with content-addressed storage to save 30-70% storage space.
//! Features:
//! - SHA256-based content addressing
//! - Reference counting for shared blocks
//! - Configurable block size (4KB-1MB)
//! - Transparent to S3 API (no client changes needed)
//! - Automatic garbage collection for unreferenced blocks
//! - Both fixed-size and content-defined chunking algorithms

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{debug, info};

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum DedupError {
    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Invalid block size: {0}")]
    InvalidBlockSize(usize),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

// ============================================================================
// Configuration
// ============================================================================

/// Chunking algorithm for deduplication
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChunkingAlgorithm {
    /// Fixed-size chunking (simpler, faster)
    #[default]
    FixedSize,
    /// Content-defined chunking using rolling hash (better dedup ratio)
    ContentDefined,
}

/// Deduplication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupConfig {
    /// Block size for chunking (4KB to 1MB)
    pub block_size: usize,
    /// Chunking algorithm
    pub algorithm: ChunkingAlgorithm,
    /// Enable deduplication
    pub enabled: bool,
    /// Minimum object size to enable dedup (smaller objects = overhead)
    pub min_object_size: usize,
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            block_size: 64 * 1024, // 64KB default
            algorithm: ChunkingAlgorithm::FixedSize,
            enabled: true,
            min_object_size: 128 * 1024, // Only dedup objects >= 128KB
        }
    }
}

impl DedupConfig {
    pub fn new(block_size: usize) -> Result<Self, DedupError> {
        if !(4096..=1_048_576).contains(&block_size) {
            return Err(DedupError::InvalidBlockSize(block_size));
        }
        Ok(Self {
            block_size,
            ..Default::default()
        })
    }

    pub fn with_algorithm(mut self, algorithm: ChunkingAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    pub fn with_min_size(mut self, size: usize) -> Self {
        self.min_object_size = size;
        self
    }

    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Default::default()
        }
    }
}

// ============================================================================
// Data Types
// ============================================================================

/// A content-addressed block hash
pub type BlockHash = String;

/// Block reference information
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlockReference {
    /// SHA256 hash of the block
    hash: BlockHash,
    /// Reference count
    ref_count: u64,
    /// Block size in bytes
    size: usize,
    /// Creation timestamp
    created_at: i64,
}

/// Object block map - stores which blocks compose an object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectBlockMap {
    /// Ordered list of block hashes
    pub blocks: Vec<BlockHash>,
    /// Total object size
    pub total_size: usize,
    /// Chunking algorithm used
    pub algorithm: ChunkingAlgorithm,
    /// Block size used
    pub block_size: usize,
}

// ============================================================================
// Rolling Hash for Content-Defined Chunking
// ============================================================================

/// Rabin-Karp rolling hash for content-defined chunking
struct RollingHash {
    window_size: usize,
    hash: u64,
    window: Vec<u8>,
    position: usize,
    base: u64,
    modulus: u64,
    power: u64,
}

impl RollingHash {
    fn new(window_size: usize) -> Self {
        let base = 256u64;
        let modulus = (1u64 << 32) - 1;
        let mut power = 1u64;
        for _ in 0..window_size - 1 {
            power = (power * base) % modulus;
        }

        Self {
            window_size,
            hash: 0,
            window: Vec::with_capacity(window_size),
            position: 0,
            base,
            modulus,
            power,
        }
    }

    fn roll(&mut self, byte: u8) -> u64 {
        if self.window.len() < self.window_size {
            self.window.push(byte);
            self.hash = (self.hash * self.base + byte as u64) % self.modulus;
        } else {
            let old_byte = self.window[self.position];
            self.window[self.position] = byte;
            self.position = (self.position + 1) % self.window_size;

            // Remove old byte influence and add new byte
            self.hash = (self.hash + self.modulus - (old_byte as u64 * self.power) % self.modulus)
                % self.modulus;
            self.hash = (self.hash * self.base + byte as u64) % self.modulus;
        }

        self.hash
    }

    fn is_boundary(&self, mask: u64) -> bool {
        self.window.len() == self.window_size && (self.hash & mask) == 0
    }
}

// ============================================================================
// Chunking Implementation
// ============================================================================

/// Chunk data into blocks using specified algorithm
fn chunk_data(data: &[u8], config: &DedupConfig) -> Vec<Bytes> {
    match config.algorithm {
        ChunkingAlgorithm::FixedSize => chunk_fixed_size(data, config.block_size),
        ChunkingAlgorithm::ContentDefined => chunk_content_defined(data, config.block_size),
    }
}

/// Fixed-size chunking
fn chunk_fixed_size(data: &[u8], block_size: usize) -> Vec<Bytes> {
    data.chunks(block_size)
        .map(Bytes::copy_from_slice)
        .collect()
}

/// Content-defined chunking using rolling hash
fn chunk_content_defined(data: &[u8], avg_block_size: usize) -> Vec<Bytes> {
    if data.is_empty() {
        return vec![];
    }

    let min_block_size = avg_block_size / 4;
    let max_block_size = avg_block_size * 4;
    let window_size = 48; // Rolling hash window

    // Mask to control average chunk size
    // Lower bits = more frequent boundaries = smaller chunks
    let mask_bits = (avg_block_size as f64).log2() as u32;
    let mask = (1u64 << mask_bits) - 1;

    let mut chunks = Vec::new();
    let mut rolling_hash = RollingHash::new(window_size);
    let mut chunk_start = 0;

    for (i, &byte) in data.iter().enumerate() {
        rolling_hash.roll(byte);

        let chunk_size = i - chunk_start;

        // Force chunk boundary at max size
        let force_boundary = chunk_size >= max_block_size;
        // Natural boundary from hash
        let natural_boundary = chunk_size >= min_block_size && rolling_hash.is_boundary(mask);

        if force_boundary || natural_boundary {
            chunks.push(Bytes::copy_from_slice(&data[chunk_start..=i]));
            chunk_start = i + 1;
        }
    }

    // Add final chunk
    if chunk_start < data.len() {
        chunks.push(Bytes::copy_from_slice(&data[chunk_start..]));
    }

    chunks
}

// ============================================================================
// Deduplication Manager
// ============================================================================

/// Manages content-addressed block storage and reference counting
pub struct DedupManager {
    root: PathBuf,
    config: DedupConfig,
    /// In-memory cache of block references for performance
    ref_cache: Arc<RwLock<HashMap<BlockHash, BlockReference>>>,
}

impl DedupManager {
    /// Create a new deduplication manager
    pub async fn new(root: PathBuf, config: DedupConfig) -> Result<Self, DedupError> {
        let blocks_dir = root.join("blocks");
        let refs_dir = root.join("block_refs");
        let index_dir = root.join("block_index");

        fs::create_dir_all(&blocks_dir).await?;
        fs::create_dir_all(&refs_dir).await?;
        fs::create_dir_all(&index_dir).await?;

        let manager = Self {
            root,
            config,
            ref_cache: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load existing references into cache
        manager.load_ref_cache().await?;

        Ok(manager)
    }

    // ------------------------------------------------------------------------
    // Path Helpers
    // ------------------------------------------------------------------------

    fn block_path(&self, hash: &str) -> PathBuf {
        // Use first 4 chars for directory structure: ab/cd/abcdef...
        let prefix1 = &hash[0..2];
        let prefix2 = &hash[2..4];
        self.root
            .join("blocks")
            .join(prefix1)
            .join(prefix2)
            .join(hash)
    }

    fn ref_path(&self, hash: &str) -> PathBuf {
        let prefix1 = &hash[0..2];
        let prefix2 = &hash[2..4];
        self.root
            .join("block_refs")
            .join(prefix1)
            .join(prefix2)
            .join(format!("{}.refs.json", hash))
    }

    fn index_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.root
            .join("block_index")
            .join(bucket)
            .join(format!("{}.blocks.json", key))
    }

    // ------------------------------------------------------------------------
    // Reference Cache Management
    // ------------------------------------------------------------------------

    async fn load_ref_cache(&self) -> Result<(), DedupError> {
        let refs_dir = self.root.join("block_refs");
        match fs::try_exists(&refs_dir).await {
            Ok(false) | Err(_) => return Ok(()),
            Ok(true) => {}
        }

        let mut cache = self.ref_cache.write().await;
        let mut count = 0;

        // Walk the directory tree
        for entry in walkdir::WalkDir::new(&refs_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                if let Ok(data) = fs::read(entry.path()).await {
                    if let Ok(block_ref) = serde_json::from_slice::<BlockReference>(&data) {
                        cache.insert(block_ref.hash.clone(), block_ref);
                        count += 1;
                    }
                }
            }
        }

        info!("Loaded {} block references into cache", count);
        Ok(())
    }

    async fn get_ref(&self, hash: &BlockHash) -> Result<Option<BlockReference>, DedupError> {
        // Check cache first
        {
            let cache = self.ref_cache.read().await;
            if let Some(block_ref) = cache.get(hash) {
                return Ok(Some(block_ref.clone()));
            }
        }

        // Load from disk
        let path = self.ref_path(hash);
        match fs::try_exists(&path).await {
            Ok(false) | Err(_) => return Ok(None),
            Ok(true) => {}
        }

        let data = fs::read(&path).await?;
        let block_ref: BlockReference =
            serde_json::from_slice(&data).map_err(|e| DedupError::Serialization(e.to_string()))?;

        // Update cache
        {
            let mut cache = self.ref_cache.write().await;
            cache.insert(hash.clone(), block_ref.clone());
        }

        Ok(Some(block_ref))
    }

    async fn save_ref(&self, block_ref: &BlockReference) -> Result<(), DedupError> {
        let path = self.ref_path(&block_ref.hash);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let data = serde_json::to_vec_pretty(block_ref)
            .map_err(|e| DedupError::Serialization(e.to_string()))?;
        fs::write(&path, data).await?;

        // Update cache
        {
            let mut cache = self.ref_cache.write().await;
            cache.insert(block_ref.hash.clone(), block_ref.clone());
        }

        Ok(())
    }

    // ------------------------------------------------------------------------
    // Block Storage Operations
    // ------------------------------------------------------------------------

    /// Store data with deduplication
    pub async fn store_object(
        &self,
        bucket: &str,
        key: &str,
        data: &Bytes,
    ) -> Result<ObjectBlockMap, DedupError> {
        // Skip dedup for small objects (overhead not worth it)
        if !self.config.enabled || data.len() < self.config.min_object_size {
            return self.store_without_dedup(bucket, key, data).await;
        }

        debug!(
            "Deduplicating object {}/{} ({} bytes)",
            bucket,
            key,
            data.len()
        );

        // Chunk the data
        let chunks = chunk_data(data, &self.config);
        let mut block_hashes = Vec::with_capacity(chunks.len());
        let mut new_blocks = 0;
        let mut dedup_blocks = 0;

        for chunk in chunks {
            let hash = hex::encode(Sha256::digest(&chunk));

            // Check if block already exists
            if let Some(mut block_ref) = self.get_ref(&hash).await? {
                // Block exists, increment reference count — full dedup savings on this block
                let block_size = block_ref.size as u64;
                block_ref.ref_count += 1;
                self.save_ref(&block_ref).await?;
                dedup_blocks += 1;
                crate::metrics::record_dedup_savings(block_size, block_size);
                debug!("Reusing block {} (ref_count={})", hash, block_ref.ref_count);
            } else {
                // New block, store it
                let block_path = self.block_path(&hash);
                if let Some(parent) = block_path.parent() {
                    fs::create_dir_all(parent).await?;
                }

                let mut file = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&block_path)
                    .await?;
                file.write_all(&chunk).await?;
                file.sync_all().await?;

                // Create reference
                let block_ref = BlockReference {
                    hash: hash.clone(),
                    ref_count: 1,
                    size: chunk.len(),
                    created_at: chrono::Utc::now().timestamp(),
                };
                self.save_ref(&block_ref).await?;
                new_blocks += 1;
            }

            block_hashes.push(hash);
        }

        // Create object block map
        let block_map = ObjectBlockMap {
            blocks: block_hashes.clone(),
            total_size: data.len(),
            algorithm: self.config.algorithm,
            block_size: self.config.block_size,
        };

        // Save block index
        let index_path = self.index_path(bucket, key);
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let index_data = serde_json::to_vec_pretty(&block_map)
            .map_err(|e| DedupError::Serialization(e.to_string()))?;
        fs::write(&index_path, index_data).await?;

        let dedup_ratio = if !block_hashes.is_empty() {
            (dedup_blocks as f64 / block_hashes.len() as f64) * 100.0
        } else {
            0.0
        };

        info!(
            "Stored object {}/{}: {} blocks ({} new, {} deduplicated, {:.1}% dedup ratio)",
            bucket,
            key,
            block_hashes.len(),
            new_blocks,
            dedup_blocks,
            dedup_ratio
        );

        Ok(block_map)
    }

    /// Store object without deduplication (small objects)
    async fn store_without_dedup(
        &self,
        bucket: &str,
        key: &str,
        data: &Bytes,
    ) -> Result<ObjectBlockMap, DedupError> {
        let hash = hex::encode(Sha256::digest(data));

        // Store as single block
        let block_path = self.block_path(&hash);
        if let Some(parent) = block_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&block_path)
            .await?;
        file.write_all(data).await?;
        file.sync_all().await?;

        let block_ref = BlockReference {
            hash: hash.clone(),
            ref_count: 1,
            size: data.len(),
            created_at: chrono::Utc::now().timestamp(),
        };
        self.save_ref(&block_ref).await?;

        let block_map = ObjectBlockMap {
            blocks: vec![hash],
            total_size: data.len(),
            algorithm: self.config.algorithm,
            block_size: data.len(),
        };

        let index_path = self.index_path(bucket, key);
        if let Some(parent) = index_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let index_data = serde_json::to_vec_pretty(&block_map)
            .map_err(|e| DedupError::Serialization(e.to_string()))?;
        fs::write(&index_path, index_data).await?;

        Ok(block_map)
    }

    /// Retrieve object by reassembling blocks
    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<Bytes, DedupError> {
        // Load block index
        let index_path = self.index_path(bucket, key);
        match fs::try_exists(&index_path).await {
            Ok(false) | Err(_) => {
                return Err(DedupError::BlockNotFound(format!("{}/{}", bucket, key)));
            }
            Ok(true) => {}
        }

        let index_data = fs::read(&index_path).await?;
        let block_map: ObjectBlockMap = serde_json::from_slice(&index_data)
            .map_err(|e| DedupError::Serialization(e.to_string()))?;

        // Reassemble blocks
        let mut result = BytesMut::with_capacity(block_map.total_size);

        for hash in &block_map.blocks {
            let block_path = self.block_path(hash);
            match fs::try_exists(&block_path).await {
                Ok(false) | Err(_) => {
                    return Err(DedupError::BlockNotFound(hash.clone()));
                }
                Ok(true) => {}
            }

            let block_data = fs::read(&block_path).await?;
            result.extend_from_slice(&block_data);
        }

        Ok(result.freeze())
    }

    /// Delete object and decrement block references
    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), DedupError> {
        let index_path = self.index_path(bucket, key);
        match fs::try_exists(&index_path).await {
            Ok(false) | Err(_) => {
                // Object doesn't exist in dedup storage, silently return
                return Ok(());
            }
            Ok(true) => {}
        }

        // Load block index
        let index_data = fs::read(&index_path).await?;
        let block_map: ObjectBlockMap = serde_json::from_slice(&index_data)
            .map_err(|e| DedupError::Serialization(e.to_string()))?;

        // Decrement references for all blocks
        for hash in &block_map.blocks {
            if let Some(mut block_ref) = self.get_ref(hash).await? {
                block_ref.ref_count = block_ref.ref_count.saturating_sub(1);

                if block_ref.ref_count == 0 {
                    // No more references, delete block
                    let block_path = self.block_path(hash);
                    if let Ok(true) = fs::try_exists(&block_path).await {
                        fs::remove_file(&block_path).await?;
                    }
                    let ref_path = self.ref_path(hash);
                    if let Ok(true) = fs::try_exists(&ref_path).await {
                        fs::remove_file(&ref_path).await?;
                    }

                    // Remove from cache
                    let mut cache = self.ref_cache.write().await;
                    cache.remove(hash);

                    debug!("Deleted block {}", hash);
                } else {
                    // Still has references, update count
                    self.save_ref(&block_ref).await?;
                    debug!(
                        "Decremented block {} (ref_count={})",
                        hash, block_ref.ref_count
                    );
                }
            }
        }

        // Delete block index
        fs::remove_file(&index_path).await?;

        info!("Deleted object {}/{}", bucket, key);
        Ok(())
    }

    /// Get deduplication statistics
    pub async fn get_stats(&self) -> Result<DedupStats, DedupError> {
        let cache = self.ref_cache.read().await;

        let total_blocks = cache.len();
        let mut total_refs = 0u64;
        let mut total_physical_bytes = 0u64;
        let mut total_logical_bytes = 0u64;

        for block_ref in cache.values() {
            total_refs += block_ref.ref_count;
            total_physical_bytes += block_ref.size as u64;
            total_logical_bytes += block_ref.size as u64 * block_ref.ref_count;
        }

        let dedup_ratio = if total_logical_bytes > 0 {
            1.0 - (total_physical_bytes as f64 / total_logical_bytes as f64)
        } else {
            0.0
        };

        let space_saved = total_logical_bytes.saturating_sub(total_physical_bytes);

        Ok(DedupStats {
            total_blocks,
            total_refs,
            total_physical_bytes,
            total_logical_bytes,
            dedup_ratio,
            space_saved,
        })
    }

    /// Garbage collection - remove unreferenced blocks (safety check)
    pub async fn garbage_collect(&self) -> Result<GarbageCollectionResult, DedupError> {
        info!("Starting garbage collection");

        let mut blocks_checked = 0;
        let mut blocks_removed = 0;
        let mut bytes_freed = 0u64;

        let cache = self.ref_cache.read().await;
        let hashes: Vec<String> = cache.keys().cloned().collect();
        drop(cache);

        for hash in hashes {
            blocks_checked += 1;

            if let Some(block_ref) = self.get_ref(&hash).await? {
                if block_ref.ref_count == 0 {
                    // Unreferenced block, remove it
                    let block_path = self.block_path(&hash);
                    if let Ok(true) = fs::try_exists(&block_path).await {
                        fs::remove_file(&block_path).await?;
                        bytes_freed += block_ref.size as u64;
                    }

                    let ref_path = self.ref_path(&hash);
                    if let Ok(true) = fs::try_exists(&ref_path).await {
                        fs::remove_file(&ref_path).await?;
                    }

                    let mut cache = self.ref_cache.write().await;
                    cache.remove(&hash);
                    drop(cache);

                    blocks_removed += 1;
                    debug!("GC: Removed unreferenced block {}", hash);
                }
            }
        }

        let result = GarbageCollectionResult {
            blocks_checked,
            blocks_removed,
            bytes_freed,
        };

        info!(
            "Garbage collection complete: {} blocks checked, {} removed, {} bytes freed",
            result.blocks_checked, result.blocks_removed, result.bytes_freed
        );

        Ok(result)
    }
}

// ============================================================================
// Statistics
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupStats {
    /// Total unique blocks
    pub total_blocks: usize,
    /// Total references across all blocks
    pub total_refs: u64,
    /// Physical bytes on disk
    pub total_physical_bytes: u64,
    /// Logical bytes (if all blocks were duplicated)
    pub total_logical_bytes: u64,
    /// Deduplication ratio (0.0 to 1.0)
    pub dedup_ratio: f64,
    /// Bytes saved by deduplication
    pub space_saved: u64,
}

#[derive(Debug, Clone)]
pub struct GarbageCollectionResult {
    pub blocks_checked: usize,
    pub blocks_removed: usize,
    pub bytes_freed: u64,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_size_chunking() {
        let data = vec![1u8; 1000];
        let chunks = chunk_fixed_size(&data, 100);
        assert_eq!(chunks.len(), 10);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[9].len(), 100);
    }

    #[test]
    fn test_fixed_size_chunking_uneven() {
        let data = vec![1u8; 1050];
        let chunks = chunk_fixed_size(&data, 100);
        assert_eq!(chunks.len(), 11);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[10].len(), 50);
    }

    #[test]
    fn test_content_defined_chunking() {
        let data = vec![1u8; 10000];
        let chunks = chunk_content_defined(&data, 1024);
        // CDC should produce variable-sized chunks
        assert!(!chunks.is_empty());
        // Average should be around target size
        let avg_size: usize = chunks.iter().map(|c| c.len()).sum::<usize>() / chunks.len();
        assert!((512..=4096).contains(&avg_size));
    }

    #[test]
    fn test_content_defined_dedup() {
        // Create data with repeated pattern
        let mut data = vec![];
        let pattern = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        for _ in 0..100 {
            data.extend_from_slice(&pattern);
        }

        let chunks1 = chunk_content_defined(&data, 64);

        // Same pattern should produce same chunks
        let mut data2 = vec![];
        for _ in 0..100 {
            data2.extend_from_slice(&pattern);
        }
        let chunks2 = chunk_content_defined(&data2, 64);

        assert_eq!(chunks1.len(), chunks2.len());
        for (c1, c2) in chunks1.iter().zip(chunks2.iter()) {
            assert_eq!(c1, c2);
        }
    }

    #[test]
    fn test_rolling_hash_boundary() {
        let mut rh = RollingHash::new(48);
        // Use varied data instead of uniform data
        let mut data = Vec::new();
        for i in 0..1000 {
            data.push((i % 256) as u8); // Generate varied data cycling through all byte values
        }

        let mut boundaries = 0;
        for &byte in &data {
            rh.roll(byte);
            if rh.is_boundary(0x0F) {
                boundaries += 1;
            }
        }

        // With varied data, we should find some boundaries
        // Using mask 0x0F means ~1/16 chance per position
        assert!(
            boundaries > 0,
            "Expected at least one boundary with varied data"
        );
    }

    #[tokio::test]
    async fn test_dedup_manager_store_retrieve() {
        let temp_dir =
            std::env::temp_dir().join(format!("rs3gw-dedup-test-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let config = DedupConfig::default();
        let manager = DedupManager::new(temp_dir.clone(), config)
            .await
            .expect("Failed to create manager");

        let data = Bytes::from(vec![1u8; 1024 * 200]); // 200KB
        let bucket = "test-bucket";
        let key = "test-key";

        // Store
        manager
            .store_object(bucket, key, &data)
            .await
            .expect("Failed to store object");

        // Retrieve
        let retrieved = manager
            .get_object(bucket, key)
            .await
            .expect("Failed to get object");

        assert_eq!(data, retrieved);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_dedup_manager_reference_counting() {
        let temp_dir =
            std::env::temp_dir().join(format!("rs3gw-dedup-test-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let config = DedupConfig::default();
        let manager = DedupManager::new(temp_dir.clone(), config)
            .await
            .expect("Failed to create manager");

        let data = Bytes::from(vec![42u8; 1024 * 200]); // Identical data
        let bucket = "test-bucket";

        // Store same data twice
        manager
            .store_object(bucket, "key1", &data)
            .await
            .expect("Failed to store key1");
        manager
            .store_object(bucket, "key2", &data)
            .await
            .expect("Failed to store key2");

        // Check stats - should have deduplication
        let stats = manager.get_stats().await.expect("Failed to get stats");
        assert!(stats.dedup_ratio > 0.0);
        assert!(stats.space_saved > 0);

        // Delete one object
        manager
            .delete_object(bucket, "key1")
            .await
            .expect("Failed to delete key1");

        // Blocks should still exist for key2
        let retrieved = manager
            .get_object(bucket, "key2")
            .await
            .expect("Failed to get key2");
        assert_eq!(data, retrieved);

        // Delete second object
        manager
            .delete_object(bucket, "key2")
            .await
            .expect("Failed to delete key2");

        // Now blocks should be gone
        let result = manager.get_object(bucket, "key2").await;
        assert!(result.is_err());

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_dedup_manager_garbage_collection() {
        let temp_dir =
            std::env::temp_dir().join(format!("rs3gw-dedup-test-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let config = DedupConfig::default();
        let manager = DedupManager::new(temp_dir.clone(), config)
            .await
            .expect("Failed to create manager");

        let data = Bytes::from(vec![1u8; 1024 * 200]);
        let bucket = "test-bucket";

        manager
            .store_object(bucket, "key1", &data)
            .await
            .expect("Failed to store");
        manager
            .delete_object(bucket, "key1")
            .await
            .expect("Failed to delete");

        // Run GC
        let result = manager.garbage_collect().await.expect("Failed to run GC");
        assert_eq!(result.blocks_removed, 0); // Already cleaned up during delete

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_small_object_no_dedup() {
        let temp_dir =
            std::env::temp_dir().join(format!("rs3gw-dedup-test-{}", uuid::Uuid::new_v4()));
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let config = DedupConfig::default().with_min_size(100 * 1024); // 100KB min
        let manager = DedupManager::new(temp_dir.clone(), config)
            .await
            .expect("Failed to create manager");

        let small_data = Bytes::from(vec![1u8; 1024]); // 1KB - too small
        let bucket = "test-bucket";

        let block_map = manager
            .store_object(bucket, "small", &small_data)
            .await
            .expect("Failed to store");

        // Should be stored as single block (no chunking)
        assert_eq!(block_map.blocks.len(), 1);

        let retrieved = manager
            .get_object(bucket, "small")
            .await
            .expect("Failed to get");
        assert_eq!(small_data, retrieved);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir).await;
    }
}
