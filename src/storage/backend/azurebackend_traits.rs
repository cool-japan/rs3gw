//! # AzureBackend - Trait Implementations
//!
//! This module contains trait implementations for `AzureBackend`.
//!
//! ## Implemented Traits
//!
//! - `StorageBackend`
//!
//! This provides Azure Blob Storage integration using `azure_storage_blobs` 0.21.
//! Azure concepts map to S3 concepts:
//! - Containers → Buckets
//! - Blobs → Objects
//! - Blob Metadata → Object Metadata
//! - Block Blobs → Multipart Uploads (emulated via committed block lists)

#![cfg(feature = "azure")]

use crate::storage::{
    BucketMetadata, ByteRange, MultipartUpload, ObjectMetadata, PartMetadata, StorageError,
    StorageStats,
};
use async_trait::async_trait;
use azure_storage_blobs::prelude::{BlobBlockType, BlockId, BlockList, Tags};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use std::collections::HashMap;
use uuid::Uuid;

use super::functions::{ByteStream, StorageBackend};
use super::types::{AzureBackend, ObjectListResult};

/// Convert an azure error (azure_storage::Error, which re-exports azure_core 0.21 Error)
/// into a `StorageError`.  We detect 404s by inspecting the error string since the
/// `as_http_error()` method lives in azure_core 0.21 but the workspace resolves a
/// different version for the direct dep.
fn az_err(context: &str, e: azure_storage::Error) -> StorageError {
    let msg = format!("Azure {context} failed: {e}");
    // Detect 404 / NotFound by inspecting the string representation
    let s = e.to_string();
    if s.contains("404") || s.contains("NotFound") || s.contains("BlobNotFound") {
        StorageError::NotFound(msg)
    } else {
        StorageError::Internal(msg)
    }
}

/// Convert a `time::OffsetDateTime` (transitive dep via azure_storage_blobs) to
/// a `chrono::DateTime<Utc>` without importing the `time` crate explicitly.
///
/// `time` 0.3's `Display` impl produces: `2023-01-15 12:34:56.000000000 +00:00:00`
/// (with sub-seconds and a ±HH:MM:SS timezone offset, not RFC3339).
/// We try several parse formats in order of likelihood.
fn odt_to_chrono_str(s: &str) -> DateTime<Utc> {
    use chrono::NaiveDateTime;

    // time 0.3 OffsetDateTime Display: "2023-01-15 12:34:56.000000000 +00:00:00"
    // The timezone part is ±HH:MM:SS – chrono %z parses ±HHMM, %:z parses ±HH:MM.
    // We strip the trailing ":SS" from the offset to get something chrono can parse.
    let normalized = {
        // Pattern: ends with " ±HH:MM:SS" (9 chars from end is "±HH:MM:SS" including space)
        // We want to convert " +00:00:00" → " +00:00"
        let trimmed = s.trim_end();
        if trimmed.len() > 6 {
            // Check if last 3 chars look like ":SS" (colon + 2 digits)
            let (body, tail) = trimmed.split_at(trimmed.len().saturating_sub(3));
            if tail.starts_with(':') && tail.chars().skip(1).all(|c| c.is_ascii_digit()) {
                body.to_owned()
            } else {
                trimmed.to_owned()
            }
        } else {
            trimmed.to_owned()
        }
    };

    // Try RFC3339 (in case the SDK changes format in a future version)
    if let Ok(dt) = DateTime::parse_from_rfc3339(&normalized) {
        return dt.with_timezone(&Utc);
    }
    // Try "YYYY-MM-DD HH:MM:SS.f +HH:MM" (time 0.3 with sub-seconds, stripped offset)
    if let Ok(dt) = DateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%.f %:z") {
        return dt.with_timezone(&Utc);
    }
    // Try "YYYY-MM-DD HH:MM:SS +HH:MM" (no sub-seconds)
    if let Ok(dt) = DateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S %:z") {
        return dt.with_timezone(&Utc);
    }
    // Try RFC2822
    if let Ok(dt) = DateTime::parse_from_rfc2822(&normalized) {
        return dt.with_timezone(&Utc);
    }
    // Fallback: use the original string with NaiveDateTime as UTC
    NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%d %H:%M:%S%.f")
        .map(|ndt| ndt.and_utc())
        .unwrap_or_else(|_| Utc::now())
}

