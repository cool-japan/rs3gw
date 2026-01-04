//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::storage::{
    BucketMetadata, ByteRange, MultipartUpload, ObjectMetadata, PartMetadata, StorageError,
    StorageStats,
};
use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::Stream;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use super::types::ObjectListResult;

/// Stream of bytes for object data
pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>;
/// Storage backend trait
///
/// This trait abstracts all storage operations, allowing different backend implementations
/// such as local filesystem, MinIO, Ceph, cloud storage, etc.
#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// List all buckets
    async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError>;
    /// Check if a bucket exists
    async fn bucket_exists(&self, bucket: &str) -> Result<bool, StorageError>;
    /// Create a new bucket
    async fn create_bucket(&self, bucket: &str) -> Result<(), StorageError>;
    /// Delete an empty bucket
    async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError>;
    /// List objects in a bucket with optional prefix and pagination
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        max_keys: usize,
        continuation_token: Option<&str>,
    ) -> Result<ObjectListResult, StorageError>;
    /// Get object metadata
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError>;
    /// Get object data (full or range)
    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, Bytes), StorageError>;
    /// Get object as a stream
    async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, ByteStream), StorageError>;
    /// Put object
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata, StorageError>;
    /// Delete object
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError>;
    /// Copy object (server-side copy)
    async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<ObjectMetadata, StorageError>;
    /// Initiate multipart upload
    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String, StorageError>;
    /// Upload a part
    async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<String, StorageError>;
    /// Complete multipart upload
    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<PartMetadata>,
    ) -> Result<ObjectMetadata, StorageError>;
    /// Abort multipart upload
    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<(), StorageError>;
    /// List parts of a multipart upload
    async fn list_parts(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError>;
    /// List multipart uploads
    async fn list_multipart_uploads(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError>;
    /// Get object tags
    async fn get_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<HashMap<String, String>, StorageError>;
    /// Put object tags
    async fn put_object_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError>;
    /// Delete object tags
    async fn delete_object_tags(&self, bucket: &str, key: &str) -> Result<(), StorageError>;
    /// Get bucket tags
    async fn get_bucket_tags(&self, bucket: &str) -> Result<HashMap<String, String>, StorageError>;
    /// Put bucket tags
    async fn put_bucket_tags(
        &self,
        bucket: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError>;
    /// Delete bucket tags
    async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), StorageError>;
    /// Get bucket policy
    async fn get_bucket_policy(&self, bucket: &str) -> Result<String, StorageError>;
    /// Put bucket policy
    async fn put_bucket_policy(&self, bucket: &str, policy: String) -> Result<(), StorageError>;
    /// Delete bucket policy
    async fn delete_bucket_policy(&self, bucket: &str) -> Result<(), StorageError>;
    /// Get storage statistics
    async fn get_storage_stats(&self) -> Result<StorageStats, StorageError>;
}
pub fn default_true() -> bool {
    true
}
/// Type alias for boxed backend trait object
pub type DynBackend = Arc<dyn StorageBackend>;

/// Create a storage backend from configuration
///
/// This factory function instantiates the appropriate backend implementation
/// based on the `backend_type` field in the configuration.
///
/// # Arguments
/// * `config` - Backend configuration specifying the type and connection details
/// * `storage_root` - Optional local storage root path (required for LocalBackend)
///
/// # Returns
/// A dynamic backend trait object that can be used for all storage operations
///
/// # Errors
/// Returns `StorageError` if:
/// - The backend configuration is invalid
/// - Required configuration parameters are missing
/// - The backend cannot be initialized
///
/// # Example
/// ```no_run
/// use rs3gw::storage::backend::{create_backend_from_config, BackendConfig, BackendType};
/// use std::path::PathBuf;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = BackendConfig {
///     backend_type: BackendType::Local,
///     endpoint: None,
///     access_key: None,
///     secret_key: None,
///     region: None,
///     use_ssl: true,
///     extra: Default::default(),
/// };
///
/// let backend = create_backend_from_config(config, Some(PathBuf::from("./data"))).await?;
/// # Ok(())
/// # }
/// ```
pub async fn create_backend_from_config(
    config: super::types::BackendConfig,
    storage_root: Option<std::path::PathBuf>,
) -> Result<DynBackend, StorageError> {
    use super::types::*;
    use crate::storage::StorageEngine;

    match config.backend_type {
        BackendType::Local => {
            let root = storage_root.ok_or_else(|| {
                StorageError::Internal("storage_root required for LocalBackend".to_string())
            })?;

            let engine = StorageEngine::new(root)?;
            let backend = LocalBackend::new(Arc::new(engine));
            Ok(Arc::new(backend) as DynBackend)
        }
        BackendType::MinIO => {
            let backend = MinIOBackend::new(config).await?;
            Ok(Arc::new(backend) as DynBackend)
        }
        BackendType::S3 => {
            let backend = S3Backend::new(config).await?;
            Ok(Arc::new(backend) as DynBackend)
        }
        BackendType::Gcs => {
            let backend = GcsBackend::new(config).await?;
            Ok(Arc::new(backend) as DynBackend)
        }
        BackendType::Azure => {
            let backend = AzureBackend::new(config).await?;
            Ok(Arc::new(backend) as DynBackend)
        }
        BackendType::Ceph => {
            let backend = CephBackend::new(config).await?;
            Ok(Arc::new(backend) as DynBackend)
        }
        BackendType::GlusterFS => {
            let backend = GlusterBackend::new(config).await?;
            Ok(Arc::new(backend) as DynBackend)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::backend::types::LocalBackend;
    use crate::storage::StorageEngine;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;
    async fn create_test_backend() -> (LocalBackend, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let engine = StorageEngine::new(PathBuf::from(temp_dir.path()))
            .expect("Failed to create storage engine");
        let backend = LocalBackend::new(Arc::new(engine));
        (backend, temp_dir)
    }
    #[tokio::test]
    async fn test_local_backend_bucket_operations() {
        let (backend, _temp_dir) = create_test_backend().await;
        backend
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        assert!(
            backend
                .bucket_exists("test-bucket")
                .await
                .expect("Failed to check bucket"),
            "Bucket should exist"
        );
        let buckets = backend
            .list_buckets()
            .await
            .expect("Failed to list buckets");
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "test-bucket");
        backend
            .delete_bucket("test-bucket")
            .await
            .expect("Failed to delete bucket");
        assert!(
            !backend
                .bucket_exists("test-bucket")
                .await
                .expect("Failed to check bucket"),
            "Bucket should not exist"
        );
    }
    #[tokio::test]
    async fn test_local_backend_object_operations() {
        let (backend, _temp_dir) = create_test_backend().await;
        backend
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        let data = Bytes::from("Hello, World!");
        let metadata = HashMap::from([("custom-key".to_string(), "custom-value".to_string())]);
        let obj_metadata = backend
            .put_object("test-bucket", "test-key", data.clone(), metadata.clone())
            .await
            .expect("Failed to put object");
        assert_eq!(obj_metadata.size, data.len() as u64);
        assert_eq!(obj_metadata.key, "test-key");
        let head_meta = backend
            .head_object("test-bucket", "test-key")
            .await
            .expect("Failed to head object");
        assert_eq!(head_meta.size, data.len() as u64);
        let (get_meta, get_data) = backend
            .get_object("test-bucket", "test-key", None)
            .await
            .expect("Failed to get object");
        assert_eq!(get_meta.size, data.len() as u64);
        assert_eq!(get_data, data);
        let result = backend
            .list_objects("test-bucket", None, None, 10, None)
            .await
            .expect("Failed to list objects");
        assert_eq!(result.objects.len(), 1);
        assert_eq!(result.objects[0].0, "test-key");
        backend
            .delete_object("test-bucket", "test-key")
            .await
            .expect("Failed to delete object");
    }
    #[tokio::test]
    async fn test_local_backend_tagging() {
        let (backend, _temp_dir) = create_test_backend().await;
        backend
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        let data = Bytes::from("test data");
        backend
            .put_object("test-bucket", "test-key", data, HashMap::new())
            .await
            .expect("Failed to put object");
        let tags = HashMap::from([
            ("env".to_string(), "test".to_string()),
            ("project".to_string(), "rs3gw".to_string()),
        ]);
        backend
            .put_object_tags("test-bucket", "test-key", tags.clone())
            .await
            .expect("Failed to put tags");
        let retrieved_tags = backend
            .get_object_tags("test-bucket", "test-key")
            .await
            .expect("Failed to get tags");
        assert_eq!(retrieved_tags.len(), 2);
        assert_eq!(retrieved_tags.get("env"), Some(&"test".to_string()));
        assert_eq!(retrieved_tags.get("project"), Some(&"rs3gw".to_string()));
        backend
            .delete_object_tags("test-bucket", "test-key")
            .await
            .expect("Failed to delete tags");
    }
    #[tokio::test]
    async fn test_local_backend_multipart_upload() {
        let (backend, _temp_dir) = create_test_backend().await;
        backend
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        let upload_id = backend
            .create_multipart_upload("test-bucket", "large-file", HashMap::new())
            .await
            .expect("Failed to create multipart upload");
        let part1_data = Bytes::from("part1");
        let _etag1 = backend
            .upload_part("test-bucket", "large-file", &upload_id, 1, part1_data)
            .await
            .expect("Failed to upload part 1");
        let part2_data = Bytes::from("part2");
        let _etag2 = backend
            .upload_part("test-bucket", "large-file", &upload_id, 2, part2_data)
            .await
            .expect("Failed to upload part 2");
        let parts = backend
            .list_parts("test-bucket", "large-file", &upload_id)
            .await
            .expect("Failed to list parts");
        assert_eq!(parts.len(), 2);
        let final_meta = backend
            .complete_multipart_upload("test-bucket", "large-file", &upload_id, parts)
            .await
            .expect("Failed to complete multipart upload");
        assert_eq!(final_meta.key, "large-file");
        assert_eq!(final_meta.size, 10);
    }
    #[tokio::test]
    async fn test_local_backend_copy_object() {
        let (backend, _temp_dir) = create_test_backend().await;
        backend
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        let data = Bytes::from("test data");
        backend
            .put_object("test-bucket", "source", data.clone(), HashMap::new())
            .await
            .expect("Failed to put source object");
        let copied_meta = backend
            .copy_object("test-bucket", "source", "test-bucket", "destination", None)
            .await
            .expect("Failed to copy object");
        assert_eq!(copied_meta.key, "destination");
        assert_eq!(copied_meta.size, data.len() as u64);
        let (_, copied_data) = backend
            .get_object("test-bucket", "destination", None)
            .await
            .expect("Failed to get copied object");
        assert_eq!(copied_data, data);
    }
    #[tokio::test]
    async fn test_local_backend_get_object_stream() {
        let (backend, _temp_dir) = create_test_backend().await;
        backend
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        let data = Bytes::from("streaming test data");
        backend
            .put_object("test-bucket", "stream-key", data.clone(), HashMap::new())
            .await
            .expect("Failed to put object");
        let (meta, stream) = backend
            .get_object_stream("test-bucket", "stream-key", None)
            .await
            .expect("Failed to get object stream");
        assert_eq!(meta.size, data.len() as u64);
        use futures::TryStreamExt;
        let chunks: Vec<Bytes> = stream
            .try_collect()
            .await
            .expect("Failed to collect stream");
        let collected_data = chunks.into_iter().fold(Bytes::new(), |mut acc, chunk| {
            acc = Bytes::from([acc.to_vec(), chunk.to_vec()].concat());
            acc
        });
        assert_eq!(collected_data, data);
    }

    #[tokio::test]
    async fn test_backend_factory_local() {
        use super::super::types::{BackendConfig, BackendType};

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let config = BackendConfig {
            backend_type: BackendType::Local,
            endpoint: None,
            access_key: None,
            secret_key: None,
            region: None,
            use_ssl: true,
            extra: Default::default(),
        };

        let backend =
            super::create_backend_from_config(config, Some(temp_dir.path().to_path_buf()))
                .await
                .expect("Failed to create backend");

        // Test basic operations
        backend
            .create_bucket("factory-test")
            .await
            .expect("Failed to create bucket");

        assert!(
            backend
                .bucket_exists("factory-test")
                .await
                .expect("Failed to check bucket"),
            "Bucket should exist"
        );

        let buckets = backend
            .list_buckets()
            .await
            .expect("Failed to list buckets");
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].name, "factory-test");
    }

    #[tokio::test]
    async fn test_backend_factory_no_storage_root() {
        use super::super::types::{BackendConfig, BackendType};

        let config = BackendConfig {
            backend_type: BackendType::Local,
            endpoint: None,
            access_key: None,
            secret_key: None,
            region: None,
            use_ssl: true,
            extra: Default::default(),
        };

        let result = super::create_backend_from_config(config, None).await;
        assert!(
            result.is_err(),
            "Should fail without storage_root for LocalBackend"
        );

        if let Err(e) = result {
            assert!(
                format!("{:?}", e).contains("storage_root required"),
                "Error should mention storage_root requirement"
            );
        }
    }
}
