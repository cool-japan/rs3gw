//! Business Metrics Module
//!
//! Provides high-level business and operational metrics beyond system-level monitoring.
//! Tracks domain-specific KPIs, usage patterns, and business outcomes.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Business metric types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    /// Counter metric (monotonically increasing)
    Counter,
    /// Gauge metric (can go up or down)
    Gauge,
    /// Histogram metric (value distribution)
    Histogram,
    /// Summary metric (quantiles)
    Summary,
}

/// Business metric value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    /// Metric type
    pub metric_type: MetricType,
    /// Current value
    pub value: f64,
    /// Unit of measurement
    pub unit: String,
    /// Timestamp of last update
    pub timestamp: DateTime<Utc>,
    /// Labels/dimensions for the metric
    pub labels: HashMap<String, String>,
}

/// Business metric definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    /// Metric name
    pub name: String,
    /// Metric description
    pub description: String,
    /// Metric type
    pub metric_type: MetricType,
    /// Unit of measurement
    pub unit: String,
    /// Help text
    pub help: String,
}

/// Storage utilization metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUtilizationMetrics {
    /// Total number of buckets
    pub total_buckets: u64,
    /// Total number of objects
    pub total_objects: u64,
    /// Total storage size in bytes
    pub total_size_bytes: u64,
    /// Storage by bucket
    pub storage_by_bucket: HashMap<String, u64>,
    /// Objects by bucket
    pub objects_by_bucket: HashMap<String, u64>,
    /// Average object size
    pub avg_object_size_bytes: f64,
    /// Largest object size
    pub max_object_size_bytes: u64,
    /// Storage growth rate (bytes per hour)
    pub growth_rate_bytes_per_hour: f64,
}

/// Data transfer metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTransferMetrics {
    /// Total bytes uploaded
    pub total_upload_bytes: u64,
    /// Total bytes downloaded
    pub total_download_bytes: u64,
    /// Upload rate (bytes per second)
    pub upload_rate_bps: f64,
    /// Download rate (bytes per second)
    pub download_rate_bps: f64,
    /// Total bandwidth used
    pub total_bandwidth_bytes: u64,
    /// Peak upload rate
    pub peak_upload_rate_bps: f64,
    /// Peak download rate
    pub peak_download_rate_bps: f64,
    /// Transfer by bucket
    pub transfer_by_bucket: HashMap<String, (u64, u64)>, // (upload, download)
}

/// Request pattern metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestPatternMetrics {
    /// Total requests
    pub total_requests: u64,
    /// Requests by operation type
    pub requests_by_operation: HashMap<String, u64>,
    /// Requests by bucket
    pub requests_by_bucket: HashMap<String, u64>,
    /// Average request latency (milliseconds)
    pub avg_latency_ms: f64,
    /// P50 latency
    pub p50_latency_ms: f64,
    /// P95 latency
    pub p95_latency_ms: f64,
    /// P99 latency
    pub p99_latency_ms: f64,
    /// Error rate (percentage)
    pub error_rate_percent: f64,
    /// Success rate (percentage)
    pub success_rate_percent: f64,
}

/// User activity metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserActivityMetrics {
    /// Active users (last hour)
    pub active_users_hourly: u64,
    /// Active users (last day)
    pub active_users_daily: u64,
    /// Total unique users
    pub total_unique_users: u64,
    /// Requests by user
    pub requests_by_user: HashMap<String, u64>,
    /// Most active users
    pub top_users: Vec<(String, u64)>,
    /// User engagement score (0-100)
    pub engagement_score: f64,
}

/// Cost-related metrics (estimated)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMetrics {
    /// Estimated storage cost
    pub storage_cost_usd: f64,
    /// Estimated bandwidth cost
    pub bandwidth_cost_usd: f64,
    /// Estimated request cost
    pub request_cost_usd: f64,
    /// Total estimated cost
    pub total_cost_usd: f64,
    /// Cost per bucket
    pub cost_by_bucket: HashMap<String, f64>,
    /// Cost trend (daily change)
    pub cost_trend_percent: f64,
}

/// Comprehensive business metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessMetricsSnapshot {
    /// Timestamp of snapshot
    pub timestamp: DateTime<Utc>,
    /// Storage utilization
    pub storage: StorageUtilizationMetrics,
    /// Data transfer
    pub transfer: DataTransferMetrics,
    /// Request patterns
    pub requests: RequestPatternMetrics,
    /// User activity
    pub users: UserActivityMetrics,
    /// Cost estimates
    pub costs: CostMetrics,
    /// Custom metrics
    pub custom: HashMap<String, MetricValue>,
}