#[async_trait]
impl StorageBackend for AzureBackend {
    async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError> {
        let mut stream = self.blob_service_client().list_containers().into_stream();

        let mut buckets = Vec::new();
        while let Some(page) = stream.next().await {
            let page = page.map_err(|e| az_err("list_containers", e))?;
            for container in page.containers {
                buckets.push(BucketMetadata {
                    name: container.name.clone(),
                    creation_date: Utc::now(),
                });
            }
        }
        Ok(buckets)
    }

    async fn bucket_exists(&self, bucket: &str) -> Result<bool, StorageError> {
        self.container_client(bucket)
            .exists()
            .await
            .map_err(|e| az_err("container_exists", e))
    }

    async fn create_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.container_client(bucket)
            .create()
            .await
            .map_err(|e| az_err("create_container", e))?;
        Ok(())
    }

    async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.container_client(bucket)
            .delete()
            .await
            .map_err(|e| az_err("delete_container", e))?;
        Ok(())
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        max_keys: usize,
        _continuation_token: Option<&str>,
    ) -> Result<ObjectListResult, StorageError> {
        // Note: azure_storage_blobs 0.21 uses azure_core 0.21 for its Prefix/Delimiter/
        // MaxResults types, but this workspace resolves azure_core to 0.35.  To avoid
        // the version mismatch we enumerate all blobs and apply filtering ourselves.
        let mut stream = self.container_client(bucket).list_blobs().into_stream();

        let mut objects: Vec<(String, ObjectMetadata)> = Vec::new();
        let mut common_prefix_set: std::collections::BTreeSet<String> = Default::default();
        let mut is_truncated = false;
        let mut next_continuation_token: Option<String> = None;

        while let Some(page) = stream.next().await {
            let page = page.map_err(|e| az_err("list_blobs", e))?;

            if page.next_marker.is_some() {
                is_truncated = true;
                next_continuation_token = page.next_marker.map(|m| m.as_str().to_string());
            }

            for blob in page.blobs.blobs() {
                let name = &blob.name;

                // Apply prefix filter
                if let Some(p) = prefix {
                    if !name.starts_with(p) {
                        continue;
                    }
                }

                // Apply delimiter — collect common prefixes and skip further keys
                if let Some(d) = delimiter {
                    let stripped = if let Some(p) = prefix {
                        name.strip_prefix(p).unwrap_or(name)
                    } else {
                        name.as_str()
                    };
                    if let Some(pos) = stripped.find(d) {
                        let cp = format!("{}{}{}", prefix.unwrap_or(""), &stripped[..pos], d);
                        common_prefix_set.insert(cp);
                        continue;
                    }
                }

                let meta = blob_to_object_metadata(blob);
                objects.push((blob.name.clone(), meta));

                // Enforce max_keys limit
                if max_keys > 0 && objects.len() >= max_keys {
                    is_truncated = true;
                    break;
                }
            }
        }

        Ok(ObjectListResult {
            objects,
            common_prefixes: common_prefix_set.into_iter().collect(),
            is_truncated,
            next_continuation_token,
        })
    }

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError> {
        let response = self
            .blob_client(bucket, key)
            .get_properties()
            .await
            .map_err(|e| az_err("get_properties", e))?;
        Ok(blob_to_object_metadata(&response.blob))
    }

    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, Bytes), StorageError> {
        // Note: azure_core 0.21 Range type is not usable directly (workspace uses 0.35).
        // We fetch the full blob and slice the range client-side.
        // A production implementation can use raw HTTP headers for range requests.
        let mut stream = self.blob_client(bucket, key).get().into_stream();

        let mut data_chunks: Vec<Bytes> = Vec::new();
        let mut meta: Option<ObjectMetadata> = None;

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| az_err("get_blob", e))?;
            if meta.is_none() {
                meta = Some(blob_to_object_metadata(&chunk.blob));
            }
            let collected = chunk
                .data
                .collect()
                .await
                .map_err(|e| az_err("get_blob_data", e))?;
            data_chunks.push(collected);
        }

        let meta = meta.ok_or_else(|| StorageError::NotFound(format!("Blob {key} not found")))?;
        let full = if data_chunks.len() == 1 {
            data_chunks.remove(0)
        } else {
            Bytes::from(
                data_chunks
                    .into_iter()
                    .flat_map(|b| b.to_vec())
                    .collect::<Vec<u8>>(),
            )
        };

        let data = if let Some(br) = range {
            let start = br.start as usize;
            let end = (br.end as usize).min(full.len());
            if start >= full.len() {
                Bytes::new()
            } else {
                full.slice(start..end)
            }
        } else {
            full
        };

        Ok((meta, data))
    }

    async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, ByteStream), StorageError> {
        // Fetch full object and wrap as a one-shot stream.
        let (meta, data) = self.get_object(bucket, key, range).await?;
        let stream = futures::stream::once(async move { Ok(data) });
        Ok((meta, Box::pin(stream)))
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata, StorageError> {
        use azure_storage_blobs::prelude::BlobContentType;

        let size = data.len() as u64;
        let content_type = metadata
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let response = self
            .blob_client(bucket, key)
            .put_block_blob(data)
            .content_type(BlobContentType::from(content_type.clone()))
            .await
            .map_err(|e| az_err("put_block_blob", e))?;

        Ok(ObjectMetadata {
            key: key.to_string(),
            size,
            etag: response.etag,
            last_modified: odt_to_chrono_str(&response.last_modified.to_string()),
            content_type,
            metadata: HashMap::new(),
            schema_version: 1,
        })
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.blob_client(bucket, key)
            .delete()
            .await
            .map_err(|e| az_err("delete_blob", e))?;
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
        let src_url = self
            .blob_client(src_bucket, src_key)
            .url()
            .map_err(|e| az_err("build_copy_source_url", e))?;

        self.blob_client(dst_bucket, dst_key)
            .copy(src_url)
            .await
            .map_err(|e| az_err("copy_blob", e))?;

        let mut meta = self.head_object(dst_bucket, dst_key).await?;
        if let Some(extra) = metadata {
            meta.metadata.extend(extra);
        }
        Ok(meta)
    }

    // ── Multipart upload (emulated with Azure block blobs) ────────────────────
    //
    // Each part is uploaded as an uncommitted block (put_block).
    // Completing the upload calls put_block_list to commit the blocks.

    async fn create_multipart_upload(
        &self,
        _bucket: &str,
        _key: &str,
        _metadata: HashMap<String, String>,
    ) -> Result<String, StorageError> {
        Ok(Uuid::new_v4().to_string())
    }

    async fn upload_part(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<String, StorageError> {
        // Build a human-readable block ID string (max 64 bytes — upload_id is a UUID
        // so "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx-00001" is well within the limit).
        // The Azure SDK base64-encodes BlockId bytes internally before sending, so we
        // pass the raw UTF-8 bytes and return the same string as the ETag for
        // round-trip identity in `list_parts` and `complete_multipart_upload`.
        let block_id_str = format!("{upload_id}-{part_number:05}");
        let block_id = BlockId::new(block_id_str.as_bytes().to_vec());

        self.blob_client(bucket, key)
            .put_block(block_id, data)
            .await
            .map_err(|e| az_err("put_block", e))?;

        // Return the block ID string as the etag for this part.
        Ok(block_id_str)
    }

    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        _upload_id: &str,
        parts: Vec<PartMetadata>,
    ) -> Result<ObjectMetadata, StorageError> {
        let mut block_list = BlockList::default();
        for part in &parts {
            // The etag is the base64-encoded block ID string from upload_part.
            let block_id_bytes = part.etag.as_bytes().to_vec();
            block_list
                .blocks
                .push(BlobBlockType::Uncommitted(BlockId::new(block_id_bytes)));
        }

        self.blob_client(bucket, key)
            .put_block_list(block_list)
            .await
            .map_err(|e| az_err("put_block_list", e))?;

        self.head_object(bucket, key).await
    }

    async fn abort_multipart_upload(
        &self,
        _bucket: &str,
        _key: &str,
        _upload_id: &str,
    ) -> Result<(), StorageError> {
        // Azure does not have an explicit abort; uncommitted blocks expire after 7 days.
        Ok(())
    }

    async fn list_parts(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError> {
        use azure_storage_blobs::prelude::BlockListType;

        let response = self
            .blob_client(bucket, key)
            .get_block_list()
            .block_list_type(BlockListType::Uncommitted)
            .await
            .map_err(|e| az_err("get_block_list", e))?;

        let mut parts = Vec::new();
        for (idx, block) in response.block_with_size_list.blocks.iter().enumerate() {
            if let BlobBlockType::Uncommitted(ref block_id) = block.block_list_type {
                // The block ID bytes are the base64 string we stored.
                let raw_id = String::from_utf8_lossy(block_id.bytes().as_ref()).to_string();
                if raw_id.starts_with(upload_id) {
                    let part_number = raw_id
                        .strip_prefix(&format!("{upload_id}-"))
                        .and_then(|s| s.parse::<u32>().ok())
                        .unwrap_or(idx as u32 + 1);
                    parts.push(PartMetadata {
                        part_number,
                        etag: raw_id,
                        size: block.size_in_bytes,
                        last_modified: Utc::now(),
                    });
                }
            }
        }
        Ok(parts)
    }

    async fn list_multipart_uploads(
        &self,
        bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError> {
        // Azure has no dedicated "list multipart uploads" API; enumerate blobs.
        let result = self.list_objects(bucket, prefix, None, 1000, None).await?;
        let uploads = result
            .objects
            .into_iter()
            .map(|(key, meta)| MultipartUpload {
                key,
                upload_id: String::new(),
                initiated: meta.last_modified,
            })
            .collect();
        Ok(uploads)
    }

    // ── Blob tagging ──────────────────────────────────────────────────────────

    async fn get_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<HashMap<String, String>, StorageError> {
        let response = self
            .blob_client(bucket, key)
            .get_tags()
            .await
            .map_err(|e| az_err("get_blob_tags", e))?;
        let tags: HashMap<String, String> = response.tags.into();
        Ok(tags)
    }

    async fn put_object_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        self.blob_client(bucket, key)
            .set_tags(Tags::from(tags))
            .await
            .map_err(|e| az_err("set_blob_tags", e))?;
        Ok(())
    }

    async fn delete_object_tags(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.blob_client(bucket, key)
            .set_tags(Tags::new())
            .await
            .map_err(|e| az_err("delete_blob_tags", e))?;
        Ok(())
    }

    // ── Bucket tagging (stored as a sidecar blob) ─────────────────────────────
    // Azure SDK v0.21 does not expose a container-metadata set endpoint,
    // so we store bucket tags as a JSON sidecar blob named "__rs3gw_bucket_tags__.json".

    async fn get_bucket_tags(&self, bucket: &str) -> Result<HashMap<String, String>, StorageError> {
        match self
            .get_object(bucket, "__rs3gw_bucket_tags__.json", None)
            .await
        {
            Ok((_, data)) => {
                let tags: HashMap<String, String> = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::Internal(format!("Tags parse error: {e}")))?;
                Ok(tags)
            }
            Err(StorageError::NotFound(_)) => Ok(HashMap::new()),
            Err(e) => Err(e),
        }
    }

    async fn put_bucket_tags(
        &self,
        bucket: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let json = serde_json::to_string(&tags)
            .map_err(|e| StorageError::Internal(format!("Tags serialization failed: {e}")))?;
        self.put_object(
            bucket,
            "__rs3gw_bucket_tags__.json",
            Bytes::from(json),
            HashMap::new(),
        )
        .await?;
        Ok(())
    }

    async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), StorageError> {
        match self
            .delete_object(bucket, "__rs3gw_bucket_tags__.json")
            .await
        {
            Ok(()) | Err(StorageError::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    // ── Bucket policies ───────────────────────────────────────────────────────
    // Azure uses RBAC/ACL rather than S3-style JSON bucket policies.

    async fn get_bucket_policy(&self, _bucket: &str) -> Result<String, StorageError> {
        Err(StorageError::NotFound(
            "Azure containers use RBAC/ACL, not S3-style bucket policies".to_string(),
        ))
    }

    async fn put_bucket_policy(&self, _bucket: &str, _policy: String) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete_bucket_policy(&self, _bucket: &str) -> Result<(), StorageError> {
        Ok(())
    }

    // ── Storage statistics ────────────────────────────────────────────────────

    async fn get_storage_stats(&self) -> Result<StorageStats, StorageError> {
        let buckets = self.list_buckets().await?;
        let mut total_objects: u64 = 0;
        let mut total_bytes: u64 = 0;

        for bucket in &buckets {
            let result = self
                .list_objects(&bucket.name, None, None, 5000, None)
                .await?;
            for (_, meta) in &result.objects {
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

// ── Private helpers ───────────────────────────────────────────────────────────

/// Convert a `Blob` reference to `ObjectMetadata`.
fn blob_to_object_metadata(blob: &azure_storage_blobs::prelude::Blob) -> ObjectMetadata {
    let props = &blob.properties;
    // Format the last_modified timestamp via its Display impl (RFC2822 / RFC1123),
    // then parse with chrono to avoid naming `time::OffsetDateTime` explicitly.
    let last_modified = odt_to_chrono_str(&props.last_modified.to_string());
    let etag = props.etag.to_string();
    let size = props.content_length;
    let content_type = props.content_type.clone();
    let user_meta = blob.metadata.clone().unwrap_or_default();

    ObjectMetadata {
        key: blob.name.clone(),
        size,
        etag,
        last_modified,
        content_type,
        metadata: user_meta,
        schema_version: 1,
    }
}
