//! Bandwidth throttling and rate limiting
//!
//! Provides per-client and per-bucket bandwidth throttling using token bucket algorithm.

use axum::http::Request;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Throttle configuration
#[derive(Debug, Clone)]
pub struct ThrottleConfig {
    /// Max bytes per second for uploads (0 = unlimited)
    pub upload_bytes_per_sec: u64,
    /// Max bytes per second for downloads (0 = unlimited)
    pub download_bytes_per_sec: u64,
    /// Max requests per second (0 = unlimited)
    pub requests_per_sec: u32,
    /// Burst allowance multiplier (e.g., 2.0 allows 2x burst)
    pub burst_multiplier: f64,
    /// Time window for rate calculation
    pub window_secs: u64,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            upload_bytes_per_sec: 0,   // Unlimited
            download_bytes_per_sec: 0, // Unlimited
            requests_per_sec: 0,       // Unlimited
            burst_multiplier: 2.0,
            window_secs: 1,
        }
    }
}

impl ThrottleConfig {
    /// Create config with upload limit in MB/s
    pub fn with_upload_mbps(mut self, mbps: u64) -> Self {
        self.upload_bytes_per_sec = mbps * 1024 * 1024;
        self
    }

    /// Create config with download limit in MB/s
    pub fn with_download_mbps(mut self, mbps: u64) -> Self {
        self.download_bytes_per_sec = mbps * 1024 * 1024;
        self
    }

    /// Create config with request rate limit
    pub fn with_requests_per_sec(mut self, rps: u32) -> Self {
        self.requests_per_sec = rps;
        self
    }

    /// Check if any limits are configured
    pub fn has_limits(&self) -> bool {
        self.upload_bytes_per_sec > 0
            || self.download_bytes_per_sec > 0
            || self.requests_per_sec > 0
    }
}

/// Token bucket for rate limiting
#[derive(Debug)]
struct TokenBucket {
    /// Tokens currently available
    tokens: f64,
    /// Maximum tokens (capacity)
    capacity: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Last refill time
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    /// Try to consume tokens, returns true if successful
    fn try_consume(&mut self, amount: f64) -> bool {
        self.refill();
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }

    /// Get time until tokens are available
    fn time_until_available(&mut self, amount: f64) -> Duration {
        self.refill();
        if self.tokens >= amount {
            Duration::ZERO
        } else {
            let needed = amount - self.tokens;
            Duration::from_secs_f64(needed / self.refill_rate)
        }
    }
}

/// Throttle state for a single client
#[derive(Debug)]
struct ClientThrottle {
    /// Request rate bucket
    request_bucket: TokenBucket,
    /// Upload bandwidth bucket
    upload_bucket: TokenBucket,
    /// Download bandwidth bucket
    download_bucket: TokenBucket,
    /// Last activity time
    last_activity: Instant,
}

impl ClientThrottle {
    fn new(config: &ThrottleConfig) -> Self {
        let request_capacity = if config.requests_per_sec > 0 {
            (config.requests_per_sec as f64) * config.burst_multiplier
        } else {
            f64::MAX
        };

        let upload_capacity = if config.upload_bytes_per_sec > 0 {
            (config.upload_bytes_per_sec as f64) * config.burst_multiplier
        } else {
            f64::MAX
        };

        let download_capacity = if config.download_bytes_per_sec > 0 {
            (config.download_bytes_per_sec as f64) * config.burst_multiplier
        } else {
            f64::MAX
        };

        Self {
            request_bucket: TokenBucket::new(request_capacity, config.requests_per_sec as f64),
            upload_bucket: TokenBucket::new(upload_capacity, config.upload_bytes_per_sec as f64),
            download_bucket: TokenBucket::new(
                download_capacity,
                config.download_bytes_per_sec as f64,
            ),
            last_activity: Instant::now(),
        }
    }
}

/// Result of a throttle check
#[derive(Debug, Clone)]
pub enum ThrottleResult {
    /// Request allowed
    Allowed,
    /// Request rate limited, retry after duration
    RateLimited { retry_after: Duration },
    /// Bandwidth limited for upload
    UploadLimited { retry_after: Duration },
    /// Bandwidth limited for download
    DownloadLimited { retry_after: Duration },
}

impl ThrottleResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::Allowed => None,
            Self::RateLimited { retry_after }
            | Self::UploadLimited { retry_after }
            | Self::DownloadLimited { retry_after } => Some(*retry_after),
        }
    }
}

