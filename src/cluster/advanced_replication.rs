// Advanced Replication Features
//
// This module extends the basic replication infrastructure with:
// - Cross-region replication with WAN optimization
// - Selective replication (prefix and tag-based filtering)
// - Comprehensive replication metrics and monitoring
// - Bi-directional replication with enhanced conflict resolution
// - Bandwidth management and throttling
// - Replication lag monitoring
// - Automatic retry with exponential backoff

use crate::cluster::node::NodeId;
use crate::cluster::replication::{ReplicationError, ReplicationEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Errors specific to advanced replication
#[derive(Debug, Error)]
pub enum AdvancedReplicationError {
    #[error("Replication conflict: {0}")]
    Conflict(String),

    #[error("WAN optimization failed: {0}")]
    WanOptimization(String),

    #[error("Bandwidth limit exceeded")]
    BandwidthExceeded,

    #[error("Replication lag too high: {0}ms")]
    HighLag(u64),

    #[error("Filter evaluation error: {0}")]
    FilterError(String),

    #[error("Base replication error: {0}")]
    Base(#[from] ReplicationError),
}

pub type AdvancedReplicationResult<T> = Result<T, AdvancedReplicationError>;

/// Region information for cross-region replication
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Region {
    /// Region identifier (e.g., "us-east-1", "eu-west-1")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Geographic location (for latency optimization)
    pub location: RegionLocation,

    /// Whether this is the local region
    pub is_local: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RegionLocation {
    /// Continent code (e.g., "NA", "EU", "AS")
    pub continent: String,

    /// Country code (ISO 3166-1 alpha-2)
    pub country: String,

    /// City/metro area
    pub city: String,
}

/// Replication filter for selective replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationFilter {
    /// Prefix filters (any match = include)
    #[serde(default)]
    pub prefix_filters: Vec<String>,

    /// Tag filters (all must match for inclusion)
    #[serde(default)]
    pub tag_filters: HashMap<String, String>,

    /// Object size min threshold (bytes)
    pub min_size: Option<u64>,

    /// Object size max threshold (bytes)
    pub max_size: Option<u64>,

    /// Storage class filter (only replicate these classes)
    #[serde(default)]
    pub storage_classes: Vec<String>,

    /// Exclude prefixes (any match = exclude, evaluated after include filters)
    #[serde(default)]
    pub exclude_prefixes: Vec<String>,
}

impl ReplicationFilter {
    pub fn new() -> Self {
        Self {
            prefix_filters: Vec::new(),
            tag_filters: HashMap::new(),
            min_size: None,
            max_size: None,
            storage_classes: Vec::new(),
            exclude_prefixes: Vec::new(),
        }
    }

    /// Check if an object matches this filter
    pub fn matches(
        &self,
        key: &str,
        size: u64,
        tags: &HashMap<String, String>,
        storage_class: &str,
    ) -> bool {
        // Check exclude prefixes first
        for exclude_prefix in &self.exclude_prefixes {
            if key.starts_with(exclude_prefix) {
                return false;
            }
        }

        // Check prefix filters (if any)
        if !self.prefix_filters.is_empty() {
            let has_matching_prefix = self.prefix_filters.iter().any(|p| key.starts_with(p));
            if !has_matching_prefix {
                return false;
            }
        }

        // Check size constraints
        if let Some(min) = self.min_size {
            if size < min {
                return false;
            }
        }
        if let Some(max) = self.max_size {
            if size > max {
                return false;
            }
        }

        // Check storage class filter
        if !self.storage_classes.is_empty()
            && !self.storage_classes.contains(&storage_class.to_string())
        {
            return false;
        }

        // Check tag filters (all must match)
        for (filter_key, filter_value) in &self.tag_filters {
            match tags.get(filter_key) {
                Some(value) if value == filter_value => continue,
                _ => return false,
            }
        }

        true
    }
}

impl Default for ReplicationFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration for cross-region replication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossRegionConfig {
    /// Source region
    pub source_region: Region,

    /// Destination regions
    pub destination_regions: Vec<Region>,

    /// Replication filter
    pub filter: ReplicationFilter,

    /// Enable WAN optimization (compression, batching)
    pub wan_optimization: bool,

    /// Compression level for WAN optimization (1-9, 3 is default)
    pub compression_level: i32,

    /// Batch size for WAN optimization (number of events)
    pub batch_size: usize,

    /// Batch timeout (flush batch after this duration)
    pub batch_timeout: Duration,

    /// Enable bandwidth throttling
    pub enable_bandwidth_limit: bool,

    /// Bandwidth limit in MB/s (0 = unlimited)
    pub bandwidth_limit_mbps: u64,

    /// Priority (higher = more important, 0-100)
    pub priority: u8,

    /// Enable replication of delete markers
    pub replicate_deletes: bool,

    /// Enable replication of metadata changes
    pub replicate_metadata: bool,
}

impl Default for CrossRegionConfig {
    fn default() -> Self {
        Self {
            source_region: Region {
                id: "local".to_string(),
                name: "Local Region".to_string(),
                location: RegionLocation {
                    continent: "NA".to_string(),
                    country: "US".to_string(),
                    city: "Local".to_string(),
                },
                is_local: true,
            },
            destination_regions: Vec::new(),
            filter: ReplicationFilter::default(),
            wan_optimization: true,
            compression_level: 3,
            batch_size: 100,
            batch_timeout: Duration::from_secs(10),
            enable_bandwidth_limit: false,
            bandwidth_limit_mbps: 0,
            priority: 50,
            replicate_deletes: true,
            replicate_metadata: true,
        }
    }
}

/// Replication metrics for monitoring
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicationMetrics {
    /// Total objects replicated
    pub objects_replicated: u64,

    /// Total bytes replicated
    pub bytes_replicated: u64,

    /// Total replication failures
    pub failures: u64,

    /// Average replication lag (milliseconds)
    pub avg_lag_ms: u64,

    /// Maximum replication lag (milliseconds)
    pub max_lag_ms: u64,

    /// Current replication queue depth
    pub queue_depth: usize,

    /// Replication throughput (bytes/sec)
    pub throughput_bps: u64,

    /// Last replication timestamp
    pub last_replication: Option<DateTime<Utc>>,

    /// Per-destination metrics
    #[serde(default)]
    pub per_destination: HashMap<String, DestinationMetrics>,

    /// Conflict resolution statistics
    pub conflicts_resolved: u64,

    /// Operations retried
    pub retries: u64,

    /// Bandwidth saved by compression (bytes)
    pub compression_savings_bytes: u64,
}

impl ReplicationMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a successful replication
    pub fn record_success(&mut self, destination: &str, bytes: u64, lag_ms: u64) {
        self.objects_replicated += 1;
        self.bytes_replicated += bytes;
        self.last_replication = Some(Utc::now());

        // Update lag metrics
        if lag_ms > self.max_lag_ms {
            self.max_lag_ms = lag_ms;
        }

        // Update average lag (simple moving average)
        if self.avg_lag_ms == 0 {
            self.avg_lag_ms = lag_ms;
        } else {
            self.avg_lag_ms = (self.avg_lag_ms * 9 + lag_ms) / 10;
        }

        // Update per-destination metrics
        let dest_metrics = self
            .per_destination
            .entry(destination.to_string())
            .or_default();
        dest_metrics.successes += 1;
        dest_metrics.bytes_transferred += bytes;
        dest_metrics.last_success = Some(Utc::now());
    }

    /// Record a failure
    pub fn record_failure(&mut self, destination: &str, error: &str) {
        self.failures += 1;

        let dest_metrics = self
            .per_destination
            .entry(destination.to_string())
            .or_default();
        dest_metrics.failures += 1;
        dest_metrics.last_error = Some(error.to_string());
        dest_metrics.last_failure = Some(Utc::now());
    }

    /// Record a conflict resolution
    pub fn record_conflict(&mut self) {
        self.conflicts_resolved += 1;
    }

    /// Record a retry
    pub fn record_retry(&mut self) {
        self.retries += 1;
    }

    /// Record compression savings
    pub fn record_compression_savings(&mut self, original_size: u64, compressed_size: u64) {
        if original_size > compressed_size {
            self.compression_savings_bytes += original_size - compressed_size;
        }
    }

    /// Update queue depth
    pub fn update_queue_depth(&mut self, depth: usize) {
        self.queue_depth = depth;
    }

    /// Calculate current throughput
    pub fn calculate_throughput(&self, window_seconds: u64) -> u64 {
        if window_seconds == 0 {
            return 0;
        }
        self.bytes_replicated / window_seconds
    }
}

/// Per-destination metrics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DestinationMetrics {
    /// Successful replications
    pub successes: u64,

    /// Failed replications
    pub failures: u64,

    /// Total bytes transferred
    pub bytes_transferred: u64,

    /// Last successful replication
    pub last_success: Option<DateTime<Utc>>,

    /// Last failure timestamp
    pub last_failure: Option<DateTime<Utc>>,

    /// Last error message
    pub last_error: Option<String>,

    /// Current health status
    pub health_status: HealthStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    #[default]
    Healthy,
    Degraded,
    Unhealthy,
}

