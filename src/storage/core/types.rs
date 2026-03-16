//! Storage engine core types and implementation.
//!
//! # On-disk layout
//!
//! ```text
//! <root>/
//!   <bucket>/
//!     objects/<encoded_key>              # object data (may be zstd/lz4 compressed)
//!     metadata/<encoded_key>.json        # JSON: ObjectMetadata (schema_version=1)
//!     sci_metadata/<encoded_key>.json    # JSON: SciMetadata (HPC/AI fields)
//!     tags/<encoded_key>.json            # JSON: ObjectTagging
//!     bucket_tags.json                   # JSON: bucket-level tags
//!     bucket_policy.json                 # JSON: bucket access policy
//!     multipart/<upload_id>/
//!       metadata.json                    # JSON: MultipartMetadata
//!       part-00001, part-00002, ...      # raw part data files
//! ```
//!
//! ## Key encoding
//!
//! S3 keys that end with `/` (directory-placeholder objects) cannot be stored as
//! a file with a trailing slash on most filesystems.  The final empty component is
//! replaced with the sentinel string `__DIROBJ__` so such keys round-trip cleanly.
//!
//! ## Atomic-rename durability
//!
//! Both object data (`put_object`) and metadata (`save_metadata`) are written to a
//! temporary file (`<final_path>.tmp.<nanos>`) and then `rename`d into place.
//! This guarantees that readers never observe a partial write.
//!
//! ## fsync flag
//!
//! When `fsync_enabled` is `true`, `file.sync_all()` is called on the temp file
//! before the rename.  This trades throughput for extra crash durability and is
//! disabled by default (it can be enabled via the `RS3GW_FSYNC=true` environment
//! variable or the `--fsync` CLI flag).
//!
//! ## schema_version
//!
//! Every `metadata/<key>.json` file contains `"schema_version": 1`.  Future
//! breaking changes to the metadata schema must increment this value so that older
//! binaries can detect and refuse to load incompatible data.
//!
//! ## Checksum validation
//!
//! When `checksum_validation` is enabled (via `RS3GW_CHECKSUM_VALIDATION=true`),
//! `get_object` reads the full object body and verifies the stored SHA-256 digest
//! (base64-encoded in `__checksum_value__` metadata) before streaming the data to
//! the caller.  A mismatch returns `StorageError::Internal` with a descriptive
//! message.