/// Business metrics collector
pub struct BusinessMetricsCollector {
    /// Metric definitions
    definitions: Arc<RwLock<HashMap<String, MetricDefinition>>>,
    /// Current metric values
    values: Arc<RwLock<HashMap<String, MetricValue>>>,
    /// Metric history for trend analysis
    history: Arc<RwLock<Vec<BusinessMetricsSnapshot>>>,
    /// Maximum history size
    max_history_size: usize,
}

impl BusinessMetricsCollector {
    /// Create a new business metrics collector
    pub fn new() -> Self {
        Self {
            definitions: Arc::new(RwLock::new(HashMap::new())),
            values: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            max_history_size: 1000, // Keep last 1000 snapshots
        }
    }

    /// Register a metric definition
    pub async fn register_metric(&self, definition: MetricDefinition) {
        let mut definitions = self.definitions.write().await;
        definitions.insert(definition.name.clone(), definition);
    }

    /// Record a metric value
    pub async fn record(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        let definitions = self.definitions.read().await;
        if let Some(def) = definitions.get(name) {
            let metric_value = MetricValue {
                metric_type: def.metric_type,
                value,
                unit: def.unit.clone(),
                timestamp: Utc::now(),
                labels,
            };

            let mut values = self.values.write().await;
            values.insert(name.to_string(), metric_value);
        }
    }

    /// Increment a counter metric
    pub async fn increment(&self, name: &str, delta: f64, labels: HashMap<String, String>) {
        let mut values = self.values.write().await;
        let current = values.get(name).map(|v| v.value).unwrap_or(0.0);

        let definitions = self.definitions.read().await;
        if let Some(def) = definitions.get(name) {
            let metric_value = MetricValue {
                metric_type: def.metric_type,
                value: current + delta,
                unit: def.unit.clone(),
                timestamp: Utc::now(),
                labels,
            };
            values.insert(name.to_string(), metric_value);
        }
    }

    /// Set a gauge metric
    pub async fn set_gauge(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        self.record(name, value, labels).await;
    }

    /// Record a histogram value
    pub async fn record_histogram(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        // In a real implementation, this would update histogram buckets
        // For now, we just record the value
        self.record(name, value, labels).await;
    }

    /// Get current value of a metric
    pub async fn get_value(&self, name: &str) -> Option<MetricValue> {
        let values = self.values.read().await;
        values.get(name).cloned()
    }

    /// Get all current metric values
    pub async fn get_all_values(&self) -> HashMap<String, MetricValue> {
        let values = self.values.read().await;
        values.clone()
    }

    /// Take a snapshot of all business metrics
    pub async fn snapshot(
        &self,
        storage_metrics: StorageUtilizationMetrics,
        transfer_metrics: DataTransferMetrics,
        request_metrics: RequestPatternMetrics,
        user_metrics: UserActivityMetrics,
        cost_metrics: CostMetrics,
    ) -> BusinessMetricsSnapshot {
        let custom = self.get_all_values().await;

        let snapshot = BusinessMetricsSnapshot {
            timestamp: Utc::now(),
            storage: storage_metrics,
            transfer: transfer_metrics,
            requests: request_metrics,
            users: user_metrics,
            costs: cost_metrics,
            custom,
        };

        // Add to history
        let mut history = self.history.write().await;
        history.push(snapshot.clone());

        // Trim history if needed
        if history.len() > self.max_history_size {
            history.remove(0);
        }

        snapshot
    }

    /// Get metric history
    pub async fn get_history(&self, since: Option<DateTime<Utc>>) -> Vec<BusinessMetricsSnapshot> {
        let history = self.history.read().await;
        if let Some(since_time) = since {
            history
                .iter()
                .filter(|s| s.timestamp >= since_time)
                .cloned()
                .collect()
        } else {
            history.clone()
        }
    }

    /// Calculate metric trend (percentage change)
    pub async fn calculate_trend(&self, name: &str, duration: Duration) -> Option<f64> {
        let history = self.history.read().await;
        if history.len() < 2 {
            return None;
        }

        let now = Utc::now();
        let since = now - chrono::Duration::from_std(duration).ok()?;

        let recent: Vec<_> = history.iter().filter(|s| s.timestamp >= since).collect();

        if recent.len() < 2 {
            return None;
        }

        let first = recent.first()?.custom.get(name)?.value;
        let last = recent.last()?.custom.get(name)?.value;

        if first == 0.0 {
            return None;
        }

        Some(((last - first) / first) * 100.0)
    }

    /// Export metrics in Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let mut output = String::new();
        let definitions = self.definitions.read().await;
        let values = self.values.read().await;