/// Bandwidth throttle manager
pub struct ThrottleManager {
    /// Default config for clients
    default_config: ThrottleConfig,
    /// Per-bucket configs
    bucket_configs: Arc<RwLock<HashMap<String, ThrottleConfig>>>,
    /// Per-client state (by IP)
    client_state: Arc<RwLock<HashMap<IpAddr, ClientThrottle>>>,
    /// Per-bucket state
    bucket_state: Arc<RwLock<HashMap<String, ClientThrottle>>>,
    /// Cleanup interval for idle clients
    cleanup_interval: Duration,
    /// Max idle time before cleanup
    max_idle: Duration,
}

impl ThrottleManager {
    /// Create a new throttle manager
    pub fn new(default_config: ThrottleConfig) -> Self {
        let manager = Self {
            default_config,
            bucket_configs: Arc::new(RwLock::new(HashMap::new())),
            client_state: Arc::new(RwLock::new(HashMap::new())),
            bucket_state: Arc::new(RwLock::new(HashMap::new())),
            cleanup_interval: Duration::from_secs(60),
            max_idle: Duration::from_secs(300),
        };

        // Start cleanup task
        manager.start_cleanup_task();
        manager
    }

    fn start_cleanup_task(&self) {
        let client_state = Arc::clone(&self.client_state);
        let bucket_state = Arc::clone(&self.bucket_state);
        let max_idle = self.max_idle;
        let interval = self.cleanup_interval;

        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;

                // Clean up idle clients
                {
                    let mut clients = client_state.write().await;
                    let before = clients.len();
                    clients.retain(|_, state| state.last_activity.elapsed() < max_idle);
                    let removed = before - clients.len();
                    if removed > 0 {
                        debug!(removed, "Cleaned up idle client throttle states");
                    }
                }

                // Clean up idle buckets
                {
                    let mut buckets = bucket_state.write().await;
                    let before = buckets.len();
                    buckets.retain(|_, state| state.last_activity.elapsed() < max_idle);
                    let removed = before - buckets.len();
                    if removed > 0 {
                        debug!(removed, "Cleaned up idle bucket throttle states");
                    }
                }
            }
        });
    }

    /// Set throttle config for a bucket
    pub async fn set_bucket_config(&self, bucket: &str, config: ThrottleConfig) {
        let mut configs = self.bucket_configs.write().await;
        configs.insert(bucket.to_string(), config);
    }

    /// Get throttle config for a bucket
    pub async fn get_bucket_config(&self, bucket: &str) -> ThrottleConfig {
        let configs = self.bucket_configs.read().await;
        configs
            .get(bucket)
            .cloned()
            .unwrap_or_else(|| self.default_config.clone())
    }

    /// Check if a request is allowed (rate limiting)
    pub async fn check_request(&self, client_ip: IpAddr) -> ThrottleResult {
        if self.default_config.requests_per_sec == 0 {
            return ThrottleResult::Allowed;
        }

        let mut clients = self.client_state.write().await;
        let state = clients
            .entry(client_ip)
            .or_insert_with(|| ClientThrottle::new(&self.default_config));

        state.last_activity = Instant::now();

        if state.request_bucket.try_consume(1.0) {
            ThrottleResult::Allowed
        } else {
            let retry_after = state.request_bucket.time_until_available(1.0);
            warn!(client = %client_ip, retry_after = ?retry_after, "Request rate limited");
            ThrottleResult::RateLimited { retry_after }
        }
    }

    /// Check if upload bandwidth is available
    pub async fn check_upload(
        &self,
        client_ip: IpAddr,
        bucket: &str,
        bytes: u64,
    ) -> ThrottleResult {
        let config = self.get_bucket_config(bucket).await;
        if config.upload_bytes_per_sec == 0 {
            return ThrottleResult::Allowed;
        }

        let mut buckets = self.bucket_state.write().await;
        let state = buckets
            .entry(bucket.to_string())
            .or_insert_with(|| ClientThrottle::new(&config));

        state.last_activity = Instant::now();

        if state.upload_bucket.try_consume(bytes as f64) {
            ThrottleResult::Allowed
        } else {
            let retry_after = state.upload_bucket.time_until_available(bytes as f64);
            warn!(
                client = %client_ip,
                bucket = %bucket,
                bytes,
                retry_after = ?retry_after,
                "Upload bandwidth limited"
            );
            ThrottleResult::UploadLimited { retry_after }
        }
    }

    /// Check if download bandwidth is available
    pub async fn check_download(
        &self,
        client_ip: IpAddr,
        bucket: &str,
        bytes: u64,
    ) -> ThrottleResult {
        let config = self.get_bucket_config(bucket).await;
        if config.download_bytes_per_sec == 0 {
            return ThrottleResult::Allowed;
        }

        let mut buckets = self.bucket_state.write().await;
        let state = buckets
            .entry(bucket.to_string())
            .or_insert_with(|| ClientThrottle::new(&config));

        state.last_activity = Instant::now();

        if state.download_bucket.try_consume(bytes as f64) {
            ThrottleResult::Allowed
        } else {
            let retry_after = state.download_bucket.time_until_available(bytes as f64);
            warn!(
                client = %client_ip,
                bucket = %bucket,
                bytes,
                retry_after = ?retry_after,
                "Download bandwidth limited"
            );
            ThrottleResult::DownloadLimited { retry_after }
        }
    }

    /// Extract client IP from request
    pub fn extract_client_ip<B>(req: &Request<B>) -> Option<IpAddr> {
        // Try X-Forwarded-For header first
        if let Some(xff) = req.headers().get("x-forwarded-for") {
            if let Ok(s) = xff.to_str() {
                if let Some(first) = s.split(',').next() {
                    if let Ok(ip) = first.trim().parse() {
                        return Some(ip);
                    }
                }
            }
        }

        // Try X-Real-IP header
        if let Some(xri) = req.headers().get("x-real-ip") {
            if let Ok(s) = xri.to_str() {
                if let Ok(ip) = s.parse() {
                    return Some(ip);
                }
            }
        }

        // Fall back to connection info (would need to be passed separately)
        None
    }

    /// Get current stats
    pub async fn stats(&self) -> ThrottleStats {
        let clients = self.client_state.read().await;
        let buckets = self.bucket_state.read().await;
        let configs = self.bucket_configs.read().await;

        ThrottleStats {
            active_clients: clients.len(),
            active_buckets: buckets.len(),
            configured_buckets: configs.len(),
        }
    }
}

