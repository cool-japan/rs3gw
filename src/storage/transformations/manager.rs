//! Transformation manager and caching

use super::compression::CompressionTransformer;
use super::image::{ImageTransformer, Transformer};
use super::types::*;
use super::video::VideoTransformer;
use super::wasm::WasmPluginTransformer;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

/// Transformation cache entry
#[derive(Clone)]
struct CacheEntry {
    result: TransformationResult,
    created_at: SystemTime,
    access_count: usize,
}

/// Transformation cache
pub struct TransformationCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    max_entries: usize,
    ttl: Duration,
}

impl TransformationCache {
    pub fn new(max_entries: usize, ttl_secs: u64) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Generate cache key
    fn cache_key(data: &[u8], transformation: &TransformationType) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.update(format!("{:?}", transformation).as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get cached result
    pub async fn get(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Option<TransformationResult> {
        let key = Self::cache_key(data, transformation);
        let mut entries = self.entries.write().await;

        if let Some(entry) = entries.get_mut(&key) {
            // Check if entry is still valid
            if let Ok(elapsed) = entry.created_at.elapsed() {
                if elapsed < self.ttl {
                    entry.access_count += 1;
                    return Some(entry.result.clone());
                }
            }
            // Entry expired, remove it
            entries.remove(&key);
        }

        None
    }

    /// Put result in cache
    pub async fn put(
        &self,
        data: &[u8],
        transformation: &TransformationType,
        result: &TransformationResult,
    ) {
        let key = Self::cache_key(data, transformation);
        let mut entries = self.entries.write().await;

        // Evict if at capacity (LRU)
        if entries.len() >= self.max_entries {
            self.evict_lru(&mut entries).await;
        }

        entries.insert(
            key,
            CacheEntry {
                result: result.clone(),
                created_at: SystemTime::now(),
                access_count: 1,
            },
        );
    }

    /// Evict least recently used entry
    async fn evict_lru(&self, entries: &mut HashMap<String, CacheEntry>) {
        if let Some((key_to_remove, _)) = entries
            .iter()
            .min_by_key(|(_, entry)| (entry.access_count, entry.created_at))
        {
            let key_to_remove = key_to_remove.clone();
            entries.remove(&key_to_remove);
        }
    }

    /// Get cache statistics
    pub async fn stats(&self) -> (usize, usize) {
        let entries = self.entries.read().await;
        (entries.len(), self.max_entries)
    }

    /// Cleanup expired entries
    pub async fn cleanup(&self) {
        let mut entries = self.entries.write().await;
        let now = SystemTime::now();

        entries.retain(|_, entry| {
            if let Ok(elapsed) = now.duration_since(entry.created_at) {
                elapsed < self.ttl
            } else {
                false
            }
        });
    }
}

/// Transformation manager
pub struct TransformationManager {
    transformers: Vec<Arc<dyn Transformer>>,
    cache: Option<Arc<TransformationCache>>,
}

impl TransformationManager {
    /// Create a new transformation manager with default transformers
    pub fn new() -> Self {
        let mut manager = Self {
            transformers: Vec::new(),
            cache: None,
        };

        // Register default transformers
        manager.register_transformer(Arc::new(ImageTransformer));
        manager.register_transformer(Arc::new(VideoTransformer));
        manager.register_transformer(Arc::new(CompressionTransformer));
        manager.register_transformer(Arc::new(WasmPluginTransformer::new()));

        manager
    }

    /// Create a new transformation manager with caching enabled
    pub fn with_cache(max_entries: usize, ttl_secs: u64) -> Self {
        let mut manager = Self::new();
        manager.cache = Some(Arc::new(TransformationCache::new(max_entries, ttl_secs)));
        manager
    }

    /// Enable caching for transformations
    pub fn enable_cache(&mut self, max_entries: usize, ttl_secs: u64) {
        self.cache = Some(Arc::new(TransformationCache::new(max_entries, ttl_secs)));
    }

    /// Register a custom transformer
    pub fn register_transformer(&mut self, transformer: Arc<dyn Transformer>) {
        self.transformers.push(transformer);
    }

    /// Apply a transformation to data
    pub async fn transform(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError> {
        // Check cache first
        if let Some(cache) = &self.cache {
            if let Some(cached_result) = cache.get(data, transformation).await {
                return Ok(cached_result);
            }
        }

        // Perform transformation
        let result = self.transform_uncached(data, transformation).await?;

        // Cache the result
        if let Some(cache) = &self.cache {
            cache.put(data, transformation, &result).await;
        }

        Ok(result)
    }

    /// Apply a transformation without using cache
    async fn transform_uncached(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError> {
        for transformer in &self.transformers {
            if transformer.supports(transformation) {
                return transformer.transform(data, transformation).await;
            }
        }

        Err(TransformationError::UnsupportedFormat(
            "No transformer found for this transformation type".to_string(),
        ))
    }

    /// Get cache statistics (if caching is enabled)
    pub async fn cache_stats(&self) -> Option<(usize, usize)> {
        if let Some(cache) = &self.cache {
            Some(cache.stats().await)
        } else {
            None
        }
    }

    /// Cleanup expired cache entries
    pub async fn cleanup_cache(&self) {
        if let Some(cache) = &self.cache {
            cache.cleanup().await;
        }
    }

    /// Apply multiple transformations in sequence
    pub async fn transform_chain(
        &self,
        mut data: Bytes,
        transformations: &[TransformationType],
    ) -> Result<TransformationResult, TransformationError> {
        let mut final_content_type = None;
        let mut all_metadata = HashMap::new();

        for transformation in transformations {
            let result = self.transform(&data, transformation).await?;
            data = result.data;
            final_content_type = Some(result.content_type);

            // Merge metadata
            for (key, value) in result.metadata {
                all_metadata.insert(key, value);
            }
        }

        Ok(TransformationResult {
            data,
            content_type: final_content_type
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            metadata: all_metadata,
        })
    }
}

impl Default for TransformationManager {
    fn default() -> Self {
        Self::new()
    }
}