        for (name, def) in definitions.iter() {
            output.push_str(&format!("# HELP {} {}\n", name, def.help));
            output.push_str(&format!("# TYPE {} {:?}\n", name, def.metric_type));

            if let Some(value) = values.get(name) {
                let labels_str = if value.labels.is_empty() {
                    String::new()
                } else {
                    let labels: Vec<String> = value
                        .labels
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect();
                    format!("{{{}}}", labels.join(","))
                };
                output.push_str(&format!("{}{} {}\n", name, labels_str, value.value));
            }
        }

        output
    }

    /// Export metrics as JSON
    pub async fn export_json(&self) -> Result<String, serde_json::Error> {
        let snapshot = {
            let history = self.history.read().await;
            history.last().cloned()
        };

        if let Some(snap) = snapshot {
            serde_json::to_string_pretty(&snap)
        } else {
            Ok("{}".to_string())
        }
    }
}

impl Default for BusinessMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_record_metric() {
        let collector = BusinessMetricsCollector::new();

        let definition = MetricDefinition {
            name: "test_counter".to_string(),
            description: "A test counter".to_string(),
            metric_type: MetricType::Counter,
            unit: "requests".to_string(),
            help: "Test counter help text".to_string(),
        };

        collector.register_metric(definition).await;

        collector.record("test_counter", 42.0, HashMap::new()).await;