/// Throttle statistics
#[derive(Debug, Clone)]
pub struct ThrottleStats {
    pub active_clients: usize,
    pub active_buckets: usize,
    pub configured_buckets: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttle_config() {
        let config = ThrottleConfig::default()
            .with_upload_mbps(100)
            .with_download_mbps(200)
            .with_requests_per_sec(1000);

        assert_eq!(config.upload_bytes_per_sec, 100 * 1024 * 1024);
        assert_eq!(config.download_bytes_per_sec, 200 * 1024 * 1024);
        assert_eq!(config.requests_per_sec, 1000);
        assert!(config.has_limits());
    }

    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10.0, 5.0);

        // Should have 10 tokens initially
        assert!(bucket.try_consume(5.0));
        assert!(bucket.try_consume(5.0));
        assert!(!bucket.try_consume(1.0)); // Empty

        // Wait for refill
        std::thread::sleep(Duration::from_millis(500));
        bucket.refill();
        assert!(bucket.tokens >= 2.0); // ~2.5 tokens after 0.5s at 5/s
    }

    #[tokio::test]
    async fn test_throttle_manager_unlimited() {
        let config = ThrottleConfig::default(); // No limits
        let manager = ThrottleManager::new(config);

        let ip: IpAddr = "127.0.0.1".parse().expect("Failed to parse IP address");

        // Should always be allowed when unlimited
        let result = manager.check_request(ip).await;
        assert!(result.is_allowed());

        let result = manager.check_upload(ip, "bucket", 1_000_000).await;
        assert!(result.is_allowed());
    }

    #[tokio::test]
    async fn test_throttle_manager_rate_limit() {
        let config = ThrottleConfig::default().with_requests_per_sec(2);
        let manager = ThrottleManager::new(config);

        let ip: IpAddr = "127.0.0.1".parse().expect("Failed to parse IP address");

        // First few should be allowed (burst)
        assert!(manager.check_request(ip).await.is_allowed());
        assert!(manager.check_request(ip).await.is_allowed());
        assert!(manager.check_request(ip).await.is_allowed());
        assert!(manager.check_request(ip).await.is_allowed());

        // Should eventually be limited
        let mut limited = false;
        for _ in 0..10 {
            if !manager.check_request(ip).await.is_allowed() {
                limited = true;
                break;
            }
        }
        assert!(limited, "Should have been rate limited");
    }

    #[tokio::test]
    async fn test_per_bucket_config() {
        let default_config = ThrottleConfig::default();
        let manager = ThrottleManager::new(default_config);

        // Set bucket-specific config
        let bucket_config = ThrottleConfig::default().with_upload_mbps(10);
        manager
            .set_bucket_config("limited-bucket", bucket_config)
            .await;

        // Default bucket should be unlimited
        let default = manager.get_bucket_config("other-bucket").await;
        assert_eq!(default.upload_bytes_per_sec, 0);

        // Limited bucket should have config
        let limited = manager.get_bucket_config("limited-bucket").await;
        assert_eq!(limited.upload_bytes_per_sec, 10 * 1024 * 1024);
    }
}
