//! Performance Anomaly Detection Module
//!
//! Provides real-time anomaly detection for performance metrics using statistical analysis.
//! Detects unusual patterns in latency, error rates, throughput, and resource utilization.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// Anomaly severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnomalySeverity {
    /// Low severity - minor deviation
    Low,
    /// Medium severity - moderate deviation
    Medium,
    /// High severity - significant deviation
    High,
    /// Critical severity - extreme deviation
    Critical,
}

/// Anomaly types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    /// Latency spike
    LatencySpike,
    /// Error rate increase
    ErrorRateIncrease,
    /// Throughput drop
    ThroughputDrop,
    /// CPU usage spike
    CpuSpike,
    /// Memory usage spike
    MemorySpike,
    /// Storage growth anomaly
    StorageGrowthAnomaly,
    /// Request rate anomaly
    RequestRateAnomaly,
}

/// Detected anomaly
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anomaly {
    /// Anomaly type
    pub anomaly_type: AnomalyType,
    /// Severity level
    pub severity: AnomalySeverity,
    /// Timestamp when detected
    pub detected_at: DateTime<Utc>,
    /// Current value
    pub current_value: f64,
    /// Expected value (baseline)
    pub expected_value: f64,
    /// Deviation from expected (standard deviations)
    pub deviation_sigma: f64,
    /// Description of the anomaly
    pub description: String,
    /// Metric name
    pub metric_name: String,
    /// Additional context
    pub context: HashMap<String, String>,
}

/// Time series data point
#[derive(Debug, Clone)]
struct DataPoint {
    timestamp: DateTime<Utc>,
    value: f64,
}

/// Statistical baseline for anomaly detection
#[derive(Debug, Clone)]
struct Baseline {
    /// Mean value
    mean: f64,
    /// Standard deviation
    std_dev: f64,
    /// Minimum value observed
    min: f64,
    /// Maximum value observed
    max: f64,
    /// Number of samples
    sample_count: usize,
    /// Last updated timestamp
    last_updated: DateTime<Utc>,
}

impl Baseline {
    fn new() -> Self {
        Self {
            mean: 0.0,
            std_dev: 0.0,
            min: f64::MAX,
            max: f64::MIN,
            sample_count: 0,
            last_updated: Utc::now(),
        }
    }

    fn update(&mut self, data: &[f64]) {
        if data.is_empty() {
            return;
        }

        self.sample_count = data.len();
        self.mean = data.iter().sum::<f64>() / data.len() as f64;

        if data.len() > 1 {
            let variance =
                data.iter().map(|x| (x - self.mean).powi(2)).sum::<f64>() / (data.len() - 1) as f64;
            self.std_dev = variance.sqrt();
        } else {
            self.std_dev = 0.0;
        }

        self.min = data.iter().cloned().fold(f64::MAX, f64::min);
        self.max = data.iter().cloned().fold(f64::MIN, f64::max);
        self.last_updated = Utc::now();
    }

    fn calculate_z_score(&self, value: f64) -> f64 {
        if self.std_dev == 0.0 {
            return 0.0;
        }
        (value - self.mean) / self.std_dev
    }
}

/// Anomaly detection configuration
#[derive(Debug, Clone)]
pub struct AnomalyDetectionConfig {
    /// Window size for baseline calculation (number of samples)
    pub baseline_window_size: usize,
    /// Threshold for low severity (standard deviations)
    pub low_threshold_sigma: f64,
    /// Threshold for medium severity (standard deviations)
    pub medium_threshold_sigma: f64,
    /// Threshold for high severity (standard deviations)
    pub high_threshold_sigma: f64,
    /// Threshold for critical severity (standard deviations)
    pub critical_threshold_sigma: f64,
    /// Minimum samples required before detection
    pub min_samples: usize,
    /// Maximum age of data points (older points are discarded)
    pub max_data_age: Duration,
}

impl Default for AnomalyDetectionConfig {
    fn default() -> Self {
        Self {
            baseline_window_size: 100,
            low_threshold_sigma: 2.0,
            medium_threshold_sigma: 3.0,
            high_threshold_sigma: 4.0,
            critical_threshold_sigma: 5.0,
            min_samples: 20,
            max_data_age: Duration::from_secs(3600), // 1 hour
        }
    }
}

/// Anomaly detector for a single metric
struct MetricDetector {
    /// Metric name
    metric_name: String,
    /// Anomaly type
    anomaly_type: AnomalyType,
    /// Time series data
    data: VecDeque<DataPoint>,
    /// Statistical baseline
    baseline: Baseline,
    /// Configuration
    config: AnomalyDetectionConfig,
}

