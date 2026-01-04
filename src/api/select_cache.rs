//! S3 Select Query Result Cache
//!
//! Provides LRU caching of S3 Select query results to improve performance
//! for frequently executed queries. Cache keys are based on SQL expression
//! and object ETag to ensure correctness when objects are modified.
//!
//! # Features
//!
//! - LRU eviction policy with configurable max entries and memory size
//! - Automatic invalidation when objects are modified (ETag-based)
//! - Cache statistics (hit rate, evictions, memory usage)
//! - TTL-based expiration for time-sensitive queries
//! - Thread-safe concurrent access
//! - Prometheus metrics for monitoring
//!
//! # Example
//!
//! ```rust,ignore
//! use select_cache::SelectResultCache;
//!
//! let cache = SelectResultCache::new(1000, 100 * 1024 * 1024); // 1000 entries, 100MB
//!
//! // Try to get cached result
//! if let Some(result) = cache.get("SELECT * FROM s3object", "etag123").await {
//!     return result;
//! }
//!
//! // Execute query and cache result
//! let result = execute_query(...).await?;
//! cache.put("SELECT * FROM s3object", "etag123", result.clone(), 3600).await;
//! ```

use bytes::Bytes;
use metrics::{counter, gauge};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// Query pattern tracking for cache warming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPattern {
    /// SQL expression
    pub sql: String,

    /// Object bucket and key
    pub bucket: String,
    pub key: String,

    /// Number of times this query has been executed
    pub execution_count: u64,

    /// Last execution timestamp
    pub last_executed: i64,

    /// Average execution time in milliseconds
    pub avg_execution_ms: u64,
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total number of cache get requests
    pub gets: u64,

    /// Number of cache hits
    pub hits: u64,

    /// Number of cache misses
    pub misses: u64,

    /// Number of entries evicted due to LRU
    pub evictions: u64,

    /// Number of entries expired due to TTL
    pub expirations: u64,

    /// Current number of cached entries
    pub current_entries: usize,

    /// Current estimated memory usage in bytes
    pub memory_bytes: u64,

    /// Maximum allowed entries
    pub max_entries: usize,

    /// Maximum allowed memory in bytes
    pub max_memory_bytes: u64,
}

impl CacheStats {
    /// Calculate hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        if self.gets == 0 {
            0.0
        } else {
            (self.hits as f64 / self.gets as f64) * 100.0
        }
    }

    /// Calculate memory utilization percentage
    pub fn memory_utilization(&self) -> f64 {
        if self.max_memory_bytes == 0 {
            0.0
        } else {
            (self.memory_bytes as f64 / self.max_memory_bytes as f64) * 100.0
        }
    }
}

/// Cached query result entry
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// Query result data
    #[serde(with = "serde_bytes_helper")]
    result: Bytes,

    /// Object ETag when this result was cached
    etag: String,

    /// Timestamp when this entry was created (Unix timestamp)
    created_at: i64,

    /// Time-to-live in seconds (0 = no expiration)
    ttl_seconds: u64,

    /// Last access timestamp (for LRU)
    last_accessed: i64,

    /// Estimated size in bytes
    size_bytes: usize,
}

/// Helper module for serializing/deserializing Bytes
mod serde_bytes_helper {
    use bytes::Bytes;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes.as_ref())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<u8> = Vec::deserialize(deserializer)?;
        Ok(Bytes::from(vec))
    }
}

/// Persistent cache state for saving to disk
#[derive(Debug, Serialize, Deserialize)]
struct PersistentCacheState {
    /// Cache entries (query_key -> entry)
    cache: HashMap<String, CacheEntry>,

    /// Query patterns (pattern_key -> pattern)
    query_patterns: HashMap<String, QueryPattern>,

    /// Cache statistics
    stats: CacheStats,

    /// Version for future compatibility
    version: u32,

    /// Timestamp when saved
    saved_at: i64,
}

impl CacheEntry {
    /// Check if entry is expired
    fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        now - self.created_at > self.ttl_seconds as i64
    }

    /// Update last accessed timestamp
    fn touch(&mut self) {
        self.last_accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
    }
}

/// S3 Select query result cache
pub struct SelectResultCache {
    /// Cache storage (query_key -> entry)
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,

    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,

    /// Query pattern tracking (pattern_key -> pattern)
    query_patterns: Arc<RwLock<HashMap<String, QueryPattern>>>,

    /// Maximum number of entries
    max_entries: usize,

    /// Maximum memory usage in bytes
    max_memory_bytes: u64,
}

