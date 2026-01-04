//! # S3Backend - Trait Implementations
//!
//! This module contains trait implementations for `S3Backend`.
//!
//! ## Implemented Traits
//!
//! - `StorageBackend`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::storage::{
    BucketMetadata, ByteRange, MultipartUpload, ObjectMetadata, PartMetadata, StorageError,
    StorageStats,
};
use async_trait::async_trait;
use bytes::Bytes;
use std::collections::HashMap;

use super::functions::{ByteStream, StorageBackend};
use super::types::{ObjectListResult, S3Backend};

#[async_trait]
impl StorageBackend for S3Backend {
    async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError> {
        let output = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 list_buckets failed: {}", e)))?;
        let buckets = output
            .buckets()
            .iter()
            .map(|b| BucketMetadata {
                name: b.name().unwrap_or_default().to_string(),
                creation_date: b
                    .creation_date()
                    .and_then(|dt| {
                        chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                            .ok()
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                    })
                    .unwrap_or_else(chrono::Utc::now),
            })
            .collect();
        Ok(buckets)
    }
    async fn bucket_exists(&self, bucket: &str) -> Result<bool, StorageError> {
        match self.client.head_bucket().bucket(bucket).send().await {
            Ok(_) => Ok(true),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("NoSuchBucket") {
                    Ok(false)
                } else {
                    Err(StorageError::Internal(format!(
                        "S3 head_bucket failed: {}",
                        e
                    )))
                }
            }
        }
    }
    async fn create_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.client
            .create_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 create_bucket failed: {}", e)))?;
        Ok(())
    }
    async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.client
            .delete_bucket()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 delete_bucket failed: {}", e)))?;
        Ok(())
    }
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        max_keys: usize,
        continuation_token: Option<&str>,
    ) -> Result<ObjectListResult, StorageError> {
        let mut req = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .max_keys(max_keys as i32);
        if let Some(p) = prefix {
            req = req.prefix(p);
        }
        if let Some(d) = delimiter {
            req = req.delimiter(d);
        }
        if let Some(token) = continuation_token {
            req = req.continuation_token(token);
        }
        let output = req
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 list_objects failed: {}", e)))?;
        let objects = output
            .contents()
            .iter()
            .filter_map(|obj| {
                let key = obj.key()?.to_string();
                let meta = ObjectMetadata {
                    key: key.clone(),
                    size: obj.size().unwrap_or(0) as u64,
                    etag: obj.e_tag().unwrap_or_default().to_string(),
                    last_modified: obj
                        .last_modified()
                        .and_then(|dt| {
                            chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                                .ok()
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                        })
                        .unwrap_or_else(chrono::Utc::now),
                    content_type: "application/octet-stream".to_string(),
                    metadata: HashMap::new(),
                };
                Some((key, meta))
            })
            .collect();
        let common_prefixes = output
            .common_prefixes()
            .iter()
            .filter_map(|cp| cp.prefix().map(|s| s.to_string()))
            .collect();
        Ok(ObjectListResult {
            objects,
            common_prefixes,
            is_truncated: output.is_truncated().unwrap_or(false),
            next_continuation_token: output.next_continuation_token().map(|s| s.to_string()),
        })
    }
    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError> {
        let output = self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("NoSuchKey") {
                    StorageError::NotFound("Object not found".to_string())
                } else {
                    StorageError::Internal(format!("S3 head_object failed: {}", e))
                }
            })?;
        let metadata = output.metadata().cloned().unwrap_or_default();
        Ok(ObjectMetadata {
            key: key.to_string(),
            size: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().unwrap_or_default().to_string(),
            last_modified: output
                .last_modified()
                .and_then(|dt| {
                    chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            content_type: output
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string(),
            metadata,
        })
    }
    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, Bytes), StorageError> {
        let mut req = self.client.get_object().bucket(bucket).key(key);
        if let Some(r) = range {
            let range_str = format!("bytes={}-{}", r.start, r.end);
            req = req.range(range_str);
        }
        let output = req.send().await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("404") || err_str.contains("NoSuchKey") {
                StorageError::NotFound("Object not found".to_string())
            } else {
                StorageError::Internal(format!("S3 get_object failed: {}", e))
            }
        })?;
        let metadata = output.metadata().cloned().unwrap_or_default();
        let obj_meta = ObjectMetadata {
            key: key.to_string(),
            size: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().unwrap_or_default().to_string(),
            last_modified: output
                .last_modified()
                .and_then(|dt| {
                    chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            content_type: output
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string(),
            metadata,
        };
        let body = output.body.collect().await.map_err(|e| {
            StorageError::Internal(format!("S3 get_object body read failed: {}", e))
        })?;
        Ok((obj_meta, body.into_bytes()))
    }
    async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, ByteStream), StorageError> {
        let mut req = self.client.get_object().bucket(bucket).key(key);
        if let Some(r) = range {
            let range_str = format!("bytes={}-{}", r.start, r.end);
            req = req.range(range_str);
        }
        let output = req.send().await.map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("404") || err_str.contains("NoSuchKey") {
                StorageError::NotFound("Object not found".to_string())
            } else {
                StorageError::Internal(format!("S3 get_object failed: {}", e))
            }
        })?;
        let metadata = output.metadata().cloned().unwrap_or_default();
        let obj_meta = ObjectMetadata {
            key: key.to_string(),
            size: output.content_length().unwrap_or(0) as u64,
            etag: output.e_tag().unwrap_or_default().to_string(),
            last_modified: output
                .last_modified()
                .and_then(|dt| {
                    chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            content_type: output
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string(),
            metadata,
        };
        use futures::stream;
        let bytes_result = output.body.collect().await.map_err(|e| {
            StorageError::Internal(format!("S3 get_object_stream body read failed: {}", e))
        })?;
        let bytes = bytes_result.into_bytes();
        let stream = stream::once(async move { Ok(bytes) });
        let boxed: ByteStream = Box::pin(stream);
        Ok((obj_meta, boxed))
    }
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata, StorageError> {
        let body = aws_sdk_s3::primitives::ByteStream::from(data.clone());
        let mut req = self.client.put_object().bucket(bucket).key(key).body(body);
        for (k, v) in &metadata {
            req = req.metadata(k, v);
        }
        let output = req
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 put_object failed: {}", e)))?;
        Ok(ObjectMetadata {
            key: key.to_string(),
            size: data.len() as u64,
            etag: output.e_tag().unwrap_or_default().to_string(),
            last_modified: chrono::Utc::now(),
            content_type: "application/octet-stream".to_string(),
            metadata,
        })
    }
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 delete_object failed: {}", e)))?;
        Ok(())
    }
    async fn copy_object(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<ObjectMetadata, StorageError> {
        let copy_source = format!("{}/{}", src_bucket, src_key);
        let mut req = self
            .client
            .copy_object()
            .copy_source(&copy_source)
            .bucket(dst_bucket)
            .key(dst_key);
        if let Some(meta) = &metadata {
            for (k, v) in meta {
                req = req.metadata(k, v);
            }
        }
        let output = req
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 copy_object failed: {}", e)))?;
        let head_output = self.head_object(dst_bucket, dst_key).await?;
        Ok(ObjectMetadata {
            key: dst_key.to_string(),
            size: head_output.size,
            etag: output
                .copy_object_result()
                .and_then(|r| r.e_tag())
                .unwrap_or_default()
                .to_string(),
            last_modified: output
                .copy_object_result()
                .and_then(|r| r.last_modified())
                .and_then(|dt| {
                    chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                        .ok()
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                })
                .unwrap_or_else(chrono::Utc::now),
            content_type: head_output.content_type,
            metadata: metadata.unwrap_or_default(),
        })
    }
    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String, StorageError> {
        let mut req = self
            .client
            .create_multipart_upload()
            .bucket(bucket)
            .key(key);
        for (k, v) in metadata {
            req = req.metadata(k, v);
        }
        let output = req.send().await.map_err(|e| {
            StorageError::Internal(format!("S3 create_multipart_upload failed: {}", e))
        })?;
        Ok(output.upload_id().unwrap_or_default().to_string())
    }
    async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<String, StorageError> {
        let body = aws_sdk_s3::primitives::ByteStream::from(data);
        let output = self
            .client
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .part_number(part_number as i32)
            .body(body)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 upload_part failed: {}", e)))?;
        Ok(output.e_tag().unwrap_or_default().to_string())
    }
    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<PartMetadata>,
    ) -> Result<ObjectMetadata, StorageError> {
        let completed_parts: Vec<_> = parts
            .into_iter()
            .map(|p| {
                aws_sdk_s3::types::CompletedPart::builder()
                    .part_number(p.part_number as i32)
                    .e_tag(&p.etag)
                    .build()
            })
            .collect();
        let multipart_upload = aws_sdk_s3::types::CompletedMultipartUpload::builder()
            .set_parts(Some(completed_parts))
            .build();
        let output = self
            .client
            .complete_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .multipart_upload(multipart_upload)
            .send()
            .await
            .map_err(|e| {
                StorageError::Internal(format!("S3 complete_multipart_upload failed: {}", e))
            })?;
        let meta = self.head_object(bucket, key).await?;
        Ok(ObjectMetadata {
            key: key.to_string(),
            size: meta.size,
            etag: output.e_tag().unwrap_or_default().to_string(),
            last_modified: chrono::Utc::now(),
            content_type: meta.content_type,
            metadata: meta.metadata,
        })
    }
    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<(), StorageError> {
        self.client
            .abort_multipart_upload()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| {
                StorageError::Internal(format!("S3 abort_multipart_upload failed: {}", e))
            })?;
        Ok(())
    }
    async fn list_parts(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError> {
        let output = self
            .client
            .list_parts()
            .bucket(bucket)
            .key(key)
            .upload_id(upload_id)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 list_parts failed: {}", e)))?;
        let parts = output
            .parts()
            .iter()
            .map(|p| PartMetadata {
                part_number: p.part_number().unwrap_or(0) as u32,
                size: p.size().unwrap_or(0) as u64,
                etag: p.e_tag().unwrap_or_default().to_string(),
                last_modified: p
                    .last_modified()
                    .and_then(|dt| {
                        chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                            .ok()
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                    })
                    .unwrap_or_else(chrono::Utc::now),
            })
            .collect();
        Ok(parts)
    }
    async fn list_multipart_uploads(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError> {
        let mut req = self.client.list_multipart_uploads().bucket(bucket);
        if let Some(p) = prefix {
            req = req.prefix(p);
        }
        let output = req.send().await.map_err(|e| {
            StorageError::Internal(format!("S3 list_multipart_uploads failed: {}", e))
        })?;
        let uploads = output
            .uploads()
            .iter()
            .map(|u| MultipartUpload {
                key: u.key().unwrap_or_default().to_string(),
                upload_id: u.upload_id().unwrap_or_default().to_string(),
                initiated: u
                    .initiated()
                    .and_then(|dt| {
                        chrono::DateTime::parse_from_rfc3339(&dt.to_string())
                            .ok()
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                    })
                    .unwrap_or_else(chrono::Utc::now),
            })
            .collect();
        Ok(uploads)
    }
    async fn get_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<HashMap<String, String>, StorageError> {
        let output = self
            .client
            .get_object_tagging()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("NoSuchKey") {
                    StorageError::NotFound("Object not found".to_string())
                } else {
                    StorageError::Internal(format!("S3 get_object_tagging failed: {}", e))
                }
            })?;
        let tags = output
            .tag_set()
            .iter()
            .map(|tag| {
                let key = tag.key();
                let value = tag.value();
                (key.to_string(), value.to_string())
            })
            .collect();
        Ok(tags)
    }
    async fn put_object_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let tag_set: Result<Vec<_>, _> = tags
            .into_iter()
            .map(|(k, v)| aws_sdk_s3::types::Tag::builder().key(k).value(v).build())
            .collect();
        let tag_set = tag_set
            .map_err(|e| StorageError::Internal(format!("Failed to build tag set: {}", e)))?;
        let tagging = aws_sdk_s3::types::Tagging::builder()
            .set_tag_set(Some(tag_set))
            .build()
            .map_err(|e| StorageError::Internal(format!("Failed to build tagging: {}", e)))?;
        self.client
            .put_object_tagging()
            .bucket(bucket)
            .key(key)
            .tagging(tagging)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 put_object_tagging failed: {}", e)))?;
        Ok(())
    }
    async fn delete_object_tags(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object_tagging()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| {
                StorageError::Internal(format!("S3 delete_object_tagging failed: {}", e))
            })?;
        Ok(())
    }
    async fn get_bucket_tags(&self, bucket: &str) -> Result<HashMap<String, String>, StorageError> {
        let output = self
            .client
            .get_bucket_tagging()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("NoSuchTagSet") {
                    StorageError::NotFound("Tag set not found".to_string())
                } else {
                    StorageError::Internal(format!("S3 get_bucket_tagging failed: {}", e))
                }
            })?;
        let tags = output
            .tag_set()
            .iter()
            .map(|tag| {
                let key = tag.key();
                let value = tag.value();
                (key.to_string(), value.to_string())
            })
            .collect();
        Ok(tags)
    }
    async fn put_bucket_tags(
        &self,
        bucket: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let tag_set: Result<Vec<_>, _> = tags
            .into_iter()
            .map(|(k, v)| aws_sdk_s3::types::Tag::builder().key(k).value(v).build())
            .collect();
        let tag_set = tag_set
            .map_err(|e| StorageError::Internal(format!("Failed to build tag set: {}", e)))?;
        let tagging = aws_sdk_s3::types::Tagging::builder()
            .set_tag_set(Some(tag_set))
            .build()
            .map_err(|e| StorageError::Internal(format!("Failed to build tagging: {}", e)))?;
        self.client
            .put_bucket_tagging()
            .bucket(bucket)
            .tagging(tagging)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 put_bucket_tagging failed: {}", e)))?;
        Ok(())
    }
    async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), StorageError> {
        self.client
            .delete_bucket_tagging()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| {
                StorageError::Internal(format!("S3 delete_bucket_tagging failed: {}", e))
            })?;
        Ok(())
    }
    async fn get_bucket_policy(&self, bucket: &str) -> Result<String, StorageError> {
        let output = self
            .client
            .get_bucket_policy()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| {
                let err_str = e.to_string();
                if err_str.contains("404") || err_str.contains("NoSuchBucketPolicy") {
                    StorageError::NotFound("Bucket policy not found".to_string())
                } else {
                    StorageError::Internal(format!("S3 get_bucket_policy failed: {}", e))
                }
            })?;
        Ok(output.policy().unwrap_or_default().to_string())
    }
    async fn put_bucket_policy(&self, bucket: &str, policy: String) -> Result<(), StorageError> {
        self.client
            .put_bucket_policy()
            .bucket(bucket)
            .policy(policy)
            .send()
            .await
            .map_err(|e| StorageError::Internal(format!("S3 put_bucket_policy failed: {}", e)))?;
        Ok(())
    }
    async fn delete_bucket_policy(&self, bucket: &str) -> Result<(), StorageError> {
        self.client
            .delete_bucket_policy()
            .bucket(bucket)
            .send()
            .await
            .map_err(|e| {
                StorageError::Internal(format!("S3 delete_bucket_policy failed: {}", e))
            })?;
        Ok(())
    }
    async fn get_storage_stats(&self) -> Result<StorageStats, StorageError> {
        let buckets = self.list_buckets().await?;
        let mut total_objects = 0;
        let mut total_bytes = 0;
        for bucket in &buckets {
            let list_result = self
                .list_objects(&bucket.name, None, None, 1000, None)
                .await?;
            for (_, meta) in &list_result.objects {
                total_objects += 1;
                total_bytes += meta.size;
            }
        }
        Ok(StorageStats {
            bucket_count: buckets.len() as u64,
            object_count: total_objects,
            total_size_bytes: total_bytes,
        })
    }
}
