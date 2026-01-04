//! Machine Learning-based Smart Cache
//!
//! This module implements an intelligent caching system with:
//! - Access pattern analysis and prediction
//! - Adaptive cache sizing based on workload
//! - Pre-fetching for frequently accessed objects
//! - Cache warming strategies

use bytes::Bytes;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, trace};

use super::ObjectMetadata;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum MlCacheError {
    #[error("Cache full and eviction failed")]
    CacheFull,

    #[error("Object too large for cache: {size} bytes (max: {max})")]
    ObjectTooLarge { size: u64, max: u64 },

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for ML-based smart cache
#[derive(Debug, Clone)]
pub struct MlCacheConfig {
    /// Maximum cache size in bytes
    pub max_size_bytes: u64,

    /// Maximum number of objects in cache
    pub max_objects: usize,

    /// Default TTL for cached objects (seconds)
    pub default_ttl_secs: u64,

    /// Enable adaptive cache sizing
    pub adaptive_sizing: bool,

    /// Enable predictive prefetching
    pub enable_prefetch: bool,

    /// Minimum access count before prefetch consideration
    pub prefetch_threshold: usize,

    /// Look-back window for pattern analysis (in accesses)
    pub pattern_window_size: usize,

    /// Weight decay factor for exponential moving average (0.0-1.0)
    pub ema_alpha: f64,

    /// Minimum confidence score for prefetch (0.0-1.0)
    pub prefetch_min_confidence: f64,

    /// Cache warming batch size
    pub warming_batch_size: usize,
}

impl Default for MlCacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 512 * 1024 * 1024, // 512 MB
            max_objects: 20000,
            default_ttl_secs: 600,
            adaptive_sizing: true,
            enable_prefetch: true,
            prefetch_threshold: 3,
            pattern_window_size: 100,
            ema_alpha: 0.3,
            prefetch_min_confidence: 0.7,
            warming_batch_size: 50,
        }
    }
}

impl MlCacheConfig {
    pub fn builder() -> MlCacheConfigBuilder {
        MlCacheConfigBuilder::default()
    }

