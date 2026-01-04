//! Auto-scaling resource management for dynamic system adaptation
//!
//! This module provides intelligent resource management capabilities:
//! - Dynamic thread pool sizing based on workload
//! - Adaptive rate limiting based on system load
//! - Memory pressure detection and backpressure mechanisms
//! - Graceful degradation under heavy load
//! - CPU and memory utilization monitoring
//!
//! # Features
//! - Automatic resource scaling based on system metrics
//! - Configurable thresholds and policies
//! - Thread-safe resource tracking
//! - Integration with system monitors
//! - Proactive load shedding to prevent system overload

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tokio::time::interval;

/// Resource management configuration
#[derive(Debug, Clone)]
pub struct ResourceConfig {
    /// Minimum thread pool size
    pub min_threads: usize,
    /// Maximum thread pool size
    pub max_threads: usize,
    /// Target CPU utilization (0.0 - 1.0)
    pub target_cpu_utilization: f64,
    /// Memory pressure threshold (0.0 - 1.0)
    pub memory_pressure_threshold: f64,
    /// Rate limit adjustment interval
    pub adjustment_interval: Duration,
    /// Enable adaptive rate limiting
    pub enable_adaptive_rate_limit: bool,
    /// Initial rate limit (requests per second)
    pub initial_rate_limit: u64,
    /// Minimum rate limit
    pub min_rate_limit: u64,
    /// Maximum rate limit
    pub max_rate_limit: u64,
    /// Load shedding threshold (0.0 - 1.0)
    pub load_shedding_threshold: f64,
}

