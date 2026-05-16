//! Type definitions for rs3ctl CLI - command enums and data structures

use clap::Subcommand;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Command action enums ──────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum BucketAction {
    /// List all buckets
    List,
    /// Create a new bucket
    Create {
        /// Bucket name
        name: String,
        /// Bucket region
        #[arg(short, long, default_value = "us-east-1")]
        region: String,
    },
    /// Delete a bucket
    Delete {
        /// Bucket name
        name: String,
        /// Force delete even if bucket is not empty
        #[arg(short, long)]
        force: bool,
    },
    /// Get bucket information
    Info {
        /// Bucket name
        name: String,
    },
    /// Get bucket policy
    GetPolicy {
        /// Bucket name
        name: String,
    },
    /// Set bucket policy from file
    SetPolicy {
        /// Bucket name
        name: String,
        /// Policy JSON file
        policy_file: PathBuf,
    },
}

#[derive(Subcommand)]
pub enum ObjectAction {
    /// List objects in a bucket
    List {
        /// Bucket name
        bucket: String,
        /// Prefix filter
        #[arg(short, long)]
        prefix: Option<String>,
        /// Maximum keys to return
        #[arg(short, long, default_value = "1000")]
        max_keys: i32,
    },
    /// Upload a file as an object
    Upload {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Local file path
        file: PathBuf,
        /// Content type
        #[arg(short = 't', long)]
        content_type: Option<String>,
    },
    /// Download an object to a file
    Download {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Local file path to save
        output: PathBuf,
    },
    /// Delete an object
    Delete {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
    },
    /// Get object information
    Info {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
    },
}