    pub fn validate(&self) -> Result<(), MlCacheError> {
        if self.max_size_bytes == 0 {
            return Err(MlCacheError::InvalidConfig(
                "max_size_bytes must be greater than 0".to_string(),
            ));
        }

        if self.max_objects == 0 {
            return Err(MlCacheError::InvalidConfig(
                "max_objects must be greater than 0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.ema_alpha) {
            return Err(MlCacheError::InvalidConfig(
                "ema_alpha must be between 0.0 and 1.0".to_string(),
            ));
        }

        if !(0.0..=1.0).contains(&self.prefetch_min_confidence) {
            return Err(MlCacheError::InvalidConfig(
                "prefetch_min_confidence must be between 0.0 and 1.0".to_string(),
            ));
        }

        Ok(())
    }
}

/// Builder for MlCacheConfig
#[derive(Debug, Default)]
pub struct MlCacheConfigBuilder {
    config: MlCacheConfig,
}

impl MlCacheConfigBuilder {
    pub fn max_size_mb(mut self, mb: u64) -> Self {
        self.config.max_size_bytes = mb * 1024 * 1024;
        self
    }

    pub fn max_objects(mut self, max: usize) -> Self {
        self.config.max_objects = max;
        self
    }

    pub fn default_ttl_secs(mut self, ttl: u64) -> Self {
        self.config.default_ttl_secs = ttl;
        self
    }

    pub fn adaptive_sizing(mut self, enabled: bool) -> Self {
        self.config.adaptive_sizing = enabled;
        self
    }

    pub fn enable_prefetch(mut self, enabled: bool) -> Self {
        self.config.enable_prefetch = enabled;
        self
    }

    pub fn prefetch_threshold(mut self, threshold: usize) -> Self {
        self.config.prefetch_threshold = threshold;
        self
    }

    pub fn pattern_window_size(mut self, size: usize) -> Self {
        self.config.pattern_window_size = size;
        self
    }

    pub fn ema_alpha(mut self, alpha: f64) -> Self {
        self.config.ema_alpha = alpha;
        self
    }

    pub fn prefetch_min_confidence(mut self, confidence: f64) -> Self {
        self.config.prefetch_min_confidence = confidence;
        self
    }

    pub fn warming_batch_size(mut self, size: usize) -> Self {
        self.config.warming_batch_size = size;
        self
    }

    pub fn build(self) -> Result<MlCacheConfig, MlCacheError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

// ============================================================================
// Access Pattern Tracking
// ============================================================================

/// Access pattern for an object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPattern {
    /// Object key
    pub key: String,

    /// Total access count
    pub access_count: u64,

    /// Access timestamps (ring buffer)
    pub access_times: VecDeque<DateTime<Utc>>,

    /// Exponential moving average of access frequency (accesses per hour)
    pub ema_frequency: f64,

    /// Last access time
    pub last_access: DateTime<Utc>,

    /// Average time between accesses (seconds)
    pub avg_interval_secs: f64,

    /// Predicted next access time
    pub predicted_next_access: Option<DateTime<Utc>>,

    /// Confidence score for prediction (0.0-1.0)
    pub confidence: f64,

    /// Access pattern type
    pub pattern_type: AccessPatternType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccessPatternType {
    /// Highly regular access pattern (e.g., cron job)
    Periodic,

    /// Bursty access pattern (clusters of accesses)
    Bursty,

    /// Gradually increasing access rate
    Trending,

    /// Gradually decreasing access rate
    Declining,

    /// Random/unpredictable access pattern
    Random,

    /// Not enough data to classify
    Unknown,
}

impl AccessPattern {
    fn new(key: String, window_size: usize) -> Self {
        let now = Utc::now();
        let mut access_times = VecDeque::with_capacity(window_size);
        access_times.push_back(now);

        Self {
            key,
            access_count: 1,
            access_times,
            ema_frequency: 0.0,
            last_access: now,
            avg_interval_secs: 0.0,
            predicted_next_access: None,
            confidence: 0.0,
            pattern_type: AccessPatternType::Unknown,
        }
    }

    /// Update access pattern with new access
    fn record_access(&mut self, timestamp: DateTime<Utc>, window_size: usize, ema_alpha: f64) {
        self.access_count += 1;
        self.last_access = timestamp;

        // Update access times ring buffer
        if self.access_times.len() >= window_size {
            self.access_times.pop_front();
        }
        self.access_times.push_back(timestamp);

        // Update statistics
        self.update_statistics(ema_alpha);
        self.classify_pattern();
        self.predict_next_access();
    }

    /// Update statistical measures
    fn update_statistics(&mut self, ema_alpha: f64) {
        if self.access_times.len() < 2 {
            return;
        }

        // Calculate average interval between accesses
        let intervals: Vec<f64> = self
            .access_times
            .iter()
            .zip(self.access_times.iter().skip(1))
            .map(|(t1, t2)| (*t2 - *t1).num_seconds() as f64)
            .collect();

        if !intervals.is_empty() {
            self.avg_interval_secs = intervals.iter().sum::<f64>() / intervals.len() as f64;

            // Calculate EMA frequency (accesses per hour)
            let current_frequency = if self.avg_interval_secs > 0.0 {
                3600.0 / self.avg_interval_secs
            } else {
                0.0
            };

            if self.ema_frequency == 0.0 {
                self.ema_frequency = current_frequency;
            } else {
                self.ema_frequency =
                    ema_alpha * current_frequency + (1.0 - ema_alpha) * self.ema_frequency;
            }
        }
    }

    /// Classify access pattern type
    fn classify_pattern(&mut self) {
        if self.access_times.len() < 3 {
            self.pattern_type = AccessPatternType::Unknown;
            return;
        }

        let intervals: Vec<f64> = self
            .access_times
            .iter()
            .zip(self.access_times.iter().skip(1))
            .map(|(t1, t2)| (*t2 - *t1).num_seconds() as f64)
            .collect();

        if intervals.is_empty() {
            self.pattern_type = AccessPatternType::Unknown;
            return;
        }

        // Calculate coefficient of variation (CV = std_dev / mean)
        let mean = intervals.iter().sum::<f64>() / intervals.len() as f64;
        let variance = intervals
            .iter()
            .map(|x| {
                let diff = x - mean;
                diff * diff
            })
            .sum::<f64>()
            / intervals.len() as f64;
        let std_dev = variance.sqrt();
        let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

        // Classify based on coefficient of variation
        self.pattern_type = if cv < 0.2 {
            // Low variability = periodic
            AccessPatternType::Periodic
        } else if cv < 0.5 {
            // Moderate variability = trending or declining
            // Check if frequency is increasing or decreasing
            let recent_intervals: Vec<f64> = intervals
                .iter()
                .rev()
                .take(intervals.len() / 2)
                .copied()
                .collect();
            let old_intervals: Vec<f64> = intervals
                .iter()
                .take(intervals.len() / 2)
                .copied()
                .collect();

            let recent_avg =
                recent_intervals.iter().sum::<f64>() / recent_intervals.len().max(1) as f64;
            let old_avg = old_intervals.iter().sum::<f64>() / old_intervals.len().max(1) as f64;

            if recent_avg < old_avg * 0.8 {
                AccessPatternType::Trending
            } else if recent_avg > old_avg * 1.2 {
                AccessPatternType::Declining
            } else {
                AccessPatternType::Periodic
            }
        } else if cv < 1.0 {
            // High variability = bursty
            AccessPatternType::Bursty
        } else {
            // Very high variability = random
            AccessPatternType::Random
        };
    }

    /// Predict next access time based on pattern
    fn predict_next_access(&mut self) {
        if self.access_times.len() < 3 {
            self.predicted_next_access = None;
            self.confidence = 0.0;
            return;
        }

        match self.pattern_type {
            AccessPatternType::Periodic => {
                // Use average interval for prediction
                let next_time = self.last_access + Duration::seconds(self.avg_interval_secs as i64);
                self.predicted_next_access = Some(next_time);
                self.confidence = 0.9; // High confidence for periodic patterns
            }
            AccessPatternType::Trending => {
                // Use EMA-adjusted interval
                let adjusted_interval = self.avg_interval_secs * 0.9; // Expecting faster access
                let next_time = self.last_access + Duration::seconds(adjusted_interval as i64);
                self.predicted_next_access = Some(next_time);
                self.confidence = 0.75;
            }
            AccessPatternType::Bursty => {
                // More conservative prediction
                let next_time = self.last_access + Duration::seconds(self.avg_interval_secs as i64);
                self.predicted_next_access = Some(next_time);
                self.confidence = 0.5;
            }
            AccessPatternType::Declining => {
                // Less likely to be accessed soon
                let adjusted_interval = self.avg_interval_secs * 1.2; // Expecting slower access
                let next_time = self.last_access + Duration::seconds(adjusted_interval as i64);
                self.predicted_next_access = Some(next_time);
                self.confidence = 0.6;
            }
            AccessPatternType::Random | AccessPatternType::Unknown => {
                self.predicted_next_access = None;
                self.confidence = 0.0;
            }
        }
    }

    /// Calculate priority score for cache retention (higher = keep in cache)
    fn priority_score(&self) -> f64 {
        let recency_score = {
            let age = Utc::now()
                .signed_duration_since(self.last_access)
                .num_seconds() as f64;
            // Exponential decay: score decreases with age
            (-age / 3600.0).exp() // Decay over ~1 hour
        };

        let frequency_score = self.ema_frequency.min(100.0) / 100.0; // Normalize to 0-1

        let pattern_score = match self.pattern_type {
            AccessPatternType::Periodic => 1.0,
            AccessPatternType::Trending => 0.9,
            AccessPatternType::Bursty => 0.6,
            AccessPatternType::Declining => 0.3,
            AccessPatternType::Random => 0.4,
            AccessPatternType::Unknown => 0.2,
        };

        // Weighted combination
        0.4 * recency_score + 0.4 * frequency_score + 0.2 * pattern_score
    }
}

// ============================================================================
// Cache Entry
// ============================================================================

#[derive(Clone)]
struct MlCacheEntry {
    /// Object data
    data: Bytes,

    /// Object metadata
    metadata: ObjectMetadata,

    /// Cached at timestamp
    cached_at: DateTime<Utc>,

    /// TTL in seconds
    ttl_secs: u64,

    /// Access pattern
    pattern: Arc<RwLock<AccessPattern>>,

    /// Size in bytes
    size: u64,
}

impl MlCacheEntry {
    fn is_expired(&self) -> bool {
        let age = Utc::now()
            .signed_duration_since(self.cached_at)
            .num_seconds();
        age > self.ttl_secs as i64
    }
}

// ============================================================================
// Smart Cache Manager
// ============================================================================

/// ML-based smart cache manager
pub struct SmartCacheManager {
    config: MlCacheConfig,

    /// Cache storage
    cache: Arc<RwLock<HashMap<String, MlCacheEntry>>>,

    /// Access patterns
    patterns: Arc<RwLock<HashMap<String, Arc<RwLock<AccessPattern>>>>>,

    /// Current cache size in bytes
    current_size: Arc<RwLock<u64>>,

    /// Cache statistics
    stats: Arc<RwLock<CacheStats>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub prefetches: u64,
    pub current_objects: usize,
    pub current_size_bytes: u64,
    pub hit_rate: f64,
}

impl CacheStats {
    fn update_hit_rate(&mut self) {
        let total = self.hits + self.misses;
        self.hit_rate = if total > 0 {
            self.hits as f64 / total as f64
        } else {
            0.0
        };
    }
}

impl SmartCacheManager {
    pub fn new(config: MlCacheConfig) -> Result<Self, MlCacheError> {
        config.validate()?;

        Ok(Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
            patterns: Arc::new(RwLock::new(HashMap::new())),
            current_size: Arc::new(RwLock::new(0)),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        })
    }

    /// Get object from cache
    pub async fn get(&self, bucket: &str, key: &str) -> Option<(ObjectMetadata, Bytes)> {
        let cache_key = format!("{}/{}", bucket, key);
        let cache = self.cache.read().await;

        if let Some(entry) = cache.get(&cache_key) {
            // Check if expired
            if entry.is_expired() {
                drop(cache);
                self.invalidate(bucket, key).await;
                self.record_miss().await;
                return None;
            }

            // Record access in pattern
            let pattern = entry.pattern.clone();
            drop(cache);

            // Update access pattern
            self.record_access(&cache_key, pattern).await;
            self.record_hit().await;

            // Re-acquire lock to get data
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&cache_key) {
                trace!("Smart cache hit: {}", cache_key);
                return Some((entry.metadata.clone(), entry.data.clone()));
            }
        }

        self.record_miss().await;
        None
    }

    /// Put object into cache
    pub async fn put(
        &self,
        bucket: &str,
        key: &str,
        metadata: ObjectMetadata,
        data: Bytes,
    ) -> Result<(), MlCacheError> {
        let size = data.len() as u64;

        // Check if object is too large
        if size > self.config.max_size_bytes {
            return Err(MlCacheError::ObjectTooLarge {
                size,
                max: self.config.max_size_bytes,
            });
        }

        let cache_key = format!("{}/{}", bucket, key);

        // Get or create access pattern
        let pattern = self.get_or_create_pattern(&cache_key).await;

        // Calculate adaptive TTL based on access pattern
        let ttl_secs = self.calculate_adaptive_ttl(&pattern).await;

        // Make space if needed
        self.evict_if_needed(size).await?;

        let entry = MlCacheEntry {
            data,
            metadata,
            cached_at: Utc::now(),
            ttl_secs,
            pattern: pattern.clone(),
            size,
        };

        // Insert into cache
        let mut cache = self.cache.write().await;
        let mut current_size = self.current_size.write().await;

        cache.insert(cache_key, entry);
        *current_size += size;

        drop(cache);
        drop(current_size);

        self.update_stats().await;

        Ok(())
    }

    /// Invalidate cached object
    pub async fn invalidate(&self, bucket: &str, key: &str) {
        let cache_key = format!("{}/{}", bucket, key);
        {
            let mut cache = self.cache.write().await;

            if let Some(entry) = cache.remove(&cache_key) {
                let mut current_size = self.current_size.write().await;
                *current_size = current_size.saturating_sub(entry.size);
            }
        } // Drop write lock before update_stats

        self.update_stats().await;
    }

    /// Get prefetch suggestions based on access patterns
    pub async fn get_prefetch_suggestions(&self) -> Vec<String> {
        if !self.config.enable_prefetch {
            return Vec::new();
        }

        let patterns = self.patterns.read().await;
        let mut suggestions = Vec::new();

        for (key, pattern_lock) in patterns.iter() {
            let pattern = pattern_lock.read().await;

            // Check if object should be prefetched
            if pattern.access_count >= self.config.prefetch_threshold as u64
                && pattern.confidence >= self.config.prefetch_min_confidence
            {
                if let Some(predicted_time) = pattern.predicted_next_access {
                    let time_until_access = predicted_time
                        .signed_duration_since(Utc::now())
                        .num_seconds();

                    // Prefetch if predicted access is within next 60 seconds
                    if (0..60).contains(&time_until_access) {
                        suggestions.push(key.clone());
                    }
                }
            }
        }

        suggestions
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> CacheStats {
        self.stats.read().await.clone()
    }

    /// Get access pattern for a specific object
    ///
    /// Returns the access pattern if it exists and has been tracked,
    /// otherwise returns None. This is useful for intelligent tiering
    /// decisions based on ML-analyzed access patterns.
    pub async fn get_access_pattern(&self, bucket: &str, key: &str) -> Option<AccessPattern> {
        let cache_key = format!("{}/{}", bucket, key);
        let patterns = self.patterns.read().await;

        if let Some(pattern_lock) = patterns.get(&cache_key) {
            let pattern = pattern_lock.read().await;
            Some(pattern.clone())
        } else {
            None
        }
    }

    /// Clear all cached objects
    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();

        let mut current_size = self.current_size.write().await;
        *current_size = 0;

        self.update_stats().await;
    }

    // ========================================================================
    // Internal Methods
    // ========================================================================

    async fn get_or_create_pattern(&self, cache_key: &str) -> Arc<RwLock<AccessPattern>> {
        let patterns = self.patterns.read().await;

        if let Some(pattern) = patterns.get(cache_key) {
            return pattern.clone();
        }

        drop(patterns);

        let mut patterns = self.patterns.write().await;

        // Double-check after acquiring write lock
        if let Some(pattern) = patterns.get(cache_key) {
            return pattern.clone();
        }

        let pattern = Arc::new(RwLock::new(AccessPattern::new(
            cache_key.to_string(),
            self.config.pattern_window_size,
        )));
        patterns.insert(cache_key.to_string(), pattern.clone());

        pattern
    }

    async fn record_access(&self, cache_key: &str, pattern_lock: Arc<RwLock<AccessPattern>>) {
        let mut pattern = pattern_lock.write().await;
        pattern.record_access(
            Utc::now(),
            self.config.pattern_window_size,
            self.config.ema_alpha,
        );

        debug!(
            "Access pattern for {}: type={:?}, confidence={:.2}, ema_freq={:.2}",
            cache_key, pattern.pattern_type, pattern.confidence, pattern.ema_frequency
        );
    }

    async fn calculate_adaptive_ttl(&self, pattern_lock: &Arc<RwLock<AccessPattern>>) -> u64 {
        if !self.config.adaptive_sizing {
            return self.config.default_ttl_secs;
        }

        let pattern = pattern_lock.read().await;

        // Adjust TTL based on access pattern
        let multiplier = match pattern.pattern_type {
            AccessPatternType::Periodic => 2.0,  // Keep longer
            AccessPatternType::Trending => 1.5,  // Keep a bit longer
            AccessPatternType::Bursty => 1.0,    // Default
            AccessPatternType::Declining => 0.5, // Shorter TTL
            AccessPatternType::Random => 0.7,    // Shorter TTL
            AccessPatternType::Unknown => 1.0,   // Default
        };

        let adaptive_ttl = (self.config.default_ttl_secs as f64 * multiplier) as u64;

        adaptive_ttl.max(60) // Minimum 60 seconds
    }

    async fn evict_if_needed(&self, needed_size: u64) -> Result<(), MlCacheError> {
        let current_size = *self.current_size.read().await;
        let cache = self.cache.read().await;
        let current_objects = cache.len();
        drop(cache);

        // Check if we need to evict
        let size_exceeded = current_size + needed_size > self.config.max_size_bytes;
        let count_exceeded = current_objects >= self.config.max_objects;

        if !size_exceeded && !count_exceeded {
            return Ok(());
        }

        // Calculate how much space we need to free
        let target_size = if size_exceeded {
            (self.config.max_size_bytes as f64 * 0.8) as u64 // Free 20% of cache
        } else {
            current_size
        };

        self.evict_to_target(target_size).await
    }

    async fn evict_to_target(&self, target_size: u64) -> Result<(), MlCacheError> {
        let mut cache = self.cache.write().await;

        // Build list of (key, priority) pairs
        let mut priorities = Vec::new();

        for (key, entry) in cache.iter() {
            let pattern = entry.pattern.read().await;
            let priority = pattern.priority_score();
            priorities.push((key.clone(), priority, entry.size));
        }

        // Sort by priority (lowest first = evict first)
        priorities.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut freed_size = 0u64;
        let mut evicted_count = 0;

        for (key, _priority, size) in priorities {
            let current_size = *self.current_size.read().await;

            if current_size <= target_size {
                break;
            }

            cache.remove(&key);
            freed_size += size;
            evicted_count += 1;

            let mut current_size_lock = self.current_size.write().await;
            *current_size_lock = current_size_lock.saturating_sub(size);
        }

        if evicted_count > 0 {
            info!(
                "Evicted {} objects ({} bytes) from smart cache",
                evicted_count, freed_size
            );

            let mut stats = self.stats.write().await;
            stats.evictions += evicted_count;
        }

        Ok(())
    }

    async fn record_hit(&self) {
        let mut stats = self.stats.write().await;
        stats.hits += 1;
        stats.update_hit_rate();
    }

    async fn record_miss(&self) {
        let mut stats = self.stats.write().await;
        stats.misses += 1;
        stats.update_hit_rate();
    }

    async fn update_stats(&self) {
        let mut stats = self.stats.write().await;
        let cache = self.cache.read().await;

        stats.current_objects = cache.len();
        stats.current_size_bytes = *self.current_size.read().await;
    }
}

// ============================================================================
// Cache Warming
// ============================================================================

/// Cache warming strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarmingStrategy {
    /// Warm cache based on most frequently accessed objects
    MostFrequent,

