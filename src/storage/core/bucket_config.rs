use super::types::{StorageEngine, StorageError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleRule {
    pub id: Option<String>,
    pub status: String,
    pub filter_prefix: Option<String>,
    pub expiration_days: Option<u64>,
    pub transitions: Vec<LifecycleTransition>,
    pub abort_incomplete_mpu_days: Option<u64>,
    pub noncurrent_expiration_days: Option<u64>,
    pub noncurrent_transitions: Vec<LifecycleTransition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleTransition {
    pub days: u64,
    pub storage_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleConfig {
    pub rules: Vec<LifecycleRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteRedirectAll {
    pub host_name: String,
    pub protocol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteRoutingRule {
    pub condition_key_prefix_equals: Option<String>,
    pub condition_http_error_code: Option<String>,
    pub redirect_host: Option<String>,
    pub redirect_protocol: Option<String>,
    pub redirect_replace_key_prefix_with: Option<String>,
    pub redirect_replace_key_with: Option<String>,
    pub redirect_http_redirect_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebsiteConfig {
    pub index_document: Option<String>,
    pub error_document: Option<String>,
    pub redirect_all_requests_to: Option<WebsiteRedirectAll>,
    pub routing_rules: Vec<WebsiteRoutingRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicAccessBlockConfig {
    pub block_public_acls: bool,
    pub ignore_public_acls: bool,
    pub block_public_policy: bool,
    pub restrict_public_buckets: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipRule {
    pub object_ownership: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnershipControlsConfig {
    pub rules: Vec<OwnershipRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub target_bucket: Option<String>,
    pub target_prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestPaymentConfig {
    pub payer: String,
}

// =========================================================================
// Notification structs
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FilterRule {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopicConfiguration {
    pub id: String,
    pub topic_arn: String,
    pub events: Vec<String>,
    pub filter_rules: Vec<FilterRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueConfiguration {
    pub id: String,
    pub queue_arn: String,
    pub events: Vec<String>,
    pub filter_rules: Vec<FilterRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LambdaFunctionConfiguration {
    pub id: String,
    pub lambda_function_arn: String,
    pub events: Vec<String>,
    pub filter_rules: Vec<FilterRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NotificationConfig {
    pub topic_configurations: Vec<TopicConfiguration>,
    pub queue_configurations: Vec<QueueConfiguration>,
    pub lambda_function_configurations: Vec<LambdaFunctionConfiguration>,
}

// =========================================================================
// Replication structs
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicationDestination {
    pub bucket: String,
    pub storage_class: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicationRule {
    pub id: String,
    pub status: String,
    pub priority: Option<u32>,
    pub filter_prefix: Option<String>,
    pub destination: ReplicationDestination,
    pub delete_marker_replication: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplicationConfig {
    pub role: String,
    pub rules: Vec<ReplicationRule>,
}

// =========================================================================
// Accelerate struct
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccelerateConfig {
    pub status: String,
}

// =========================================================================
// IntelligentTiering structs
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tiering {
    pub days: u32,
    pub access_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IntelligentTieringConfig {
    pub id: String,
    pub status: String,
    pub tierings: Vec<Tiering>,
    pub filter_prefix: Option<String>,
}

impl StorageEngine {
    fn bucket_lifecycle_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_lifecycle.json")
    }

    fn bucket_website_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_website.json")
    }

    fn bucket_public_access_block_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_public_access_block.json")
    }

    fn bucket_ownership_controls_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_ownership_controls.json")
    }

    fn bucket_logging_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_logging.json")
    }

    fn bucket_request_payment_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_request_payment.json")
    }

    pub async fn get_bucket_lifecycle(
        &self,
        bucket: &str,
    ) -> Result<LifecycleConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_lifecycle_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Lifecycle configuration for '{}' not found",
                bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_lifecycle(
        &self,
        bucket: &str,
        cfg: &LifecycleConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_lifecycle_path(bucket), data).await?;
        Ok(())
    }

    pub async fn delete_bucket_lifecycle(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_lifecycle_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    pub async fn get_bucket_website(&self, bucket: &str) -> Result<WebsiteConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_website_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Website configuration for '{}' not found",
                bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_website(
        &self,
        bucket: &str,
        cfg: &WebsiteConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_website_path(bucket), data).await?;
        Ok(())
    }

    pub async fn delete_bucket_website(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_website_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    pub async fn get_bucket_public_access_block(
        &self,
        bucket: &str,
    ) -> Result<PublicAccessBlockConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_public_access_block_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "PublicAccessBlock configuration for '{}' not found",
                bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_public_access_block(
        &self,
        bucket: &str,
        cfg: &PublicAccessBlockConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_public_access_block_path(bucket), data).await?;
        Ok(())
    }

    pub async fn delete_bucket_public_access_block(
        &self,
        bucket: &str,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_public_access_block_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    pub async fn get_bucket_ownership_controls(
        &self,
        bucket: &str,
    ) -> Result<OwnershipControlsConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_ownership_controls_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "OwnershipControls configuration for '{}' not found",
                bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_ownership_controls(
        &self,
        bucket: &str,
        cfg: &OwnershipControlsConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_ownership_controls_path(bucket), data).await?;
        Ok(())
    }

    pub async fn delete_bucket_ownership_controls(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_ownership_controls_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    pub async fn get_bucket_logging(&self, bucket: &str) -> Result<LoggingConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_logging_path(bucket);
        if !path.exists() {
            return Ok(LoggingConfig {
                target_bucket: None,
                target_prefix: None,
            });
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_logging(
        &self,
        bucket: &str,
        cfg: &LoggingConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_logging_path(bucket), data).await?;
        Ok(())
    }

    pub async fn get_bucket_request_payment(
        &self,
        bucket: &str,
    ) -> Result<RequestPaymentConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_request_payment_path(bucket);
        if !path.exists() {
            return Ok(RequestPaymentConfig {
                payer: "BucketOwner".to_string(),
            });
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_request_payment(
        &self,
        bucket: &str,
        cfg: &RequestPaymentConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_request_payment_path(bucket), data).await?;
        Ok(())
    }

    // =========================================================================
    // Notification
    // =========================================================================

    fn bucket_notification_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_notification.json")
    }

    pub async fn get_bucket_notification(
        &self,
        bucket: &str,
    ) -> Result<NotificationConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_notification_path(bucket);
        if !path.exists() {
            return Ok(NotificationConfig::default());
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_notification(
        &self,
        bucket: &str,
        cfg: &NotificationConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_notification_path(bucket), data).await?;
        Ok(())
    }

    // =========================================================================
    // Replication
    // =========================================================================

    fn bucket_replication_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_replication.json")
    }

    pub async fn get_bucket_replication(
        &self,
        bucket: &str,
    ) -> Result<ReplicationConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_replication_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Replication configuration for '{}' not found",
                bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_replication(
        &self,
        bucket: &str,
        cfg: &ReplicationConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_replication_path(bucket), data).await?;
        Ok(())
    }

    pub async fn delete_bucket_replication(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_replication_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    // =========================================================================
    // Accelerate
    // =========================================================================

    fn bucket_accelerate_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_accelerate.json")
    }

    pub async fn get_bucket_accelerate(
        &self,
        bucket: &str,
    ) -> Result<AccelerateConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_accelerate_path(bucket);
        if !path.exists() {
            return Ok(AccelerateConfig {
                status: "Suspended".to_string(),
            });
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_accelerate(
        &self,
        bucket: &str,
        cfg: &AccelerateConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_accelerate_path(bucket), data).await?;
        Ok(())
    }

    // =========================================================================
    // IntelligentTiering (id-keyed — uses a subdirectory)
    // =========================================================================

    fn bucket_intelligent_tiering_dir(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_intelligent_tiering")
    }

    fn intelligent_tiering_path(&self, bucket: &str, id: &str) -> PathBuf {
        let sanitized = id.replace(['/', '\\', ':', '?', '*'], "_");
        self.bucket_intelligent_tiering_dir(bucket)
            .join(format!("{}.json", sanitized))
    }

    pub async fn get_bucket_intelligent_tiering(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<IntelligentTieringConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.intelligent_tiering_path(bucket, id);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "IntelligentTiering configuration '{}' for '{}' not found",
                id, bucket
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    pub async fn put_bucket_intelligent_tiering(
        &self,
        bucket: &str,
        id: &str,
        cfg: &IntelligentTieringConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_intelligent_tiering_dir(bucket);
        fs::create_dir_all(&dir).await?;
        let path = self.intelligent_tiering_path(bucket, id);
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(path, data).await?;
        Ok(())
    }

    pub async fn delete_bucket_intelligent_tiering(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.intelligent_tiering_path(bucket, id);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    // === Object Lock Configuration ===

    fn bucket_object_lock_path(&self, bucket: &str) -> PathBuf {
        self.get_root_path()
            .join(bucket)
            .join("bucket_object_lock.json")
    }

    /// Read the bucket-level Object Lock configuration.
    ///
    /// Returns `StorageError::NotFound` when Object Lock was not enabled at
    /// bucket creation time.
    pub async fn get_bucket_object_lock(
        &self,
        bucket: &str,
    ) -> Result<ObjectLockConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let meta = self.read_bucket_lock_metadata(bucket).await?;
        if !meta.object_lock_enabled {
            return Err(StorageError::NotFound(format!(
                "Object Lock not enabled for bucket '{}'",
                bucket
            )));
        }
        let path = self.bucket_object_lock_path(bucket);
        if !path.exists() {
            // Enabled at creation but no rule configured yet — return minimal config.
            return Ok(ObjectLockConfig {
                object_lock_enabled: "Enabled".to_string(),
                rule: None,
            });
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Write the bucket-level Object Lock configuration.
    ///
    /// Only allowed on buckets created with Object Lock enabled.
    pub async fn put_bucket_object_lock(
        &self,
        bucket: &str,
        cfg: &ObjectLockConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let meta = self.read_bucket_lock_metadata(bucket).await?;
        if !meta.object_lock_enabled {
            return Err(StorageError::InvalidBucketState(
                "Object Lock can only be configured on buckets created with object lock enabled"
                    .to_string(),
            ));
        }
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(self.bucket_object_lock_path(bucket), data).await?;
        Ok(())
    }
}

// === Object Lock Configuration types ===

/// Default retention rule within an Object Lock configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DefaultRetention {
    /// Retention mode: `"GOVERNANCE"` or `"COMPLIANCE"`.
    pub mode: String,
    /// Number of days to retain objects.
    pub days: Option<u32>,
    /// Number of years to retain objects.
    pub years: Option<u32>,
}

/// Object Lock rule containing an optional default retention policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectLockRule {
    pub default_retention: Option<DefaultRetention>,
}

/// Bucket-level Object Lock configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectLockConfig {
    /// Always `"Enabled"` once configured.
    pub object_lock_enabled: String,
    pub rule: Option<ObjectLockRule>,
}

// === ID-keyed configuration families ===

/// Sanitize a configuration ID into a safe filename component.
fn sanitize_config_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

// --- Metrics ---

/// Optional filter for a bucket metrics configuration (S3 API).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BucketMetricsFilter {
    pub prefix: Option<String>,
}

/// Bucket metrics configuration identified by a user-supplied ID.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfig {
    pub id: String,
    pub filter: Option<BucketMetricsFilter>,
}

// --- Analytics ---

/// Optional filter for a bucket analytics configuration (S3 API).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BucketAnalyticsFilter {
    pub prefix: Option<String>,
}

/// StorageClass analysis export destination (S3 bucket).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyticsS3BucketDestination {
    pub format: String,
    pub bucket: String,
    pub prefix: Option<String>,
}

/// Outer export destination wrapper for bucket analytics (S3 API).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BucketAnalyticsExportDestination {
    pub s3_bucket_destination: Option<AnalyticsS3BucketDestination>,
}

/// Data export settings for storage-class analysis (S3 API bucket analytics).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyticsDataExport {
    pub output_schema_version: String,
    pub destination: Option<BucketAnalyticsExportDestination>,
}

/// Storage-class analysis wrapper for bucket analytics (S3 API).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BucketStorageClassAnalysis {
    pub data_export: Option<AnalyticsDataExport>,
}

/// Bucket analytics configuration identified by a user-supplied ID.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyticsConfig {
    pub id: String,
    pub storage_class_analysis: Option<BucketStorageClassAnalysis>,
    pub filter: Option<BucketAnalyticsFilter>,
}

