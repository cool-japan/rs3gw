//! Bucket analytics and metrics reporting
//!
//! Provides analytics configuration and metrics collection for S3 buckets.
//! This includes storage class analysis, request metrics, and inventory reporting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Analytics configuration error types
#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error("Analytics configuration not found: {0}")]
    NotFound(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Storage class analysis filter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AnalyticsFilter {
    /// Prefix filter for objects
    pub prefix: Option<String>,
    /// Tag filters
    pub tags: HashMap<String, String>,
}

/// Storage class analysis data export destination
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsExportDestination {
    /// S3 bucket ARN for export
    pub s3_bucket_arn: String,
    /// Bucket account ID (optional)
    pub s3_bucket_account_id: Option<String>,
    /// Prefix for exported data
    pub s3_prefix: Option<String>,
    /// Export format (CSV, Parquet, ORC)
    pub format: ExportFormat,
}

/// Export format for analytics data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum ExportFormat {
    #[default]
    Csv,
    Parquet,
    Orc,
}

/// Storage class analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StorageClassAnalysis {
    /// Data export configuration
    pub data_export: Option<AnalyticsExportDestination>,
}

/// Analytics configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnalyticsConfiguration {
    /// Configuration ID
    pub id: String,
    /// Filter for objects to analyze
    pub filter: Option<AnalyticsFilter>,
    /// Storage class analysis
    pub storage_class_analysis: StorageClassAnalysis,
}

/// Bucket metrics configuration filter
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct MetricsFilter {
    /// Access point ARN (optional)
    pub access_point_arn: Option<String>,
    /// Prefix filter
    pub prefix: Option<String>,
    /// Tag filters
    pub tags: HashMap<String, String>,
}

/// Bucket metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsConfiguration {
    /// Configuration ID
    pub id: String,
    /// Filter for metrics
    pub filter: Option<MetricsFilter>,
}

/// Analytics report entry for storage class analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageClassAnalysisEntry {
    /// Timestamp of analysis
    pub timestamp: DateTime<Utc>,
    /// Storage class
    pub storage_class: String,
    /// Number of objects
    pub object_count: u64,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Average object size
    pub avg_object_size_bytes: u64,
    /// Objects by age bucket (days)
    pub age_distribution: HashMap<String, u64>,
}

/// Request metrics report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetrics {
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Configuration ID
    pub config_id: String,
    /// Total requests
    pub total_requests: u64,
    /// GET requests
    pub get_requests: u64,
    /// PUT requests
    pub put_requests: u64,
    /// DELETE requests
    pub delete_requests: u64,
    /// HEAD requests
    pub head_requests: u64,
    /// POST requests
    pub post_requests: u64,
    /// LIST requests
    pub list_requests: u64,
    /// Bytes downloaded
    pub bytes_downloaded: u64,
    /// Bytes uploaded
    pub bytes_uploaded: u64,
    /// 4xx errors
    pub errors_4xx: u64,
    /// 5xx errors
    pub errors_5xx: u64,
}

/// Analytics manager for bucket analytics and metrics
pub struct AnalyticsManager {
    /// Analytics configurations by bucket and ID
    analytics_configs: HashMap<String, HashMap<String, AnalyticsConfiguration>>,
    /// Metrics configurations by bucket and ID
    metrics_configs: HashMap<String, HashMap<String, MetricsConfiguration>>,
    /// Request metrics accumulator
    request_metrics: HashMap<String, HashMap<String, RequestMetrics>>,
}

impl AnalyticsManager {
    /// Create a new analytics manager
    pub fn new() -> Self {
        Self {
            analytics_configs: HashMap::new(),
            metrics_configs: HashMap::new(),
            request_metrics: HashMap::new(),
        }
    }

    /// Put analytics configuration for a bucket
    pub fn put_analytics_configuration(
        &mut self,
        bucket: &str,
        config: AnalyticsConfiguration,
    ) -> Result<(), AnalyticsError> {
        let bucket_configs = self
            .analytics_configs
            .entry(bucket.to_string())
            .or_default();
        bucket_configs.insert(config.id.clone(), config);
        Ok(())
    }

