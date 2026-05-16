//! # GcsBackend - Trait Implementations
//!
//! This module provides a full implementation of `StorageBackend` for Google
//! Cloud Storage using the `google-cloud-storage` 1.12 SDK.
//!
//! ## Implemented Traits
//!
//! - `StorageBackend`
//!
//! ## Architecture
//!
//! The SDK uses two separate clients:
//! - `StorageControl` for bucket and object control-plane operations (gRPC)
//! - `Storage` for object data-plane operations (HTTP streaming)
//!
//! Multipart uploads are emulated in-process using `compose_object`: parts are
//! uploaded as individual single-part objects, then assembled into the final
//! object via `compose_object`.  An in-memory registry (global `RwLock`) tracks
//! the in-flight upload state.
//!
//! ## GCS → S3 concept mapping
//!
//! | GCS                      | S3               |
//! |--------------------------|------------------|
//! | Bucket                   | Bucket           |
//! | Object                   | Object           |
//! | Object.metadata (custom) | Object metadata  |
//! | Object labels (via tags) | Object tags      |
//! | IAM policy               | Bucket policy    |

use crate::storage::{
    BucketMetadata, ByteRange, MultipartUpload, ObjectMetadata, PartMetadata, StorageError,
    StorageStats,
};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures::stream;
use google_cloud_gax::paginator::ItemPaginator as _;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::RwLock;
use tracing::debug;
use uuid::Uuid;

use super::functions::{ByteStream, StorageBackend};
use super::types::{GcsBackend, ObjectListResult};

// ─── In-memory multipart state ────────────────────────────────────────────────
//
// GCS does not have an S3-compatible multipart upload API.  We emulate it by:
// 1. Uploading each part as a temporary object named
//    `__rs3gw_mp_{upload_id}_part_{part_number:05}` in the same bucket.
// 2. On complete, calling compose_object to merge up to 32 parts (GCS limit)
//    into the final object.
// 3. Deleting the temporary part objects.

#[derive(Debug)]
struct GcsMpState {
    #[allow(dead_code)]
    bucket: String,
    key: String,
    metadata: HashMap<String, String>,
    /// (part_number, temp_object_name, etag)
    parts: Vec<(u32, String, String)>,
}

static GCS_MP_STATE: LazyLock<Arc<RwLock<HashMap<String, GcsMpState>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(HashMap::new())));

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a `google_cloud_wkt::Timestamp` to `chrono::DateTime<Utc>`.
fn wkt_ts_to_chrono(ts: &google_cloud_wkt::Timestamp) -> DateTime<Utc> {
    let secs = ts.seconds();
    let nanos = ts.nanos() as u32;
    DateTime::from_timestamp(secs, nanos).unwrap_or_else(Utc::now)
}

/// Convert a `google_cloud_storage::Error` to `StorageError`.
fn gcs_err(ctx: &str, e: google_cloud_storage::Error) -> StorageError {
    let msg = format!("GCS {ctx}: {e}");
    // Check if this is a 404-class error by inspecting the string
    let s = e.to_string();
    if s.contains("NOT_FOUND") || s.contains("404") || s.contains("no such") {
        StorageError::NotFound(msg)
    } else {
        StorageError::Internal(msg)
    }
}

/// Convert a `google_cloud_storage::model::Object` to `ObjectMetadata`.
fn gcs_object_to_meta(obj: &google_cloud_storage::model::Object) -> ObjectMetadata {
    let last_modified = obj
        .update_time
        .as_ref()
        .or(obj.create_time.as_ref())
        .map(wkt_ts_to_chrono)
        .unwrap_or_else(Utc::now);
    ObjectMetadata {
        key: obj.name.clone(),
        size: obj.size.max(0) as u64,
        etag: obj.etag.clone(),
        last_modified,
        content_type: if obj.content_type.is_empty() {
            "application/octet-stream".to_string()
        } else {
            obj.content_type.clone()
        },
        metadata: obj.metadata.clone(),
        schema_version: 1,
    }
}