// --- Inventory ---

/// S3 bucket destination for an inventory configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InventoryS3BucketDestination {
    pub bucket: String,
    pub format: String,
    pub prefix: Option<String>,
}

/// Destination wrapper for an inventory configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InventoryDestination {
    pub s3_bucket_destination: InventoryS3BucketDestination,
}

/// Bucket inventory configuration identified by a user-supplied ID.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InventoryConfig {
    pub id: String,
    pub destination: InventoryDestination,
    pub is_enabled: bool,
    /// `"All"` or `"Current"`.
    pub included_object_versions: String,
    /// `"Daily"` or `"Weekly"`.
    pub schedule_frequency: String,
    pub optional_fields: Vec<String>,
}

impl StorageEngine {
    // --- Metrics helpers ---

    fn bucket_metrics_dir(&self, bucket: &str) -> std::path::PathBuf {
        self.get_root_path().join(bucket).join("bucket_metrics")
    }

    /// Retrieve a single metrics configuration by ID.
    pub async fn get_bucket_metrics(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<MetricsConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self
            .bucket_metrics_dir(bucket)
            .join(format!("{}.json", sanitize_config_id(id)));
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Metrics configuration '{}' not found",
                id
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Persist a metrics configuration.
    pub async fn put_bucket_metrics(
        &self,
        bucket: &str,
        id: &str,
        cfg: &MetricsConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_metrics_dir(bucket);
        fs::create_dir_all(&dir).await?;
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(dir.join(format!("{}.json", sanitize_config_id(id))), data).await?;
        Ok(())
    }

    /// Delete a metrics configuration by ID (no-op if not found).
    pub async fn delete_bucket_metrics(&self, bucket: &str, id: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self
            .bucket_metrics_dir(bucket)
            .join(format!("{}.json", sanitize_config_id(id)));
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// List all metrics configurations for a bucket.
    pub async fn list_bucket_metrics(
        &self,
        bucket: &str,
    ) -> Result<Vec<MetricsConfig>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_metrics_dir(bucket);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut entries = fs::read_dir(&dir).await?;
        let mut configs = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let data = fs::read(&path).await?;
                if let Ok(cfg) = serde_json::from_slice::<MetricsConfig>(&data) {
                    configs.push(cfg);
                }
            }
        }
        configs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(configs)
    }

    // --- Analytics helpers ---

    fn bucket_analytics_dir(&self, bucket: &str) -> std::path::PathBuf {
        self.get_root_path().join(bucket).join("bucket_analytics")
    }

    /// Retrieve a single analytics configuration by ID.
    pub async fn get_bucket_analytics(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<AnalyticsConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self
            .bucket_analytics_dir(bucket)
            .join(format!("{}.json", sanitize_config_id(id)));
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Analytics configuration '{}' not found",
                id
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Persist an analytics configuration.
    pub async fn put_bucket_analytics(
        &self,
        bucket: &str,
        id: &str,
        cfg: &AnalyticsConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_analytics_dir(bucket);
        fs::create_dir_all(&dir).await?;
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(dir.join(format!("{}.json", sanitize_config_id(id))), data).await?;
        Ok(())
    }

    /// Delete an analytics configuration by ID (no-op if not found).
    pub async fn delete_bucket_analytics(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self
            .bucket_analytics_dir(bucket)
            .join(format!("{}.json", sanitize_config_id(id)));
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// List all analytics configurations for a bucket.
    pub async fn list_bucket_analytics(
        &self,
        bucket: &str,
    ) -> Result<Vec<AnalyticsConfig>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_analytics_dir(bucket);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut entries = fs::read_dir(&dir).await?;
        let mut configs = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let data = fs::read(&path).await?;
                if let Ok(cfg) = serde_json::from_slice::<AnalyticsConfig>(&data) {
                    configs.push(cfg);
                }
            }
        }
        configs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(configs)
    }

    // --- Inventory helpers ---

    fn bucket_inventory_dir(&self, bucket: &str) -> std::path::PathBuf {
        self.get_root_path().join(bucket).join("bucket_inventory")
    }

    /// Retrieve a single inventory configuration by ID.
    pub async fn get_bucket_inventory(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<InventoryConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self
            .bucket_inventory_dir(bucket)
            .join(format!("{}.json", sanitize_config_id(id)));
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Inventory configuration '{}' not found",
                id
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Persist an inventory configuration.
    pub async fn put_bucket_inventory(
        &self,
        bucket: &str,
        id: &str,
        cfg: &InventoryConfig,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_inventory_dir(bucket);
        fs::create_dir_all(&dir).await?;
        let data = serde_json::to_vec_pretty(cfg).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(dir.join(format!("{}.json", sanitize_config_id(id))), data).await?;
        Ok(())
    }

    /// Delete an inventory configuration by ID (no-op if not found).
    pub async fn delete_bucket_inventory(
        &self,
        bucket: &str,
        id: &str,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self
            .bucket_inventory_dir(bucket)
            .join(format!("{}.json", sanitize_config_id(id)));
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// List all inventory configurations for a bucket.
    pub async fn list_bucket_inventory(
        &self,
        bucket: &str,
    ) -> Result<Vec<InventoryConfig>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let dir = self.bucket_inventory_dir(bucket);
        if !dir.exists() {
            return Ok(vec![]);
        }
        let mut entries = fs::read_dir(&dir).await?;
        let mut configs = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let data = fs::read(&path).await?;
                if let Ok(cfg) = serde_json::from_slice::<InventoryConfig>(&data) {
                    configs.push(cfg);
                }
            }
        }
        configs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(configs)
    }
}