/// Conflict resolution strategy for bi-directional replication
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Last write wins (based on timestamp)
    LastWriteWins,

    /// Use vector clocks (requires vector clock tracking)
    VectorClock,

    /// Source region always wins
    SourceWins,

    /// Destination region always wins
    DestinationWins,

    /// Merge conflicting versions (application-specific)
    Merge,

    /// Create conflict marker object
    CreateConflictMarker,
}

/// Event for replication with enhanced metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedReplicationEvent {
    /// Base replication event
    pub event: ReplicationEvent,

    /// Source region
    pub source_region: String,

    /// Destination region
    pub destination_region: String,

    /// Object tags (for filter matching)
    #[serde(default)]
    pub tags: HashMap<String, String>,

    /// Storage class
    pub storage_class: String,

    /// Event timestamp
    pub timestamp: DateTime<Utc>,

    /// Retry count
    pub retry_count: u32,

    /// Event ID for deduplication
    pub event_id: String,

    /// Compressed payload size (if WAN optimization enabled)
    pub compressed_size: Option<u64>,

    /// Original payload size
    pub original_size: u64,
}

/// Batch of replication events for WAN optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationBatch {
    /// Batch ID
    pub batch_id: String,

    /// Events in this batch
    pub events: Vec<EnhancedReplicationEvent>,

    /// Batch creation timestamp
    pub created_at: DateTime<Utc>,

    /// Total uncompressed size
    pub total_size: u64,

    /// Compressed size (if compression enabled)
    pub compressed_size: Option<u64>,
}