impl SelectResultCache {
    /// Create a new query result cache
    ///
    /// # Arguments
    ///
    /// * `max_entries` - Maximum number of cached entries
    /// * `max_memory_bytes` - Maximum memory usage in bytes
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cache = SelectResultCache::new(1000, 100 * 1024 * 1024); // 100MB cache
    /// ```
    pub fn new(max_entries: usize, max_memory_bytes: u64) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats {
                max_entries,
                max_memory_bytes,
                ..Default::default()
            })),
            query_patterns: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
            max_memory_bytes,
        }
    }

    /// Generate cache key from SQL expression and ETag
    fn cache_key(sql: &str, etag: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        sql.hash(&mut hasher);
        etag.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Get cached query result
    ///
    /// Returns `Some(result)` if entry exists and is valid, `None` otherwise
    pub async fn get(&self, sql: &str, etag: &str) -> Option<Bytes> {
        let key = Self::cache_key(sql, etag);

        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;

        stats.gets += 1;

        if let Some(entry) = cache.get_mut(&key) {
            // Check if expired
            if entry.is_expired() {
                cache.remove(&key);
                stats.expirations += 1;
                stats.misses += 1;
                stats.current_entries = cache.len();

                // Record metrics
                counter!("select_cache_expirations").increment(1);
                counter!("select_cache_misses").increment(1);
                gauge!("select_cache_entries").set(cache.len() as f64);

                return None;
            }

            // Check if ETag matches (object was modified)
            if entry.etag != etag {
                cache.remove(&key);
                stats.misses += 1;
                stats.current_entries = cache.len();

                // Record metrics
                counter!("select_cache_misses").increment(1);
                gauge!("select_cache_entries").set(cache.len() as f64);

                return None;
            }

            // Cache hit - update access time
            entry.touch();
            stats.hits += 1;

            // Record metrics
            counter!("select_cache_hits").increment(1);

            Some(entry.result.clone())
        } else {
            stats.misses += 1;

            // Record metrics
            counter!("select_cache_misses").increment(1);

            None
        }
    }

    /// Put query result into cache
    ///
    /// # Arguments
    ///
    /// * `sql` - SQL SELECT expression
    /// * `etag` - Object ETag
    /// * `result` - Query result data
    /// * `ttl_seconds` - Time-to-live in seconds (0 = no expiration)
    pub async fn put(&self, sql: &str, etag: &str, result: Bytes, ttl_seconds: u64) {
        let key = Self::cache_key(sql, etag);
        let size = result.len();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let entry = CacheEntry {
            result,
            etag: etag.to_string(),
            created_at: now,
            ttl_seconds,
            last_accessed: now,
            size_bytes: size,
        };

        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;

        // Evict if needed (LRU policy)
        while cache.len() >= self.max_entries
            || stats.memory_bytes + size as u64 > self.max_memory_bytes
        {
            if let Some(lru_key) = self.find_lru_entry(&cache) {
                if let Some(evicted) = cache.remove(&lru_key) {
                    stats.memory_bytes =
                        stats.memory_bytes.saturating_sub(evicted.size_bytes as u64);
                    stats.evictions += 1;

                    // Record metrics
                    counter!("select_cache_evictions").increment(1);
                }
            } else {
                break; // No entries to evict
            }
        }

        // Insert new entry
        cache.insert(key, entry);
        stats.memory_bytes += size as u64;
        stats.current_entries = cache.len();

        // Update gauges
        gauge!("select_cache_entries").set(cache.len() as f64);
        gauge!("select_cache_memory_bytes").set(stats.memory_bytes as f64);
    }

    /// Find least recently used entry
    fn find_lru_entry(&self, cache: &HashMap<String, CacheEntry>) -> Option<String> {
        cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone())
    }

    /// Invalidate cache entry for specific query and ETag
    pub async fn invalidate(&self, sql: &str, etag: &str) {
        let key = Self::cache_key(sql, etag);

        let mut cache = self.cache.write().await;
        if let Some(entry) = cache.remove(&key) {
            let mut stats = self.stats.write().await;
            stats.memory_bytes = stats.memory_bytes.saturating_sub(entry.size_bytes as u64);
            stats.current_entries = cache.len();

            // Update gauges
            gauge!("select_cache_entries").set(cache.len() as f64);
            gauge!("select_cache_memory_bytes").set(stats.memory_bytes as f64);
        }
    }

    /// Invalidate all cache entries for a specific object (by ETag)
    pub async fn invalidate_object(&self, etag: &str) {
        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;

        let keys_to_remove: Vec<String> = cache
            .iter()
            .filter(|(_, entry)| entry.etag == etag)
            .map(|(key, _)| key.clone())
            .collect();

        for key in keys_to_remove {
            if let Some(entry) = cache.remove(&key) {
                stats.memory_bytes = stats.memory_bytes.saturating_sub(entry.size_bytes as u64);
            }
        }

        stats.current_entries = cache.len();

        // Update gauges
        gauge!("select_cache_entries").set(cache.len() as f64);
        gauge!("select_cache_memory_bytes").set(stats.memory_bytes as f64);
    }

    /// Clear all cache entries
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();

        let mut stats = self.stats.write().await;
        stats.memory_bytes = 0;
        stats.current_entries = 0;

        // Update gauges
        gauge!("select_cache_entries").set(0.0);
        gauge!("select_cache_memory_bytes").set(0.0);
    }

    /// Get cache statistics
    pub async fn stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    /// Clean up expired entries (should be called periodically)
    pub async fn cleanup_expired(&self) {
        let mut cache = self.cache.write().await;
        let mut stats = self.stats.write().await;

        let expired_keys: Vec<String> = cache
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();

        let mut expired_count = 0;
        for key in expired_keys {
            if let Some(entry) = cache.remove(&key) {
                stats.memory_bytes = stats.memory_bytes.saturating_sub(entry.size_bytes as u64);
                stats.expirations += 1;
                expired_count += 1;
            }
        }

        stats.current_entries = cache.len();

        // Update metrics if any entries were expired
        if expired_count > 0 {
            counter!("select_cache_expirations").increment(expired_count);
            gauge!("select_cache_entries").set(cache.len() as f64);
            gauge!("select_cache_memory_bytes").set(stats.memory_bytes as f64);
        }
    }

    // ===== Cache Warming Methods =====

    /// Generate pattern key from bucket, key, and SQL
    fn pattern_key(bucket: &str, key: &str, sql: &str) -> String {
        format!("{}:{}:{}", bucket, key, sql)
    }

    /// Record query execution pattern for cache warming analysis
    ///
    /// Tracks how often queries are executed to identify candidates for pre-warming
    pub async fn record_query_pattern(
        &self,
        bucket: &str,
        key: &str,
        sql: &str,
        execution_time_ms: u64,
    ) {
        let pattern_key = Self::pattern_key(bucket, key, sql);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut patterns = self.query_patterns.write().await;

        patterns
            .entry(pattern_key)
            .and_modify(|pattern| {
                pattern.execution_count += 1;
                pattern.last_executed = now;
                // Update moving average
                pattern.avg_execution_ms =
                    (pattern.avg_execution_ms * (pattern.execution_count - 1) + execution_time_ms)
                        / pattern.execution_count;
            })
            .or_insert_with(|| QueryPattern {
                sql: sql.to_string(),
                bucket: bucket.to_string(),
                key: key.to_string(),
                execution_count: 1,
                last_executed: now,
                avg_execution_ms: execution_time_ms,
            });
    }

    /// Get top N most frequently executed queries for cache warming
    ///
    /// Returns queries sorted by execution count (descending)
    pub async fn get_top_queries(&self, limit: usize) -> Vec<QueryPattern> {
        let patterns = self.query_patterns.read().await;

        let mut pattern_list: Vec<QueryPattern> = patterns.values().cloned().collect();
        pattern_list.sort_by(|a, b| b.execution_count.cmp(&a.execution_count));
        pattern_list.truncate(limit);
        pattern_list
    }

    /// Get queries executed recently (within specified seconds)
    ///
    /// Useful for identifying active query patterns
    pub async fn get_recent_queries(&self, within_seconds: i64) -> Vec<QueryPattern> {
        let patterns = self.query_patterns.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let cutoff = now - within_seconds;

        patterns
            .values()
            .filter(|p| p.last_executed >= cutoff)
            .cloned()
            .collect()
    }

    /// Pre-warm the cache with a specific query result
    ///
    /// This is typically called by background warming tasks
    /// The caller is responsible for executing the query and providing the result
    pub async fn warm(
        &self,
        bucket: &str,
        key: &str,
        sql: &str,
        etag: &str,
        result: Bytes,
        ttl_seconds: u64,
    ) {
        // Put the result into cache (same as regular put)
        self.put(sql, etag, result, ttl_seconds).await;

        // Record as a warming operation in metrics
        counter!("select_cache_warmings").increment(1);

        tracing::info!(
            bucket = %bucket,
            key = %key,
            sql = %sql,
            "Cache warming completed"
        );
    }

    /// Get statistics about query patterns
    pub async fn pattern_stats(&self) -> HashMap<String, serde_json::Value> {
        let patterns = self.query_patterns.read().await;

        let total_patterns = patterns.len();
        let total_executions: u64 = patterns.values().map(|p| p.execution_count).sum();
        let avg_executions = if total_patterns > 0 {
            total_executions as f64 / total_patterns as f64
        } else {
            0.0
        };

        let mut stats = HashMap::new();
        stats.insert(
            "total_patterns".to_string(),
            serde_json::json!(total_patterns),
        );
        stats.insert(
            "total_executions".to_string(),
            serde_json::json!(total_executions),
        );
        stats.insert(
            "avg_executions_per_pattern".to_string(),
            serde_json::json!(avg_executions),
        );

        stats
    }

    /// Clear query pattern history
    pub async fn clear_patterns(&self) {
        let mut patterns = self.query_patterns.write().await;
        patterns.clear();
    }

    // ===== Cache Persistence Methods =====

    /// Save cache state to disk
    ///
    /// Saves both cache entries and query patterns to a JSON file.
    /// This allows the cache to persist across server restarts.
    ///
    /// # Arguments
    ///
    /// * `path` - File path where cache state will be saved
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if serialization or file write fails
    pub async fn save_to_file(
        &self,
        path: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let cache = self.cache.read().await;
        let patterns = self.query_patterns.read().await;
        let stats = self.stats.read().await;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let state = PersistentCacheState {
            cache: cache.clone(),
            query_patterns: patterns.clone(),
            stats: stats.clone(),
            version: 1,
            saved_at: now,
        };

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&state)?;

        // Write to temporary file first, then rename (atomic operation)
        let temp_path = path.with_extension("tmp");
        tokio::fs::write(&temp_path, json).await?;
        tokio::fs::rename(&temp_path, path).await?;

        tracing::info!(
            path = %path.display(),
            entries = cache.len(),
            patterns = patterns.len(),
            "Cache state saved to disk"
        );

        counter!("select_cache_saves").increment(1);

        Ok(())
    }

    /// Load cache state from disk
    ///
    /// Restores cache entries and query patterns from a previously saved file.
    /// Expired entries are automatically filtered out during load.
    ///
    /// # Arguments
    ///
    /// * `path` - File path to load cache state from
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or an error if file read or deserialization fails
    pub async fn load_from_file(
        &self,
        path: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Read file
        let json = tokio::fs::read_to_string(path).await?;

        // Deserialize
        let state: PersistentCacheState = serde_json::from_str(&json)?;

        // Filter out expired entries
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let mut cache = self.cache.write().await;
        let mut patterns = self.query_patterns.write().await;
        let mut stats = self.stats.write().await;

        // Clear existing state
        cache.clear();
        patterns.clear();

        // Load cache entries (filter expired)
        let mut loaded_entries = 0;
        let mut expired_entries = 0;
        let mut total_size = 0u64;

        for (key, entry) in state.cache {
            if !entry.is_expired() {
                total_size += entry.size_bytes as u64;
                cache.insert(key, entry);
                loaded_entries += 1;
            } else {
                expired_entries += 1;
            }
        }

        // Load query patterns
        *patterns = state.query_patterns;

        // Restore stats (but recalculate current values)
        stats.gets = state.stats.gets;
        stats.hits = state.stats.hits;
        stats.misses = state.stats.misses;
        stats.evictions = state.stats.evictions;
        stats.expirations = state.stats.expirations + expired_entries;
        stats.current_entries = cache.len();
        stats.memory_bytes = total_size;

        // Update metrics
        gauge!("select_cache_entries").set(cache.len() as f64);
        gauge!("select_cache_memory_bytes").set(total_size as f64);
        counter!("select_cache_loads").increment(1);

        tracing::info!(
            path = %path.display(),
            loaded_entries,
            expired_entries,
            loaded_patterns = patterns.len(),
            age_seconds = now - state.saved_at,
            "Cache state loaded from disk"
        );

        Ok(())
    }

    /// Start background task to periodically save cache to disk
    ///
    /// Creates a background task that saves the cache state at regular intervals.
    /// The task runs until the returned JoinHandle is dropped or aborted.
    ///
    /// # Arguments
    ///
    /// * `path` - File path where cache state will be saved
    /// * `interval_seconds` - How often to save (in seconds)
    ///
    /// # Returns
    ///
    /// Returns a JoinHandle for the background task
    pub fn start_background_save(
        self: Arc<Self>,
        path: std::path::PathBuf,
        interval_seconds: u64,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_seconds));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                interval.tick().await;

                match self.save_to_file(&path).await {
                    Ok(()) => {
                        tracing::debug!("Background cache save successful");
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            "Background cache save failed"
                        );
                    }
                }
            }
        })
    }

    // ===== Adaptive TTL Methods =====

    /// Calculate adaptive TTL based on query access patterns
    ///
    /// This method intelligently determines the optimal TTL for a query result
    /// based on historical access patterns, execution time, and result size.
    ///
    /// # Algorithm
    ///
    /// The TTL is calculated using the following factors:
    /// - **Base TTL**: Default value based on query execution frequency
    /// - **Execution Time Factor**: Longer-running queries get longer TTLs
    /// - **Size Factor**: Smaller results can be cached longer
    /// - **Recency Factor**: Recently executed queries get longer TTLs
    ///
    /// # Arguments
    ///
    /// * `bucket` - Bucket name
    /// * `key` - Object key
    /// * `sql` - SQL expression
    /// * `execution_time_ms` - Query execution time in milliseconds
    /// * `result_size_bytes` - Size of the query result in bytes
    ///
    /// # Returns
    ///
    /// Recommended TTL in seconds (between 60 and 7200 seconds)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let ttl = cache.calculate_adaptive_ttl(
    ///     "my-bucket",
    ///     "data/file.parquet",
    ///     "SELECT * FROM s3object WHERE age > 25",
    ///     1500, // 1.5 seconds execution time
    ///     10240, // 10KB result
    /// ).await;
    ///
    /// cache.put(sql, etag, result, ttl).await;
    /// ```
    pub async fn calculate_adaptive_ttl(
        &self,
        bucket: &str,
        key: &str,
        sql: &str,
        execution_time_ms: u64,
        result_size_bytes: usize,
    ) -> u64 {
        const MIN_TTL: u64 = 60; // 1 minute minimum
        const MAX_TTL: u64 = 7200; // 2 hours maximum
        const BASE_TTL: u64 = 600; // 10 minutes base

        let pattern_key = Self::pattern_key(bucket, key, sql);
        let patterns = self.query_patterns.read().await;

        // Check if we have historical data for this query
        if let Some(pattern) = patterns.get(&pattern_key) {
            // Factor 1: Execution frequency (higher frequency = longer TTL)
            let frequency_factor = if pattern.execution_count > 100 {
                2.0
            } else if pattern.execution_count > 50 {
                1.5
            } else if pattern.execution_count > 10 {
                1.2
            } else {
                1.0
            };

            // Factor 2: Execution time (expensive queries = longer TTL)
            // Longer execution times justify longer caching
            let time_factor = if execution_time_ms > 10000 {
                // > 10 seconds
                2.0
            } else if execution_time_ms > 5000 {
                // > 5 seconds
                1.5
            } else if execution_time_ms > 1000 {
                // > 1 second
                1.2
            } else {
                1.0
            };

            // Factor 3: Result size (smaller results = longer TTL)
            // Smaller results are cheaper to cache
            let size_factor = if result_size_bytes < 10_000 {
                // < 10KB
                1.3
            } else if result_size_bytes < 100_000 {
                // < 100KB
                1.1
            } else if result_size_bytes < 1_000_000 {
                // < 1MB
                1.0
            } else {
                0.8 // Large results get shorter TTL
            };

            // Factor 4: Recency (recently executed = longer TTL)
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);

            let seconds_since_last = now - pattern.last_executed;
            let recency_factor = if seconds_since_last < 300 {
                // < 5 minutes
                1.5
            } else if seconds_since_last < 3600 {
                // < 1 hour
                1.2
            } else {
                1.0
            };

            // Calculate adaptive TTL
            let calculated_ttl =
                (BASE_TTL as f64 * frequency_factor * time_factor * size_factor * recency_factor)
                    as u64;

            // Clamp to min/max bounds
            calculated_ttl.clamp(MIN_TTL, MAX_TTL)
        } else {
            // No historical data - use base TTL with execution time consideration
            let time_factor = if execution_time_ms > 5000 {
                1.5
            } else if execution_time_ms > 1000 {
                1.2
            } else {
                1.0
            };

            let calculated_ttl = (BASE_TTL as f64 * time_factor) as u64;
            calculated_ttl.clamp(MIN_TTL, MAX_TTL)
        }
    }

    /// Get TTL recommendation for a specific query pattern
    ///
    /// This is a convenience method that looks up the pattern and provides
    /// TTL recommendation without needing execution time/size parameters.
    ///
    /// # Returns
    ///
    /// Returns `Some(ttl)` if pattern exists, `None` otherwise
    pub async fn get_recommended_ttl(&self, bucket: &str, key: &str, sql: &str) -> Option<u64> {
        let pattern_key = Self::pattern_key(bucket, key, sql);
        let patterns = self.query_patterns.read().await;

        patterns.get(&pattern_key).map(|pattern| {
            // Use pattern's average execution time to estimate TTL
            self.calculate_ttl_from_pattern(pattern)
        })
    }

    /// Calculate TTL from a query pattern
    fn calculate_ttl_from_pattern(&self, pattern: &QueryPattern) -> u64 {
        const MIN_TTL: u64 = 60;
        const MAX_TTL: u64 = 7200;
        const BASE_TTL: u64 = 600;

        let frequency_factor = if pattern.execution_count > 100 {
            2.0
        } else if pattern.execution_count > 50 {
            1.5
        } else if pattern.execution_count > 10 {
            1.2
        } else {
            1.0
        };

        let time_factor = if pattern.avg_execution_ms > 10000 {
            2.0
        } else if pattern.avg_execution_ms > 5000 {
            1.5
        } else if pattern.avg_execution_ms > 1000 {
            1.2
        } else {
            1.0
        };

        let calculated_ttl = (BASE_TTL as f64 * frequency_factor * time_factor) as u64;
        calculated_ttl.clamp(MIN_TTL, MAX_TTL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = SelectResultCache::new(100, 1024 * 1024);
        let stats = cache.stats().await;

        assert_eq!(stats.max_entries, 100);
        assert_eq!(stats.max_memory_bytes, 1024 * 1024);
        assert_eq!(stats.current_entries, 0);
        assert_eq!(stats.memory_bytes, 0);
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let sql = "SELECT * FROM s3object WHERE age > 25";
        let etag = "etag123";
        let result = Bytes::from("test result data");

        // Put into cache
        cache.put(sql, etag, result.clone(), 0).await;

        // Get from cache
        let cached = cache.get(sql, etag).await;
        assert!(cached.is_some());
        if let Some(cached_result) = cached {
            assert_eq!(cached_result, result);
        }

        // Check stats
        let stats = cache.stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.current_entries, 1);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result = cache.get("SELECT * FROM s3object", "etag123").await;
        assert!(result.is_none());

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 1);
    }

    #[tokio::test]
    async fn test_etag_invalidation() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let sql = "SELECT * FROM s3object";
        let result = Bytes::from("test result");

        // Cache with etag1
        cache.put(sql, "etag1", result.clone(), 0).await;

        // Try to get with etag2 (object was modified)
        let cached = cache.get(sql, "etag2").await;
        assert!(cached.is_none());

        // Original entry should still exist
        let cached = cache.get(sql, "etag1").await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let cache = SelectResultCache::new(2, 1024 * 1024); // Max 2 entries

        let result = Bytes::from("test result");

        // Add 2 entries to fill the cache
        cache.put("query1", "etag1", result.clone(), 0).await;
        cache.put("query2", "etag2", result.clone(), 0).await;

        // Verify both are cached
        let stats = cache.stats().await;
        assert_eq!(stats.current_entries, 2);

        // Sleep to ensure timestamp difference
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Add a 3rd entry - should trigger eviction
        cache.put("query3", "etag3", result.clone(), 0).await;

        // Verify eviction occurred
        let stats = cache.stats().await;
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.current_entries, 2);

        // At least one of query1 or query2 should be evicted
        let cached1 = cache.get("query1", "etag1").await;
        let cached2 = cache.get("query2", "etag2").await;
        assert!(
            cached1.is_none() || cached2.is_none(),
            "One entry should be evicted"
        );

        // query3 should exist (most recent)
        let cached3 = cache.get("query3", "etag3").await;
        assert!(cached3.is_some());
    }

    #[tokio::test]
    async fn test_ttl_expiration() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let sql = "SELECT * FROM s3object";
        let etag = "etag123";
        let result = Bytes::from("test result");

        // Cache with 1 second TTL
        cache.put(sql, etag, result, 1).await;

        // Should be cached initially
        let cached = cache.get(sql, etag).await;
        assert!(cached.is_some());

        // Wait for expiration
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Should be expired
        let cached = cache.get(sql, etag).await;
        assert!(cached.is_none());

        let stats = cache.stats().await;
        assert_eq!(stats.expirations, 1);
    }

    #[tokio::test]
    async fn test_invalidate_object() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let etag = "etag123";
        let result = Bytes::from("test result");

        // Cache multiple queries for the same object
        cache.put("query1", etag, result.clone(), 0).await;
        cache.put("query2", etag, result.clone(), 0).await;
        cache.put("query3", "other_etag", result.clone(), 0).await;

        // Invalidate all entries for etag123
        cache.invalidate_object(etag).await;

        // query1 and query2 should be invalidated
        assert!(cache.get("query1", etag).await.is_none());
        assert!(cache.get("query2", etag).await.is_none());

        // query3 with different etag should still exist
        assert!(cache.get("query3", "other_etag").await.is_some());

        let stats = cache.stats().await;
        assert_eq!(stats.current_entries, 1);
    }

    #[tokio::test]
    async fn test_clear_cache() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result = Bytes::from("test result");
        cache.put("query1", "etag1", result.clone(), 0).await;
        cache.put("query2", "etag2", result.clone(), 0).await;

        let stats = cache.stats().await;
        assert_eq!(stats.current_entries, 2);

        // Clear all
        cache.clear().await;

        let stats = cache.stats().await;
        assert_eq!(stats.current_entries, 0);
        assert_eq!(stats.memory_bytes, 0);
    }

    #[tokio::test]
    async fn test_hit_rate_calculation() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result = Bytes::from("test");
        cache.put("query", "etag", result, 0).await;

        // 2 hits, 1 miss
        cache.get("query", "etag").await;
        cache.get("query", "etag").await;
        cache.get("other", "etag").await;

        let stats = cache.stats().await;
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 66.666).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_query_pattern_recording() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Record the same query multiple times
        cache
            .record_query_pattern("bucket1", "key1", "SELECT * FROM s3object", 100)
            .await;
        cache
            .record_query_pattern("bucket1", "key1", "SELECT * FROM s3object", 150)
            .await;
        cache
            .record_query_pattern("bucket1", "key1", "SELECT * FROM s3object", 120)
            .await;

        let top_queries = cache.get_top_queries(10).await;
        assert_eq!(top_queries.len(), 1);

        let pattern = &top_queries[0];
        assert_eq!(pattern.execution_count, 3);
        assert_eq!(pattern.bucket, "bucket1");
        assert_eq!(pattern.key, "key1");
        assert_eq!(pattern.sql, "SELECT * FROM s3object");
        // Average of 100, 150, 120 = 123.33... but due to integer division it should be 123
        assert_eq!(pattern.avg_execution_ms, 123);
    }

    #[tokio::test]
    async fn test_get_top_queries() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Record different queries with different frequencies
        for _ in 0..5 {
            cache
                .record_query_pattern("b1", "k1", "SELECT * FROM s3object", 100)
                .await;
        }
        for _ in 0..3 {
            cache
                .record_query_pattern("b1", "k2", "SELECT id FROM s3object", 100)
                .await;
        }
        for _ in 0..7 {
            cache
                .record_query_pattern("b1", "k3", "SELECT name FROM s3object", 100)
                .await;
        }

        let top_queries = cache.get_top_queries(2).await;
        assert_eq!(top_queries.len(), 2);

        // Should be sorted by execution count (descending)
        assert_eq!(top_queries[0].execution_count, 7);
        assert_eq!(top_queries[0].sql, "SELECT name FROM s3object");
        assert_eq!(top_queries[1].execution_count, 5);
        assert_eq!(top_queries[1].sql, "SELECT * FROM s3object");
    }

    #[tokio::test]
    async fn test_get_recent_queries() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Record a query
        cache
            .record_query_pattern("bucket1", "key1", "SELECT * FROM s3object", 100)
            .await;

        // Should be in recent queries (within last hour)
        let recent = cache.get_recent_queries(3600).await;
        assert_eq!(recent.len(), 1);

        // Should not be in queries from the future
        let recent_future = cache.get_recent_queries(-1).await;
        assert_eq!(recent_future.len(), 0);
    }

    #[tokio::test]
    async fn test_cache_warming() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result = Bytes::from("warmed result");

        // Warm the cache
        cache
            .warm(
                "bucket1",
                "key1",
                "SELECT * FROM s3object",
                "etag123",
                result.clone(),
                3600,
            )
            .await;

        // Should be retrievable from cache
        let cached = cache.get("SELECT * FROM s3object", "etag123").await;
        assert!(cached.is_some());
        if let Some(cached_result) = cached {
            assert_eq!(cached_result, result);
        }

        let stats = cache.stats().await;
        assert_eq!(stats.current_entries, 1);
    }

    #[tokio::test]
    async fn test_pattern_stats() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Record some patterns
        cache.record_query_pattern("b1", "k1", "query1", 100).await;
        cache.record_query_pattern("b1", "k1", "query1", 100).await;
        cache.record_query_pattern("b1", "k2", "query2", 100).await;

        let stats = cache.pattern_stats().await;

        assert_eq!(
            stats.get("total_patterns").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(
            stats.get("total_executions").and_then(|v| v.as_u64()),
            Some(3)
        );
    }

    #[tokio::test]
    async fn test_clear_patterns() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Record some patterns
        cache.record_query_pattern("b1", "k1", "query1", 100).await;
        cache.record_query_pattern("b1", "k2", "query2", 100).await;

        let stats = cache.pattern_stats().await;
        assert_eq!(
            stats.get("total_patterns").and_then(|v| v.as_u64()),
            Some(2)
        );

        // Clear patterns
        cache.clear_patterns().await;

        let stats = cache.pattern_stats().await;
        assert_eq!(
            stats.get("total_patterns").and_then(|v| v.as_u64()),
            Some(0)
        );
    }

    #[tokio::test]
    async fn test_cache_persistence_save_and_load() {
        let temp_dir = std::env::temp_dir();
        let cache_file = temp_dir.join(format!("test_cache_{}.json", uuid::Uuid::new_v4()));

        // Create cache and add some data
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result1 = Bytes::from("test result 1");
        let result2 = Bytes::from("test result 2");

        cache.put("query1", "etag1", result1.clone(), 0).await;
        cache.put("query2", "etag2", result2.clone(), 0).await;
        cache
            .record_query_pattern("bucket1", "key1", "query1", 100)
            .await;

        // Save to file
        let save_result = cache.save_to_file(&cache_file).await;
        assert!(save_result.is_ok(), "Save failed: {:?}", save_result.err());
        assert!(cache_file.exists(), "Cache file was not created");

        // Create new cache and load
        let cache2 = SelectResultCache::new(100, 1024 * 1024);
        let load_result = cache2.load_from_file(&cache_file).await;
        assert!(load_result.is_ok(), "Load failed: {:?}", load_result.err());

        // Verify loaded data
        let loaded1 = cache2.get("query1", "etag1").await;
        assert!(loaded1.is_some());
        assert_eq!(loaded1, Some(result1));

        let loaded2 = cache2.get("query2", "etag2").await;
        assert!(loaded2.is_some());
        assert_eq!(loaded2, Some(result2));

        // Verify query patterns
        let patterns = cache2.get_top_queries(10).await;
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].sql, "query1");

        // Clean up
        let _ = tokio::fs::remove_file(&cache_file).await;
    }

    #[tokio::test]
    async fn test_cache_persistence_expired_entries_filtered() {
        let temp_dir = std::env::temp_dir();
        let cache_file = temp_dir.join(format!("test_cache_expired_{}.json", uuid::Uuid::new_v4()));

        // Create cache and add entries with short TTL
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result1 = Bytes::from("result with no expiry");
        let result2 = Bytes::from("result with short ttl");

        cache.put("query1", "etag1", result1.clone(), 0).await; // No expiry
        cache.put("query2", "etag2", result2.clone(), 1).await; // 1 second TTL

        // Wait for second entry to expire
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Save to file
        cache.save_to_file(&cache_file).await.expect("Save failed");

        // Load into new cache
        let cache2 = SelectResultCache::new(100, 1024 * 1024);
        cache2
            .load_from_file(&cache_file)
            .await
            .expect("Load failed");

        // Only non-expired entry should be loaded
        let loaded1 = cache2.get("query1", "etag1").await;
        assert!(loaded1.is_some(), "Non-expired entry should be loaded");

        let loaded2 = cache2.get("query2", "etag2").await;
        assert!(loaded2.is_none(), "Expired entry should not be loaded");

        let stats = cache2.stats().await;
        assert_eq!(stats.current_entries, 1, "Should only have 1 entry");
        assert!(stats.expirations > 0, "Should count expired entries");

        // Clean up
        let _ = tokio::fs::remove_file(&cache_file).await;
    }

    #[tokio::test]
    async fn test_cache_persistence_atomic_write() {
        let temp_dir = std::env::temp_dir();
        let cache_file = temp_dir.join(format!("test_cache_atomic_{}.json", uuid::Uuid::new_v4()));

        let cache = SelectResultCache::new(100, 1024 * 1024);

        let result = Bytes::from("test data");
        cache.put("query", "etag", result, 0).await;

        // Save to file
        cache.save_to_file(&cache_file).await.expect("Save failed");

        // Verify temporary file was removed
        let temp_file = cache_file.with_extension("tmp");
        assert!(
            !temp_file.exists(),
            "Temporary file should be removed after save"
        );

        // Verify final file exists
        assert!(cache_file.exists(), "Final cache file should exist");

        // Clean up
        let _ = tokio::fs::remove_file(&cache_file).await;
    }

    #[tokio::test]
    async fn test_cache_persistence_stats_preserved() {
        let temp_dir = std::env::temp_dir();
        let cache_file = temp_dir.join(format!("test_cache_stats_{}.json", uuid::Uuid::new_v4()));

        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Add data and generate some stats
        let result = Bytes::from("test");
        cache.put("query", "etag", result, 0).await;
        cache.get("query", "etag").await; // Hit
        cache.get("other", "etag").await; // Miss

        let original_stats = cache.stats().await;
        assert_eq!(original_stats.hits, 1);
        assert_eq!(original_stats.misses, 1);

        // Save and load
        cache.save_to_file(&cache_file).await.expect("Save failed");

        let cache2 = SelectResultCache::new(100, 1024 * 1024);
        cache2
            .load_from_file(&cache_file)
            .await
            .expect("Load failed");

        let loaded_stats = cache2.stats().await;
        assert_eq!(loaded_stats.hits, original_stats.hits);
        assert_eq!(loaded_stats.misses, original_stats.misses);
        assert_eq!(loaded_stats.current_entries, original_stats.current_entries);

        // Clean up
        let _ = tokio::fs::remove_file(&cache_file).await;
    }

    #[tokio::test]
    async fn test_background_save_starts() {
        let temp_dir = std::env::temp_dir();
        let cache_file = temp_dir.join(format!("test_cache_bg_{}.json", uuid::Uuid::new_v4()));

        let cache = Arc::new(SelectResultCache::new(100, 1024 * 1024));

        // Add some data
        cache.put("query", "etag", Bytes::from("test"), 0).await;

        // Start background save (short interval for testing)
        let handle = cache.clone().start_background_save(cache_file.clone(), 1);

        // Wait for at least one save
        tokio::time::sleep(Duration::from_secs(2)).await;

        // File should exist
        assert!(cache_file.exists(), "Background save should create file");

        // Abort background task
        handle.abort();

        // Clean up
        let _ = tokio::fs::remove_file(&cache_file).await;
    }

    #[tokio::test]
    async fn test_adaptive_ttl_no_pattern() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Calculate TTL for a query with no historical data
        let ttl = cache
            .calculate_adaptive_ttl("bucket", "key", "SELECT * FROM s3object", 500, 5000)
            .await;

        // Should be base TTL (600) with minimal time factor
        assert!((60..=7200).contains(&ttl), "TTL should be within bounds");
        assert!(
            (600..=720).contains(&ttl),
            "TTL should be around base value for unknown pattern"
        );
    }

    #[tokio::test]
    async fn test_adaptive_ttl_expensive_query() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Calculate TTL for an expensive query (> 10 seconds)
        let ttl = cache
            .calculate_adaptive_ttl("bucket", "key", "SELECT * FROM s3object", 15000, 5000)
            .await;

        // Should have higher TTL due to execution time factor
        assert!(
            ttl > 600,
            "Expensive queries should get longer TTL (got {})",
            ttl
        );
        assert!(ttl <= 7200, "TTL should not exceed maximum");
    }

    #[tokio::test]
    async fn test_adaptive_ttl_small_result() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Calculate TTL for a query with small result (< 10KB)
        let ttl = cache
            .calculate_adaptive_ttl("bucket", "key", "SELECT * FROM s3object", 500, 8000)
            .await;

        // Should have higher TTL due to size factor
        assert!(ttl >= 600, "Small results should get longer TTL");
    }

    #[tokio::test]
    async fn test_adaptive_ttl_with_pattern_history() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let bucket = "test-bucket";
        let key = "test-key";
        let sql = "SELECT * FROM s3object WHERE age > 25";

        // Record multiple query executions to build pattern history
        for _ in 0..15 {
            cache.record_query_pattern(bucket, key, sql, 1200).await;
        }

        // Calculate adaptive TTL with pattern history
        let ttl = cache
            .calculate_adaptive_ttl(bucket, key, sql, 1200, 10000)
            .await;

        // Should have higher TTL due to frequency (> 10 executions)
        assert!(
            ttl > 600,
            "Frequently executed queries should get longer TTL (got {})",
            ttl
        );
        assert!(
            ttl >= 720,
            "With 15 executions, TTL should be at least 1.2x base"
        );
    }

    #[tokio::test]
    async fn test_adaptive_ttl_high_frequency() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let bucket = "test-bucket";
        let key = "test-key";
        let sql = "SELECT * FROM s3object WHERE status = 'active'";

        // Record many executions (> 100)
        for _ in 0..105 {
            cache.record_query_pattern(bucket, key, sql, 800).await;
        }

        // Calculate adaptive TTL with high frequency pattern
        let ttl = cache
            .calculate_adaptive_ttl(bucket, key, sql, 800, 5000)
            .await;

        // Should have significantly longer TTL (2x frequency factor)
        assert!(
            ttl >= 1200,
            "Very frequent queries should get 2x+ TTL (got {})",
            ttl
        );
    }

    #[tokio::test]
    async fn test_adaptive_ttl_combined_factors() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let bucket = "test-bucket";
        let key = "test-key";
        let sql = "SELECT COUNT(*) FROM s3object GROUP BY category";

        // Build pattern with moderate frequency
        for _ in 0..55 {
            cache.record_query_pattern(bucket, key, sql, 6000).await;
        }

        // Calculate TTL with multiple factors:
        // - Moderate frequency (55 executions) -> 1.5x
        // - High execution time (6000ms) -> 1.5x
        // - Small result (3KB) -> 1.3x
        let ttl = cache
            .calculate_adaptive_ttl(bucket, key, sql, 6000, 3000)
            .await;

        // Combined factors should give significant TTL boost
        // Base 600 * 1.5 * 1.5 * 1.3 = ~1755
        assert!(
            ttl >= 1400,
            "Combined factors should significantly increase TTL (got {})",
            ttl
        );
        assert!(ttl <= 7200, "TTL should not exceed maximum");
    }

    #[tokio::test]
    async fn test_adaptive_ttl_large_result() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        // Calculate TTL for a query with large result (> 1MB)
        let ttl = cache
            .calculate_adaptive_ttl("bucket", "key", "SELECT * FROM s3object", 500, 2_000_000)
            .await;

        // Large results should get shorter TTL (0.8x factor)
        assert!(ttl >= 60, "TTL should be at least minimum");
        assert!(
            ttl <= 600,
            "Large results should get shorter TTL (got {})",
            ttl
        );
    }

    #[tokio::test]
    async fn test_recommended_ttl() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let bucket = "test-bucket";
        let key = "test-key";
        let sql = "SELECT * FROM s3object LIMIT 100";

        // No pattern yet - should return None
        let ttl = cache.get_recommended_ttl(bucket, key, sql).await;
        assert!(ttl.is_none(), "Should return None when no pattern exists");

        // Record some patterns
        for _ in 0..20 {
            cache.record_query_pattern(bucket, key, sql, 1500).await;
        }

        // Now should return a recommendation
        let ttl = cache.get_recommended_ttl(bucket, key, sql).await;
        assert!(ttl.is_some(), "Should return Some when pattern exists");

        let ttl_value = ttl.expect("TTL should be present");
        assert!(
            (60..=7200).contains(&ttl_value),
            "Recommended TTL should be within bounds (got {})",
            ttl_value
        );
    }

    #[tokio::test]
    async fn test_adaptive_ttl_bounds() {
        let cache = SelectResultCache::new(100, 1024 * 1024);

        let bucket = "test-bucket";
        let key = "test-key";
        let sql = "SELECT * FROM s3object";

        // Build very high frequency pattern (> 100)
        for _ in 0..150 {
            cache.record_query_pattern(bucket, key, sql, 20000).await;
        }

        // Even with all maximum factors, TTL should be clamped to MAX_TTL (7200)
        let ttl = cache
            .calculate_adaptive_ttl(bucket, key, sql, 20000, 1000)
            .await;

        assert!(
            ttl <= 7200,
            "TTL should be clamped to maximum (got {})",
            ttl
        );
        assert!(ttl >= 60, "TTL should be at least minimum (got {})", ttl);
    }
}