impl MetricDetector {
    fn new(metric_name: String, anomaly_type: AnomalyType, config: AnomalyDetectionConfig) -> Self {
        Self {
            metric_name,
            anomaly_type,
            data: VecDeque::new(),
            baseline: Baseline::new(),
            config,
        }
    }

    fn add_sample(&mut self, value: f64) {
        let now = Utc::now();

        // Add new data point
        self.data.push_back(DataPoint {
            timestamp: now,
            value,
        });

        // Remove old data points
        let cutoff = now - chrono::Duration::from_std(self.config.max_data_age).unwrap_or_default();
        while let Some(point) = self.data.front() {
            if point.timestamp < cutoff {
                self.data.pop_front();
            } else {
                break;
            }
        }

        // Limit to window size
        while self.data.len() > self.config.baseline_window_size {
            self.data.pop_front();
        }

        // Update baseline
        if self.data.len() >= self.config.min_samples {
            let values: Vec<f64> = self.data.iter().map(|p| p.value).collect();
            self.baseline.update(&values);
        }
    }

    fn detect(&self, current_value: f64) -> Option<Anomaly> {
        if self.baseline.sample_count < self.config.min_samples {
            return None;
        }

        let z_score = self.baseline.calculate_z_score(current_value);
        let abs_z_score = z_score.abs();

        let severity = if abs_z_score >= self.config.critical_threshold_sigma {
            Some(AnomalySeverity::Critical)
        } else if abs_z_score >= self.config.high_threshold_sigma {
            Some(AnomalySeverity::High)
        } else if abs_z_score >= self.config.medium_threshold_sigma {
            Some(AnomalySeverity::Medium)
        } else if abs_z_score >= self.config.low_threshold_sigma {
            Some(AnomalySeverity::Low)
        } else {
            None
        };

        severity.map(|sev| {
            let direction = if z_score > 0.0 { "above" } else { "below" };
            let description = format!(
                "{} anomaly detected: {} is {:.2}σ {} baseline (current: {:.2}, expected: {:.2})",
                self.metric_name,
                self.metric_name,
                abs_z_score,
                direction,
                current_value,
                self.baseline.mean
            );

            Anomaly {
                anomaly_type: self.anomaly_type,
                severity: sev,
                detected_at: Utc::now(),
                current_value,
                expected_value: self.baseline.mean,
                deviation_sigma: abs_z_score,
                description,
                metric_name: self.metric_name.clone(),
                context: HashMap::new(),
            }
        })
    }
}

/// Anomaly detection engine
pub struct AnomalyDetector {
    /// Metric detectors
    detectors: Arc<RwLock<HashMap<String, MetricDetector>>>,
    /// Configuration
    config: AnomalyDetectionConfig,
    /// Detected anomalies history
    anomaly_history: Arc<RwLock<VecDeque<Anomaly>>>,
    /// Maximum history size
    max_history_size: usize,
}

impl AnomalyDetector {
    /// Create a new anomaly detector
    pub fn new(config: AnomalyDetectionConfig) -> Self {
        Self {
            detectors: Arc::new(RwLock::new(HashMap::new())),
            config,
            anomaly_history: Arc::new(RwLock::new(VecDeque::new())),
            max_history_size: 1000,
        }
    }

    /// Register a metric for anomaly detection
    pub async fn register_metric(&self, metric_name: String, anomaly_type: AnomalyType) {
        let mut detectors = self.detectors.write().await;
        detectors.insert(
            metric_name.clone(),
            MetricDetector::new(metric_name, anomaly_type, self.config.clone()),
        );
    }

    /// Record a metric value and check for anomalies
    pub async fn record_and_detect(&self, metric_name: &str, value: f64) -> Option<Anomaly> {
        let mut detectors = self.detectors.write().await;

        if let Some(detector) = detectors.get_mut(metric_name) {
            detector.add_sample(value);
            let anomaly = detector.detect(value);

            if let Some(ref anom) = anomaly {
                // Add to history
                let mut history = self.anomaly_history.write().await;
                history.push_back(anom.clone());

                // Trim history
                while history.len() > self.max_history_size {
                    history.pop_front();
                }
            }

            anomaly
        } else {
            None
        }
    }

    /// Get anomaly history
    pub async fn get_anomaly_history(&self, since: Option<DateTime<Utc>>) -> Vec<Anomaly> {
        let history = self.anomaly_history.read().await;
        if let Some(since_time) = since {
            history
                .iter()
                .filter(|a| a.detected_at >= since_time)
                .cloned()
                .collect()
        } else {
            history.iter().cloned().collect()
        }
    }