    /// Get analytics configuration
    pub fn get_analytics_configuration(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<AnalyticsConfiguration, AnalyticsError> {
        self.analytics_configs
            .get(bucket)
            .and_then(|configs| configs.get(id))
            .cloned()
            .ok_or_else(|| AnalyticsError::NotFound(format!("{}:{}", bucket, id)))
    }

    /// List analytics configurations for a bucket
    pub fn list_analytics_configurations(&self, bucket: &str) -> Vec<AnalyticsConfiguration> {
        self.analytics_configs
            .get(bucket)
            .map(|configs| configs.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Delete analytics configuration
    pub fn delete_analytics_configuration(
        &mut self,
        bucket: &str,
        id: &str,
    ) -> Result<(), AnalyticsError> {
        if let Some(bucket_configs) = self.analytics_configs.get_mut(bucket) {
            bucket_configs.remove(id);
        }
        Ok(())
    }

    /// Put metrics configuration for a bucket
    pub fn put_metrics_configuration(
        &mut self,
        bucket: &str,
        config: MetricsConfiguration,
    ) -> Result<(), AnalyticsError> {
        let config_id = config.id.clone();
        let bucket_configs = self.metrics_configs.entry(bucket.to_string()).or_default();
        bucket_configs.insert(config.id.clone(), config);

        // Initialize request metrics for this configuration
        let metrics = RequestMetrics {
            timestamp: Utc::now(),
            config_id: config_id.clone(),
            total_requests: 0,
            get_requests: 0,
            put_requests: 0,
            delete_requests: 0,
            head_requests: 0,
            post_requests: 0,
            list_requests: 0,
            bytes_downloaded: 0,
            bytes_uploaded: 0,
            errors_4xx: 0,
            errors_5xx: 0,
        };

        let bucket_metrics = self.request_metrics.entry(bucket.to_string()).or_default();
        bucket_metrics.insert(config_id, metrics);

        Ok(())
    }

    /// Get metrics configuration
    pub fn get_metrics_configuration(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<MetricsConfiguration, AnalyticsError> {
        self.metrics_configs
            .get(bucket)
            .and_then(|configs| configs.get(id))
            .cloned()
            .ok_or_else(|| AnalyticsError::NotFound(format!("{}:{}", bucket, id)))
    }

    /// List metrics configurations for a bucket
    pub fn list_metrics_configurations(&self, bucket: &str) -> Vec<MetricsConfiguration> {
        self.metrics_configs
            .get(bucket)
            .map(|configs| configs.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Delete metrics configuration
    pub fn delete_metrics_configuration(
        &mut self,
        bucket: &str,
        id: &str,
    ) -> Result<(), AnalyticsError> {
        if let Some(bucket_configs) = self.metrics_configs.get_mut(bucket) {
            bucket_configs.remove(id);
        }
        if let Some(bucket_metrics) = self.request_metrics.get_mut(bucket) {
            bucket_metrics.remove(id);
        }
        Ok(())
    }

    /// Record a request for metrics
    pub fn record_request(
        &mut self,
        bucket: &str,
        method: &str,
        status_code: u16,
        bytes_sent: u64,
        bytes_received: u64,
    ) {
        if let Some(bucket_metrics) = self.request_metrics.get_mut(bucket) {
            for metrics in bucket_metrics.values_mut() {
                metrics.total_requests += 1;
                metrics.bytes_uploaded += bytes_received;
                metrics.bytes_downloaded += bytes_sent;

                match method {
                    "GET" => metrics.get_requests += 1,
                    "PUT" => metrics.put_requests += 1,
                    "DELETE" => metrics.delete_requests += 1,
                    "HEAD" => metrics.head_requests += 1,
                    "POST" => metrics.post_requests += 1,
                    "LIST" => metrics.list_requests += 1,
                    _ => {}
                }

                if (400..500).contains(&status_code) {
                    metrics.errors_4xx += 1;
                } else if status_code >= 500 {
                    metrics.errors_5xx += 1;
                }
            }
        }
    }

    /// Get request metrics for a configuration
    pub fn get_request_metrics(&self, bucket: &str, config_id: &str) -> Option<RequestMetrics> {
        self.request_metrics
            .get(bucket)
            .and_then(|bucket_metrics| bucket_metrics.get(config_id))
            .cloned()
    }

    /// Get all request metrics for a bucket
    pub fn get_all_request_metrics(&self, bucket: &str) -> Vec<RequestMetrics> {
        self.request_metrics
            .get(bucket)
            .map(|bucket_metrics| bucket_metrics.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Generate storage class analysis report
    pub fn generate_storage_class_analysis(
        &self,
        bucket: &str,
        config_id: &str,
        object_metadata: &[(String, u64, DateTime<Utc>)], // (key, size, last_modified)
    ) -> Result<StorageClassAnalysisEntry, AnalyticsError> {
        let config = self.get_analytics_configuration(bucket, config_id)?;

        // Filter objects based on configuration
        let filtered_objects: Vec<_> = object_metadata
            .iter()
            .filter(|(key, _, _)| {
                if let Some(filter) = &config.filter {
                    if let Some(prefix) = &filter.prefix {
                        if !key.starts_with(prefix) {
                            return false;
                        }
                    }
                    // Tag filtering would require object metadata with tags
                }
                true
            })
            .collect();

        let object_count = filtered_objects.len() as u64;
        let total_size_bytes: u64 = filtered_objects.iter().map(|(_, size, _)| size).sum();
        let avg_object_size_bytes = total_size_bytes.checked_div(object_count).unwrap_or(0);

        // Calculate age distribution
        let now = Utc::now();
        let mut age_distribution = HashMap::new();
        for (_, _, last_modified) in filtered_objects.iter() {
            let age_days = (now - *last_modified).num_days();
            let bucket = match age_days {
                0..=30 => "0-30 days",
                31..=90 => "31-90 days",
                91..=180 => "91-180 days",
                181..=365 => "181-365 days",
                _ => "365+ days",
            };
            *age_distribution.entry(bucket.to_string()).or_insert(0) += 1;
        }

        Ok(StorageClassAnalysisEntry {
            timestamp: Utc::now(),
            storage_class: "STANDARD".to_string(),
            object_count,
            total_size_bytes,
            avg_object_size_bytes,
            age_distribution,
        })
    }
}

impl Default for AnalyticsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytics_configuration() {
        let mut manager = AnalyticsManager::new();

        let config = AnalyticsConfiguration {
            id: "test-analytics".to_string(),
            filter: Some(AnalyticsFilter {
                prefix: Some("logs/".to_string()),
                tags: HashMap::new(),
            }),
            storage_class_analysis: StorageClassAnalysis {
                data_export: Some(AnalyticsExportDestination {
                    s3_bucket_arn: "arn:aws:s3:::analytics-bucket".to_string(),
                    s3_bucket_account_id: None,
                    s3_prefix: Some("analytics/".to_string()),
                    format: ExportFormat::Csv,
                }),
            },
        };

        manager
            .put_analytics_configuration("test-bucket", config.clone())
            .expect("Failed to put analytics configuration");

        let retrieved = manager
            .get_analytics_configuration("test-bucket", "test-analytics")
            .expect("Failed to get analytics configuration");
        assert_eq!(retrieved.id, "test-analytics");

        let list = manager.list_analytics_configurations("test-bucket");
        assert_eq!(list.len(), 1);

        manager
            .delete_analytics_configuration("test-bucket", "test-analytics")
            .expect("Failed to delete analytics configuration");
        assert!(manager
            .get_analytics_configuration("test-bucket", "test-analytics")
            .is_err());
    }

    #[test]
    fn test_metrics_configuration() {
        let mut manager = AnalyticsManager::new();

        let config = MetricsConfiguration {
            id: "test-metrics".to_string(),
            filter: Some(MetricsFilter {
                access_point_arn: None,
                prefix: Some("data/".to_string()),
                tags: HashMap::new(),
            }),
        };

        manager
            .put_metrics_configuration("test-bucket", config.clone())
            .expect("Failed to put metrics configuration");

        let retrieved = manager
            .get_metrics_configuration("test-bucket", "test-metrics")
            .expect("Failed to get metrics configuration");
        assert_eq!(retrieved.id, "test-metrics");

        let list = manager.list_metrics_configurations("test-bucket");
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_request_metrics_recording() {
        let mut manager = AnalyticsManager::new();

        let config = MetricsConfiguration {
            id: "test-metrics".to_string(),
            filter: None,
        };

        manager
            .put_metrics_configuration("test-bucket", config)
            .expect("Failed to put metrics configuration");

        // Record some requests
        manager.record_request("test-bucket", "GET", 200, 1024, 0);
        manager.record_request("test-bucket", "PUT", 201, 0, 2048);
        manager.record_request("test-bucket", "GET", 404, 0, 0);

        let metrics = manager
            .get_request_metrics("test-bucket", "test-metrics")
            .expect("Failed to get request metrics");

        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.get_requests, 2);
        assert_eq!(metrics.put_requests, 1);
        assert_eq!(metrics.bytes_downloaded, 1024);
        assert_eq!(metrics.bytes_uploaded, 2048);
        assert_eq!(metrics.errors_4xx, 1);
    }

    #[test]
    fn test_storage_class_analysis() {
        let mut manager = AnalyticsManager::new();

        let config = AnalyticsConfiguration {
            id: "test-analysis".to_string(),
            filter: Some(AnalyticsFilter {
                prefix: Some("data/".to_string()),
                tags: HashMap::new(),
            }),
            storage_class_analysis: StorageClassAnalysis { data_export: None },
        };

        manager
            .put_analytics_configuration("test-bucket", config)
            .expect("Failed to put analytics configuration");

        let now = Utc::now();
        let objects = vec![
            ("data/file1.txt".to_string(), 1024, now),
            (
                "data/file2.txt".to_string(),
                2048,
                now - chrono::Duration::days(60),
            ),
            ("other/file3.txt".to_string(), 512, now),
        ];

        let analysis = manager
            .generate_storage_class_analysis("test-bucket", "test-analysis", &objects)
            .expect("Failed to generate storage class analysis");

        // Should only count objects with "data/" prefix
        assert_eq!(analysis.object_count, 2);
        assert_eq!(analysis.total_size_bytes, 3072);
        assert_eq!(analysis.avg_object_size_bytes, 1536);
    }
}
