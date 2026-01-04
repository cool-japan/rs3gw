//! Real-time Metrics Tracker for Predictive Analytics
//!
//! This module provides a lightweight metrics tracking system that feeds data
//! to the predictive analytics engine. It tracks request rates and bandwidth usage
//! in a rolling window without the overhead of querying Prometheus.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Real-time metrics tracker
#[derive(Clone)]
pub struct MetricsTracker {
    /// Total requests counter (all time)
    total_requests: Arc<AtomicU64>,
    /// Total bytes uploaded (all time)
    total_bytes_uploaded: Arc<AtomicU64>,
    /// Total bytes downloaded (all time)
    total_bytes_downloaded: Arc<AtomicU64>,
    /// Last reset time for periodic calculations
    last_reset: Arc<RwLock<std::time::Instant>>,
    /// Requests since last reset
    requests_since_reset: Arc<AtomicU64>,
    /// Bytes uploaded since last reset
    bytes_uploaded_since_reset: Arc<AtomicU64>,
    /// Bytes downloaded since last reset
    bytes_downloaded_since_reset: Arc<AtomicU64>,
}

impl Default for MetricsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsTracker {
    /// Create a new metrics tracker
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            total_requests: Arc::new(AtomicU64::new(0)),
            total_bytes_uploaded: Arc::new(AtomicU64::new(0)),
            total_bytes_downloaded: Arc::new(AtomicU64::new(0)),
            last_reset: Arc::new(RwLock::new(now)),
            requests_since_reset: Arc::new(AtomicU64::new(0)),
            bytes_uploaded_since_reset: Arc::new(AtomicU64::new(0)),
            bytes_downloaded_since_reset: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a request
    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.requests_since_reset.fetch_add(1, Ordering::Relaxed);
    }

    /// Record bytes uploaded
    pub fn record_bytes_uploaded(&self, bytes: u64) {
        self.total_bytes_uploaded
            .fetch_add(bytes, Ordering::Relaxed);
        self.bytes_uploaded_since_reset
            .fetch_add(bytes, Ordering::Relaxed);
    }

    /// Record bytes downloaded
    pub fn record_bytes_downloaded(&self, bytes: u64) {
        self.total_bytes_downloaded
            .fetch_add(bytes, Ordering::Relaxed);
        self.bytes_downloaded_since_reset
            .fetch_add(bytes, Ordering::Relaxed);
    }

    /// Get current requests per second
    ///
    /// Calculates RPS based on requests since last reset and elapsed time
    pub async fn get_current_rps(&self) -> f64 {
        let requests = self.requests_since_reset.load(Ordering::Relaxed);
        let last_reset = self.last_reset.read().await;
        let elapsed = last_reset.elapsed().as_secs_f64();

        if elapsed > 0.0 {
            requests as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Get current upload bandwidth (bytes/sec)
    pub async fn get_upload_bandwidth(&self) -> f64 {
        let bytes = self.bytes_uploaded_since_reset.load(Ordering::Relaxed);
        let last_reset = self.last_reset.read().await;
        let elapsed = last_reset.elapsed().as_secs_f64();

        if elapsed > 0.0 {
            bytes as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Get current download bandwidth (bytes/sec)
    pub async fn get_download_bandwidth(&self) -> f64 {
        let bytes = self.bytes_downloaded_since_reset.load(Ordering::Relaxed);
        let last_reset = self.last_reset.read().await;
        let elapsed = last_reset.elapsed().as_secs_f64();

        if elapsed > 0.0 {
            bytes as f64 / elapsed
        } else {
            0.0
        }
    }

    /// Get total bandwidth (upload + download) in bytes/sec
    pub async fn get_total_bandwidth(&self) -> f64 {
        self.get_upload_bandwidth().await + self.get_download_bandwidth().await
    }

    /// Reset the counters for the next measurement window
    ///
    /// This should be called periodically (e.g., every 60 seconds) to maintain
    /// accurate rate calculations
    pub async fn reset_window(&self) {
        self.requests_since_reset.store(0, Ordering::Relaxed);
        self.bytes_uploaded_since_reset.store(0, Ordering::Relaxed);
        self.bytes_downloaded_since_reset
            .store(0, Ordering::Relaxed);
        let mut last_reset = self.last_reset.write().await;
        *last_reset = std::time::Instant::now();
    }

    /// Get lifetime statistics
    pub fn get_lifetime_stats(&self) -> LifetimeStats {
        LifetimeStats {
            total_requests: self.total_requests.load(Ordering::Relaxed),
            total_bytes_uploaded: self.total_bytes_uploaded.load(Ordering::Relaxed),
            total_bytes_downloaded: self.total_bytes_downloaded.load(Ordering::Relaxed),
        }
    }
}

/// Lifetime statistics
#[derive(Debug, Clone)]
pub struct LifetimeStats {
    pub total_requests: u64,
    pub total_bytes_uploaded: u64,
    pub total_bytes_downloaded: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_tracker_creation() {
        let tracker = MetricsTracker::new();
        assert_eq!(tracker.get_current_rps().await, 0.0);
        assert_eq!(tracker.get_total_bandwidth().await, 0.0);
    }

    #[tokio::test]
    async fn test_record_request() {
        let tracker = MetricsTracker::new();
        tracker.record_request();
        tracker.record_request();
        tracker.record_request();

        let stats = tracker.get_lifetime_stats();
        assert_eq!(stats.total_requests, 3);
    }

    #[tokio::test]
    async fn test_record_bandwidth() {
        let tracker = MetricsTracker::new();
        tracker.record_bytes_uploaded(1000);
        tracker.record_bytes_downloaded(2000);

        let stats = tracker.get_lifetime_stats();
        assert_eq!(stats.total_bytes_uploaded, 1000);
        assert_eq!(stats.total_bytes_downloaded, 2000);
    }

    #[tokio::test]
    async fn test_rps_calculation() {
        let tracker = MetricsTracker::new();

        // Record some requests
        for _ in 0..10 {
            tracker.record_request();
        }

        // Wait a bit to establish a time window
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let rps = tracker.get_current_rps().await;
        // Should be roughly 100 rps (10 requests / 0.1 seconds)
        assert!(rps > 50.0 && rps < 150.0, "RPS should be approximately 100");
    }

    #[tokio::test]
    async fn test_bandwidth_calculation() {
        let tracker = MetricsTracker::new();

        // Record some bandwidth
        tracker.record_bytes_uploaded(10_000);
        tracker.record_bytes_downloaded(20_000);

        // Wait a bit to establish a time window
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let upload_bw = tracker.get_upload_bandwidth().await;
        let download_bw = tracker.get_download_bandwidth().await;
        let total_bw = tracker.get_total_bandwidth().await;

        assert!(upload_bw > 0.0, "Upload bandwidth should be positive");
        assert!(download_bw > 0.0, "Download bandwidth should be positive");
        // Due to timing differences between separate await calls, we use a larger tolerance
        assert!(
            (total_bw - (upload_bw + download_bw)).abs() < 100.0,
            "Total bandwidth should approximately equal sum (got total={}, upload+download={})",
            total_bw,
            upload_bw + download_bw
        );
    }

    #[tokio::test]
    async fn test_reset_window() {
        let tracker = MetricsTracker::new();

        tracker.record_request();
        tracker.record_bytes_uploaded(1000);

        // Reset window
        tracker.reset_window().await;

        // Lifetime stats should still reflect totals
        let stats = tracker.get_lifetime_stats();
        assert_eq!(stats.total_requests, 1);
        assert_eq!(stats.total_bytes_uploaded, 1000);

        // But window-based calculations should reset
        // Wait a bit for time to pass
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // New RPS should be 0 since we reset and recorded no new requests
        let rps = tracker.get_current_rps().await;
        assert_eq!(rps, 0.0, "RPS should be 0 after reset with no new requests");
    }
}
