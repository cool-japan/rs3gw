//! # LocalBackend - Trait Implementations
//!
//! This module contains trait implementations for `LocalBackend`.
//!
//! ## Implemented Traits
//!
//! - `StorageBackend`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::storage::{
    BucketMetadata, ByteRange, MultipartUpload, ObjectMetadata, ObjectTagging, PartMetadata,
    StorageError, StorageStats,
};
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;

use super::functions::{ByteStream, StorageBackend};
use super::types::{LocalBackend, ObjectListResult};

#[async_trait]
impl StorageBackend for LocalBackend {
    async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError> {
        self.engine.list_buckets().await
    }
    async fn bucket_exists(&self, bucket: &str) -> Result<bool, StorageError> {
        self.engine.bucket_exists(bucket).await
    }
    async fn create_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.engine.create_bucket(bucket).await
    }
    async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.engine.delete_bucket(bucket).await
    }
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        max_keys: usize,
        continuation_token: Option<&str>,
    ) -> Result<ObjectListResult, StorageError> {
        let (objects, common_prefixes, is_truncated) = self
            .engine
            .list_objects_with_pagination(
                bucket,
                prefix.unwrap_or(""),
                delimiter,
                max_keys,
                continuation_token,
            )
            .await?;
        let next_continuation_token = if is_truncated {
            objects.last().map(|obj| obj.key.clone())
        } else {
            None
        };
        let objects: Vec<(String, ObjectMetadata)> = objects
            .into_iter()
            .map(|meta| (meta.key.clone(), meta))
            .collect();
        Ok(ObjectListResult {
            objects,
            common_prefixes,
            is_truncated,
            next_continuation_token,
        })
    }
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError> {
        self.engine.head_object(bucket, key).await
    }
    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, Bytes), StorageError> {
        let (metadata, stream) = match range {
            Some(byte_range) => {
                self.engine
                    .get_object_range(bucket, key, &byte_range)
                    .await?
            }
            None => self.engine.get_object(bucket, key).await?,
        };
        use futures::TryStreamExt;
        let chunks: Vec<Bytes> = stream.try_collect().await?;
        let data = chunks.into_iter().fold(Bytes::new(), |mut acc, chunk| {
            acc = Bytes::from([acc.to_vec(), chunk.to_vec()].concat());
            acc
        });
        Ok((metadata, data))
    }
    async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, ByteStream), StorageError> {
        let (metadata, stream) = match range {
            Some(byte_range) => {
                self.engine
                    .get_object_range(bucket, key, &byte_range)
                    .await?
            }
            None => self.engine.get_object(bucket, key).await?,
        };
        Ok((metadata, Box::pin(stream)))
    }
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata, StorageError> {
        let content_type = metadata
            .get("Content-Type")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let _etag = self
            .engine
            .put_object(bucket, key, &content_type, metadata, data)
            .await?;
        self.engine.head_object(bucket, key).await
    }
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.engine.delete_object(bucket, key).await
    }
    async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<ObjectMetadata, StorageError> {
        let (metadata_directive, new_metadata, new_content_type) = if let Some(meta) = metadata {
            (
                Some("REPLACE"),
                Some(meta),
                Some("application/octet-stream"),
            )
        } else {
            (None, None, None)
        };
        self.engine
            .copy_object(
                src_bucket,
                src_key,
                dst_bucket,
                dst_key,
                metadata_directive,
                new_metadata,
                new_content_type,
            )
            .await
    }
    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String, StorageError> {
        let content_type = metadata
            .get("Content-Type")
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        self.engine
            .create_multipart_upload(bucket, key, &content_type, metadata)
            .await
    }
    async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<String, StorageError> {
        self.engine
            .upload_part(bucket, key, upload_id, part_number, data)
            .await
    }
    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<PartMetadata>,
    ) -> Result<ObjectMetadata, StorageError> {
        let parts_vec: Vec<(u32, String)> = parts
            .iter()
            .map(|p| (p.part_number, p.etag.clone()))
            .collect();
        let _etag = self
            .engine
            .complete_multipart_upload(bucket, key, upload_id, &parts_vec)
            .await?;
        self.engine.head_object(bucket, key).await
    }
    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<(), StorageError> {
        self.engine
            .abort_multipart_upload(bucket, key, upload_id)
            .await
    }
    async fn list_parts(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError> {
        self.engine.list_parts(bucket, key, upload_id).await
    }
    async fn list_multipart_uploads(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError> {
        self.engine.list_multipart_uploads(bucket, prefix).await
    }
    async fn get_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<HashMap<String, String>, StorageError> {
        let tagging = self.engine.get_object_tagging(bucket, key).await?;
        Ok(tagging.tags)
    }
    async fn put_object_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let tagging = ObjectTagging { tags };
        self.engine.put_object_tagging(bucket, key, &tagging).await
    }
    async fn delete_object_tags(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.engine.delete_object_tagging(bucket, key).await
    }
    async fn get_bucket_tags(&self, bucket: &str) -> Result<HashMap<String, String>, StorageError> {
        let tagging = self.engine.get_bucket_tagging(bucket).await?;
        Ok(tagging.tags)
    }
    async fn put_bucket_tags(
        &self,
        bucket: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let tagging = ObjectTagging { tags };
        self.engine.put_bucket_tagging(bucket, &tagging).await
    }
    async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), StorageError> {
        self.engine.delete_bucket_tagging(bucket).await
    }
    async fn get_bucket_policy(&self, bucket: &str) -> Result<String, StorageError> {
        self.engine.get_bucket_policy(bucket).await
    }
    async fn put_bucket_policy(&self, bucket: &str, policy: String) -> Result<(), StorageError> {
        self.engine.put_bucket_policy(bucket, &policy).await
    }
    async fn delete_bucket_policy(&self, bucket: &str) -> Result<(), StorageError> {
        self.engine.delete_bucket_policy(bucket).await
    }
    async fn get_storage_stats(&self) -> Result<StorageStats, StorageError> {
        self.engine.get_storage_stats().await
    }
}