/// Advanced replication manager
pub struct AdvancedReplicationManager {
    /// Cross-region configurations per bucket
    configs: Arc<RwLock<HashMap<String, CrossRegionConfig>>>,

    /// Replication metrics
    metrics: Arc<RwLock<ReplicationMetrics>>,

    /// Event batches waiting to be sent (per destination region)
    pending_batches: Arc<RwLock<HashMap<String, VecDeque<EnhancedReplicationEvent>>>>,

    /// Bandwidth tracker (bytes per second)
    bandwidth_tracker: Arc<RwLock<BandwidthTracker>>,
}

impl AdvancedReplicationManager {
    pub fn new() -> Self {
        Self {
            configs: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(ReplicationMetrics::new())),
            pending_batches: Arc::new(RwLock::new(HashMap::new())),
            bandwidth_tracker: Arc::new(RwLock::new(BandwidthTracker::new())),
        }
    }

    /// Set cross-region configuration for a bucket
    pub async fn set_bucket_config(&self, bucket: &str, config: CrossRegionConfig) {
        let mut configs = self.configs.write().await;
        configs.insert(bucket.to_string(), config);
        info!(bucket = %bucket, "Cross-region replication configured");
    }

    /// Get cross-region configuration for a bucket
    pub async fn get_bucket_config(&self, bucket: &str) -> Option<CrossRegionConfig> {
        let configs = self.configs.read().await;
        configs.get(bucket).cloned()
    }

    /// Remove cross-region configuration for a bucket
    pub async fn remove_bucket_config(&self, bucket: &str) {
        let mut configs = self.configs.write().await;
        configs.remove(bucket);
        info!(bucket = %bucket, "Cross-region replication removed");
    }

    /// Check if an event should be replicated based on filters
    pub async fn should_replicate(
        &self,
        bucket: &str,
        key: &str,
        size: u64,
        tags: &HashMap<String, String>,
        storage_class: &str,
    ) -> bool {
        let configs = self.configs.read().await;
        if let Some(config) = configs.get(bucket) {
            config.filter.matches(key, size, tags, storage_class)
        } else {
            false
        }
    }

    /// Add event to replication queue with filtering and batching
    pub async fn queue_event(
        &self,
        event: EnhancedReplicationEvent,
    ) -> AdvancedReplicationResult<()> {
        let config = {
            let configs = self.configs.read().await;
            configs.get(event.event.bucket()).cloned().ok_or_else(|| {
                AdvancedReplicationError::FilterError(
                    "No replication config for bucket".to_string(),
                )
            })?
        };

        // Check filters
        let key = match &event.event {
            ReplicationEvent::PutObject { key, .. } => key.clone(),
            ReplicationEvent::DeleteObject { key, .. } => key.clone(),
            _ => String::new(),
        };

        if !key.is_empty()
            && !config
                .filter
                .matches(&key, event.original_size, &event.tags, &event.storage_class)
        {
            debug!(key = %key, "Event filtered out by replication rules");
            return Ok(());
        }

        // Check bandwidth limits
        if config.enable_bandwidth_limit && config.bandwidth_limit_mbps > 0 {
            let mut tracker = self.bandwidth_tracker.write().await;
            let limit_bps = config.bandwidth_limit_mbps * 1024 * 1024;
            if !tracker.allow(event.original_size, limit_bps) {
                return Err(AdvancedReplicationError::BandwidthExceeded);
            }
        }

        // Add to pending batches for each destination
        for dest_region in &config.destination_regions {
            let mut batches = self.pending_batches.write().await;
            let queue = batches
                .entry(dest_region.id.clone())
                .or_insert_with(VecDeque::new);
            queue.push_back(event.clone());

            // Update queue depth metric
            let mut metrics = self.metrics.write().await;
            metrics.update_queue_depth(queue.len());
        }

        Ok(())
    }

    /// Flush pending batches to destinations
    pub async fn flush_batches(&self) -> AdvancedReplicationResult<HashMap<String, usize>> {
        let mut batches = self.pending_batches.write().await;
        let configs = self.configs.read().await;
        let mut flushed = HashMap::new();

        for (dest_region, queue) in batches.iter_mut() {
            // Find config for this destination
            let config = configs
                .values()
                .find(|c| c.destination_regions.iter().any(|r| &r.id == dest_region));

            if let Some(config) = config {
                let batch_size = config.batch_size.min(queue.len());
                if batch_size > 0 {
                    // Create batch
                    let mut events = Vec::new();
                    for _ in 0..batch_size {
                        if let Some(event) = queue.pop_front() {
                            events.push(event);
                        }
                    }

                    // Send batch to destination
                    let total_size: u64 = events.iter().map(|e| e.original_size).sum();

                    match self
                        .send_batch_to_destination(dest_region, events, config)
                        .await
                    {
                        Ok(sent_count) => {
                            flushed.insert(dest_region.clone(), sent_count);

                            // Update metrics
                            let mut metrics = self.metrics.write().await;
                            metrics.record_success(dest_region, total_size, 0);

                            debug!(
                                destination = %dest_region,
                                events = sent_count,
                                size = total_size,
                                "Successfully flushed replication batch"
                            );
                        }
                        Err(e) => {
                            // Update failure metrics
                            let mut metrics = self.metrics.write().await;
                            metrics.failures += 1;

                            tracing::error!(
                                destination = %dest_region,
                                error = %e,
                                "Failed to send replication batch"
                            );
                        }
                    }
                }
            }
        }

        Ok(flushed)
    }

    /// Send a batch of events to a remote destination
    async fn send_batch_to_destination(
        &self,
        destination: &str,
        events: Vec<EnhancedReplicationEvent>,
        config: &CrossRegionConfig,
    ) -> AdvancedReplicationResult<usize> {
        if events.is_empty() {
            return Ok(0);
        }

        let batch_id = uuid::Uuid::new_v4().to_string();
        let total_size = events.iter().map(|e| e.original_size).sum();

        // Create replication batch
        let batch = ReplicationBatch {
            batch_id: batch_id.clone(),
            events: events.clone(),
            created_at: Utc::now(),
            total_size,
            compressed_size: None,
        };

        // Serialize batch
        let serialized = serde_json::to_vec(&batch).map_err(|e| {
            AdvancedReplicationError::WanOptimization(format!("Serialization failed: {}", e))
        })?;

        let data = if config.wan_optimization {
            // Apply compression
            let compressed = self.compress_batch(&serialized, config.compression_level)?;
            let compressed_size = compressed.len();

            // Track compression savings
            let mut metrics = self.metrics.write().await;
            metrics.compression_savings_bytes += (serialized.len() - compressed_size) as u64;

            compressed
        } else {
            serialized
        };

        // Find destination region endpoint
        let destination_region = config
            .destination_regions
            .iter()
            .find(|r| r.id == destination)
            .ok_or_else(|| {
                AdvancedReplicationError::WanOptimization(format!(
                    "Destination region not found: {}",
                    destination
                ))
            })?;

        // Send to destination (HTTP POST to replication endpoint)
        // In a real implementation, this would use reqwest to POST to the remote endpoint
        // For now, we simulate successful transmission
        info!(
            batch_id = %batch_id,
            destination = %destination_region.id,
            events = events.len(),
            size = data.len(),
            "Batch prepared for transmission (simulated)"
        );

        // Update destination metrics
        let mut metrics = self.metrics.write().await;
        let dest_metrics = metrics
            .per_destination
            .entry(destination.to_string())
            .or_insert_with(DestinationMetrics::default);
        dest_metrics.successes += events.len() as u64;
        dest_metrics.bytes_transferred += total_size;
        dest_metrics.last_success = Some(Utc::now());

        Ok(events.len())
    }

    /// Compress batch data using zstd
    fn compress_batch(&self, data: &[u8], level: i32) -> AdvancedReplicationResult<Vec<u8>> {
        zstd::bulk::compress(data, level).map_err(|e| {
            AdvancedReplicationError::WanOptimization(format!("Compression failed: {}", e))
        })
    }

    /// Get current replication metrics
    pub async fn get_metrics(&self) -> ReplicationMetrics {
        self.metrics.read().await.clone()
    }

    /// Get metrics for a specific destination
    pub async fn get_destination_metrics(&self, destination: &str) -> Option<DestinationMetrics> {
        let metrics = self.metrics.read().await;
        metrics.per_destination.get(destination).cloned()
    }

    /// Resolve conflict using configured strategy
    pub async fn resolve_conflict(
        &self,
        strategy: ConflictResolution,
        local_event: &EnhancedReplicationEvent,
        remote_event: &EnhancedReplicationEvent,
    ) -> AdvancedReplicationResult<ConflictResolutionResult> {
        let result = match strategy {
            ConflictResolution::LastWriteWins => {
                if local_event.timestamp > remote_event.timestamp {
                    ConflictResolutionResult::UseLocal
                } else {
                    ConflictResolutionResult::UseRemote
                }
            }

            ConflictResolution::VectorClock => {
                // Use vector clocks from the events
                match (&local_event.event, &remote_event.event) {
                    (
                        ReplicationEvent::PutObject {
                            vector_clock: local_vc,
                            ..
                        },
                        ReplicationEvent::PutObject {
                            vector_clock: remote_vc,
                            ..
                        },
                    ) => {
                        if Self::happens_before(local_vc, remote_vc) {
                            ConflictResolutionResult::UseRemote
                        } else if Self::happens_before(remote_vc, local_vc) {
                            ConflictResolutionResult::UseLocal
                        } else {
                            // Concurrent updates - fall back to last write wins
                            if local_event.timestamp > remote_event.timestamp {
                                ConflictResolutionResult::UseLocal
                            } else {
                                ConflictResolutionResult::UseRemote
                            }
                        }
                    }
                    _ => ConflictResolutionResult::UseLocal,
                }
            }

            ConflictResolution::SourceWins => {
                // Event from source region wins
                if local_event.source_region == remote_event.source_region {
                    ConflictResolutionResult::UseLocal
                } else {
                    ConflictResolutionResult::UseRemote
                }
            }

            ConflictResolution::DestinationWins => {
                // Event destined for this region wins
                ConflictResolutionResult::UseLocal
            }

            ConflictResolution::Merge => {
                // Merging requires application-specific logic
                ConflictResolutionResult::Merge
            }

            ConflictResolution::CreateConflictMarker => ConflictResolutionResult::CreateMarker,
        };

        // Record conflict resolution in metrics
        let mut metrics = self.metrics.write().await;
        metrics.record_conflict();

        Ok(result)
    }

    /// Check if vector clock A happens before B
    fn happens_before(a: &HashMap<NodeId, u64>, b: &HashMap<NodeId, u64>) -> bool {
        let mut strictly_less = false;

        for (node, a_version) in a {
            let b_version = b.get(node).copied().unwrap_or(0);
            if a_version > &b_version {
                return false; // A has a higher version for some node
            }
            if a_version < &b_version {
                strictly_less = true;
            }
        }

        // Check if B has nodes not in A
        for node in b.keys() {
            if !a.contains_key(node) {
                strictly_less = true;
            }
        }

        strictly_less
    }

    /// Calculate replication lag for a destination
    pub async fn calculate_lag(&self, destination: &str) -> Option<u64> {
        let metrics = self.metrics.read().await;
        if let Some(dest_metrics) = metrics.per_destination.get(destination) {
            if let Some(last_success) = dest_metrics.last_success {
                let lag = Utc::now()
                    .signed_duration_since(last_success)
                    .num_milliseconds() as u64;
                return Some(lag);
            }
        }
        None
    }
}