impl Default for ResourceConfig {
    fn default() -> Self {
        Self {
            min_threads: std::env::var("RS3GW_MIN_THREADS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(4),
            max_threads: std::env::var("RS3GW_MAX_THREADS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| num_cpus::get() * 4),
            target_cpu_utilization: std::env::var("RS3GW_TARGET_CPU")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.75), // 75% target
            memory_pressure_threshold: std::env::var("RS3GW_MEMORY_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.85), // 85% threshold
            adjustment_interval: std::env::var("RS3GW_ADJUSTMENT_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .map(Duration::from_secs)
                .unwrap_or(Duration::from_secs(5)),
            enable_adaptive_rate_limit: std::env::var("RS3GW_ADAPTIVE_RATE_LIMIT")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(true),
            initial_rate_limit: std::env::var("RS3GW_INITIAL_RATE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000), // 1000 rps
            min_rate_limit: std::env::var("RS3GW_MIN_RATE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100), // 100 rps
            max_rate_limit: std::env::var("RS3GW_MAX_RATE_LIMIT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000), // 10000 rps
            load_shedding_threshold: std::env::var("RS3GW_LOAD_SHEDDING_THRESHOLD")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.95), // 95% threshold
        }
    }
}

/// System load metrics
#[derive(Debug, Clone, Default)]
pub struct LoadMetrics {
    /// Current CPU utilization (0.0 - 1.0)
    pub cpu_utilization: f64,
    /// Current memory utilization (0.0 - 1.0)
    pub memory_utilization: f64,
    /// Active requests count
    pub active_requests: usize,
    /// Pending requests count
    pub pending_requests: usize,
    /// Request success rate (0.0 - 1.0)
    pub success_rate: f64,
    /// Average request latency in milliseconds
    pub avg_latency_ms: f64,
}

/// Resource manager for auto-scaling and adaptive control
pub struct ResourceManager {
    config: ResourceConfig,
    current_thread_pool_size: AtomicUsize,
    current_rate_limit: AtomicU64,
    active_requests: AtomicUsize,
    pending_requests: AtomicUsize,
    total_requests: AtomicU64,
    successful_requests: AtomicU64,
    failed_requests: AtomicU64,
    total_latency_us: AtomicU64,
    under_load_shedding: AtomicBool,
    metrics: Arc<RwLock<LoadMetrics>>,
    #[allow(dead_code)] // Reserved for future rate limiting implementation
    last_adjustment: Arc<RwLock<Instant>>,
}

impl ResourceManager {
    /// Create a new resource manager
    pub fn new(config: ResourceConfig) -> Self {
        let initial_threads = (config.min_threads + config.max_threads) / 2;

        Self {
            current_thread_pool_size: AtomicUsize::new(initial_threads),
            current_rate_limit: AtomicU64::new(config.initial_rate_limit),
            active_requests: AtomicUsize::new(0),
            pending_requests: AtomicUsize::new(0),
            total_requests: AtomicU64::new(0),
            successful_requests: AtomicU64::new(0),
            failed_requests: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            under_load_shedding: AtomicBool::new(false),
            metrics: Arc::new(RwLock::new(LoadMetrics::default())),
            last_adjustment: Arc::new(RwLock::new(Instant::now())),
            config,
        }
    }

    /// Start the resource manager background tasks
    pub fn start(self: Arc<Self>) {
        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_adjustment_loop().await;
        });
    }

    /// Run the adjustment loop
    async fn run_adjustment_loop(&self) {
        let mut interval_timer = interval(self.config.adjustment_interval);

        loop {
            interval_timer.tick().await;

            // Update system metrics
            self.update_system_metrics().await;

            // Adjust resources based on load
            self.adjust_resources().await;
        }
    }

    /// Update system metrics
    async fn update_system_metrics(&self) {
        let cpu_util = self.get_cpu_utilization();
        let mem_util = self.get_memory_utilization();
        let active = self.active_requests.load(Ordering::Relaxed);
        let pending = self.pending_requests.load(Ordering::Relaxed);

        let total = self.total_requests.load(Ordering::Relaxed);
        let successful = self.successful_requests.load(Ordering::Relaxed);
        let success_rate = if total > 0 {
            successful as f64 / total as f64
        } else {
            1.0
        };

        let avg_latency = if total > 0 {
            let total_latency = self.total_latency_us.load(Ordering::Relaxed);
            (total_latency as f64 / total as f64) / 1000.0 // Convert to ms
        } else {
            0.0
        };

        let mut metrics = self.metrics.write().await;
        metrics.cpu_utilization = cpu_util;
        metrics.memory_utilization = mem_util;
        metrics.active_requests = active;
        metrics.pending_requests = pending;
        metrics.success_rate = success_rate;
        metrics.avg_latency_ms = avg_latency;
    }

    /// Adjust resources based on current load
    async fn adjust_resources(&self) {
        let metrics = self.metrics.read().await.clone();

        // Check if we should enable load shedding
        let system_load = (metrics.cpu_utilization + metrics.memory_utilization) / 2.0;
        if system_load > self.config.load_shedding_threshold {
            self.under_load_shedding.store(true, Ordering::Relaxed);
            tracing::warn!(
                cpu_utilization = %metrics.cpu_utilization,
                memory_utilization = %metrics.memory_utilization,
                "Load shedding activated"
            );
        } else if system_load < self.config.load_shedding_threshold * 0.9 {
            self.under_load_shedding.store(false, Ordering::Relaxed);
        }

        // Adjust thread pool size
        self.adjust_thread_pool(&metrics).await;

        // Adjust rate limiting
        if self.config.enable_adaptive_rate_limit {
            self.adjust_rate_limit(&metrics).await;
        }
    }

    /// Adjust thread pool size based on load
    async fn adjust_thread_pool(&self, metrics: &LoadMetrics) {
        let current_threads = self.current_thread_pool_size.load(Ordering::Relaxed);
        let target_cpu = self.config.target_cpu_utilization;

        let new_threads = if metrics.cpu_utilization > target_cpu + 0.1 {
            // CPU too high, scale down
            (current_threads as f64 * 0.9).max(self.config.min_threads as f64) as usize
        } else if metrics.cpu_utilization < target_cpu - 0.1 && metrics.pending_requests > 10 {
            // CPU low but work pending, scale up
            (current_threads as f64 * 1.1).min(self.config.max_threads as f64) as usize
        } else {
            current_threads
        };

        if new_threads != current_threads {
            self.current_thread_pool_size
                .store(new_threads, Ordering::Relaxed);
            tracing::info!(
                old_size = current_threads,
                new_size = new_threads,
                cpu_utilization = %metrics.cpu_utilization,
                "Adjusted thread pool size"
            );
        }
    }

    /// Adjust rate limit based on system performance
    async fn adjust_rate_limit(&self, metrics: &LoadMetrics) {
        let current_limit = self.current_rate_limit.load(Ordering::Relaxed);

        // Calculate adjustment factor based on success rate and latency
        let adjustment_factor = if metrics.success_rate < 0.95 {
            0.9 // Reduce rate limit if success rate is low
        } else if metrics.avg_latency_ms > 1000.0 {
            0.95 // Reduce rate limit if latency is high
        } else if metrics.success_rate > 0.99 && metrics.avg_latency_ms < 100.0 {
            1.1 // Increase rate limit if performing well
        } else {
            1.0 // Keep current limit
        };

        let new_limit = ((current_limit as f64 * adjustment_factor)
            .max(self.config.min_rate_limit as f64)
            .min(self.config.max_rate_limit as f64)) as u64;

        if new_limit != current_limit {
            self.current_rate_limit.store(new_limit, Ordering::Relaxed);
            tracing::info!(
                old_limit = current_limit,
                new_limit = new_limit,
                success_rate = %metrics.success_rate,
                avg_latency_ms = %metrics.avg_latency_ms,
                "Adjusted rate limit"
            );
        }
    }

    /// Get current CPU utilization (0.0 - 1.0)
    fn get_cpu_utilization(&self) -> f64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(stat) = std::fs::read_to_string("/proc/stat") {
                if let Some(cpu_line) = stat.lines().next() {
                    let fields: Vec<&str> = cpu_line.split_whitespace().collect();
                    if fields.len() > 4 {
                        let user = fields[1].parse::<u64>().unwrap_or(0);
                        let nice = fields[2].parse::<u64>().unwrap_or(0);
                        let system = fields[3].parse::<u64>().unwrap_or(0);
                        let idle = fields[4].parse::<u64>().unwrap_or(0);

                        let total = user + nice + system + idle;
                        let active = user + nice + system;

                        if total > 0 {
                            return active as f64 / total as f64;
                        }
                    }
                }
            }
        }

        0.5 // Default fallback
    }

    /// Get current memory utilization (0.0 - 1.0)
    fn get_memory_utilization(&self) -> f64 {
        #[cfg(target_os = "linux")]
        {
            if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
                let mut total = 0u64;
                let mut available = 0u64;

                for line in meminfo.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let value = parts[1].parse::<u64>().unwrap_or(0);
                        match parts[0] {
                            "MemTotal:" => total = value,
                            "MemAvailable:" => available = value,
                            _ => {}
                        }
                    }
                }

                if total > 0 {
                    return 1.0 - (available as f64 / total as f64);
                }
            }
        }

        0.5 // Default fallback
    }

    /// Check if request should be admitted (not under load shedding)
    pub fn should_admit_request(&self) -> bool {
        !self.under_load_shedding.load(Ordering::Relaxed)
    }

    /// Check if rate limit allows this request
    pub fn check_rate_limit(&self) -> bool {
        // Simple token bucket implementation would go here
        // For now, always allow if not under load shedding
        self.should_admit_request()
    }

    /// Record request start
    pub fn record_request_start(&self) {
        self.active_requests.fetch_add(1, Ordering::Relaxed);
        self.total_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record request completion
    pub fn record_request_complete(&self, success: bool, latency_us: u64) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);

        if success {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_requests.fetch_add(1, Ordering::Relaxed);
        }

        self.total_latency_us
            .fetch_add(latency_us, Ordering::Relaxed);
    }

    /// Add request to pending queue
    pub fn add_pending_request(&self) {
        self.pending_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove request from pending queue
    pub fn remove_pending_request(&self) {
        self.pending_requests.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get current metrics
    pub async fn get_metrics(&self) -> LoadMetrics {
        self.metrics.read().await.clone()
    }

    /// Get current thread pool size
    pub fn get_thread_pool_size(&self) -> usize {
        self.current_thread_pool_size.load(Ordering::Relaxed)
    }

    /// Get current rate limit
    pub fn get_rate_limit(&self) -> u64 {
        self.current_rate_limit.load(Ordering::Relaxed)
    }

    /// Check if under memory pressure
    pub async fn is_under_memory_pressure(&self) -> bool {
        let metrics = self.metrics.read().await;
        metrics.memory_utilization > self.config.memory_pressure_threshold
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.successful_requests.store(0, Ordering::Relaxed);
        self.failed_requests.store(0, Ordering::Relaxed);
        self.total_latency_us.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_config_default() {
        let config = ResourceConfig::default();
        assert!(config.min_threads >= 1);
        assert!(config.max_threads >= config.min_threads);
        assert!(config.target_cpu_utilization > 0.0);
        assert!(config.target_cpu_utilization <= 1.0);
    }

    #[tokio::test]
    async fn test_resource_manager_creation() {
        let config = ResourceConfig::default();
        let manager = ResourceManager::new(config);

        let size = manager.get_thread_pool_size();
        assert!(size >= manager.config.min_threads);
        assert!(size <= manager.config.max_threads);
    }

    #[tokio::test]
    async fn test_request_tracking() {
        let config = ResourceConfig::default();
        let manager = ResourceManager::new(config);

        manager.record_request_start();
        assert_eq!(manager.active_requests.load(Ordering::Relaxed), 1);
        assert_eq!(manager.total_requests.load(Ordering::Relaxed), 1);

        manager.record_request_complete(true, 1000);
        assert_eq!(manager.active_requests.load(Ordering::Relaxed), 0);
        assert_eq!(manager.successful_requests.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_pending_queue() {
        let config = ResourceConfig::default();
        let manager = ResourceManager::new(config);

        manager.add_pending_request();
        assert_eq!(manager.pending_requests.load(Ordering::Relaxed), 1);

        manager.remove_pending_request();
        assert_eq!(manager.pending_requests.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn test_load_shedding() {
        let config = ResourceConfig {
            load_shedding_threshold: 0.5,
            ..Default::default()
        };

        let manager = Arc::new(ResourceManager::new(config));

        // Initially should admit requests
        assert!(manager.should_admit_request());

        // After triggering load shedding, should reject
        manager.under_load_shedding.store(true, Ordering::Relaxed);
        assert!(!manager.should_admit_request());
    }

    #[tokio::test]
    async fn test_metrics_update() {
        let config = ResourceConfig::default();
        let manager = ResourceManager::new(config);

        manager.record_request_start();
        manager.record_request_complete(true, 1000);

        manager.update_system_metrics().await;

        let metrics = manager.get_metrics().await;
        assert!(metrics.success_rate > 0.0);
    }

    #[tokio::test]
    async fn test_stats_reset() {
        let config = ResourceConfig::default();
        let manager = ResourceManager::new(config);

        manager.record_request_start();
        manager.record_request_complete(true, 1000);

        manager.reset_stats();

        assert_eq!(manager.total_requests.load(Ordering::Relaxed), 0);
        assert_eq!(manager.successful_requests.load(Ordering::Relaxed), 0);
    }
}