pub use crate::storage::versioning::{
    BucketVersionIndex, BucketVersioningConfig, ObjectVersionIndex, ObjectVersionMetadata,
    VersioningManager, VersioningStatus,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::stream::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::collections::HashMap;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Object tagging
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectTagging {
    pub tags: HashMap<String, String>,
}
#[derive(Debug, Clone, Default)]
pub struct QuotaConfig {
    pub max_storage_bytes: u64,
    pub max_objects: u64,
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
/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub bucket_count: u64,
    pub object_count: u64,
    pub total_size_bytes: u64,
}
/// Part metadata for multipart uploads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartMetadata {
    pub part_number: u32,
    pub etag: String,
    pub size: u64,
    pub last_modified: DateTime<Utc>,
}
#[derive(Debug, Clone, Default)]
struct BucketQuota {
    storage_bytes: u64,
    object_count: u64,
}
#[derive(Clone)]
struct CacheEntry {
    data: Bytes,
    metadata: ObjectMetadata,
    cached_at: DateTime<Utc>,
}
fn default_schema_version() -> u32 {
    1
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
pub struct StorageEngine {
    root: PathBuf,
    compression: CompressionMode,
    versioning_manager: Arc<VersioningManager>,
    /// When true, fsync object file before atomic rename for extra durability.
    fsync_enabled: bool,
    /// When true, SHA-256 checksum is verified on every `get_object` call
    /// for objects that carry `__checksum_algo__` / `__checksum_value__` metadata.
    checksum_validation: bool,
}
impl StorageEngine {
    /// Create a new storage engine at the given root path
    pub fn new(root: PathBuf) -> Result<Self, StorageError> {
        std::fs::create_dir_all(&root)?;
        let versioning_manager = Arc::new(VersioningManager::new(root.clone()));
        Ok(Self {
            root,
            compression: CompressionMode::None,
            versioning_manager,
            fsync_enabled: false,
            checksum_validation: false,
        })
    }
    /// Set compression mode
    pub fn with_compression(mut self, mode: CompressionMode) -> Self {
        self.compression = mode;
        self
    }
    /// Enable or disable fsync before rename (safer but slower)
    pub fn with_fsync(mut self, enabled: bool) -> Self {
        self.fsync_enabled = enabled;
        self
    }

    /// Enable or disable SHA-256 checksum validation on read.
    ///
    /// When enabled, `get_object` verifies the stored checksum (if present in
    /// `__checksum_algo__` / `__checksum_value__` metadata) before returning
    /// data.  A mismatch is treated as object corruption and returns
    /// `StorageError::Internal`.
    pub fn with_checksum_validation(mut self, enabled: bool) -> Self {
        self.checksum_validation = enabled;
        self
    }

    /// Decompress raw bytes if a compression algorithm is specified.
    ///
    /// Returns the original bytes unchanged when `algo` is `None`.
    fn maybe_decompress(raw: &[u8], algo: Option<&str>) -> Result<Bytes, StorageError> {
        match algo {
            Some("zstd") => {
                let decompressed = zstd::decode_all(raw).map_err(|e| {
                    StorageError::Internal(format!("zstd decompression failed: {e}"))
                })?;
                Ok(Bytes::from(decompressed))
            }
            Some("lz4") => {
                let decompressed = lz4_flex::decompress_size_prepended(raw).map_err(|e| {
                    StorageError::Internal(format!("lz4 decompression failed: {e}"))
                })?;
                Ok(Bytes::from(decompressed))
            }
            _ => Ok(Bytes::copy_from_slice(raw)),
        }
    }

    /// Validate an object key for safety.
    ///
    /// Rejects:
    /// - Keys containing null bytes
    /// - Any path component that is exactly `..` (path traversal)
    fn validate_key(key: &str) -> Result<(), StorageError> {
        if key.contains('\0') {
            return Err(StorageError::InvalidKey(
                "key contains null byte".to_string(),
            ));
        }
        for component in key.split('/') {
            if component == ".." {
                return Err(StorageError::InvalidKey(
                    "path traversal not allowed".to_string(),
                ));
            }
        }
        Ok(())
    }
    /// Get the storage root path
    pub fn get_root_path(&self) -> PathBuf {
        self.root.clone()
    }
    fn bucket_path(&self, bucket: &str) -> PathBuf {
        self.root.join(bucket)
    }

    /// Encode an S3 object key into a filesystem-safe path component.
    ///
    /// S3 keys may end with `/` (used as directory-placeholder objects). On most
    /// filesystems a trailing slash in a path means "this is a directory", which
    /// makes it impossible to create a *file* at that path. To resolve the
    /// ambiguity we rewrite the final empty component (the one produced by the
    /// trailing slash) to the sentinel filename `__DIROBJ__`.
    ///
    /// Examples:
    ///   `"folder/"` → `"folder/__DIROBJ__"`
    ///   `"a/b/c/"` → `"a/b/c/__DIROBJ__"`
    ///   `"plain-key"` → `"plain-key"` (unchanged)
    fn sanitize_key_for_fs(key: &str) -> String {
        // Strip any leading slashes to prevent absolute-path injection when
        // the sanitised key is joined onto a PathBuf.  S3 treats "/foo" and
        // "foo" as the same key for filesystem storage purposes.
        let key = key.trim_start_matches('/');
        if key.ends_with('/') {
            format!("{}__DIROBJ__", key)
        } else {
            key.to_string()
        }
    }

    fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.bucket_path(bucket)
            .join("objects")
            .join(Self::sanitize_key_for_fs(key))
    }
    fn metadata_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.bucket_path(bucket)
            .join("metadata")
            .join(format!("{}.json", Self::sanitize_key_for_fs(key)))
    }
    fn sci_metadata_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.bucket_path(bucket)
            .join("sci_metadata")
            .join(format!("{}.json", Self::sanitize_key_for_fs(key)))
    }
    fn tagging_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.bucket_path(bucket)
            .join("tags")
            .join(format!("{}.json", Self::sanitize_key_for_fs(key)))
    }
    fn bucket_tagging_path(&self, bucket: &str) -> PathBuf {
        self.bucket_path(bucket).join("bucket_tags.json")
    }
    fn bucket_policy_path(&self, bucket: &str) -> PathBuf {
        self.bucket_path(bucket).join("bucket_policy.json")
    }
    fn multipart_path(&self, bucket: &str, upload_id: &str) -> PathBuf {
        self.bucket_path(bucket).join("multipart").join(upload_id)
    }
    fn multipart_metadata_path(&self, bucket: &str, upload_id: &str) -> PathBuf {
        self.multipart_path(bucket, upload_id).join("metadata.json")
    }
    fn multipart_part_path(&self, bucket: &str, upload_id: &str, part_number: u32) -> PathBuf {
        self.multipart_path(bucket, upload_id)
            .join(format!("part-{:05}", part_number))
    }
    /// List all buckets
    pub async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError> {
        let mut buckets = Vec::new();
        let mut entries = fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let metadata = entry.metadata().await?;
                let name = entry.file_name().to_string_lossy().to_string();
                let creation_date = metadata
                    .created()
                    .or_else(|_| metadata.modified())
                    .map(DateTime::from)
                    .unwrap_or_else(|_| Utc::now());
                buckets.push(BucketMetadata {
                    name,
                    creation_date,
                });
            }
        }
        Ok(buckets)
    }
    /// Check if a bucket exists
    pub async fn bucket_exists(&self, bucket: &str) -> Result<bool, StorageError> {
        let path = self.bucket_path(bucket);
        Ok(path.exists())
    }
    /// Create a new bucket
    pub async fn create_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        let path = self.bucket_path(bucket);
        if path.exists() {
            return Err(StorageError::BucketAlreadyExists);
        }
        fs::create_dir_all(&path).await?;
        fs::create_dir_all(path.join("objects")).await?;
        fs::create_dir_all(path.join("metadata")).await?;
        fs::create_dir_all(path.join("sci_metadata")).await?;
        fs::create_dir_all(path.join("tags")).await?;
        fs::create_dir_all(path.join("multipart")).await?;
        info!("Created bucket: {}", bucket);
        Ok(())
    }
    /// Delete a bucket
    pub async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        let path = self.bucket_path(bucket);
        if !path.exists() {
            return Err(StorageError::BucketNotFound);
        }
        let objects_path = path.join("objects");
        if objects_path.exists() {
            let mut entries = fs::read_dir(&objects_path).await?;
            if entries.next_entry().await?.is_some() {
                return Err(StorageError::BucketNotEmpty);
            }
        }
        fs::remove_dir_all(&path).await?;
        info!("Deleted bucket: {}", bucket);
        Ok(())
    }
    /// List objects in a bucket
    pub async fn list_objects(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
        max_keys: usize,
    ) -> Result<(Vec<ObjectMetadata>, Vec<String>), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let objects_path = self.bucket_path(bucket).join("objects");
        if !objects_path.exists() {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut objects = Vec::new();
        let mut common_prefixes = std::collections::HashSet::new();
        self.collect_objects_recursive(
            &objects_path,
            "",
            prefix,
            delimiter,
            bucket,
            &mut objects,
            &mut common_prefixes,
        )
        .await?;
        objects.truncate(max_keys);
        let common_prefixes: Vec<String> = common_prefixes.into_iter().collect();
        Ok((objects, common_prefixes))
    }
    /// List objects with pagination support
    pub async fn list_objects_with_pagination(
        &self,
        bucket: &str,
        prefix: &str,
        delimiter: Option<&str>,
        max_keys: usize,
        start_after: Option<&str>,
    ) -> Result<(Vec<ObjectMetadata>, Vec<String>, bool), StorageError> {
        // Load ALL objects in the bucket/prefix so that start_after filtering
        // works correctly regardless of filesystem enumeration order.
        let (mut objects, common_prefixes) = self
            .list_objects(bucket, prefix, delimiter, usize::MAX)
            .await?;

        // Sort by key so lexicographic ordering and pagination are deterministic.
        objects.sort_by(|a, b| a.key.cmp(&b.key));

        if let Some(marker) = start_after {
            objects.retain(|obj| obj.key.as_str() > marker);
        }
        let is_truncated = objects.len() > max_keys;
        if is_truncated {
            objects.truncate(max_keys);
        }
        Ok((objects, common_prefixes, is_truncated))
    }
    fn collect_objects_recursive<'a>(
        &'a self,
        dir: &'a Path,
        rel_path: &'a str,
        prefix: &'a str,
        delimiter: Option<&'a str>,
        bucket: &'a str,
        objects: &'a mut Vec<ObjectMetadata>,
        common_prefixes: &'a mut std::collections::HashSet<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), StorageError>> + 'a + Send>>
    {
        Box::pin(async move {
            let mut entries = fs::read_dir(dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let key = if rel_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", rel_path, name)
                };
                if path.is_dir() {
                    let key_with_slash = format!("{}/", key);
                    if !prefix.is_empty()
                        && !prefix.starts_with(&key_with_slash)
                        && !key_with_slash.starts_with(prefix)
                    {
                        continue;
                    }
                    if let Some(delim) = delimiter {
                        if let Some(after_prefix) = key_with_slash.strip_prefix(prefix) {
                            if after_prefix.contains(delim) {
                                if let Some(delim_pos) = after_prefix.find(delim) {
                                    let prefix_end = prefix.len() + delim_pos + delim.len();
                                    common_prefixes
                                        .insert(key_with_slash[..prefix_end].to_string());
                                    continue;
                                }
                            }
                        }
                    }
                    self.collect_objects_recursive(
                        &path,
                        &key,
                        prefix,
                        delimiter,
                        bucket,
                        objects,
                        common_prefixes,
                    )
                    .await?;
                } else {
                    // Reverse the filesystem key encoding: `folder/__DIROBJ__`
                    // was stored as a directory-placeholder object with logical
                    // key `folder/`.
                    let logical_key = if key.ends_with("/__DIROBJ__") {
                        key[..key.len() - "__DIROBJ__".len()].to_string()
                    } else {
                        key.clone()
                    };
                    if !logical_key.starts_with(prefix) {
                        continue;
                    }
                    if let Ok(metadata) = self.load_metadata(bucket, &logical_key).await {
                        objects.push(metadata);
                    }
                }
            }
            Ok(())
        })
    }
    /// Get object metadata
    pub async fn head_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectMetadata, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.load_metadata(bucket, key).await
    }
    /// Get an object
    ///
    /// When `checksum_validation` is enabled and the object metadata contains
    /// `__checksum_algo__ = "sha256"` together with `__checksum_value__` (a
    /// base64-encoded SHA-256 digest), the full object is read, the digest is
    /// computed and compared.  A mismatch returns
    /// `StorageError::Internal("checksum mismatch: object data corrupted")`.
    pub async fn get_object(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<
        (
            ObjectMetadata,
            Box<dyn Stream<Item = Result<Bytes, StorageError>> + Unpin + Send>,
        ),
        StorageError,
    > {
        let metadata = self.head_object(bucket, key).await?;
        let path = self.object_path(bucket, key);

        // Determine if the stored data is compressed.
        let compression_algo = metadata.metadata.get("__compression__").cloned();

        if self.checksum_validation {
            if let (Some(algo), Some(stored_value)) = (
                metadata.metadata.get("__checksum_algo__"),
                metadata.metadata.get("__checksum_value__"),
            ) {
                if algo == "sha256" {
                    let raw = fs::read(&path).await?;
                    let digest = sha2::Sha256::digest(&raw);
                    let computed = BASE64_STANDARD.encode(digest);
                    if computed != *stored_value {
                        return Err(StorageError::Internal(
                            "checksum mismatch: object data corrupted".to_string(),
                        ));
                    }
                    // Checksum verified — decompress if needed, then stream.
                    let decompressed = Self::maybe_decompress(&raw, compression_algo.as_deref())?;
                    let stream = futures::stream::iter(std::iter::once(Ok::<Bytes, StorageError>(
                        decompressed,
                    )));
                    return Ok((metadata, Box::new(stream)));
                }
            }
        }

        // If data is compressed, read into memory and decompress.
        if compression_algo.is_some() {
            let raw = fs::read(&path).await?;
            let decompressed = Self::maybe_decompress(&raw, compression_algo.as_deref())?;
            let stream =
                futures::stream::iter(std::iter::once(Ok::<Bytes, StorageError>(decompressed)));
            return Ok((metadata, Box::new(stream)));
        }

        let file = File::open(&path).await?;
        let stream = tokio_util::io::ReaderStream::new(file);
        let stream = stream.map(|result| result.map_err(StorageError::from));
        Ok((metadata, Box::new(stream)))
    }
    /// Get a range of an object
    pub async fn get_object_range(
        &self,
        bucket: &str,
        key: &str,
        range: &ByteRange,
    ) -> Result<
        (
            ObjectMetadata,
            Box<dyn Stream<Item = Result<Bytes, StorageError>> + Unpin + Send>,
        ),
        StorageError,
    > {
        let metadata = self.head_object(bucket, key).await?;
        let path = self.object_path(bucket, key);
        if range.end >= metadata.size {
            return Err(StorageError::InvalidRange);
        }
        let mut file = File::open(&path).await?;
        file.seek(SeekFrom::Start(range.start)).await?;
        let length = range.length();
        let stream = tokio_util::io::ReaderStream::new(file.take(length));
        let stream = stream.map(|result| result.map_err(StorageError::from));
        Ok((metadata, Box::new(stream)))
    }
    /// Put an object
    ///
    /// Implements storage hardening:
    /// - Path traversal protection via `validate_key`
    /// - Atomic write via temp file + rename
    /// - Optional fsync before rename (controlled by `fsync_enabled`)
    /// - ENOSPC → `StorageError::InsufficientStorage`
    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        metadata: HashMap<String, String>,
        data: Bytes,
    ) -> Result<String, StorageError> {
        // Validate key before any filesystem operations.
        Self::validate_key(key)?;

        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let object_path = self.object_path(bucket, key);
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Compress data if compression is enabled.
        let original_size = data.len() as u64;
        let (write_data, metadata) = match self.compression {
            CompressionMode::Zstd(level) => {
                let compressed = zstd::encode_all(data.as_ref(), level)
                    .map_err(|e| StorageError::Internal(format!("zstd compression failed: {e}")))?;
                let compressed_size = compressed.len() as u64;
                crate::metrics::record_compression("zstd", original_size, compressed_size);
                let mut meta = metadata;
                meta.insert("__compression__".to_string(), "zstd".to_string());
                meta.insert("__original_size__".to_string(), original_size.to_string());
                (Bytes::from(compressed), meta)
            }
            CompressionMode::Lz4 => {
                let compressed = lz4_flex::compress_prepend_size(data.as_ref());
                let compressed_size = compressed.len() as u64;
                crate::metrics::record_compression("lz4", original_size, compressed_size);
                let mut meta = metadata;
                meta.insert("__compression__".to_string(), "lz4".to_string());
                meta.insert("__original_size__".to_string(), original_size.to_string());
                (Bytes::from(compressed), meta)
            }
            CompressionMode::None => (data.clone(), metadata),
        };

        // Build a temp path alongside the final destination.
        let tmp_path = {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            PathBuf::from(format!("{}.tmp.{}", object_path.display(), nanos))
        };

        // Write to temp file, cleaning up on any error.
        let write_result: Result<(), StorageError> = async {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .await?;

            file.write_all(&write_data).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::StorageFull {
                    StorageError::InsufficientStorage
                } else {
                    StorageError::Io(e)
                }
            })?;

            if self.fsync_enabled {
                file.sync_all().await.map_err(|e| {
                    if e.kind() == std::io::ErrorKind::StorageFull {
                        StorageError::InsufficientStorage
                    } else {
                        StorageError::Io(e)
                    }
                })?;
            }
            Ok(())
        }
        .await;

        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(e);
        }

        // Atomic rename: old content is never partially visible.
        if let Err(e) = fs::rename(&tmp_path, &object_path).await {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(StorageError::Io(e));
        }

        let etag = format!("{:x}", sha2::Sha256::digest(&data));
        let obj_metadata = ObjectMetadata {
            key: key.to_string(),
            size: original_size,
            etag: etag.clone(),
            last_modified: Utc::now(),
            content_type: content_type.to_string(),
            metadata: metadata.clone(),
            schema_version: 1,
        };
        self.save_metadata(bucket, key, &obj_metadata).await?;

        // When versioning is enabled (or suspended), record this new object version.
        if let Ok(versioning_cfg) = self.versioning_manager.get_config(bucket).await {
            match versioning_cfg.status {
                crate::storage::VersioningStatus::Enabled
                | crate::storage::VersioningStatus::Suspended => {
                    let version = crate::storage::versioning::ObjectVersionMetadata::new(
                        key.to_string(),
                        obj_metadata.size,
                        etag.clone(),
                    );
                    if let Err(e) = self
                        .versioning_manager
                        .add_version(bucket, key.to_string(), version)
                        .await
                    {
                        error!("Failed to record version for {}/{}: {}", bucket, key, e);
                    }
                }
                crate::storage::VersioningStatus::Unversioned => {}
            }
        }

        crate::metrics::record_object_size(bucket, data.len() as u64);
        info!("Put object: {}/{} ({} bytes)", bucket, key, data.len());
        Ok(etag)
    }

    /// Store an object whose data is already written to a temporary file on disk.
    ///
    /// This is used by the streaming PUT handler: chunks are written to
    /// `temp_path` incrementally while the hash is computed, then this method
    /// performs the atomic rename, metadata write, versioning bookkeeping, etc.
    pub async fn put_object_from_path(
        &self,
        bucket: &str,
        key: &str,
        content_type: Option<String>,
        metadata: HashMap<String, String>,
        temp_path: &std::path::Path,
        size: u64,
        etag: String,
    ) -> Result<String, StorageError> {
        Self::validate_key(key)?;

        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }

        let object_path = self.object_path(bucket, key);
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Handle compression: if enabled, read the temp file, compress,
        // and write the compressed data back before the atomic rename.
        let mut metadata = metadata;
        match self.compression {
            CompressionMode::Zstd(level) => {
                let raw_data = fs::read(temp_path).await?;
                let compressed = zstd::encode_all(raw_data.as_slice(), level)
                    .map_err(|e| StorageError::Internal(format!("zstd compression failed: {e}")))?;
                let compressed_size = compressed.len() as u64;
                crate::metrics::record_compression("zstd", size, compressed_size);
                metadata.insert("__compression__".to_string(), "zstd".to_string());
                metadata.insert("__original_size__".to_string(), size.to_string());
                fs::write(temp_path, &compressed).await?;
            }
            CompressionMode::Lz4 => {
                let raw_data = fs::read(temp_path).await?;
                let compressed = lz4_flex::compress_prepend_size(&raw_data);
                let compressed_size = compressed.len() as u64;
                crate::metrics::record_compression("lz4", size, compressed_size);
                metadata.insert("__compression__".to_string(), "lz4".to_string());
                metadata.insert("__original_size__".to_string(), size.to_string());
                fs::write(temp_path, &compressed).await?;
            }
            CompressionMode::None => {}
        }

        // Atomic rename from temp path to final object path.
        if let Err(e) = fs::rename(temp_path, &object_path).await {
            let _ = fs::remove_file(temp_path).await;
            return Err(StorageError::Io(e));
        }

        let ct = content_type.unwrap_or_else(|| "application/octet-stream".to_string());
        let obj_metadata = ObjectMetadata {
            key: key.to_string(),
            size,
            etag: etag.clone(),
            last_modified: Utc::now(),
            content_type: ct,
            metadata,
            schema_version: 1,
        };
        self.save_metadata(bucket, key, &obj_metadata).await?;

        // Record versioning if enabled.
        if let Ok(versioning_cfg) = self.versioning_manager.get_config(bucket).await {
            match versioning_cfg.status {
                crate::storage::VersioningStatus::Enabled
                | crate::storage::VersioningStatus::Suspended => {
                    let version = crate::storage::versioning::ObjectVersionMetadata::new(
                        key.to_string(),
                        obj_metadata.size,
                        etag.clone(),
                    );
                    if let Err(e) = self
                        .versioning_manager
                        .add_version(bucket, key.to_string(), version)
                        .await
                    {
                        error!("Failed to record version for {}/{}: {}", bucket, key, e);
                    }
                }
                crate::storage::VersioningStatus::Unversioned => {}
            }
        }

        crate::metrics::record_object_size(bucket, size);
        info!("Put object from path: {}/{} ({} bytes)", bucket, key, size);
        Ok(etag)
    }

    /// Delete an object
    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let object_path = self.object_path(bucket, key);
        let metadata_path = self.metadata_path(bucket, key);
        let sci_metadata_path = self.sci_metadata_path(bucket, key);
        let tagging_path = self.tagging_path(bucket, key);
        if object_path.exists() {
            fs::remove_file(&object_path).await?;
            // Clean up empty parent directories within the objects sub-tree to
            // avoid leaving orphan directories when keys contain `/` separators.
            let objects_root = self.bucket_path(bucket).join("objects");
            if let Some(mut parent) = object_path.parent() {
                while parent != objects_root {
                    // Attempt to remove the directory — this is a no-op if not empty.
                    if fs::remove_dir(parent).await.is_err() {
                        break;
                    }
                    match parent.parent() {
                        Some(p) => parent = p,
                        None => break,
                    }
                }
            }
        } else {
            return Err(StorageError::NotFound(format!(
                "Object '{}/{}' not found",
                bucket, key
            )));
        }
        if metadata_path.exists() {
            let _ = fs::remove_file(&metadata_path).await;
            // Clean up empty parent directories within the metadata sub-tree.
            let metadata_root = self.bucket_path(bucket).join("metadata");
            if let Some(mut parent) = metadata_path.parent() {
                while parent != metadata_root {
                    if fs::remove_dir(parent).await.is_err() {
                        break;
                    }
                    match parent.parent() {
                        Some(p) => parent = p,
                        None => break,
                    }
                }
            }
        }
        if sci_metadata_path.exists() {
            let _ = fs::remove_file(&sci_metadata_path).await;
        }
        if tagging_path.exists() {
            let _ = fs::remove_file(&tagging_path).await;
        }
        info!("Deleted object: {}/{}", bucket, key);
        Ok(())
    }
    /// Copy an object
    pub async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        metadata_directive: Option<&str>,
        new_metadata: Option<HashMap<String, String>>,
        new_content_type: Option<&str>,
    ) -> Result<ObjectMetadata, StorageError> {
        let src_metadata = self.head_object(src_bucket, src_key).await?;
        let src_path = self.object_path(src_bucket, src_key);
        let data = fs::read(&src_path).await?;
        let (content_type, metadata) = if metadata_directive == Some("REPLACE") {
            (
                new_content_type.unwrap_or("application/octet-stream"),
                new_metadata.unwrap_or_default(),
            )
        } else {
            (
                src_metadata.content_type.as_str(),
                src_metadata.metadata.clone(),
            )
        };
        let _etag = self
            .put_object(
                dst_bucket,
                dst_key,
                content_type,
                metadata,
                Bytes::from(data),
            )
            .await?;
        self.head_object(dst_bucket, dst_key).await
    }
    async fn load_metadata(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError> {
        let path = self.metadata_path(bucket, key);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Metadata for '{}/{}' not found",
                bucket, key
            )));
        }
        let data = fs::read(&path).await?;
        let metadata: ObjectMetadata = serde_json::from_slice(&data).map_err(|e| {
            error!("Failed to parse metadata for {}/{}: {}", bucket, key, e);
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        Ok(metadata)
    }
    async fn save_metadata(
        &self,
        bucket: &str,
        key: &str,
        metadata: &ObjectMetadata,
    ) -> Result<(), StorageError> {
        let path = self.metadata_path(bucket, key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_vec_pretty(metadata).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;

        // Atomic write: write to tmp file then rename into place so that readers
        // never observe a partially-written metadata file.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp_path = PathBuf::from(format!("{}.tmp.{}", path.display(), nanos));

        let write_result: Result<(), StorageError> = async {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp_path)
                .await?;
            file.write_all(&data).await?;
            Ok(())
        }
        .await;

        if let Err(e) = write_result {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(e);
        }

        if let Err(e) = fs::rename(&tmp_path, &path).await {
            let _ = fs::remove_file(&tmp_path).await;
            return Err(StorageError::Io(e));
        }

        Ok(())
    }
    /// Get scientific metadata
    pub async fn get_scientific_metadata(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<SciMetadata>, StorageError> {
        let path = self.sci_metadata_path(bucket, key);
        if !path.exists() {
            return Ok(None);
        }
        let data = fs::read(&path).await?;
        let metadata: SciMetadata = serde_json::from_slice(&data).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        Ok(Some(metadata))
    }
    /// Get storage statistics
    pub async fn get_storage_stats(&self) -> Result<StorageStats, StorageError> {
        let buckets = self.list_buckets().await?;
        let bucket_count = buckets.len() as u64;
        let mut object_count = 0u64;
        let mut total_size_bytes = 0u64;
        for bucket in buckets {
            let (objects, _) = self
                .list_objects(&bucket.name, "", None, usize::MAX)
                .await?;
            object_count += objects.len() as u64;
            total_size_bytes += objects.iter().map(|o| o.size).sum::<u64>();
        }
        Ok(StorageStats {
            bucket_count,
            object_count,
            total_size_bytes,
        })
    }
    /// Get object tagging
    pub async fn get_object_tagging(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<ObjectTagging, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.tagging_path(bucket, key);
        if !path.exists() {
            return Ok(ObjectTagging::default());
        }
        let data = fs::read(&path).await?;
        let tagging: ObjectTagging = serde_json::from_slice(&data).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        Ok(tagging)
    }
    /// Put object tagging
    pub async fn put_object_tagging(
        &self,
        bucket: &str,
        key: &str,
        tagging: &ObjectTagging,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.head_object(bucket, key).await?;
        let path = self.tagging_path(bucket, key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_vec_pretty(tagging).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(&path, data).await?;
        Ok(())
    }
    /// Delete object tagging
    pub async fn delete_object_tagging(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.tagging_path(bucket, key);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }
    /// Get bucket tagging
    pub async fn get_bucket_tagging(&self, bucket: &str) -> Result<ObjectTagging, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_tagging_path(bucket);
        if !path.exists() {
            return Ok(ObjectTagging::default());
        }
        let data = fs::read(&path).await?;
        let tagging: ObjectTagging = serde_json::from_slice(&data).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        Ok(tagging)
    }
    /// Put bucket tagging
    pub async fn put_bucket_tagging(
        &self,
        bucket: &str,
        tagging: &ObjectTagging,
    ) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_tagging_path(bucket);
        let data = serde_json::to_vec_pretty(tagging).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(&path, data).await?;
        Ok(())
    }
    /// Delete bucket tagging
    pub async fn delete_bucket_tagging(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_tagging_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }
    /// Get bucket policy
    pub async fn get_bucket_policy(&self, bucket: &str) -> Result<String, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_policy_path(bucket);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "Bucket policy for '{}' not found",
                bucket
            )));
        }
        let policy = fs::read_to_string(&path).await?;
        Ok(policy)
    }
    /// Put bucket policy
    pub async fn put_bucket_policy(&self, bucket: &str, policy: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_policy_path(bucket);
        fs::write(&path, policy).await?;
        Ok(())
    }
    /// Delete bucket policy
    pub async fn delete_bucket_policy(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let path = self.bucket_policy_path(bucket);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }
    /// Create a multipart upload
    pub async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        content_type: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let upload_id = uuid::Uuid::new_v4().to_string();
        let multipart_metadata = MultipartMetadata {
            bucket: bucket.to_string(),
            key: key.to_string(),
            upload_id: upload_id.clone(),
            content_type: content_type.to_string(),
            metadata,
            initiated: Utc::now(),
            parts: HashMap::new(),
        };
        let multipart_dir = self.multipart_path(bucket, &upload_id);
        fs::create_dir_all(&multipart_dir).await?;
        let metadata_path = self.multipart_metadata_path(bucket, &upload_id);
        let data = serde_json::to_vec_pretty(&multipart_metadata).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(&metadata_path, data).await?;
        info!(
            "Created multipart upload: {}/{} ({})",
            bucket, key, upload_id
        );
        Ok(upload_id)
    }
    /// Upload a part
    pub async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<String, StorageError> {
        if !(1..=10000).contains(&part_number) {
            return Err(StorageError::InvalidPartNumber);
        }
        let metadata_path = self.multipart_metadata_path(bucket, upload_id);
        if !metadata_path.exists() {
            return Err(StorageError::MultipartNotFound);
        }
        let part_path = self.multipart_part_path(bucket, upload_id, part_number);
        fs::write(&part_path, &data).await?;
        let etag = format!("{:x}", sha2::Sha256::digest(&data));
        let metadata_data = fs::read(&metadata_path).await?;
        let mut multipart_metadata: MultipartMetadata = serde_json::from_slice(&metadata_data)
            .map_err(|e| {
                StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
        multipart_metadata.parts.insert(
            part_number,
            PartMetadata {
                part_number,
                etag: etag.clone(),
                size: data.len() as u64,
                last_modified: Utc::now(),
            },
        );
        let data = serde_json::to_vec_pretty(&multipart_metadata).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(&metadata_path, data).await?;
        info!(
            "Uploaded part: {}/{} ({}) part {}",
            bucket, key, upload_id, part_number
        );
        Ok(etag)
    }
    /// Upload a part by copying from another object
    pub async fn upload_part_copy(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: u32,
        src_bucket: &str,
        src_key: &str,
        range: Option<ByteRange>,
    ) -> Result<(String, DateTime<Utc>), StorageError> {
        if !(1..=10000).contains(&part_number) {
            return Err(StorageError::InvalidPartNumber);
        }
        let metadata_path = self.multipart_metadata_path(bucket, upload_id);
        if !metadata_path.exists() {
            return Err(StorageError::MultipartNotFound);
        }
        let src_path = self.object_path(src_bucket, src_key);
        let mut data = fs::read(&src_path).await?;
        if let Some(r) = range {
            if r.end >= data.len() as u64 {
                return Err(StorageError::InvalidRange);
            }
            data = data[(r.start as usize)..=(r.end as usize)].to_vec();
        }
        let data = Bytes::from(data);
        let etag = self
            .upload_part(bucket, key, upload_id, part_number, data)
            .await?;
        Ok((etag, Utc::now()))
    }
    /// Complete a multipart upload
    pub async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<String, StorageError> {
        let metadata_path = self.multipart_metadata_path(bucket, upload_id);
        if !metadata_path.exists() {
            return Err(StorageError::MultipartNotFound);
        }
        let metadata_data = fs::read(&metadata_path).await?;
        let multipart_metadata: MultipartMetadata = serde_json::from_slice(&metadata_data)
            .map_err(|e| {
                StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
        for (part_number, expected_etag) in parts {
            let part_meta = multipart_metadata.parts.get(part_number).ok_or_else(|| {
                StorageError::InvalidPart(format!("Part {} was not uploaded", part_number))
            })?;
            let stored_etag = part_meta.etag.trim_matches('"').trim().to_lowercase();
            let expected_etag_normalized = expected_etag.trim_matches('"').trim().to_lowercase();
            if stored_etag != expected_etag_normalized {
                return Err(StorageError::InvalidPart(format!(
                    "ETag mismatch for part {}: stored={}, expected={}",
                    part_number, stored_etag, expected_etag_normalized
                )));
            }
        }
        let object_path = self.object_path(bucket, key);
        if let Some(parent) = object_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut output_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&object_path)
            .await?;
        let mut total_size = 0u64;
        for (part_number, _) in parts {
            let part_path = self.multipart_part_path(bucket, upload_id, *part_number);
            let part_data = fs::read(&part_path).await?;
            output_file.write_all(&part_data).await?;
            total_size += part_data.len() as u64;
        }
        output_file.sync_all().await?;
        let final_data = fs::read(&object_path).await?;
        let etag = format!("{:x}", sha2::Sha256::digest(&final_data));
        let obj_metadata = ObjectMetadata {
            key: key.to_string(),
            size: total_size,
            etag: etag.clone(),
            last_modified: Utc::now(),
            content_type: multipart_metadata.content_type.clone(),
            metadata: multipart_metadata.metadata.clone(),
            schema_version: 1,
        };
        self.save_metadata(bucket, key, &obj_metadata).await?;
        let multipart_dir = self.multipart_path(bucket, upload_id);
        let _ = fs::remove_dir_all(&multipart_dir).await;
        info!(
            "Completed multipart upload: {}/{} ({})",
            bucket, key, upload_id
        );
        Ok(etag)
    }
    /// Abort a multipart upload
    pub async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<(), StorageError> {
        let multipart_dir = self.multipart_path(bucket, upload_id);
        if !multipart_dir.exists() {
            return Err(StorageError::MultipartNotFound);
        }
        fs::remove_dir_all(&multipart_dir).await?;
        info!(
            "Aborted multipart upload: {}/{} ({})",
            bucket, key, upload_id
        );
        Ok(())
    }
    /// List parts for a multipart upload
    pub async fn list_parts(
        &self,
        bucket: &str,
        _key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError> {
        let metadata_path = self.multipart_metadata_path(bucket, upload_id);
        if !metadata_path.exists() {
            return Err(StorageError::MultipartNotFound);
        }
        let metadata_data = fs::read(&metadata_path).await?;
        let multipart_metadata: MultipartMetadata = serde_json::from_slice(&metadata_data)
            .map_err(|e| {
                StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            })?;
        let mut parts: Vec<PartMetadata> = multipart_metadata.parts.values().cloned().collect();
        parts.sort_by_key(|p| p.part_number);
        Ok(parts)
    }
    /// List multipart uploads
    pub async fn list_multipart_uploads(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        let multipart_dir = self.bucket_path(bucket).join("multipart");
        if !multipart_dir.exists() {
            return Ok(Vec::new());
        }
        let mut uploads = Vec::new();
        let mut entries = fs::read_dir(&multipart_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let upload_id = entry.file_name().to_string_lossy().to_string();
            let metadata_path = self.multipart_metadata_path(bucket, &upload_id);
            if metadata_path.exists() {
                let metadata_data = fs::read(&metadata_path).await?;
                if let Ok(multipart_metadata) =
                    serde_json::from_slice::<MultipartMetadata>(&metadata_data)
                {
                    if let Some(p) = prefix {
                        if !multipart_metadata.key.starts_with(p) {
                            continue;
                        }
                    }
                    uploads.push(MultipartUpload {
                        key: multipart_metadata.key,
                        upload_id: multipart_metadata.upload_id,
                        initiated: multipart_metadata.initiated,
                    });
                }
            }
        }
        Ok(uploads)
    }
    /// Garbage-collect abandoned multipart uploads older than `retention_hours` hours.
    ///
    /// Scans all buckets (or just `bucket` if `Some`) and removes upload directories whose
    /// `initiated` timestamp is older than the retention window.
    ///
    /// Returns the total number of uploads removed.
    pub async fn gc_abandoned_multipart(
        &self,
        bucket: Option<&str>,
        retention_hours: u64,
    ) -> Result<u64, StorageError> {
        let cutoff = Utc::now() - chrono::Duration::hours(retention_hours as i64);

        let buckets: Vec<String> = match bucket {
            Some(b) => {
                if !self.bucket_exists(b).await? {
                    return Err(StorageError::BucketNotFound);
                }
                vec![b.to_string()]
            }
            None => self
                .list_buckets()
                .await?
                .into_iter()
                .map(|m| m.name)
                .collect(),
        };

        let mut removed: u64 = 0;

        for bucket_name in &buckets {
            let multipart_dir = self.bucket_path(bucket_name).join("multipart");
            if !multipart_dir.exists() {
                continue;
            }

            let mut entries = match fs::read_dir(&multipart_dir).await {
                Ok(e) => e,
                Err(_) => continue,
            };

            while let Ok(Some(entry)) = entries.next_entry().await {
                let upload_id = entry.file_name().to_string_lossy().to_string();
                let metadata_path = self.multipart_metadata_path(bucket_name, &upload_id);
                if !metadata_path.exists() {
                    continue;
                }
                let metadata_data = match fs::read(&metadata_path).await {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let mm: MultipartMetadata = match serde_json::from_slice(&metadata_data) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if mm.initiated < cutoff {
                    let upload_dir = self.multipart_path(bucket_name, &upload_id);
                    if fs::remove_dir_all(&upload_dir).await.is_ok() {
                        removed += 1;
                        info!(
                            "GC: removed abandoned multipart upload {}/{} ({})",
                            bucket_name, mm.key, upload_id
                        );
                    }
                }
            }
        }

        Ok(removed)
    }

    /// Enable versioning for a bucket
    pub async fn enable_bucket_versioning(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.versioning_manager.enable_versioning(bucket).await
    }
    /// Suspend versioning for a bucket
    pub async fn suspend_bucket_versioning(&self, bucket: &str) -> Result<(), StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.versioning_manager.suspend_versioning(bucket).await
    }
    /// Get versioning configuration for a bucket
    pub async fn get_bucket_versioning(
        &self,
        bucket: &str,
    ) -> Result<BucketVersioningConfig, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.versioning_manager.get_config(bucket).await
    }
    /// Get the versioning manager (for advanced operations)
    pub fn versioning_manager(&self) -> Arc<VersioningManager> {
        Arc::clone(&self.versioning_manager)
    }
    /// Get all versions of an object
    pub async fn list_object_versions(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Vec<ObjectVersionMetadata>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.versioning_manager
            .list_object_versions(bucket, key)
            .await
    }
    /// Get a specific version of an object
    pub async fn get_object_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<Option<ObjectVersionMetadata>, StorageError> {
        if !self.bucket_exists(bucket).await? {
            return Err(StorageError::BucketNotFound);
        }
        self.versioning_manager
            .get_version(bucket, key, version_id)
            .await
    }
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
/// Multipart upload info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultipartUpload {
    pub key: String,
    pub upload_id: String,
    pub initiated: DateTime<Utc>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CompressionMode {
    #[default]
    None,
    Zstd(i32),
    Lz4,
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
/// Bucket metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketMetadata {
    pub name: String,
    pub creation_date: DateTime<Utc>,
}
/// Multipart upload metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MultipartMetadata {
    pub bucket: String,
    pub key: String,
    pub upload_id: String,
    pub content_type: String,
    pub metadata: HashMap<String, String>,
    pub initiated: DateTime<Utc>,
    pub parts: HashMap<u32, PartMetadata>,
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
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => StorageError::AccessDenied,
            std::io::ErrorKind::StorageFull => StorageError::InsufficientStorage,
            _ => StorageError::Io(e),
        }
    }
}