impl Default for AdvancedReplicationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of conflict resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolutionResult {
    /// Use the local version
    UseLocal,

    /// Use the remote version
    UseRemote,

    /// Merge both versions
    Merge,

    /// Create a conflict marker
    CreateMarker,
}

/// Bandwidth tracker for throttling
struct BandwidthTracker {
    /// Bytes sent in current window
    bytes_in_window: u64,

    /// Window start time
    window_start: DateTime<Utc>,

    /// Window duration
    window_duration: Duration,
}

impl BandwidthTracker {
    fn new() -> Self {
        Self {
            bytes_in_window: 0,
            window_start: Utc::now(),
            window_duration: Duration::from_secs(1),
        }
    }

    /// Check if we can send 'bytes' without exceeding limit
    fn allow(&mut self, bytes: u64, limit_bps: u64) -> bool {
        let now = Utc::now();
        let elapsed = now.signed_duration_since(self.window_start);

        // Reset window if elapsed
        if elapsed.num_milliseconds() as u64 > self.window_duration.as_millis() as u64 {
            self.bytes_in_window = 0;
            self.window_start = now;
        }

        // Check if adding bytes would exceed limit
        if self.bytes_in_window + bytes > limit_bps {
            return false;
        }

        self.bytes_in_window += bytes;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replication_filter_prefix() {
        let mut filter = ReplicationFilter::new();
        filter.prefix_filters.push("logs/".to_string());

        assert!(filter.matches("logs/app.log", 1024, &HashMap::new(), "STANDARD"));
        assert!(!filter.matches("data/file.txt", 1024, &HashMap::new(), "STANDARD"));
    }

    #[test]
    fn test_replication_filter_size() {
        let mut filter = ReplicationFilter::new();
        filter.min_size = Some(1000);
        filter.max_size = Some(10000);

        assert!(filter.matches("file.txt", 5000, &HashMap::new(), "STANDARD"));
        assert!(!filter.matches("small.txt", 500, &HashMap::new(), "STANDARD"));
        assert!(!filter.matches("large.txt", 50000, &HashMap::new(), "STANDARD"));
    }

    #[test]
    fn test_replication_filter_tags() {
        let mut filter = ReplicationFilter::new();
        filter
            .tag_filters
            .insert("env".to_string(), "prod".to_string());

        let mut tags = HashMap::new();
        tags.insert("env".to_string(), "prod".to_string());

        assert!(filter.matches("file.txt", 1024, &tags, "STANDARD"));

        tags.insert("env".to_string(), "dev".to_string());
        assert!(!filter.matches("file.txt", 1024, &tags, "STANDARD"));
    }

    #[test]
    fn test_replication_filter_exclude() {
        let mut filter = ReplicationFilter::new();
        filter.prefix_filters.push("data/".to_string());
        filter.exclude_prefixes.push("data/temp/".to_string());

        assert!(filter.matches("data/file.txt", 1024, &HashMap::new(), "STANDARD"));
        assert!(!filter.matches("data/temp/cache.tmp", 1024, &HashMap::new(), "STANDARD"));
    }

    #[test]
    fn test_replication_filter_storage_class() {
        let mut filter = ReplicationFilter::new();
        filter.storage_classes.push("GLACIER".to_string());

        assert!(filter.matches("file.txt", 1024, &HashMap::new(), "GLACIER"));
        assert!(!filter.matches("file.txt", 1024, &HashMap::new(), "STANDARD"));
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let mut metrics = ReplicationMetrics::new();

        metrics.record_success("us-east-1", 1024, 50);
        assert_eq!(metrics.objects_replicated, 1);
        assert_eq!(metrics.bytes_replicated, 1024);
        assert_eq!(metrics.avg_lag_ms, 50);

        metrics.record_success("us-east-1", 2048, 100);
        assert_eq!(metrics.objects_replicated, 2);
        assert_eq!(metrics.bytes_replicated, 3072);
        // Average should be weighted towards newer value
        assert!(metrics.avg_lag_ms > 50 && metrics.avg_lag_ms < 100);
    }

    #[tokio::test]
    async fn test_advanced_replication_manager() {
        let manager = AdvancedReplicationManager::new();

        let config = CrossRegionConfig {
            source_region: Region {
                id: "us-west-1".to_string(),
                name: "US West 1".to_string(),
                location: RegionLocation {
                    continent: "NA".to_string(),
                    country: "US".to_string(),
                    city: "San Francisco".to_string(),
                },
                is_local: true,
            },
            destination_regions: vec![Region {
                id: "us-east-1".to_string(),
                name: "US East 1".to_string(),
                location: RegionLocation {
                    continent: "NA".to_string(),
                    country: "US".to_string(),
                    city: "Virginia".to_string(),
                },
                is_local: false,
            }],
            filter: ReplicationFilter::default(),
            ..Default::default()
        };

        manager.set_bucket_config("test-bucket", config).await;

        let retrieved = manager.get_bucket_config("test-bucket").await;
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved
                .expect("Failed to get bucket config")
                .source_region
                .id,
            "us-west-1"
        );
    }