    /// Get anomalies by severity
    pub async fn get_anomalies_by_severity(&self, severity: AnomalySeverity) -> Vec<Anomaly> {
        let history = self.anomaly_history.read().await;
        history
            .iter()
            .filter(|a| a.severity == severity)
            .cloned()
            .collect()
    }

    /// Get anomalies by type
    pub async fn get_anomalies_by_type(&self, anomaly_type: AnomalyType) -> Vec<Anomaly> {
        let history = self.anomaly_history.read().await;
        history
            .iter()
            .filter(|a| a.anomaly_type == anomaly_type)
            .cloned()
            .collect()
    }

    /// Get recent critical anomalies
    pub async fn get_recent_critical_anomalies(&self, duration: Duration) -> Vec<Anomaly> {
        let cutoff = Utc::now() - chrono::Duration::from_std(duration).unwrap_or_default();
        let history = self.anomaly_history.read().await;
        history
            .iter()
            .filter(|a| a.severity == AnomalySeverity::Critical && a.detected_at >= cutoff)
            .cloned()
            .collect()
    }

    /// Clear anomaly history
    pub async fn clear_history(&self) {
        let mut history = self.anomaly_history.write().await;
        history.clear();
    }

    /// Get detector statistics
    pub async fn get_statistics(&self) -> HashMap<String, DetectorStats> {
        let detectors = self.detectors.read().await;
        detectors
            .iter()
            .map(|(name, detector)| {
                let stats = DetectorStats {
                    metric_name: name.clone(),
                    sample_count: detector.baseline.sample_count,
                    mean: detector.baseline.mean,
                    std_dev: detector.baseline.std_dev,
                    min: detector.baseline.min,
                    max: detector.baseline.max,
                    last_updated: detector.baseline.last_updated,
                };
                (name.clone(), stats)
            })
            .collect()
    }
}

impl Default for AnomalyDetector {
    fn default() -> Self {
        Self::new(AnomalyDetectionConfig::default())
    }
}

