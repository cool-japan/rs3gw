//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::debug;

use super::functions::default_schema_version;

/// Part metadata for multipart uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartMetadata {
    pub part_number: u32,
    pub etag: String,
    pub size: u64,
    pub last_modified: DateTime<Utc>,
}
/// Multipart upload info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
    pub key: String,
    pub upload_id: String,
    pub initiated: DateTime<Utc>,
}
pub struct QuotaManager {
    config: QuotaConfig,
    usage: Arc<RwLock<HashMap<String, BucketQuota>>>,
}
impl QuotaManager {
    pub async fn new(config: QuotaConfig) -> Self {
        Self {
            config,
            usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub async fn check_quota(
        &self,
        bucket: &str,
        additional_bytes: u64,
    ) -> Result<(), StorageError> {
        if self.config.max_storage_bytes == 0 {
            return Ok(());
        }
        let usage = self.usage.read().await;
        let current = usage.get(bucket).cloned().unwrap_or_default();
        if current.storage_bytes + additional_bytes > self.config.max_storage_bytes {
            return Err(StorageError::Io(std::io::Error::other("Quota exceeded")));
        }
        Ok(())
    }
    pub async fn add_object(&self, bucket: &str, size: u64) {
        let mut usage = self.usage.write().await;
        let quota = usage.entry(bucket.to_string()).or_default();
        quota.storage_bytes += size;
        quota.object_count += 1;
    }
    pub async fn remove_object(&self, bucket: &str, size: u64) {
        let mut usage = self.usage.write().await;
        if let Some(quota) = usage.get_mut(bucket) {
            quota.storage_bytes = quota.storage_bytes.saturating_sub(size);
            quota.object_count = quota.object_count.saturating_sub(1);
        }
    }
}
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub max_size_bytes: u64,
    pub max_objects: usize,
    pub ttl_secs: u64,
}
impl CacheConfig {
    pub fn with_max_size_mb(mut self, mb: u64) -> Self {
        self.max_size_bytes = mb * 1024 * 1024;
        self
    }
    pub fn with_max_objects(mut self, max: usize) -> Self {
        self.max_objects = max;
        self
    }
    pub fn with_ttl_secs(mut self, ttl: u64) -> Self {
        self.ttl_secs = ttl;
        self
    }
}
#[derive(Debug, Clone, Default)]
struct BucketQuota {
    storage_bytes: u64,
    object_count: u64,
}
/// Scientific metadata for HPC/AI workloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SciMetadata {
    pub dataset_name: Option<String>,
    pub experiment_id: Option<String>,
    pub model_version: Option<String>,
    pub checkpoint_epoch: Option<u64>,
    pub tensor_shape: Option<Vec<u64>>,
    pub dtype: Option<String>,
    pub framework: Option<String>,
    pub custom_fields: HashMap<String, String>,
}
impl SciMetadata {
    /// Convert scientific metadata to S3 metadata headers
    pub fn to_s3_metadata(&self) -> HashMap<String, String> {
        let mut meta = HashMap::new();
        if let Some(ref name) = self.dataset_name {
            meta.insert("sci-dataset-name".to_string(), name.clone());
        }
        if let Some(ref id) = self.experiment_id {
            meta.insert("sci-experiment-id".to_string(), id.clone());
        }
        if let Some(ref version) = self.model_version {
            meta.insert("sci-model-version".to_string(), version.clone());
        }
        if let Some(epoch) = self.checkpoint_epoch {
            meta.insert("sci-checkpoint-epoch".to_string(), epoch.to_string());
        }
        if let Some(ref shape) = self.tensor_shape {
            meta.insert(
                "sci-tensor-shape".to_string(),
                shape
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if let Some(ref dtype) = self.dtype {
            meta.insert("sci-dtype".to_string(), dtype.clone());
        }
        if let Some(ref framework) = self.framework {
            meta.insert("sci-framework".to_string(), framework.clone());
        }
        for (k, v) in &self.custom_fields {
            meta.insert(format!("sci-{}", k), v.clone());
        }
        meta
    }
}
/// Bucket metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketMetadata {
    pub name: String,
    pub creation_date: DateTime<Utc>,
}
/// Multipart upload metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct MultipartMetadata {
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
    pub content_type: String,
    pub metadata: HashMap<String, String>,
    pub initiated: DateTime<Utc>,
    pub parts: HashMap<u32, PartMetadata>,
}
/// Byte range for partial object reads
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
}
impl ByteRange {
    /// Parse a Range header value (e.g., "bytes=0-1023")
    pub fn parse(range_str: &str, file_size: u64) -> Result<Self, StorageError> {
        let range_str = range_str.trim();
        if !range_str.starts_with("bytes=") {
            return Err(StorageError::InvalidRange);
        }
        let range_part = &range_str[6..];
        if let Some(stripped) = range_part.strip_prefix('-') {
            let suffix: u64 = stripped.parse().map_err(|_| StorageError::InvalidRange)?;
            let start = file_size.saturating_sub(suffix);
            Ok(ByteRange {
                start,
                end: file_size - 1,
            })
        } else {
            let parts: Vec<&str> = range_part.split('-').collect();
            if parts.len() != 2 {
                return Err(StorageError::InvalidRange);
            }
            let start: u64 = parts[0].parse().map_err(|_| StorageError::InvalidRange)?;
            let end = if parts[1].is_empty() {
                file_size - 1
            } else {
                parts[1]
                    .parse::<u64>()
                    .map_err(|_| StorageError::InvalidRange)?
            };
            if start > end || start >= file_size {
                return Err(StorageError::InvalidRange);
            }
            Ok(ByteRange {
                start,
                end: end.min(file_size - 1),
            })
        }
    }
    /// Get the length of the range
    pub fn length(&self) -> u64 {
        self.end - self.start + 1
    }
}
#[derive(Clone)]
pub(crate) struct CacheEntry {
    pub(crate) data: Bytes,
    pub(crate) metadata: ObjectMetadata,
    pub(crate) cached_at: DateTime<Utc>,
}
/// Simple LRU cache for object data
pub struct CacheManager {
    config: CacheConfig,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
}
impl CacheManager {
    pub fn new(config: CacheConfig) -> Self {
        Self {
            config,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub async fn get(&self, bucket: &str, key: &str) -> Option<(ObjectMetadata, Bytes)> {
        let cache_key = format!("{}/{}", bucket, key);
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(&cache_key) {
            let age = Utc::now().signed_duration_since(entry.cached_at);
            if age.num_seconds() < self.config.ttl_secs as i64 {
                debug!("Cache hit: {}", cache_key);
                return Some((entry.metadata.clone(), entry.data.clone()));
            }
        }
        None
    }
    pub async fn put(&self, bucket: &str, key: &str, metadata: ObjectMetadata, data: Bytes) {
        if data.len() as u64 > self.config.max_size_bytes {
            return;
        }
        let cache_key = format!("{}/{}", bucket, key);
        let mut cache = self.cache.write().await;
        if cache.len() >= self.config.max_objects {
            cache.clear();
        }
        cache.insert(
            cache_key,
            CacheEntry {
                data,
                metadata,
                cached_at: Utc::now(),
            },
        );
    }
    pub async fn invalidate(&self, bucket: &str, key: &str) {
        let cache_key = format!("{}/{}", bucket, key);
        let mut cache = self.cache.write().await;
        cache.remove(&cache_key);
    }
}
/// A single CORS rule within a CorsConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsRule {
    pub id: Option<String>,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub expose_headers: Vec<String>,
    pub max_age_seconds: Option<u32>,
}
/// Bucket CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    pub rules: Vec<CorsRule>,
}
#[derive(Debug, Clone, Default)]
pub struct QuotaConfig {
    pub max_storage_bytes: u64,
    pub max_objects: u64,
}
/// Bucket server-side encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    pub rules: Vec<SseRule>,
}
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Object not found: {0}")]
    NotFound(String),
    #[error("Bucket not found")]
    BucketNotFound,
    #[error("Bucket already exists")]
    BucketAlreadyExists,
    #[error("Bucket not empty")]
    BucketNotEmpty,
    #[error("Access denied")]
    AccessDenied,
    #[error("Invalid bucket name: {0}")]
    InvalidBucketName(String),
    #[error("Too many buckets")]
    TooManyBuckets,
    #[error("Invalid range")]
    InvalidRange,
    #[error("Multipart upload not found")]
    MultipartNotFound,
    #[error("Invalid part number")]
    InvalidPartNumber,
    #[error("Invalid part: {0}")]
    InvalidPart(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("I/O error: {0}")]
    Io(std::io::Error),
    /// Returned when an object key contains path traversal sequences or null bytes.
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    /// Returned when the underlying filesystem has no space left (ENOSPC).
    #[error("Insufficient storage: no space left on device")]
    InsufficientStorage,
    /// Returned when an Object Lock policy prevents the requested operation.
    #[error("Object locked: {0}")]
    ObjectLocked(String),
    /// Returned when the bucket is not in the state required for the operation.
    #[error("Invalid bucket state: {0}")]
    InvalidBucketState(String),
}
/// A single SSE rule within an EncryptionConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseRule {
    pub sse_algorithm: String,
    pub kms_master_key_id: Option<String>,
    pub bucket_key_enabled: bool,
}
/// Tracks whether Object Lock was enabled at bucket creation time.
///
/// Written as `bucket_lock_metadata.json` inside the bucket directory when the
/// bucket is created with `x-amz-bucket-object-lock-enabled: true`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BucketLockMetadata {
    /// `true` when the bucket was created with Object Lock enabled.
    pub object_lock_enabled: bool,
}
/// Object metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub last_modified: DateTime<Utc>,
    pub content_type: String,
    pub metadata: HashMap<String, String>,
    /// Schema version for forward-compatibility. Always 1 for current format.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
}
