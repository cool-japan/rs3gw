//! rs3gw - High-Performance AI/HPC Object Storage Gateway
//!
//! A lightweight, zero-GC S3-compatible gateway powered by scirs2-io.

pub mod api;
pub mod auth;
pub mod cluster;
pub mod grpc;
pub mod metrics;
pub mod observability;
pub mod storage;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::api::{EventBroadcaster, ThrottleConfig, ThrottleManager};
use crate::cluster::{
    AdvancedReplicationManager, ClusterConfig, ReplicationConfig, ReplicationMode,
};
use crate::storage::{
    CacheConfig, CacheManager, ChunkingAlgorithm, CompressionMode, DedupConfig, QuotaConfig,
    QuotaManager, StorageEngine, ZeroCopyConfig,
};
use metrics_exporter_prometheus::PrometheusHandle;

/// TLS configuration
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct TlsConfig {
    /// Path to TLS certificate file (PEM format)
    pub cert_path: Option<PathBuf>,
    /// Path to TLS private key file (PEM format)
    pub key_path: Option<PathBuf>,
}

impl TlsConfig {
    /// Check if TLS is enabled
    pub fn is_enabled(&self) -> bool {
        self.cert_path.is_some() && self.key_path.is_some()
    }
}

/// HTTP client connection pool configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionPoolConfig {
    /// Maximum idle connections per host (default: 32)
    pub pool_max_idle_per_host: usize,
    /// Idle connection timeout in seconds (default: 90)
    pub pool_idle_timeout_secs: u64,
    /// Connection timeout in seconds (default: 30)
    pub connect_timeout_secs: u64,
    /// Request timeout in seconds (default: 300)
    pub request_timeout_secs: u64,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            pool_max_idle_per_host: 32,
            pool_idle_timeout_secs: 90,
            connect_timeout_secs: 30,
            request_timeout_secs: 300,
        }
    }
}

impl ConnectionPoolConfig {
    /// Load from environment variables
    pub fn from_env() -> Self {
        Self {
            pool_max_idle_per_host: std::env::var("RS3GW_POOL_MAX_IDLE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(32),
            pool_idle_timeout_secs: std::env::var("RS3GW_POOL_IDLE_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(90),
            connect_timeout_secs: std::env::var("RS3GW_CONNECT_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(30),
            request_timeout_secs: std::env::var("RS3GW_CLIENT_TIMEOUT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
        }
    }

    /// Build a configured reqwest client
    pub fn build_client(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .pool_max_idle_per_host(self.pool_max_idle_per_host)
            .pool_idle_timeout(std::time::Duration::from_secs(self.pool_idle_timeout_secs))
            .connect_timeout(std::time::Duration::from_secs(self.connect_timeout_secs))
            .timeout(std::time::Duration::from_secs(self.request_timeout_secs))
            .build()
            .unwrap_or_default()
    }
}

/// Server configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Config {
    /// Address to bind the server
    #[serde(default = "default_bind_addr")]
    pub bind_addr: SocketAddr,
    /// Root directory for storage
    #[serde(default = "default_storage_root")]
    pub storage_root: PathBuf,
    /// Default bucket name (for MVP single-bucket mode)
    #[serde(default = "default_bucket_name")]
    pub default_bucket: String,
    /// Access key (empty for no auth in MVP)
    #[serde(default)]
    pub access_key: String,
    /// Secret key (empty for no auth in MVP)
    #[serde(default)]
    pub secret_key: String,
    /// Compression mode for stored objects
    #[serde(default)]
    pub compression: CompressionMode,
    /// Request timeout in seconds (0 = no timeout)
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,
    /// Max concurrent requests (0 = no limit)
    #[serde(default)]
    pub max_concurrent_requests: usize,
    /// TLS configuration
    #[serde(default)]
    pub tls: TlsConfig,
    /// HTTP client connection pool configuration
    #[serde(default)]
    pub connection_pool: ConnectionPoolConfig,
    /// Cluster configuration
    #[serde(default)]
    pub cluster: ClusterConfig,
    /// Deduplication configuration
    #[serde(default)]
    pub dedup: DedupConfig,
    /// Zero-copy optimizations configuration
    #[serde(default)]
    pub zerocopy: ZeroCopyConfig,
    /// S3 Select cache configuration
    #[serde(default)]
    pub select_cache: SelectCacheConfig,
    /// Hours before abandoned multipart uploads are garbage collected (default: 168 = 7 days)
    #[serde(default = "default_multipart_retention_hours")]
    pub multipart_retention_hours: u64,
    /// When true, call sync_all() on object files before rename. Safer but slower.
    /// Set via RS3GW_FSYNC=true env var. Default: false.
    #[serde(default)]
    pub fsync: bool,
}

/// S3 Select query result cache configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectCacheConfig {
    /// Enable S3 Select result caching
    #[serde(default = "default_select_cache_enabled")]
    pub enabled: bool,
    /// Maximum number of cached query results
    #[serde(default = "default_select_cache_max_entries")]
    pub max_entries: usize,
    /// Maximum memory usage for cache in bytes
    #[serde(default = "default_select_cache_max_memory_mb")]
    pub max_memory_mb: usize,
    /// Default TTL for cached results in seconds
    #[serde(default = "default_select_cache_ttl")]
    pub ttl_seconds: u64,
}

fn default_select_cache_enabled() -> bool {
    true
}

fn default_select_cache_max_entries() -> usize {
    1000
}

fn default_select_cache_max_memory_mb() -> usize {
    100
}

fn default_select_cache_ttl() -> u64 {
    3600 // 1 hour
}

impl Default for SelectCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 1000,
            max_memory_mb: 100,
            ttl_seconds: 3600,
        }
    }
}

