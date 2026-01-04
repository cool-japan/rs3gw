//! # AzureBackend - Trait Implementations
//!
//! This module contains trait implementations for `AzureBackend`.
//!
//! ## Implemented Traits
//!
//! - `StorageBackend`
//!
//! This provides a stub implementation of Azure Blob Storage integration.
//! Azure concepts map to S3 concepts:
//! - Containers → Buckets
//! - Blobs → Objects
//! - Blob Metadata → Object Metadata
//! - Block Blobs → Multipart Uploads
//!
//! **NOTE**: This implementation is currently a stub pending Azure SDK stabilization.
//! All methods return NotImplemented errors. Tracked in TODO.md v4.0.0.

use crate::storage::{
    BucketMetadata, ByteRange, MultipartUpload, ObjectMetadata, PartMetadata, StorageError,
    StorageStats,
};
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;

use super::functions::{ByteStream, StorageBackend};
use super::types::{AzureBackend, ObjectListResult};

#[async_trait]
impl StorageBackend for AzureBackend {
    async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn bucket_exists(&self, _bucket: &str) -> Result<bool, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn create_bucket(&self, _bucket: &str) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn delete_bucket(&self, _bucket: &str) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn list_objects(
        &self,
        _bucket: &str,
        _prefix: Option<&str>,
        _delimiter: Option<&str>,
        _max_keys: usize,
        _continuation_token: Option<&str>,
    ) -> Result<ObjectListResult, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn head_object(&self, _bucket: &str, _key: &str) -> Result<ObjectMetadata, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn get_object(
        &self,
        _bucket: &str,
        _key: &str,
        _range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, Bytes), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn get_object_stream(
        &self,
        _bucket: &str,
        _key: &str,
        _range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, ByteStream), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn put_object(
        &self,
        _bucket: &str,
        _key: &str,
        _data: Bytes,
        _metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn delete_object(&self, _bucket: &str, _key: &str) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn copy_object(
        &self,
        _src_bucket: &str,
        _src_key: &str,
        _dst_bucket: &str,
        _dst_key: &str,
        _metadata: Option<HashMap<String, String>>,
    ) -> Result<ObjectMetadata, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn create_multipart_upload(
        &self,
        _bucket: &str,
        _key: &str,
        _metadata: HashMap<String, String>,
    ) -> Result<String, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn upload_part(
        &self,
        _bucket: &str,
        _key: &str,
        _upload_id: &str,
        _part_number: u32,
        _data: Bytes,
    ) -> Result<String, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn complete_multipart_upload(
        &self,
        _bucket: &str,
        _key: &str,
        _upload_id: &str,
        _parts: Vec<PartMetadata>,
    ) -> Result<ObjectMetadata, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn abort_multipart_upload(
        &self,
        _bucket: &str,
        _key: &str,
        _upload_id: &str,
    ) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn list_parts(
        &self,
        _bucket: &str,
        _key: &str,
        _upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn list_multipart_uploads(
        &self,
        _bucket: &str,
        _prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn get_object_tags(
        &self,
        _bucket: &str,
        _key: &str,
    ) -> Result<HashMap<String, String>, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn put_object_tags(
        &self,
        _bucket: &str,
        _key: &str,
        _tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn delete_object_tags(&self, _bucket: &str, _key: &str) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn get_bucket_tags(
        &self,
        _bucket: &str,
    ) -> Result<HashMap<String, String>, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn put_bucket_tags(
        &self,
        _bucket: &str,
        _tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn delete_bucket_tags(&self, _bucket: &str) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn get_bucket_policy(&self, _bucket: &str) -> Result<String, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn put_bucket_policy(&self, _bucket: &str, _policy: String) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn delete_bucket_policy(&self, _bucket: &str) -> Result<(), StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }

    async fn get_storage_stats(&self) -> Result<StorageStats, StorageError> {
        Err(StorageError::Internal(
            "Azure backend not yet implemented - pending SDK stabilization".to_string(),
        ))
    }
}