#[derive(Subcommand)]
pub enum ReplicationAction {
    /// Get replication configuration for a bucket
    GetConfig {
        /// Bucket name
        bucket: String,
    },
    /// Set replication configuration from file
    SetConfig {
        /// Bucket name
        bucket: String,
        /// Configuration JSON file
        config_file: PathBuf,
    },
    /// Get replication metrics
    Metrics {
        /// Destination name (optional)
        #[arg(short, long)]
        destination: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum MetricsAction {
    /// Get Prometheus metrics
    Get,
    /// Get storage statistics
    Storage,
    /// Get operation statistics
    Operations,
}

#[derive(Subcommand)]
pub enum MaintenanceAction {
    /// Create a backup snapshot
    Backup {
        /// Snapshot name
        #[arg(short, long)]
        name: Option<String>,
        /// Snapshot type (full, incremental)
        #[arg(short = 't', long, default_value = "full")]
        snapshot_type: String,
    },
    /// Trigger integrity check
    IntegrityCheck {
        /// Bucket name (optional, checks all if not specified)
        #[arg(short, long)]
        bucket: Option<String>,
    },
    /// Trigger cleanup of old objects
    Cleanup {
        /// Retention period in days
        #[arg(short, long, default_value = "30")]
        retention_days: u32,
    },
}

#[derive(Subcommand)]
pub enum VersioningAction {
    /// Get versioning status for a bucket
    GetStatus {
        /// Bucket name
        bucket: String,
    },
    /// Enable versioning for a bucket
    Enable {
        /// Bucket name
        bucket: String,
    },
    /// Suspend versioning for a bucket
    Suspend {
        /// Bucket name
        bucket: String,
    },
    /// List object versions
    ListVersions {
        /// Bucket name
        bucket: String,
        /// Object key prefix filter
        #[arg(short, long)]
        prefix: Option<String>,
        /// Maximum number of versions to return
        #[arg(short, long, default_value = "1000")]
        max_keys: i32,
    },
}

#[derive(Subcommand)]
pub enum ObservabilityAction {
    /// Get profiling data (CPU, memory, I/O)
    Profiling {
        /// Export in pprof format
        #[arg(short, long)]
        pprof: bool,
    },
    /// Get business metrics and KPIs
    BusinessMetrics,
    /// Get detected anomalies
    Anomalies {
        /// Filter by anomaly type
        #[arg(short = 't', long)]
        anomaly_type: Option<String>,
        /// Filter by severity (Low, Medium, High, Critical)
        #[arg(short, long)]
        severity: Option<String>,
        /// Limit number of results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Get resource manager statistics
    Resources,
    /// Get comprehensive health check
    Health,
}

#[derive(Subcommand)]
pub enum TransformAction {
    /// Apply image transformation to an object
    Image {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Target width
        #[arg(short = 'w', long)]
        width: Option<u32>,
        /// Target height
        #[arg(short = 'H', long)]
        height: Option<u32>,
        /// Output format (jpeg, png, webp, gif, bmp, tiff)
        #[arg(short = 'f', long)]
        format: Option<String>,
        /// Quality (1-100 for JPEG)
        #[arg(short = 'q', long)]
        quality: Option<u8>,
        /// Output file path
        #[arg(short = 'o', long)]
        output: PathBuf,
    },
    /// Apply compression transformation
    Compress {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Algorithm (zstd, gzip, lz4)
        #[arg(short = 'a', long, default_value = "zstd")]
        algorithm: String,
        /// Compression level
        #[arg(short = 'l', long)]
        level: Option<u8>,
        /// Output file path
        #[arg(short = 'o', long)]
        output: PathBuf,
    },
    /// Test a WASM plugin
    TestWasm {
        /// Path to WASM plugin file
        plugin: PathBuf,
        /// Test input file
        input: PathBuf,
        /// Parameters (plugin-specific)
        #[arg(short = 'p', long)]
        params: Option<String>,
        /// Output file path
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum BatchAction {
    /// Delete multiple objects by prefix
    DeleteByPrefix {
        /// Bucket name
        bucket: String,
        /// Prefix to match objects for deletion
        prefix: String,
        /// Dry run (show what would be deleted without deleting)
        #[arg(long)]
        dry_run: bool,
        /// Maximum number of objects to delete
        #[arg(short = 'm', long)]
        max: Option<usize>,
    },
    /// Copy multiple objects
    CopyMultiple {
        /// Source bucket
        source_bucket: String,
        /// Destination bucket
        dest_bucket: String,
        /// Source prefix
        #[arg(short = 's', long)]
        source_prefix: String,
        /// Destination prefix
        #[arg(short = 'd', long)]
        dest_prefix: Option<String>,
        /// Maximum number of objects to copy
        #[arg(short = 'm', long)]
        max: Option<usize>,
    },
    /// Tag multiple objects
    TagMultiple {
        /// Bucket name
        bucket: String,
        /// Prefix filter
        prefix: String,
        /// Tags in format key1=value1,key2=value2
        tags: String,
        /// Maximum number of objects to tag
        #[arg(short = 'm', long)]
        max: Option<usize>,
    },
}

#[derive(Subcommand)]
pub enum LifecycleAction {
    /// Get lifecycle policy for a bucket
    Get {
        /// Bucket name
        bucket: String,
    },
    /// Set lifecycle policy from file
    Set {
        /// Bucket name
        bucket: String,
        /// Policy JSON file
        policy_file: PathBuf,
    },
    /// Delete lifecycle policy
    Delete {
        /// Bucket name
        bucket: String,
    },
    /// List rules in lifecycle policy
    ListRules {
        /// Bucket name
        bucket: String,
    },
}

#[derive(Subcommand)]
pub enum PreprocessingAction {
    /// Create a preprocessing pipeline
    Create {
        /// Pipeline ID
        #[arg(short, long)]
        id: String,
        /// Pipeline name
        #[arg(short, long)]
        name: String,
        /// Pipeline definition JSON file
        #[arg(short, long)]
        file: PathBuf,
    },
    /// List all preprocessing pipelines
    List {
        /// Bucket containing pipelines (default: ml-datasets)
        #[arg(short, long, default_value = "ml-datasets")]
        bucket: String,
    },
    /// Get pipeline definition
    Get {
        /// Pipeline ID
        #[arg(short, long)]
        id: String,
        /// Bucket containing pipelines (default: ml-datasets)
        #[arg(short, long, default_value = "ml-datasets")]
        bucket: String,
    },
    /// Delete a preprocessing pipeline
    Delete {
        /// Pipeline ID
        #[arg(short, long)]
        id: String,
        /// Bucket containing pipelines (default: ml-datasets)
        #[arg(short, long, default_value = "ml-datasets")]
        bucket: String,
    },
    /// Apply pipeline to an object
    Apply {
        /// Pipeline ID
        #[arg(short, long)]
        pipeline: String,
        /// Source bucket
        #[arg(long)]
        source_bucket: String,
        /// Source object key
        #[arg(long)]
        source_key: String,
        /// Destination bucket
        #[arg(long)]
        dest_bucket: String,
        /// Destination object key
        #[arg(long)]
        dest_key: String,
    },
    /// Validate pipeline definition
    Validate {
        /// Pipeline definition JSON file
        file: PathBuf,
    },
}

/// Arguments for the `gc-multipart` command
#[derive(Debug, Clone)]
pub struct GcMultipartArgs {
    /// Restrict GC to this bucket (optional; all buckets if absent)
    pub bucket: Option<String>,
    /// Retention window in hours (default: 168 = 7 days)
    pub retention_hours: u64,
}

/// New: Config management actions
#[derive(Subcommand)]
pub enum ConfigAction {
    /// Display current server configuration
    Show,
    /// Validate a config file
    Validate {
        /// Path to config file (TOML or JSON) to validate
        #[arg(long)]
        config_file: Option<String>,
    },
}

/// New: Server info actions
#[derive(Subcommand)]
pub enum ServerInfoAction {
    /// Comprehensive server status (health + storage + metrics)
    Status,
}

// ── Data structs ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct BucketInfo {
    pub name: String,
    pub creation_date: Option<String>,
    pub region: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub last_modified: String,
    pub etag: String,
    pub storage_class: Option<String>,
}

/// Latency statistics for benchmark results
#[derive(Debug)]
pub struct LatencyStats {
    pub min_ms: f64,
    pub max_ms: f64,
    pub avg_ms: f64,
    pub p99_ms: f64,
    pub samples: usize,
}

impl LatencyStats {
    /// Compute statistics from a sorted-in-place sample vector
    pub fn compute(mut samples: Vec<f64>) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let count = samples.len();
        let min_ms = samples[0];
        let max_ms = samples[count - 1];
        let avg_ms = samples.iter().sum::<f64>() / count as f64;
        let p99_idx = ((count as f64 * 0.99) as usize).min(count - 1);
        let p99_ms = samples[p99_idx];
        Some(Self {
            min_ms,
            max_ms,
            avg_ms,
            p99_ms,
            samples: count,
        })
    }
}