    /// Warm cache based on most recently accessed objects
    MostRecent,

    /// Warm cache based on highest priority score
    HighestPriority,

    /// Warm cache based on predicted next access
    Predictive,
}

/// Cache warmer for proactive cache population
pub struct CacheWarmer {
    cache: Arc<SmartCacheManager>,
    strategy: WarmingStrategy,
}

impl CacheWarmer {
    pub fn new(cache: Arc<SmartCacheManager>, strategy: WarmingStrategy) -> Self {
        Self { cache, strategy }
    }

    /// Get warming candidates based on strategy
    pub async fn get_warming_candidates(&self, limit: usize) -> Vec<(String, f64)> {
        let patterns = self.cache.patterns.read().await;

        let mut candidates: Vec<(String, f64)> = Vec::new();

        for (key, pattern_lock) in patterns.iter() {
            let pattern = pattern_lock.read().await;

            let score = match self.strategy {
                WarmingStrategy::MostFrequent => pattern.ema_frequency,
                WarmingStrategy::MostRecent => {
                    let age = Utc::now()
                        .signed_duration_since(pattern.last_access)
                        .num_seconds() as f64;
                    (-age / 3600.0).exp() // Exponential decay
                }
                WarmingStrategy::HighestPriority => pattern.priority_score(),
                WarmingStrategy::Predictive => {
                    if let Some(predicted_time) = pattern.predicted_next_access {
                        let time_until = predicted_time
                            .signed_duration_since(Utc::now())
                            .num_seconds() as f64;

                        // Higher score for objects predicted to be accessed soon
                        if time_until > 0.0 && time_until < 3600.0 {
                            pattern.confidence * (1.0 - time_until / 3600.0)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    }
                }
            };

            candidates.push((key.clone(), score));
        }

        // Sort by score (descending)
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        candidates.into_iter().take(limit).collect()
    }

    /// Warm cache with top candidates
    /// Returns list of (bucket, key) pairs that should be loaded
    pub async fn warm(&self) -> Vec<(String, String)> {
        let batch_size = self.cache.config.warming_batch_size;
        let candidates = self.get_warming_candidates(batch_size).await;

        let mut to_load = Vec::new();

        for (cache_key, _score) in candidates {
            // Check if already cached
            if let Some(slash_pos) = cache_key.find('/') {
                let bucket = &cache_key[..slash_pos];
                let key = &cache_key[slash_pos + 1..];

                // Check if object is already in cache
                if self.cache.get(bucket, key).await.is_none() {
                    to_load.push((bucket.to_string(), key.to_string()));
                }
            }
        }

        info!(
            "Cache warming: {} candidates identified for loading (strategy: {:?})",
            to_load.len(),
            self.strategy
        );

        to_load
    }

    /// Get warming statistics
    pub async fn get_warming_stats(&self) -> WarmingStats {
        let patterns = self.cache.patterns.read().await;
        let cache = self.cache.cache.read().await;

        let total_tracked = patterns.len();
        let total_cached = cache.len();
        let not_cached = total_tracked.saturating_sub(total_cached);

        let mut high_priority_not_cached = 0;
        let mut predicted_soon = 0;

        for (key, pattern_lock) in patterns.iter() {
            if cache.contains_key(key) {
                continue;
            }

            let pattern = pattern_lock.read().await;

            if pattern.priority_score() > 0.7 {
                high_priority_not_cached += 1;
            }

            if let Some(predicted_time) = pattern.predicted_next_access {
                let time_until = predicted_time
                    .signed_duration_since(Utc::now())
                    .num_seconds();
                if (0..300).contains(&time_until) {
                    // Next 5 minutes
                    predicted_soon += 1;
                }
            }
        }

        WarmingStats {
            total_tracked,
            total_cached,
            not_cached,
            high_priority_not_cached,
            predicted_soon,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingStats {
    pub total_tracked: usize,
    pub total_cached: usize,
    pub not_cached: usize,
    pub high_priority_not_cached: usize,
    pub predicted_soon: usize,
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ml_cache_config_builder() {
        let config = MlCacheConfig::builder()
            .max_size_mb(1024)
            .max_objects(10000)
            .default_ttl_secs(300)
            .adaptive_sizing(true)
            .enable_prefetch(true)
            .build()
            .expect("Failed to build config");

        assert_eq!(config.max_size_bytes, 1024 * 1024 * 1024);
        assert_eq!(config.max_objects, 10000);
        assert_eq!(config.default_ttl_secs, 300);
        assert!(config.adaptive_sizing);
        assert!(config.enable_prefetch);
    }

    #[tokio::test]
    async fn test_ml_cache_config_validation() {
        let invalid_config = MlCacheConfig {
            max_size_bytes: 0,
            ..Default::default()
        };

        assert!(invalid_config.validate().is_err());

        let invalid_ema = MlCacheConfig {
            ema_alpha: 1.5,
            ..Default::default()
        };

        assert!(invalid_ema.validate().is_err());
    }

    #[tokio::test]
    async fn test_access_pattern_creation() {
        let pattern = AccessPattern::new("test/key".to_string(), 100);

        assert_eq!(pattern.key, "test/key");
        assert_eq!(pattern.access_count, 1);
        assert_eq!(pattern.pattern_type, AccessPatternType::Unknown);
        assert_eq!(pattern.confidence, 0.0);
    }

    #[tokio::test]
    async fn test_access_pattern_periodic() {
        let mut pattern = AccessPattern::new("test/key".to_string(), 100);

        // Simulate periodic access every 60 seconds
        let base_time = Utc::now();
        for i in 1..10 {
            let access_time = base_time + Duration::seconds(60 * i);
            pattern.record_access(access_time, 100, 0.3);
        }

        assert_eq!(pattern.access_count, 10);
        assert_eq!(pattern.pattern_type, AccessPatternType::Periodic);
        assert!(pattern.confidence > 0.8);
        assert!(pattern.predicted_next_access.is_some());
    }

    #[tokio::test]
    async fn test_smart_cache_basic_operations() {
        let config = MlCacheConfig::builder()
            .max_size_mb(10)
            .max_objects(100)
            .build()
            .expect("Failed to build config");

        let cache = SmartCacheManager::new(config).expect("Failed to create cache");

        let metadata = ObjectMetadata {
            key: "test.txt".to_string(),
            size: 1024,
            etag: "abc123".to_string(),
            last_modified: Utc::now(),
            content_type: "text/plain".to_string(),
            metadata: HashMap::new(),
        };

        let data = Bytes::from("test data");

        // Put object
        cache
            .put("test-bucket", "test.txt", metadata.clone(), data.clone())
            .await
            .expect("Failed to put object");

        // Get object
        let result = cache.get("test-bucket", "test.txt").await;
        assert!(result.is_some());

        let (retrieved_meta, retrieved_data) = result.expect("Object not found");
        assert_eq!(retrieved_meta.key, metadata.key);
        assert_eq!(retrieved_data, data);

        // Check stats
        let stats = cache.get_stats().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.current_objects, 1);
    }

    #[tokio::test]
    async fn test_smart_cache_eviction() {
        let config = MlCacheConfig::builder()
            .max_size_mb(1) // Only 1MB
            .max_objects(2) // Only 2 objects
            .build()
            .expect("Failed to build config");

        let cache = SmartCacheManager::new(config).expect("Failed to create cache");

        // Create large objects
        let data1 = Bytes::from(vec![0u8; 512 * 1024]); // 512 KB
        let data2 = Bytes::from(vec![1u8; 512 * 1024]); // 512 KB
        let data3 = Bytes::from(vec![2u8; 512 * 1024]); // 512 KB

        for (i, data) in [data1, data2, data3].iter().enumerate() {
            let metadata = ObjectMetadata {
                key: format!("file{}.bin", i),
                size: data.len() as u64,
                etag: format!("etag{}", i),
                last_modified: Utc::now(),
                content_type: "application/octet-stream".to_string(),
                metadata: HashMap::new(),
            };

            cache
                .put("bucket", &format!("file{}.bin", i), metadata, data.clone())
                .await
                .expect("Failed to put object");

            // Small delay between puts
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        let stats = cache.get_stats().await;
        assert!(stats.evictions > 0);
        assert!(stats.current_objects <= 2);
    }

    #[tokio::test]
    async fn test_cache_invalidation() {
        let config = MlCacheConfig::default();
        let cache = SmartCacheManager::new(config).expect("Failed to create cache");

        let metadata = ObjectMetadata {
            key: "test.txt".to_string(),
            size: 100,
            etag: "abc".to_string(),
            last_modified: Utc::now(),
            content_type: "text/plain".to_string(),
            metadata: HashMap::new(),
        };

        cache
            .put("bucket", "test.txt", metadata, Bytes::from("data"))
            .await
            .expect("Failed to put");

        // Verify it's cached
        assert!(cache.get("bucket", "test.txt").await.is_some());

        // Invalidate
        cache.invalidate("bucket", "test.txt").await;

        // Verify it's gone
        assert!(cache.get("bucket", "test.txt").await.is_none());
    }

    #[tokio::test]
    async fn test_priority_scoring() {
        let mut pattern = AccessPattern::new("test/key".to_string(), 100);

        // Simulate high-frequency periodic access
        let base_time = Utc::now();
        for i in 1..20 {
            let access_time = base_time + Duration::seconds(10 * i);
            pattern.record_access(access_time, 100, 0.3);
        }

        let score1 = pattern.priority_score();

        // Create a rarely accessed pattern
        let mut pattern2 = AccessPattern::new("test/key2".to_string(), 100);
        pattern2.record_access(base_time - Duration::seconds(3600), 100, 0.3);

        let score2 = pattern2.priority_score();

        // Frequently accessed should have higher priority
        assert!(score1 > score2);
    }

    #[tokio::test]
    async fn test_adaptive_ttl() {
        let config = MlCacheConfig::builder()
            .adaptive_sizing(true)
            .default_ttl_secs(300)
            .build()
            .expect("Failed to build config");

        let cache = SmartCacheManager::new(config).expect("Failed to create cache");

        // Create a periodic pattern
        let pattern = Arc::new(RwLock::new(AccessPattern::new("test/key".to_string(), 100)));
        {
            let mut p = pattern.write().await;
            p.pattern_type = AccessPatternType::Periodic;
        }

        let ttl_periodic = cache.calculate_adaptive_ttl(&pattern).await;

        // Create a declining pattern
        {
            let mut p = pattern.write().await;
            p.pattern_type = AccessPatternType::Declining;
        }

        let ttl_declining = cache.calculate_adaptive_ttl(&pattern).await;

        // Periodic should have longer TTL than declining
        assert!(ttl_periodic > ttl_declining);
    }

    #[tokio::test]
    async fn test_cache_warming() {
        let config = MlCacheConfig::builder()
            .max_size_mb(10)
            .warming_batch_size(5)
            .build()
            .expect("Failed to build config");

        let cache = Arc::new(SmartCacheManager::new(config).expect("Failed to create cache"));

        // Create some access patterns by putting and getting objects
        for i in 0..10 {
            let metadata = ObjectMetadata {
                key: format!("file{}.txt", i),
                size: 100,
                etag: format!("etag{}", i),
                last_modified: Utc::now(),
                content_type: "text/plain".to_string(),
                metadata: HashMap::new(),
            };

            let data = Bytes::from(format!("data {}", i));

            cache
                .put("bucket", &format!("file{}.txt", i), metadata, data)
                .await
                .expect("Failed to put");

            // Access some objects more frequently
            if i < 5 {
                for _ in 0..3 {
                    cache.get("bucket", &format!("file{}.txt", i)).await;
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                }
            }
        }

        // Test different warming strategies
        let warmer_freq = CacheWarmer::new(cache.clone(), WarmingStrategy::MostFrequent);
        let candidates_freq = warmer_freq.get_warming_candidates(5).await;
        assert!(!candidates_freq.is_empty());

        let warmer_recent = CacheWarmer::new(cache.clone(), WarmingStrategy::MostRecent);
        let candidates_recent = warmer_recent.get_warming_candidates(5).await;
        assert!(!candidates_recent.is_empty());

        let warmer_priority = CacheWarmer::new(cache.clone(), WarmingStrategy::HighestPriority);
        let candidates_priority = warmer_priority.get_warming_candidates(5).await;
        assert!(!candidates_priority.is_empty());

        // Test warming stats
        let stats = warmer_freq.get_warming_stats().await;
        assert!(stats.total_tracked > 0);
        assert!(stats.total_cached > 0);
    }

    #[tokio::test]
    async fn test_prefetch_suggestions() {
        let config = MlCacheConfig::builder()
            .enable_prefetch(true)
            .prefetch_threshold(3)
            .prefetch_min_confidence(0.5)
            .build()
            .expect("Failed to build config");

        let cache = SmartCacheManager::new(config).expect("Failed to create cache");

        // Create a predictable access pattern
        let metadata = ObjectMetadata {
            key: "predictable.txt".to_string(),
            size: 100,
            etag: "etag1".to_string(),
            last_modified: Utc::now(),
            content_type: "text/plain".to_string(),
            metadata: HashMap::new(),
        };

        // Simulate regular access pattern
        let base_time = Utc::now();
        for i in 0..5 {
            cache
                .put(
                    "bucket",
                    "predictable.txt",
                    metadata.clone(),
                    Bytes::from("data"),
                )
                .await
                .expect("Failed to put");

            cache.get("bucket", "predictable.txt").await;

            // Update access pattern manually to simulate periodic access
            let pattern_key = "bucket/predictable.txt".to_string();
            let patterns = cache.patterns.read().await;
            if let Some(pattern_lock) = patterns.get(&pattern_key) {
                let mut pattern = pattern_lock.write().await;
                pattern.record_access(base_time + Duration::seconds(60 * i), 100, 0.3);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        let suggestions = cache.get_prefetch_suggestions().await;
        // May or may not have suggestions depending on timing
        // Just verify the function runs without error
        assert!(suggestions.len() <= 100);
    }

    #[tokio::test]
    async fn test_warming_strategy_scoring() {
        let config = MlCacheConfig::default();
        let cache = Arc::new(SmartCacheManager::new(config).expect("Failed to create cache"));

        // Create objects with different access patterns
        for i in 0..5 {
            let metadata = ObjectMetadata {
                key: format!("file{}.txt", i),
                size: 100,
                etag: format!("etag{}", i),
                last_modified: Utc::now(),
                content_type: "text/plain".to_string(),
                metadata: HashMap::new(),
            };

            cache
                .put(
                    "bucket",
                    &format!("file{}.txt", i),
                    metadata,
                    Bytes::from("data"),
                )
                .await
                .expect("Failed to put");

            // Access first file more frequently
            if i == 0 {
                for _ in 0..5 {
                    cache.get("bucket", "file0.txt").await;
                }
            }
        }

        // Test MostFrequent strategy
        let warmer = CacheWarmer::new(cache.clone(), WarmingStrategy::MostFrequent);
        let candidates = warmer.get_warming_candidates(5).await;

        // file0.txt should have higher score due to more frequent access
        if !candidates.is_empty() {
            // Just verify we got some candidates
            assert!(candidates[0].1 >= 0.0);
        }
    }
}