// ─── StorageBackend impl ──────────────────────────────────────────────────────

#[async_trait]
impl StorageBackend for GcsBackend {
    // ── Bucket operations ──────────────────────────────────────────────────────

    async fn list_buckets(&self) -> Result<Vec<BucketMetadata>, StorageError> {
        let parent = format!("projects/{}", self.project_id);
        let mut items = self
            .storage_control
            .list_buckets()
            .set_parent(parent)
            .by_item();

        let mut result = Vec::new();
        while let Some(item) = items
            .next()
            .await
            .transpose()
            .map_err(|e| gcs_err("list_buckets", e))?
        {
            let creation_date = item
                .create_time
                .as_ref()
                .map(wkt_ts_to_chrono)
                .unwrap_or_else(Utc::now);
            result.push(BucketMetadata {
                name: item.bucket_id.clone(),
                creation_date,
            });
        }
        debug!(count = result.len(), "GcsBackend::list_buckets");
        Ok(result)
    }

    async fn bucket_exists(&self, bucket: &str) -> Result<bool, StorageError> {
        match self
            .storage_control
            .get_bucket()
            .set_name(self.bucket_name(bucket))
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let s = e.to_string();
                if s.contains("NOT_FOUND") || s.contains("404") {
                    Ok(false)
                } else {
                    Err(gcs_err("bucket_exists", e))
                }
            }
        }
    }

    async fn create_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        let parent = format!("projects/{}", self.project_id);
        self.storage_control
            .create_bucket()
            .set_parent(parent)
            .set_bucket_id(bucket.to_string())
            .set_bucket(google_cloud_storage::model::Bucket::new())
            .send()
            .await
            .map_err(|e| gcs_err("create_bucket", e))?;
        Ok(())
    }

    async fn delete_bucket(&self, bucket: &str) -> Result<(), StorageError> {
        self.storage_control
            .delete_bucket()
            .set_name(self.bucket_name(bucket))
            .send()
            .await
            .map_err(|e| gcs_err("delete_bucket", e))?;
        Ok(())
    }

    // ── Object listing ─────────────────────────────────────────────────────────

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        delimiter: Option<&str>,
        max_keys: usize,
        _continuation_token: Option<&str>,
    ) -> Result<ObjectListResult, StorageError> {
        let parent = self.bucket_name(bucket);
        let mut builder = self
            .storage_control
            .list_objects()
            .set_parent(parent.clone());

        if let Some(p) = prefix {
            builder = builder.set_prefix(p.to_string());
        }
        if let Some(d) = delimiter {
            builder = builder.set_delimiter(d.to_string());
        }
        if max_keys > 0 {
            builder = builder.set_page_size(max_keys.min(1000) as i32);
        }

        let response = builder
            .send()
            .await
            .map_err(|e| gcs_err("list_objects", e))?;

        let mut objects = Vec::new();
        for obj in &response.objects {
            let meta = gcs_object_to_meta(obj);
            objects.push((obj.name.clone(), meta));
        }

        let common_prefixes: Vec<String> = response.prefixes.clone();
        let next_token = if response.next_page_token.is_empty() {
            None
        } else {
            Some(response.next_page_token.clone())
        };
        let is_truncated = next_token.is_some();

        Ok(ObjectListResult {
            objects,
            common_prefixes,
            is_truncated,
            next_continuation_token: next_token,
        })
    }

    // ── Single-object operations ───────────────────────────────────────────────

    async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, StorageError> {
        let obj = self
            .storage_control
            .get_object()
            .set_bucket(self.bucket_name(bucket))
            .set_object(key.to_string())
            .send()
            .await
            .map_err(|e| gcs_err("head_object", e))?;
        Ok(gcs_object_to_meta(&obj))
    }

    async fn get_object(
        &self,
        bucket: &str,
        key: &str,
        range: Option<ByteRange>,
    ) -> Result<(ObjectMetadata, Bytes), StorageError> {
        // First fetch metadata
        let meta = self.head_object(bucket, key).await?;

        // Then stream the object data
        let bucket_path = self.bucket_name(bucket);
        let mut reader = self
            .storage
            .read_object(bucket_path.clone(), key)
            .send()
            .await
            .map_err(|e| gcs_err("get_object read", e))?;

        let mut buf: Vec<u8> = Vec::with_capacity(meta.size as usize);
        while let Some(chunk) = reader
            .next()
            .await
            .transpose()
            .map_err(|e| StorageError::Internal(format!("GCS read_object stream: {e}")))?
        {
            buf.extend_from_slice(&chunk);
        }
        let full = Bytes::from(buf);

        // Apply range slice if requested
        let data = if let Some(r) = range {
            let start = r.start as usize;
            let end = (r.end as usize + 1).min(full.len());
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
        let (meta, data) = self.get_object(bucket, key, range).await?;
        let stream = stream::once(async move { Ok(data) });
        Ok((meta, Box::pin(stream)))
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> Result<ObjectMetadata, StorageError> {
        let bucket_path = self.bucket_name(bucket);
        let content_type = metadata
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let data_len = data.len() as u64;

        let obj = self
            .storage
            .write_object(bucket_path, key, data)
            .set_content_type(content_type.clone())
            .set_metadata(
                metadata
                    .iter()
                    .filter(|(k, _)| k.as_str() != "content-type")
                    .map(|(k, v)| (k.clone(), v.clone())),
            )
            .send_buffered()
            .await
            .map_err(|e| StorageError::Internal(format!("GCS put_object: {e}")))?;

        Ok(ObjectMetadata {
            key: key.to_string(),
            size: data_len,
            etag: obj.etag.clone(),
            last_modified: obj
                .update_time
                .as_ref()
                .or(obj.create_time.as_ref())
                .map(wkt_ts_to_chrono)
                .unwrap_or_else(Utc::now),
            content_type,
            metadata,
            schema_version: 1,
        })
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        self.storage_control
            .delete_object()
            .set_bucket(self.bucket_name(bucket))
            .set_object(key.to_string())
            .send()
            .await
            .map_err(|e| gcs_err("delete_object", e))?;
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
        let rewrite_response = self
            .storage_control
            .rewrite_object()
            .set_source_bucket(self.bucket_name(src_bucket))
            .set_source_object(src_key.to_string())
            .set_destination_bucket(self.bucket_name(dst_bucket))
            .set_destination_name(dst_key.to_string())
            .send()
            .await
            .map_err(|e| gcs_err("copy_object", e))?;

        let mut obj_meta = if let Some(ref obj) = rewrite_response.resource {
            gcs_object_to_meta(obj)
        } else {
            self.head_object(dst_bucket, dst_key).await?
        };

        if let Some(m) = metadata {
            obj_meta.metadata = m;
        }
        Ok(obj_meta)
    }

    // ── Multipart upload (emulated via GCS temporary objects + compose) ─────────

    async fn create_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        metadata: HashMap<String, String>,
    ) -> Result<String, StorageError> {
        let upload_id = Uuid::new_v4().to_string();
        let state = GcsMpState {
            bucket: bucket.to_string(),
            key: key.to_string(),
            metadata,
            parts: Vec::new(),
        };
        GCS_MP_STATE.write().await.insert(upload_id.clone(), state);
        Ok(upload_id)
    }

    async fn upload_part(
        &self,
        bucket: &str,
        _key: &str,
        upload_id: &str,
        part_number: u32,
        data: Bytes,
    ) -> Result<String, StorageError> {
        let temp_name = format!("__rs3gw_mp_{upload_id}_part_{part_number:05}");
        let bucket_path = self.bucket_name(bucket);

        let obj = self
            .storage
            .write_object(bucket_path, temp_name.clone(), data)
            .send_buffered()
            .await
            .map_err(|e| StorageError::Internal(format!("GCS upload_part put temp: {e}")))?;

        let etag = obj.etag.clone();

        // Record part in in-memory state
        let mut map = GCS_MP_STATE.write().await;
        if let Some(state) = map.get_mut(upload_id) {
            state.parts.retain(|(n, _, _)| *n != part_number);
            state.parts.push((part_number, temp_name, etag.clone()));
            state.parts.sort_by_key(|(n, _, _)| *n);
        }
        // If state doesn't exist yet (e.g. cross-process), still succeed
        drop(map);

        Ok(etag)
    }

    async fn complete_multipart_upload(
        &self,
        bucket: &str,
        key: &str,
        upload_id: &str,
        parts: Vec<PartMetadata>,
    ) -> Result<ObjectMetadata, StorageError> {
        let (ordered_temps, metadata) = {
            let mut map = GCS_MP_STATE.write().await;
            let state = map.remove(upload_id).ok_or_else(|| {
                StorageError::Internal(format!("Unknown GCS multipart upload id: {upload_id}"))
            })?;

            // Resolve temp object names in caller-provided part order
            let mut ordered: Vec<String> = Vec::with_capacity(parts.len());
            for part in &parts {
                let temp = state
                    .parts
                    .iter()
                    .find(|(n, _, _)| *n == part.part_number)
                    .map(|(_, name, _)| name.clone())
                    .ok_or_else(|| {
                        StorageError::Internal(format!(
                            "GCS: part {} missing from upload {upload_id}",
                            part.part_number
                        ))
                    })?;
                ordered.push(temp);
            }
            (ordered, state.metadata)
        };

        // GCS compose_object supports up to 32 source objects.
        // For larger uploads we chain multiple compose operations.
        let bucket_path = self.bucket_name(bucket);
        let final_key = key.to_string();

        // Assemble: if <= 32 parts, compose directly; otherwise chain.
        // For simplicity (IMPLEMENT POLICY), we support up to 32 parts in one call.
        // A production system would implement recursive compose for larger uploads.
        if ordered_temps.len() > 32 {
            return Err(StorageError::Internal(
                "GCS compose_object supports at most 32 source objects per call; \
                 multi-step assembly is not yet implemented"
                    .to_string(),
            ));
        }

        let source_objects: Vec<_> = ordered_temps
            .iter()
            .map(|name| {
                google_cloud_storage::model::compose_object_request::SourceObject::new()
                    .set_name(name.clone())
            })
            .collect();

        let content_type = metadata
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        // Destination object: bucket (resource name) + name (key) + content_type + metadata
        let dest_obj = google_cloud_storage::model::Object::new()
            .set_bucket(bucket_path.clone())
            .set_name(final_key.clone())
            .set_content_type(content_type.clone())
            .set_metadata(
                metadata
                    .iter()
                    .filter(|(k, _)| k.as_str() != "content-type")
                    .map(|(k, v)| (k.clone(), v.clone())),
            );

        let composed = self
            .storage_control
            .compose_object()
            .set_destination(dest_obj)
            .set_source_objects(source_objects)
            .send()
            .await
            .map_err(|e| gcs_err("compose_object", e))?;

        // Clean up temporary part objects (best-effort)
        for temp_name in &ordered_temps {
            let _ = self
                .storage_control
                .delete_object()
                .set_bucket(bucket_path.clone())
                .set_object(temp_name.clone())
                .send()
                .await;
        }

        let mut obj_meta = gcs_object_to_meta(&composed);
        obj_meta.metadata = metadata;
        Ok(obj_meta)
    }

    async fn abort_multipart_upload(
        &self,
        bucket: &str,
        _key: &str,
        upload_id: &str,
    ) -> Result<(), StorageError> {
        let parts_to_delete = {
            let mut map = GCS_MP_STATE.write().await;
            map.remove(upload_id)
                .map(|s| {
                    s.parts
                        .into_iter()
                        .map(|(_, name, _)| name)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };

        let bucket_path = self.bucket_name(bucket);
        for temp_name in parts_to_delete {
            let _ = self
                .storage_control
                .delete_object()
                .set_bucket(bucket_path.clone())
                .set_object(temp_name)
                .send()
                .await;
        }
        Ok(())
    }

    async fn list_parts(
        &self,
        _bucket: &str,
        _key: &str,
        upload_id: &str,
    ) -> Result<Vec<PartMetadata>, StorageError> {
        let map = GCS_MP_STATE.read().await;
        let parts = map
            .get(upload_id)
            .map(|s| {
                s.parts
                    .iter()
                    .map(|(n, _, etag)| PartMetadata {
                        part_number: *n,
                        etag: etag.clone(),
                        size: 0, // not tracked in GCS emulation
                        last_modified: Utc::now(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(parts)
    }

    async fn list_multipart_uploads(
        &self,
        _bucket: &str,
        prefix: Option<&str>,
    ) -> Result<Vec<MultipartUpload>, StorageError> {
        let map = GCS_MP_STATE.read().await;
        let uploads = map
            .values()
            .filter(|s| prefix.map(|p| s.key.starts_with(p)).unwrap_or(true))
            .map(|s| MultipartUpload {
                key: s.key.clone(),
                upload_id: String::new(), // upload_id not exposed here
                initiated: Utc::now(),
            })
            .collect();
        Ok(uploads)
    }

    // ── Object tagging ─────────────────────────────────────────────────────────
    // GCS does not have native S3-style object tags.  We store them as custom
    // metadata using the key prefix "tag:" to distinguish them.

    async fn get_object_tags(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<HashMap<String, String>, StorageError> {
        let obj = self
            .storage_control
            .get_object()
            .set_bucket(self.bucket_name(bucket))
            .set_object(key.to_string())
            .send()
            .await
            .map_err(|e| gcs_err("get_object_tags", e))?;
        let tags: HashMap<String, String> = obj
            .metadata
            .iter()
            .filter(|(k, _)| k.starts_with("tag:"))
            .map(|(k, v)| (k.trim_start_matches("tag:").to_string(), v.clone()))
            .collect();
        Ok(tags)
    }

    async fn put_object_tags(
        &self,
        bucket: &str,
        key: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        // Read current metadata, merge tag entries, write back
        let obj = self
            .storage_control
            .get_object()
            .set_bucket(self.bucket_name(bucket))
            .set_object(key.to_string())
            .send()
            .await
            .map_err(|e| gcs_err("put_object_tags get", e))?;

        let mut meta: HashMap<String, String> = obj
            .metadata
            .iter()
            .filter(|(k, _)| !k.starts_with("tag:"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (k, v) in tags {
            meta.insert(format!("tag:{k}"), v);
        }

        let update = google_cloud_storage::model::Object::new()
            .set_bucket(self.bucket_name(bucket))
            .set_name(key.to_string())
            .set_metadata(meta);

        self.storage_control
            .update_object()
            .set_object(update)
            .send()
            .await
            .map_err(|e| gcs_err("put_object_tags update", e))?;
        Ok(())
    }

    async fn delete_object_tags(&self, bucket: &str, key: &str) -> Result<(), StorageError> {
        let obj = self
            .storage_control
            .get_object()
            .set_bucket(self.bucket_name(bucket))
            .set_object(key.to_string())
            .send()
            .await
            .map_err(|e| gcs_err("delete_object_tags get", e))?;

        let meta: HashMap<String, String> = obj
            .metadata
            .iter()
            .filter(|(k, _)| !k.starts_with("tag:"))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let update = google_cloud_storage::model::Object::new()
            .set_bucket(self.bucket_name(bucket))
            .set_name(key.to_string())
            .set_metadata(meta);

        self.storage_control
            .update_object()
            .set_object(update)
            .send()
            .await
            .map_err(|e| gcs_err("delete_object_tags update", e))?;
        Ok(())
    }

    // ── Bucket tagging ─────────────────────────────────────────────────────────
    // GCS buckets have labels (key→value) that serve the same purpose as S3
    // bucket tags.

    async fn get_bucket_tags(&self, bucket: &str) -> Result<HashMap<String, String>, StorageError> {
        let bkt = self
            .storage_control
            .get_bucket()
            .set_name(self.bucket_name(bucket))
            .send()
            .await
            .map_err(|e| gcs_err("get_bucket_tags", e))?;
        Ok(bkt.labels.clone())
    }

    async fn put_bucket_tags(
        &self,
        bucket: &str,
        tags: HashMap<String, String>,
    ) -> Result<(), StorageError> {
        let update = google_cloud_storage::model::Bucket::new()
            .set_name(self.bucket_name(bucket))
            .set_labels(tags);
        self.storage_control
            .update_bucket()
            .set_bucket(update)
            .send()
            .await
            .map_err(|e| gcs_err("put_bucket_tags", e))?;
        Ok(())
    }

    async fn delete_bucket_tags(&self, bucket: &str) -> Result<(), StorageError> {
        self.put_bucket_tags(bucket, HashMap::new()).await
    }

    // ── Bucket policy ─────────────────────────────────────────────────────────
    // GCS uses IAM policies rather than S3-style JSON bucket policies.
    // We return NotFound for get (IAM ≠ S3 policy) and no-op for put/delete.

    async fn get_bucket_policy(&self, _bucket: &str) -> Result<String, StorageError> {
        Err(StorageError::NotFound(
            "GCS uses IAM policies, not S3-style bucket policies".to_string(),
        ))
    }

    async fn put_bucket_policy(&self, _bucket: &str, _policy: String) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete_bucket_policy(&self, _bucket: &str) -> Result<(), StorageError> {
        Ok(())
    }

    // ── Stats ──────────────────────────────────────────────────────────────────

    async fn get_storage_stats(&self) -> Result<StorageStats, StorageError> {
        let buckets = self.list_buckets().await?;
        let mut total_objects: u64 = 0;
        let mut total_bytes: u64 = 0;
        for bucket in &buckets {
            let list = self
                .list_objects(&bucket.name, None, None, 1000, None)
                .await?;
            for (_, meta) in &list.objects {
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

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wkt_ts_to_chrono_epoch() {
        let ts = google_cloud_wkt::Timestamp::new(0, 0).expect("zero timestamp");
        let dt = wkt_ts_to_chrono(&ts);
        assert_eq!(dt.timestamp(), 0);
    }

    #[test]
    fn wkt_ts_to_chrono_positive() {
        let ts = google_cloud_wkt::Timestamp::new(1_000_000, 500_000_000).expect("valid timestamp");
        let dt = wkt_ts_to_chrono(&ts);
        assert_eq!(dt.timestamp(), 1_000_000);
    }

    #[test]
    fn gcs_object_to_meta_defaults() {
        let obj = google_cloud_storage::model::Object::new()
            .set_name("test/key.txt")
            .set_size(42)
            .set_etag("abc123")
            .set_content_type("text/plain");
        let meta = gcs_object_to_meta(&obj);
        assert_eq!(meta.key, "test/key.txt");
        assert_eq!(meta.size, 42);
        assert_eq!(meta.etag, "abc123");
        assert_eq!(meta.content_type, "text/plain");
        assert_eq!(meta.schema_version, 1);
    }

    #[test]
    fn gcs_object_to_meta_empty_content_type() {
        let obj = google_cloud_storage::model::Object::new()
            .set_name("x")
            .set_content_type("");
        let meta = gcs_object_to_meta(&obj);
        assert_eq!(meta.content_type, "application/octet-stream");
    }

    #[test]
    fn gcs_mp_state_lifecycle() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio rt");
        rt.block_on(async {
            let id = "test-gcs-mp-id";
            let state = GcsMpState {
                bucket: "bucket".into(),
                key: "key".into(),
                metadata: HashMap::new(),
                parts: Vec::new(),
            };
            GCS_MP_STATE.write().await.insert(id.to_string(), state);
            assert!(GCS_MP_STATE.read().await.contains_key(id));
            GCS_MP_STATE.write().await.remove(id);
            assert!(!GCS_MP_STATE.read().await.contains_key(id));
        });
    }
}