        let value = collector.get_value("test_counter").await;
        assert!(value.is_some());
        assert_eq!(value.as_ref().map(|v| v.value), Some(42.0));
    }

    #[tokio::test]
    async fn test_increment_counter() {
        let collector = BusinessMetricsCollector::new();

        let definition = MetricDefinition {
            name: "request_count".to_string(),
            description: "Total requests".to_string(),
            metric_type: MetricType::Counter,
            unit: "requests".to_string(),
            help: "Total number of requests".to_string(),
        };

        collector.register_metric(definition).await;

        collector
            .increment("request_count", 1.0, HashMap::new())
            .await;
        collector
            .increment("request_count", 5.0, HashMap::new())
            .await;
        collector
            .increment("request_count", 3.0, HashMap::new())
            .await;

        let value = collector.get_value("request_count").await;
        assert_eq!(value.as_ref().map(|v| v.value), Some(9.0));
    }

    #[tokio::test]
    async fn test_gauge_metric() {
        let collector = BusinessMetricsCollector::new();

        let definition = MetricDefinition {
            name: "cpu_usage".to_string(),
            description: "CPU usage percentage".to_string(),
            metric_type: MetricType::Gauge,
            unit: "percent".to_string(),
            help: "Current CPU usage".to_string(),
        };

        collector.register_metric(definition).await;

        collector.set_gauge("cpu_usage", 45.5, HashMap::new()).await;
        let value1 = collector.get_value("cpu_usage").await;
        assert_eq!(value1.as_ref().map(|v| v.value), Some(45.5));

        collector.set_gauge("cpu_usage", 62.3, HashMap::new()).await;
        let value2 = collector.get_value("cpu_usage").await;
        assert_eq!(value2.as_ref().map(|v| v.value), Some(62.3));
    }

    #[tokio::test]
    async fn test_metric_with_labels() {
        let collector = BusinessMetricsCollector::new();

        let definition = MetricDefinition {
            name: "http_requests".to_string(),
            description: "HTTP requests by status".to_string(),
            metric_type: MetricType::Counter,
            unit: "requests".to_string(),
            help: "Total HTTP requests".to_string(),
        };

        collector.register_metric(definition).await;

        let mut labels = HashMap::new();
        labels.insert("status".to_string(), "200".to_string());
        labels.insert("method".to_string(), "GET".to_string());

        collector
            .record("http_requests", 100.0, labels.clone())
            .await;

        let value = collector.get_value("http_requests").await;
        assert!(value.is_some());
        assert_eq!(value.as_ref().map(|v| v.value), Some(100.0));
        assert_eq!(
            value.as_ref().map(|v| v.labels.get("status")),
            Some(Some(&"200".to_string()))
        );
    }

    #[tokio::test]
    async fn test_snapshot_creation() {
        let collector = BusinessMetricsCollector::new();

        let storage_metrics = StorageUtilizationMetrics {
            total_buckets: 10,
            total_objects: 1000,
            total_size_bytes: 1024 * 1024 * 100, // 100 MB
            storage_by_bucket: HashMap::new(),
            objects_by_bucket: HashMap::new(),
            avg_object_size_bytes: 102400.0,
            max_object_size_bytes: 1024 * 1024,
            growth_rate_bytes_per_hour: 1024.0 * 1024.0,
        };

        let transfer_metrics = DataTransferMetrics {
            total_upload_bytes: 1024 * 1024 * 50,
            total_download_bytes: 1024 * 1024 * 150,
            upload_rate_bps: 1024.0 * 100.0,
            download_rate_bps: 1024.0 * 300.0,
            total_bandwidth_bytes: 1024 * 1024 * 200,
            peak_upload_rate_bps: 1024.0 * 500.0,
            peak_download_rate_bps: 1024.0 * 1000.0,
            transfer_by_bucket: HashMap::new(),
        };

        let request_metrics = RequestPatternMetrics {
            total_requests: 5000,
            requests_by_operation: HashMap::new(),
            requests_by_bucket: HashMap::new(),
            avg_latency_ms: 45.5,
            p50_latency_ms: 40.0,
            p95_latency_ms: 120.0,
            p99_latency_ms: 200.0,
            error_rate_percent: 0.5,
            success_rate_percent: 99.5,
        };

        let user_metrics = UserActivityMetrics {
            active_users_hourly: 50,
            active_users_daily: 200,
            total_unique_users: 1000,
            requests_by_user: HashMap::new(),
            top_users: Vec::new(),
            engagement_score: 75.5,
        };

        let cost_metrics = CostMetrics {
            storage_cost_usd: 5.0,
            bandwidth_cost_usd: 10.0,
            request_cost_usd: 2.0,
            total_cost_usd: 17.0,
            cost_by_bucket: HashMap::new(),
            cost_trend_percent: 5.5,
        };

        let snapshot = collector
            .snapshot(
                storage_metrics,
                transfer_metrics,
                request_metrics,
                user_metrics,
                cost_metrics,
            )
            .await;

        assert_eq!(snapshot.storage.total_buckets, 10);
        assert_eq!(snapshot.transfer.total_upload_bytes, 1024 * 1024 * 50);
        assert_eq!(snapshot.requests.total_requests, 5000);
        assert_eq!(snapshot.users.active_users_hourly, 50);
        assert_eq!(snapshot.costs.total_cost_usd, 17.0);
    }

    #[tokio::test]
    async fn test_export_prometheus() {
        let collector = BusinessMetricsCollector::new();

        let definition = MetricDefinition {
            name: "test_metric".to_string(),
            description: "Test metric".to_string(),
            metric_type: MetricType::Counter,
            unit: "count".to_string(),
            help: "A test metric for export".to_string(),
        };

        collector.register_metric(definition).await;
        collector.record("test_metric", 123.0, HashMap::new()).await;

        let prometheus_output = collector.export_prometheus().await;
        assert!(prometheus_output.contains("# HELP test_metric"));
        assert!(prometheus_output.contains("# TYPE test_metric"));
        assert!(prometheus_output.contains("test_metric 123"));
    }

    #[tokio::test]
    async fn test_export_json() {
        let collector = BusinessMetricsCollector::new();

        let storage_metrics = StorageUtilizationMetrics {
            total_buckets: 5,
            total_objects: 100,
            total_size_bytes: 1024,
            storage_by_bucket: HashMap::new(),
            objects_by_bucket: HashMap::new(),
            avg_object_size_bytes: 10.24,
            max_object_size_bytes: 512,
            growth_rate_bytes_per_hour: 100.0,
        };

        let transfer_metrics = DataTransferMetrics {
            total_upload_bytes: 500,
            total_download_bytes: 1000,
            upload_rate_bps: 50.0,
            download_rate_bps: 100.0,
            total_bandwidth_bytes: 1500,
            peak_upload_rate_bps: 200.0,
            peak_download_rate_bps: 400.0,
            transfer_by_bucket: HashMap::new(),
        };

        let request_metrics = RequestPatternMetrics {
            total_requests: 100,
            requests_by_operation: HashMap::new(),
            requests_by_bucket: HashMap::new(),
            avg_latency_ms: 10.0,
            p50_latency_ms: 8.0,
            p95_latency_ms: 25.0,
            p99_latency_ms: 50.0,
            error_rate_percent: 1.0,
            success_rate_percent: 99.0,
        };

        let user_metrics = UserActivityMetrics {
            active_users_hourly: 10,
            active_users_daily: 50,
            total_unique_users: 100,
            requests_by_user: HashMap::new(),
            top_users: Vec::new(),
            engagement_score: 80.0,
        };

        let cost_metrics = CostMetrics {
            storage_cost_usd: 1.0,
            bandwidth_cost_usd: 2.0,
            request_cost_usd: 0.5,
            total_cost_usd: 3.5,
            cost_by_bucket: HashMap::new(),
            cost_trend_percent: 2.0,
        };

        collector
            .snapshot(
                storage_metrics,
                transfer_metrics,
                request_metrics,
                user_metrics,
                cost_metrics,
            )
            .await;

        let json_output = collector.export_json().await;
        assert!(json_output.is_ok());

        let json_str = json_output.ok();
        assert!(json_str.is_some());
        assert!(json_str
            .as_ref()
            .map(|s| s.contains("total_buckets"))
            .unwrap_or(false));
    }
}
