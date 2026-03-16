//! Preprocessing result cache.

use super::error::PreprocessingResult;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cached preprocessing result
#[derive(Debug, Clone)]
pub(super) struct CachedResult {
    pub(super) data: Bytes,
    #[allow(dead_code)]
    pub(super) metadata: HashMap<String, String>,
    #[allow(dead_code)]
    pub(super) cached_at: DateTime<Utc>,
}

/// Cache for preprocessing results
pub struct PreprocessingCache {
    pub(super) cache: Arc<RwLock<HashMap<String, CachedResult>>>,
    pub(super) max_size_bytes: usize,
    pub(super) current_size: Arc<RwLock<usize>>,
}

impl PreprocessingCache {
    pub fn new(max_size_bytes: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size_bytes,
            current_size: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn get(&self, key: &str) -> Option<Bytes> {
        let cache = self.cache.read().await;
        cache.get(key).map(|r| r.data.clone())
    }

    pub async fn put(
        &self,
        key: String,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> PreprocessingResult<()> {
        let data_size = data.len();

        // Check if adding this would exceed max size
        let current_size = *self.current_size.read().await;
        if current_size + data_size > self.max_size_bytes {
            // Simple eviction: clear cache if too large
            let mut cache = self.cache.write().await;
            cache.clear();
            *self.current_size.write().await = 0;
        }

        let mut cache = self.cache.write().await;
        cache.insert(
            key,
            CachedResult {
                data,
                metadata,
                cached_at: Utc::now(),
            },
        );

        *self.current_size.write().await += data_size;
        Ok(())
    }

    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.cache.write().await;
        if let Some(entry) = cache.remove(key) {
            let mut size = self.current_size.write().await;
            *size = size.saturating_sub(entry.data.len());
        }
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        *self.current_size.write().await = 0;
    }

    pub async fn stats(&self) -> (usize, usize, usize) {
        let cache = self.cache.read().await;
        let current_size = *self.current_size.read().await;
        (cache.len(), current_size, self.max_size_bytes)
    }
}