fn default_bind_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 9000))
}

fn default_storage_root() -> PathBuf {
    PathBuf::from("./data")
}

fn default_bucket_name() -> String {
    "default".to_string()
}

fn default_multipart_retention_hours() -> u64 {
    168
}

fn default_request_timeout() -> u64 {
    300
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind_addr: SocketAddr::from(([0, 0, 0, 0], 9000)),
            storage_root: PathBuf::from("./data"),
            default_bucket: "default".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            compression: CompressionMode::None,
            request_timeout_secs: 300,  // 5 minutes default
            max_concurrent_requests: 0, // No limit by default
            tls: TlsConfig::default(),
            connection_pool: ConnectionPoolConfig::default(),
            cluster: ClusterConfig::default(),
            dedup: DedupConfig::default(),
            zerocopy: ZeroCopyConfig::default(),
            select_cache: SelectCacheConfig::default(),
            multipart_retention_hours: 168,
            fsync: false,
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("RS3GW_BIND_ADDR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 9000)));

        let storage_root = std::env::var("RS3GW_STORAGE_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data"));

        let default_bucket =
            std::env::var("RS3GW_DEFAULT_BUCKET").unwrap_or_else(|_| "default".to_string());

        let access_key = std::env::var("RS3GW_ACCESS_KEY").unwrap_or_default();
        let secret_key = std::env::var("RS3GW_SECRET_KEY").unwrap_or_default();

        // Parse compression setting: "none", "zstd", "zstd:LEVEL" (1-22), or "lz4"
        let compression = match std::env::var("RS3GW_COMPRESSION")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "" | "none" | "off" | "false" | "0" => CompressionMode::None,
            "zstd" | "on" | "true" | "1" => CompressionMode::Zstd(3), // Default level 3
            "lz4" => CompressionMode::Lz4,
            s if s.starts_with("zstd:") => {
                let level: i32 = s[5..].parse().unwrap_or(3).clamp(1, 22);
                CompressionMode::Zstd(level)
            }
            _ => CompressionMode::None,
        };

        // Request timeout in seconds (0 = no timeout, default 300s = 5 minutes)
        let request_timeout_secs = std::env::var("RS3GW_REQUEST_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(300);

        // Max concurrent requests (0 = no limit)
        let max_concurrent_requests = std::env::var("RS3GW_MAX_CONCURRENT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        // TLS configuration
        let tls = TlsConfig {
            cert_path: std::env::var("RS3GW_TLS_CERT").ok().map(PathBuf::from),
            key_path: std::env::var("RS3GW_TLS_KEY").ok().map(PathBuf::from),
        };

        // Connection pool configuration
        let connection_pool = ConnectionPoolConfig::from_env();

        // Cluster configuration
        let cluster = {
            let enabled = std::env::var("RS3GW_CLUSTER_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false);

            let node_id = std::env::var("RS3GW_CLUSTER_NODE_ID").ok();

            let advertise_addr = std::env::var("RS3GW_CLUSTER_ADVERTISE_ADDR")
                .unwrap_or_else(|_| "127.0.0.1:9001".to_string());

            let cluster_port = std::env::var("RS3GW_CLUSTER_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(9001);

            let seed_nodes: Vec<String> = std::env::var("RS3GW_CLUSTER_SEED_NODES")
                .map(|s| {
                    s.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default();

            let replication_mode = match std::env::var("RS3GW_REPLICATION_MODE")
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "sync" | "synchronous" => ReplicationMode::Synchronous,
                "quorum" => ReplicationMode::Quorum,
                _ => ReplicationMode::Asynchronous,
            };

            let replication_factor = std::env::var("RS3GW_REPLICATION_FACTOR")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(2);

            ClusterConfig {
                node_id,
                advertise_addr,
                seed_nodes,
                cluster_port,
                default_replication: ReplicationConfig {
                    mode: replication_mode,
                    replication_factor,
                    ..Default::default()
                },
                enabled,
                ..Default::default()
            }
        };

        // Deduplication configuration
        let dedup = {
            let enabled = std::env::var("RS3GW_DEDUP_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true);

            let block_size = std::env::var("RS3GW_DEDUP_BLOCK_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(64 * 1024); // 64KB default

            let algorithm = match std::env::var("RS3GW_DEDUP_ALGORITHM")
                .unwrap_or_default()
                .to_lowercase()
                .as_str()
            {
                "content-defined" | "cdc" | "content_defined" => ChunkingAlgorithm::ContentDefined,
                _ => ChunkingAlgorithm::FixedSize,
            };

            let min_object_size = std::env::var("RS3GW_DEDUP_MIN_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(128 * 1024); // 128KB default

            if !enabled {
                DedupConfig::disabled()
            } else {
                DedupConfig::new(block_size)
                    .unwrap_or_default()
                    .with_algorithm(algorithm)
                    .with_min_size(min_object_size)
            }
        };

        // Zero-copy optimizations configuration
        let zerocopy = {
            let direct_io_enabled = std::env::var("RS3GW_ZEROCOPY_DIRECT_IO")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true);

            let direct_io_threshold = std::env::var("RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1024 * 1024); // 1MB default

            let splice_enabled = std::env::var("RS3GW_ZEROCOPY_SPLICE")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true);

            let mmap_metadata_enabled = std::env::var("RS3GW_ZEROCOPY_MMAP")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true);

            ZeroCopyConfig::new()
                .with_direct_io(direct_io_enabled)
                .with_direct_io_threshold(direct_io_threshold)
                .with_splice(splice_enabled)
                .with_mmap_metadata(mmap_metadata_enabled)
        };

        // S3 Select cache configuration
        let select_cache = {
            let enabled = std::env::var("RS3GW_SELECT_CACHE_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true);

            let max_entries = std::env::var("RS3GW_SELECT_CACHE_MAX_ENTRIES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000);

            let max_memory_mb = std::env::var("RS3GW_SELECT_CACHE_MAX_MEMORY_MB")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100);

            let ttl_seconds = std::env::var("RS3GW_SELECT_CACHE_TTL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(3600);

            SelectCacheConfig {
                enabled,
                max_entries,
                max_memory_mb,
                ttl_seconds,
            }
        };

        Self {
            bind_addr,
            storage_root,
            default_bucket,
            access_key,
            secret_key,
            compression,
            request_timeout_secs,
            max_concurrent_requests,
            tls,
            connection_pool,
            cluster,
            dedup,
            zerocopy,
            select_cache,
            multipart_retention_hours: std::env::var("RS3GW_MULTIPART_RETENTION_HOURS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(168),
            fsync: std::env::var("RS3GW_FSYNC")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        }
    }

    /// Load configuration from a TOML file
    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, std::io::Error> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(config)
    }

    /// Load configuration with priority: CLI args > File > Environment > Defaults
    ///
    /// Priority chain:
    /// 1. Config file (if specified and exists): `rs3gw.toml` or path from `--config`
    /// 2. Environment variables (override file values)
    /// 3. Default values
    pub fn load(config_path: Option<&str>) -> Self {
        // Start with defaults
        let mut config = Config::default();

        // Try loading from file if path is specified or default file exists
        let file_path = config_path.unwrap_or("rs3gw.toml");
        if std::path::Path::new(file_path).exists() {
            match Self::from_file(file_path) {
                Ok(file_config) => config = file_config,
                Err(e) => {
                    eprintln!("Warning: Failed to load config file '{}': {}", file_path, e);
                    eprintln!("Falling back to environment variables and defaults");
                }
            }
        }

        // Override with environment variables if set
        if let Ok(bind_addr) = std::env::var("RS3GW_BIND_ADDR") {
            if let Ok(addr) = bind_addr.parse() {
                config.bind_addr = addr;
            }
        }

        if let Ok(storage_root) = std::env::var("RS3GW_STORAGE_ROOT") {
            config.storage_root = PathBuf::from(storage_root);
        }

        if let Ok(default_bucket) = std::env::var("RS3GW_DEFAULT_BUCKET") {
            config.default_bucket = default_bucket;
        }

        if let Ok(access_key) = std::env::var("RS3GW_ACCESS_KEY") {
            config.access_key = access_key;
        }

        if let Ok(secret_key) = std::env::var("RS3GW_SECRET_KEY") {
            config.secret_key = secret_key;
        }

        // Compression override
        if let Ok(compression_str) = std::env::var("RS3GW_COMPRESSION") {
            config.compression = match compression_str.to_lowercase().as_str() {
                "" | "none" | "off" | "false" | "0" => CompressionMode::None,
                "zstd" | "on" | "true" | "1" => CompressionMode::Zstd(3),
                "lz4" => CompressionMode::Lz4,
                s if s.starts_with("zstd:") => {
                    let level: i32 = s[5..].parse().unwrap_or(3).clamp(1, 22);
                    CompressionMode::Zstd(level)
                }
                _ => config.compression,
            };
        }

        if let Ok(timeout) = std::env::var("RS3GW_REQUEST_TIMEOUT") {
            if let Ok(secs) = timeout.parse() {
                config.request_timeout_secs = secs;
            }
        }

        if let Ok(max_concurrent) = std::env::var("RS3GW_MAX_CONCURRENT") {
            if let Ok(max) = max_concurrent.parse() {
                config.max_concurrent_requests = max;
            }
        }

        if let Ok(retention) = std::env::var("RS3GW_MULTIPART_RETENTION_HOURS") {
            if let Ok(hours) = retention.parse() {
                config.multipart_retention_hours = hours;
            }
        }

        // TLS overrides
        if let Ok(cert_path) = std::env::var("RS3GW_TLS_CERT") {
            config.tls.cert_path = Some(PathBuf::from(cert_path));
        }
        if let Ok(key_path) = std::env::var("RS3GW_TLS_KEY") {
            config.tls.key_path = Some(PathBuf::from(key_path));
        }

        // fsync override
        if let Ok(fsync_val) = std::env::var("RS3GW_FSYNC") {
            config.fsync = fsync_val == "true" || fsync_val == "1";
        }

        config
    }
}

/// Cache configuration wrapper
#[derive(Debug, Clone)]
pub struct CacheSettings {
    /// Enable caching (default: true)
    pub enabled: bool,
    /// Maximum cache size in MB (default: 256)
    pub max_size_mb: u64,
    /// Maximum objects to cache (default: 10000)
    pub max_objects: usize,
    /// TTL in seconds (default: 300)
    pub ttl_secs: u64,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size_mb: 256,
            max_objects: 10000,
            ttl_secs: 300,
        }
    }
}

impl CacheSettings {
    /// Load from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("RS3GW_CACHE_ENABLED")
                .map(|v| v != "false" && v != "0")
                .unwrap_or(true),
            max_size_mb: std::env::var("RS3GW_CACHE_MAX_SIZE_MB")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(256),
            max_objects: std::env::var("RS3GW_CACHE_MAX_OBJECTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            ttl_secs: std::env::var("RS3GW_CACHE_TTL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(300),
        }
    }

    /// Convert to CacheConfig
    pub fn to_cache_config(&self) -> CacheConfig {
        if !self.enabled {
            CacheConfig {
                max_size_bytes: 0,
                ..Default::default()
            }
        } else {
            CacheConfig::default()
                .with_max_size_mb(self.max_size_mb)
                .with_max_objects(self.max_objects)
                .with_ttl_secs(self.ttl_secs)
        }
    }
}

/// Throttle configuration wrapper
#[derive(Debug, Clone, Default)]
pub struct ThrottleSettings {
    /// Enable throttling (default: false)
    pub enabled: bool,
    /// Max requests per second per client (0 = unlimited)
    pub requests_per_sec: u32,
    /// Upload bandwidth limit in MB/s (0 = unlimited)
    pub upload_mbps: u64,
    /// Download bandwidth limit in MB/s (0 = unlimited)
    pub download_mbps: u64,
}

impl ThrottleSettings {
    /// Load from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("RS3GW_THROTTLE_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            requests_per_sec: std::env::var("RS3GW_THROTTLE_RPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            upload_mbps: std::env::var("RS3GW_THROTTLE_UPLOAD_MBPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            download_mbps: std::env::var("RS3GW_THROTTLE_DOWNLOAD_MBPS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        }
    }

    /// Convert to ThrottleConfig
    pub fn to_throttle_config(&self) -> ThrottleConfig {
        if !self.enabled {
            ThrottleConfig::default()
        } else {
            ThrottleConfig::default()
                .with_requests_per_sec(self.requests_per_sec)
                .with_upload_mbps(self.upload_mbps)
                .with_download_mbps(self.download_mbps)
        }
    }
}

/// Quota configuration wrapper
#[derive(Debug, Clone, Default)]
pub struct QuotaSettings {
    /// Enable quotas (default: false)
    pub enabled: bool,
    /// Default max storage per bucket in GB (0 = unlimited)
    pub default_max_storage_gb: u64,
    /// Default max objects per bucket (0 = unlimited)
    pub default_max_objects: u64,
}

impl QuotaSettings {
    /// Load from environment variables
    pub fn from_env() -> Self {
        Self {
            enabled: std::env::var("RS3GW_QUOTA_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
            default_max_storage_gb: std::env::var("RS3GW_QUOTA_MAX_STORAGE_GB")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            default_max_objects: std::env::var("RS3GW_QUOTA_MAX_OBJECTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
        }
    }

    /// Convert to QuotaConfig
    pub fn to_quota_config(&self) -> QuotaConfig {
        QuotaConfig {
            max_storage_bytes: self.default_max_storage_gb * 1024 * 1024 * 1024,
            max_objects: self.default_max_objects,
        }
    }
}

/// Tracks in-flight requests for graceful shutdown drain
#[derive(Clone)]
pub struct InFlightTracker {
    count: Arc<AtomicUsize>,
    notify: Arc<tokio::sync::Notify>,
}

impl InFlightTracker {
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicUsize::new(0)),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn track_start(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    pub fn track_end(&self) {
        let prev = self.count.fetch_sub(1, Ordering::SeqCst);
        if prev == 1 {
            self.notify.notify_waiters();
        }
    }

    pub fn active_count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    pub async fn wait_drain(&self, timeout: Duration) {
        if self.active_count() == 0 {
            return;
        }
        let _ = tokio::time::timeout(timeout, async {
            while self.active_count() > 0 {
                self.notify.notified().await;
            }
        })
        .await;
    }
}

impl Default for InFlightTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII guard that tracks an in-flight request lifetime
pub struct InFlightGuard {
    tracker: InFlightTracker,
}

impl InFlightGuard {
    pub fn new(tracker: &InFlightTracker) -> Self {
        tracker.track_start();
        Self {
            tracker: tracker.clone(),
        }
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.tracker.track_end();
    }
}

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub storage: Arc<StorageEngine>,
    pub metrics_handle: PrometheusHandle,
    pub cache: Option<Arc<CacheManager>>,
    pub throttle: Option<Arc<ThrottleManager>>,
    pub quota: Option<Arc<QuotaManager>>,
    pub event_broadcaster: EventBroadcaster,
    pub query_plan_cache: Option<Arc<api::select_optimizer::QueryPlanCache>>,
    pub select_result_cache: Arc<api::SelectResultCache>,
    pub query_intelligence: Arc<api::QueryIntelligence>,
    pub advanced_replication: Option<Arc<AdvancedReplicationManager>>,
    pub preprocessing_manager: Arc<storage::preprocessing::PreprocessingManager>,
    pub predictive_analytics: Arc<observability::PredictiveAnalytics>,
    pub metrics_tracker: Arc<observability::MetricsTracker>,
    pub training_manager: Arc<storage::TrainingManager>,
    pub start_time: std::time::Instant,
    /// Optional SigV4 verifier — None means auth is disabled (passthrough mode)
    pub verifier: Option<Arc<crate::auth::v4::SigV4Verifier>>,
    /// Auth failure rate limiter: IP -> (failure_count, window_start)
    pub auth_failure_counts:
        Arc<std::sync::Mutex<HashMap<std::net::IpAddr, (u32, std::time::Instant)>>>,
    /// Tracks in-flight requests for graceful shutdown drain
    pub in_flight: InFlightTracker,
}

impl AppState {
    /// Create a new AppState with all managers initialized
    pub fn new(
        config: Config,
        storage: Arc<StorageEngine>,
        metrics_handle: PrometheusHandle,
        cache_settings: Option<CacheSettings>,
        throttle_settings: Option<ThrottleSettings>,
        _quota_settings: Option<QuotaSettings>,
    ) -> Self {
        let cache = cache_settings
            .filter(|s| s.enabled)
            .map(|s| Arc::new(CacheManager::new(s.to_cache_config())));

        let throttle = throttle_settings
            .filter(|s| s.enabled)
            .map(|s| Arc::new(ThrottleManager::new(s.to_throttle_config())));

        // Note: QuotaManager creation is async, so we can't use it directly in sync constructor
        // Users should initialize it separately if needed via with_quota()
        let quota: Option<Arc<QuotaManager>> = None;

        // Initialize advanced replication if cluster mode is enabled
        let advanced_replication = if config.cluster.enabled {
            Some(Arc::new(AdvancedReplicationManager::new()))
        } else {
            None
        };

        // Initialize preprocessing manager
        let preprocessing_path =
            std::path::PathBuf::from(&config.storage_root).join("preprocessing");
        let preprocessing_manager = Arc::new(storage::preprocessing::PreprocessingManager::new(
            preprocessing_path,
        ));

        // Initialize training manager
        let training_path = std::path::PathBuf::from(&config.storage_root).join("training");
        let training_manager = Arc::new(storage::TrainingManager::new(training_path));

        // Initialize predictive analytics
        // Default configuration: S3-like pricing (us-east-1)
        // Storage: $0.023/GB-month, Bandwidth: $0.09/GB, Requests: $0.0004/1k requests
        let predictive_analytics = Arc::new(observability::PredictiveAnalytics::new(
            10_000,            // max_history_points
            0.023,             // cost_per_gb_month
            0.09,              // cost_per_gb_bandwidth
            0.0004,            // cost_per_1k_requests
            1_000_000_000_000, // total_capacity (1TB default)
        ));

        // Initialize real-time metrics tracker for predictive analytics
        let metrics_tracker = Arc::new(observability::MetricsTracker::new());

        // Initialize S3 Select result cache from configuration
        let select_result_cache = Arc::new(api::SelectResultCache::new(
            config.select_cache.max_entries,
            (config.select_cache.max_memory_mb * 1024 * 1024) as u64, // Convert MB to bytes
        ));

        // Initialize query intelligence for AI-powered query optimization
        let query_intelligence = Arc::new(api::QueryIntelligence::new());

        // Initialize SigV4 verifier if credentials are configured
        let region = std::env::var("RS3GW_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let verifier = if !config.access_key.is_empty() && !config.secret_key.is_empty() {
            Some(Arc::new(crate::auth::v4::SigV4Verifier::new(
                config.access_key.clone(),
                config.secret_key.clone(),
                region,
            )))
        } else {
            None
        };

        Self {
            config,
            storage,
            metrics_handle,
            cache,
            throttle,
            quota,
            event_broadcaster: EventBroadcaster::new(),
            query_plan_cache: Some(Arc::new(api::select_optimizer::QueryPlanCache::new(1000))),
            select_result_cache,
            query_intelligence,
            advanced_replication,
            preprocessing_manager,
            predictive_analytics,
            metrics_tracker,
            training_manager,
            start_time: std::time::Instant::now(),
            verifier,
            auth_failure_counts: Arc::new(std::sync::Mutex::new(HashMap::new())),
            in_flight: InFlightTracker::new(),
        }
    }

    /// Set the quota manager (async initialization required)
    pub fn with_quota(mut self, quota: Arc<QuotaManager>) -> Self {
        self.quota = Some(quota);
        self
    }

    /// Start background metrics collection for predictive analytics
    ///
    /// This spawns a background task that periodically collects storage size,
    /// request rate, and bandwidth metrics for predictive analytics.
    ///
    /// # Arguments
    ///
    /// * `interval_seconds` - How often to collect metrics (default: 60 seconds)
    pub fn start_metrics_collection(&self, interval_seconds: u64) {
        let state = self.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(interval_seconds));

            loop {
                interval.tick().await;

                // Get current timestamp
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                // Collect storage size
                let storage_size = if let Ok(stats) = state.storage.get_storage_stats().await {
                    state
                        .predictive_analytics
                        .record_storage_size(timestamp, stats.total_size_bytes)
                        .await;
                    stats.total_size_bytes
                } else {
                    0
                };

                // Collect request rate from metrics tracker
                let rps = state.metrics_tracker.get_current_rps().await;
                state
                    .predictive_analytics
                    .record_request_rate(timestamp, rps)
                    .await;

                // Collect bandwidth from metrics tracker
                let bandwidth = state.metrics_tracker.get_total_bandwidth().await;
                state
                    .predictive_analytics
                    .record_bandwidth(timestamp, bandwidth)
                    .await;

                // Reset the metrics tracker window for next interval
                state.metrics_tracker.reset_window().await;

                tracing::debug!(
                    "Collected metrics for predictive analytics: timestamp={}, storage={}, rps={:.2}, bandwidth={:.2} bytes/sec",
                    timestamp,
                    storage_size,
                    rps,
                    bandwidth
                );
            }
        });
    }
}

#[cfg(test)]
pub mod test_helpers {
    use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
    use std::sync::OnceLock;

    /// Get or initialize a shared Prometheus handle for all tests
    /// This ensures only one metrics recorder is installed globally
    ///
    /// Returns a PrometheusHandle that can be used by all tests.
    /// If the recorder was already installed by another test module, this will
    /// still succeed by using the first initialization.
    pub fn get_test_metrics_handle() -> PrometheusHandle {
        static TEST_METRICS: OnceLock<PrometheusHandle> = OnceLock::new();

        TEST_METRICS
            .get_or_init(|| {
                // Try to install the recorder - only the first test module will succeed
                PrometheusBuilder::new()
                    .with_http_listener(([127, 0, 0, 1], 0)) // Random available port
                    .install_recorder()
                    .expect("Failed to install Prometheus recorder - this should only fail if called outside test context")
            })
            .clone()
    }
}