/// Detector statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectorStats {
    pub metric_name: String,
    pub sample_count: usize,
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub last_updated: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_anomaly_detector_creation() {
        let config = AnomalyDetectionConfig::default();
        let detector = AnomalyDetector::new(config);

        let stats = detector.get_statistics().await;
        assert_eq!(stats.len(), 0);
    }

    #[tokio::test]
    async fn test_register_metric() {
        let detector = AnomalyDetector::default();

        detector
            .register_metric("latency_ms".to_string(), AnomalyType::LatencySpike)
            .await;

        let stats = detector.get_statistics().await;
        assert_eq!(stats.len(), 1);
        assert!(stats.contains_key("latency_ms"));
    }

    #[tokio::test]
    async fn test_no_anomaly_in_normal_range() {
        let config = AnomalyDetectionConfig {
            min_samples: 10,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);

        detector
            .register_metric("latency_ms".to_string(), AnomalyType::LatencySpike)
            .await;

        // Add normal samples around 100ms
        for i in 0..20 {
            let value = 100.0 + (i as f64 % 5.0);
            detector.record_and_detect("latency_ms", value).await;
        }

        // Test a value within normal range
        let result = detector.record_and_detect("latency_ms", 102.0).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_detect_latency_spike() {
        let config = AnomalyDetectionConfig {
            min_samples: 10,
            low_threshold_sigma: 2.0,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);

        detector
            .register_metric("latency_ms".to_string(), AnomalyType::LatencySpike)
            .await;

        // Add normal samples around 100ms
        for _ in 0..20 {
            detector.record_and_detect("latency_ms", 100.0).await;
        }

        // Add a spike (significantly above normal)
        let result = detector.record_and_detect("latency_ms", 500.0).await;
        assert!(result.is_some());

        let anomaly = result.expect("Failed to get anomaly result");
        assert_eq!(anomaly.anomaly_type, AnomalyType::LatencySpike);
        assert_eq!(anomaly.current_value, 500.0);
        assert!(anomaly.deviation_sigma > 2.0);
    }

    #[tokio::test]
    async fn test_severity_levels() {
        let config = AnomalyDetectionConfig {
            min_samples: 10,
            low_threshold_sigma: 2.0,
            medium_threshold_sigma: 3.0,
            high_threshold_sigma: 4.0,
            critical_threshold_sigma: 5.0,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);
        detector
            .register_metric("error_rate".to_string(), AnomalyType::ErrorRateIncrease)
            .await;

        // Baseline: 1% error rate with some variation
        for i in 0..20 {
            let value = 1.0 + (i as f64 % 5.0) * 0.1;
            detector.record_and_detect("error_rate", value).await;
        }

        // High severity - significantly above baseline
        let result = detector.record_and_detect("error_rate", 10.0).await;
        assert!(result.is_some());
        if let Some(anomaly) = result {
            assert!(matches!(
                anomaly.severity,
                AnomalySeverity::Medium | AnomalySeverity::High | AnomalySeverity::Critical
            ));
        }
    }

    #[tokio::test]
    async fn test_anomaly_history() {
        let config = AnomalyDetectionConfig {
            min_samples: 5,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);

        detector
            .register_metric("cpu_percent".to_string(), AnomalyType::CpuSpike)
            .await;

        // Add baseline
        for _ in 0..10 {
            detector.record_and_detect("cpu_percent", 50.0).await;
        }

        // Add spike
        detector.record_and_detect("cpu_percent", 95.0).await;

        let history = detector.get_anomaly_history(None).await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].anomaly_type, AnomalyType::CpuSpike);
    }

    #[tokio::test]
    async fn test_get_anomalies_by_type() {
        let config = AnomalyDetectionConfig {
            min_samples: 5,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);

        detector
            .register_metric("latency".to_string(), AnomalyType::LatencySpike)
            .await;
        detector
            .register_metric("errors".to_string(), AnomalyType::ErrorRateIncrease)
            .await;

        // Add baselines
        for _ in 0..10 {
            detector.record_and_detect("latency", 100.0).await;
            detector.record_and_detect("errors", 1.0).await;
        }

        // Add spikes
        detector.record_and_detect("latency", 500.0).await;
        detector.record_and_detect("errors", 20.0).await;

        let latency_anomalies = detector
            .get_anomalies_by_type(AnomalyType::LatencySpike)
            .await;
        let error_anomalies = detector
            .get_anomalies_by_type(AnomalyType::ErrorRateIncrease)
            .await;

        assert_eq!(latency_anomalies.len(), 1);
        assert_eq!(error_anomalies.len(), 1);
    }

    #[tokio::test]
    async fn test_get_recent_critical_anomalies() {
        let config = AnomalyDetectionConfig {
            min_samples: 5,
            critical_threshold_sigma: 3.0,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);

        detector
            .register_metric("memory_mb".to_string(), AnomalyType::MemorySpike)
            .await;

        // Add baseline
        for _ in 0..10 {
            detector.record_and_detect("memory_mb", 1000.0).await;
        }

        // Add critical spike
        detector.record_and_detect("memory_mb", 10000.0).await;

        let critical = detector
            .get_recent_critical_anomalies(Duration::from_secs(60))
            .await;
        assert_eq!(critical.len(), 1);
        assert_eq!(critical[0].severity, AnomalySeverity::Critical);
    }

    #[tokio::test]
    async fn test_clear_history() {
        let config = AnomalyDetectionConfig {
            min_samples: 5,
            ..Default::default()
        };
        let detector = AnomalyDetector::new(config);

        detector
            .register_metric("throughput".to_string(), AnomalyType::ThroughputDrop)
            .await;

        // Add baseline and spike
        for _ in 0..10 {
            detector.record_and_detect("throughput", 1000.0).await;
        }
        detector.record_and_detect("throughput", 100.0).await;

        let history_before = detector.get_anomaly_history(None).await;
        assert_eq!(history_before.len(), 1);

        detector.clear_history().await;

        let history_after = detector.get_anomaly_history(None).await;
        assert_eq!(history_after.len(), 0);
    }

    #[tokio::test]
    async fn test_baseline_calculation() {
        let detector = AnomalyDetector::default();
        detector
            .register_metric("test_metric".to_string(), AnomalyType::LatencySpike)
            .await;

        // Add samples
        for i in 0..20 {
            detector.record_and_detect("test_metric", i as f64).await;
        }

        let stats = detector.get_statistics().await;
        let metric_stats = stats
            .get("test_metric")
            .expect("Failed to get test_metric statistics");

        assert_eq!(metric_stats.sample_count, 20);
        assert!(metric_stats.mean > 0.0);
        assert!(metric_stats.std_dev > 0.0);
        assert_eq!(metric_stats.min, 0.0);
        assert_eq!(metric_stats.max, 19.0);
    }
}