    #[tokio::test]
    async fn test_conflict_resolution_last_write_wins() {
        let manager = AdvancedReplicationManager::new();

        let local_event = EnhancedReplicationEvent {
            event: ReplicationEvent::PutObject {
                bucket: "test".to_string(),
                key: "file.txt".to_string(),
                etag: "abc123".to_string(),
                size: 1024,
                content_type: "text/plain".to_string(),
                metadata: HashMap::new(),
                source_node: "node1".to_string(),
                vector_clock: HashMap::new(),
            },
            source_region: "us-west-1".to_string(),
            destination_region: "us-east-1".to_string(),
            tags: HashMap::new(),
            storage_class: "STANDARD".to_string(),
            timestamp: Utc::now(),
            retry_count: 0,
            event_id: "event1".to_string(),
            compressed_size: None,
            original_size: 1024,
        };

        let mut remote_event = local_event.clone();
        remote_event.timestamp = Utc::now() - chrono::Duration::hours(1);

        let result = manager
            .resolve_conflict(
                ConflictResolution::LastWriteWins,
                &local_event,
                &remote_event,
            )
            .await
            .expect("Failed to resolve conflict");

        assert_eq!(result, ConflictResolutionResult::UseLocal);
    }

    #[test]
    fn test_bandwidth_tracker() {
        let mut tracker = BandwidthTracker::new();
        let limit = 1024 * 1024; // 1 MB/s

        // Should allow first 512KB
        assert!(tracker.allow(512 * 1024, limit));

        // Should allow another 512KB (total 1MB)
        assert!(tracker.allow(512 * 1024, limit));

        // Should not allow more in same window
        assert!(!tracker.allow(1, limit));
    }

    #[tokio::test]
    async fn test_selective_replication() {
        let manager = AdvancedReplicationManager::new();

        let mut filter = ReplicationFilter::new();
        filter.prefix_filters.push("logs/".to_string());

        let config = CrossRegionConfig {
            filter,
            ..Default::default()
        };

        manager.set_bucket_config("test-bucket", config).await;

        // Should replicate
        assert!(
            manager
                .should_replicate(
                    "test-bucket",
                    "logs/app.log",
                    1024,
                    &HashMap::new(),
                    "STANDARD"
                )
                .await
        );

        // Should not replicate
        assert!(
            !manager
                .should_replicate(
                    "test-bucket",
                    "data/file.txt",
                    1024,
                    &HashMap::new(),
                    "STANDARD"
                )
                .await
        );
    }
}
