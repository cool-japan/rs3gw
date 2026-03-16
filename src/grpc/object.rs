//! gRPC Object Operations Handlers

use bytes::{Bytes, BytesMut};
use futures::stream::{self, StreamExt as FuturesStreamExt};
use futures::Stream;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::StreamExt as TokioStreamExt;
use tonic::{Request, Response, Status, Streaming};

use crate::storage::{ByteRange, ObjectTagging, StorageEngine};

use super::proto::object::*;

pub type ListObjectsStream = Pin<Box<dyn Stream<Item = Result<ObjectInfo, Status>> + Send>>;
pub type GetObjectStream = Pin<Box<dyn Stream<Item = Result<GetObjectResponse, Status>> + Send>>;

/// Convert storage errors to gRPC Status
fn map_storage_error(err: impl std::fmt::Display) -> Status {
    Status::internal(format!("Storage error: {}", err))
}

/// List objects as a stream
pub async fn list_objects_stream(
    storage: Arc<StorageEngine>,
    request: Request<ListObjectsRequest>,
) -> Result<Response<ListObjectsStream>, Status> {
    let req = request.into_inner();

    let prefix = req.prefix.as_deref().unwrap_or("");
    let max_keys = req.max_keys.unwrap_or(1000) as usize;

    let (objects, _common_prefixes, _is_truncated) = storage
        .list_objects_with_pagination(
            &req.bucket,
            prefix,
            req.delimiter.as_deref(),
            max_keys,
            req.start_after.as_deref(),
        )
        .await
        .map_err(map_storage_error)?;

    let stream = tokio_stream::iter(objects.into_iter().map(|obj| {
        Ok(ObjectInfo {
            key: obj.key,
            size: obj.size,
            etag: obj.etag,
            last_modified: Some(prost_types::Timestamp {
                seconds: obj.last_modified.timestamp(),
                nanos: obj.last_modified.timestamp_subsec_nanos() as i32,
            }),
            storage_class: "STANDARD".to_string(),
            version_id: None,
            is_delete_marker: false,
        })
    }));

    Ok(Response::new(Box::pin(stream) as ListObjectsStream))
}

/// List objects with pagination (unary)
pub async fn list_objects_paginated(
    storage: Arc<StorageEngine>,
    request: Request<ListObjectsRequest>,
) -> Result<Response<ListObjectsResponse>, Status> {
    let req = request.into_inner();

    let prefix = req.prefix.as_deref().unwrap_or("");
    let max_keys = req.max_keys.unwrap_or(1000) as usize;

    // Continuation token (S3 ListObjectsV2 style) takes precedence over start_after.
    // The token encodes the last key seen on the previous page; we resume from there.
    let effective_start_after = req
        .continuation_token
        .as_deref()
        .or(req.start_after.as_deref());

    let (object_list, common_prefixes, is_truncated) = storage
        .list_objects_with_pagination(
            &req.bucket,
            prefix,
            req.delimiter.as_deref(),
            max_keys,
            effective_start_after,
        )
        .await
        .map_err(map_storage_error)?;

    let objects: Vec<ObjectInfo> = object_list
        .iter()
        .map(|obj| ObjectInfo {
            key: obj.key.clone(),
            size: obj.size,
            etag: obj.etag.clone(),
            last_modified: Some(prost_types::Timestamp {
                seconds: obj.last_modified.timestamp(),
                nanos: obj.last_modified.timestamp_subsec_nanos() as i32,
            }),
            storage_class: "STANDARD".to_string(),
            version_id: None,
            is_delete_marker: false,
        })
        .collect();

    let key_count = objects.len();

    // Generate continuation token if results are truncated
    let next_continuation_token = if is_truncated {
        // Use the last object key as continuation token
        object_list.last().map(|obj| obj.key.clone())
    } else {
        None
    };

    Ok(Response::new(ListObjectsResponse {
        bucket: req.bucket,
        objects,
        common_prefixes,
        is_truncated,
        next_continuation_token,
        delimiter: req.delimiter,
        prefix: Some(prefix.to_string()),
        max_keys: max_keys as i32,
        key_count: key_count as i32,
    }))
}

/// Get object (unary - loads entire object, or partial if range fields set)
pub async fn get_object(
    storage: Arc<StorageEngine>,
    request: Request<GetObjectRequest>,
) -> Result<Response<GetObjectResponse>, Status> {
    let req = request.into_inner();

    let has_range = req.range_start.is_some() || req.range_end.is_some();

    if has_range {
        // Range GET: fetch only the requested byte range.
        // First, HEAD to get total size for computing defaults and validation.
        let head_meta = storage
            .head_object(&req.bucket, &req.key)
            .await
            .map_err(map_storage_error)?;

        let total_size = head_meta.size;
        let range_start = req.range_start.unwrap_or(0);
        let range_end = req.range_end.unwrap_or(total_size as i64 - 1);

        // Validate range
        if range_start < 0 || range_end < 0 {
            return Err(Status::invalid_argument(
                "Range values must be non-negative",
            ));
        }
        if range_start as u64 >= total_size {
            return Err(Status::out_of_range(format!(
                "range_start ({}) exceeds object size ({})",
                range_start, total_size
            )));
        }
        if range_end < range_start {
            return Err(Status::invalid_argument(format!(
                "range_end ({}) must be >= range_start ({})",
                range_end, range_start
            )));
        }

        // Clamp range_end to object size - 1
        let effective_end = (range_end as u64).min(total_size - 1);

        let byte_range = ByteRange {
            start: range_start as u64,
            end: effective_end,
        };

        let (meta, mut range_stream) = storage
            .get_object_range(&req.bucket, &req.key, &byte_range)
            .await
            .map_err(map_storage_error)?;

        let mut data = BytesMut::new();
        while let Some(chunk_result) = FuturesStreamExt::next(&mut range_stream).await {
            let chunk = chunk_result.map_err(map_storage_error)?;
            data.extend_from_slice(&chunk);
        }

        let obj_metadata = ObjectMetadata {
            key: meta.key,
            bucket: req.bucket,
            size: meta.size,
            etag: meta.etag,
            last_modified: Some(prost_types::Timestamp {
                seconds: meta.last_modified.timestamp(),
                nanos: meta.last_modified.timestamp_subsec_nanos() as i32,
            }),
            content_type: meta.content_type,
            metadata: meta.metadata,
            version_id: None,
            storage_class: Some("STANDARD".to_string()),
            checksum_crc32c: None,
            checksum_crc32: None,
            checksum_sha256: None,
            checksum_sha1: None,
        };

        Ok(Response::new(GetObjectResponse {
            metadata: Some(obj_metadata),
            data: data.to_vec(),
            content_range_start: Some(range_start),
            content_range_end: Some(effective_end as i64),
            content_range_total: Some(total_size as i64),
        }))
    } else {
        // Full GET: fetch entire object.
        let (meta, mut obj_stream) = storage
            .get_object(&req.bucket, &req.key)
            .await
            .map_err(map_storage_error)?;

        let mut data = BytesMut::new();
        while let Some(chunk_result) = FuturesStreamExt::next(&mut obj_stream).await {
            let chunk = chunk_result.map_err(map_storage_error)?;
            data.extend_from_slice(&chunk);
        }

        let obj_metadata = ObjectMetadata {
            key: meta.key,
            bucket: req.bucket,
            size: meta.size,
            etag: meta.etag,
            last_modified: Some(prost_types::Timestamp {
                seconds: meta.last_modified.timestamp(),
                nanos: meta.last_modified.timestamp_subsec_nanos() as i32,
            }),
            content_type: meta.content_type,
            metadata: meta.metadata,
            version_id: None,
            storage_class: Some("STANDARD".to_string()),
            checksum_crc32c: None,
            checksum_crc32: None,
            checksum_sha256: None,
            checksum_sha1: None,
        };

        Ok(Response::new(GetObjectResponse {
            metadata: Some(obj_metadata),
            data: data.to_vec(),
            content_range_start: None,
            content_range_end: None,
            content_range_total: None,
        }))
    }
}

/// Get object as a stream (for large objects)
pub async fn get_object_stream(
    storage: Arc<StorageEngine>,
    request: Request<GetObjectRequest>,
) -> Result<Response<GetObjectStream>, Status> {
    let req = request.into_inner();
    let bucket = req.bucket.clone();
    let key = req.key.clone();

    // Get metadata first
    let metadata = storage
        .head_object(&bucket, &key)
        .await
        .map_err(map_storage_error)?;

    let obj_metadata = ObjectMetadata {
        key: metadata.key,
        bucket: bucket.clone(),
        size: metadata.size,
        etag: metadata.etag.clone(),
        last_modified: Some(prost_types::Timestamp {
            seconds: metadata.last_modified.timestamp(),
            nanos: metadata.last_modified.timestamp_subsec_nanos() as i32,
        }),
        content_type: metadata.content_type,
        metadata: metadata.metadata,
        version_id: None,
        storage_class: Some("STANDARD".to_string()),
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    };

    // Stream the object in chunks
    const CHUNK_SIZE: u64 = 256 * 1024; // 256KB chunks
    let total_size = metadata.size;
    let num_chunks = total_size.div_ceil(CHUNK_SIZE);

    let stream = TokioStreamExt::then(tokio_stream::iter(0..num_chunks), move |chunk_idx: u64| {
        let storage = storage.clone();
        let bucket = bucket.clone();
        let key = key.clone();
        let obj_metadata = obj_metadata.clone();

        async move {
            let start: u64 = chunk_idx * CHUNK_SIZE;
            let end: u64 = ((chunk_idx + 1) * CHUNK_SIZE - 1).min(total_size - 1);

            let range = ByteRange { start, end };
            match storage.get_object_range(&bucket, &key, &range).await {
                Ok((_, mut stream)) => {
                    // Collect stream into bytes
                    let mut data = Vec::new();
                    while let Some(chunk_result) = TokioStreamExt::next(&mut stream).await {
                        match chunk_result {
                            Ok(chunk) => data.extend_from_slice(&chunk),
                            Err(e) => return Err(Status::internal(format!("Stream error: {}", e))),
                        }
                    }
                    Ok(GetObjectResponse {
                        metadata: Some(obj_metadata),
                        data,
                        content_range_start: Some(start as i64),
                        content_range_end: Some(end as i64),
                        content_range_total: Some(total_size as i64),
                    })
                }
                Err(e) => Err(Status::internal(format!("Failed to read chunk: {}", e))),
            }
        }
    });

    Ok(Response::new(Box::pin(stream) as GetObjectStream))
}

/// Put object (unary - entire object in one request)
pub async fn put_object(
    storage: Arc<StorageEngine>,
    request: Request<PutObjectRequest>,
) -> Result<Response<PutObjectResponse>, Status> {
    let req = request.into_inner();

    let data = Bytes::from(req.data);
    let content_type = req
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Note: storage_class is currently not used by the storage engine
    // Future enhancement: pass storage class to storage.put_object

    let etag = storage
        .put_object(&req.bucket, &req.key, &content_type, req.metadata, data)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(PutObjectResponse {
        etag,
        version_id: None,
    }))
}

/// Put object from stream (client streaming)
pub async fn put_object_stream(
    storage: Arc<StorageEngine>,
    request: Request<Streaming<PutObjectStreamRequest>>,
) -> Result<Response<PutObjectResponse>, Status> {
    let mut stream = request.into_inner();

    // First message should contain metadata
    let first_msg = stream
        .message()
        .await
        .map_err(|e| Status::internal(format!("Failed to receive metadata: {}", e)))?
        .ok_or_else(|| Status::invalid_argument("Empty stream"))?;

    let metadata = match first_msg.request {
        Some(put_object_stream_request::Request::Metadata(m)) => m,
        _ => {
            return Err(Status::invalid_argument(
                "First message must contain metadata",
            ))
        }
    };

    // Collect data chunks
    let mut data = Vec::new();
    while let Some(msg) = stream
        .message()
        .await
        .map_err(|e| Status::internal(format!("Stream error: {}", e)))?
    {
        match msg.request {
            Some(put_object_stream_request::Request::Chunk(chunk)) => {
                data.extend_from_slice(&chunk);
            }
            _ => return Err(Status::invalid_argument("Invalid message in stream")),
        }
    }

    let content_type = metadata
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // Note: storage_class is currently not used by the storage engine
    // Future enhancement: pass storage class to storage.put_object

    let etag = storage
        .put_object(
            &metadata.bucket,
            &metadata.key,
            &content_type,
            metadata.metadata,
            Bytes::from(data),
        )
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(PutObjectResponse {
        etag,
        version_id: None,
    }))
}

/// Delete object
pub async fn delete_object(
    storage: Arc<StorageEngine>,
    request: Request<DeleteObjectRequest>,
) -> Result<Response<DeleteObjectResponse>, Status> {
    let req = request.into_inner();

    storage
        .delete_object(&req.bucket, &req.key)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(DeleteObjectResponse {
        deleted: true,
        version_id: None,
        delete_marker: false,
    }))
}

/// Delete multiple objects (parallel execution with concurrency limit)
pub async fn delete_objects(
    storage: Arc<StorageEngine>,
    request: Request<DeleteObjectsRequest>,
) -> Result<Response<DeleteObjectsResponse>, Status> {
    let req = request.into_inner();
    let bucket = req.bucket;

    let futs: Vec<_> = req
        .keys
        .into_iter()
        .map(|key: String| {
            let storage = storage.clone();
            let bucket = bucket.clone();
            async move {
                let result = storage
                    .delete_object(&bucket, &key)
                    .await
                    .map_err(|e| format!("{}", e));
                (key, result)
            }
        })
        .collect();
    let results: Vec<(String, Result<(), String>)> =
        FuturesStreamExt::collect(FuturesStreamExt::buffer_unordered(stream::iter(futs), 10)).await;

    let mut deleted = Vec::new();
    let mut errors = Vec::new();

    for (key, result) in results {
        match result {
            Ok(()) => {
                deleted.push(DeletedObject {
                    key,
                    version_id: None,
                    delete_marker: false,
                });
            }
            Err(e) => {
                errors.push(DeleteError {
                    key,
                    code: "InternalError".to_string(),
                    message: e,
                });
            }
        }
    }

    Ok(Response::new(DeleteObjectsResponse { deleted, errors }))
}

/// Head object (get metadata only)
pub async fn head_object(
    storage: Arc<StorageEngine>,
    request: Request<HeadObjectRequest>,
) -> Result<Response<HeadObjectResponse>, Status> {
    let req = request.into_inner();

    let metadata = storage
        .head_object(&req.bucket, &req.key)
        .await
        .map_err(map_storage_error)?;

    let obj_metadata = ObjectMetadata {
        key: metadata.key,
        bucket: req.bucket,
        size: metadata.size,
        etag: metadata.etag,
        last_modified: Some(prost_types::Timestamp {
            seconds: metadata.last_modified.timestamp(),
            nanos: metadata.last_modified.timestamp_subsec_nanos() as i32,
        }),
        content_type: metadata.content_type,
        metadata: metadata.metadata,
        version_id: None,
        storage_class: Some("STANDARD".to_string()),
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    };

    Ok(Response::new(HeadObjectResponse {
        metadata: Some(obj_metadata),
    }))
}

/// Copy object
pub async fn copy_object(
    storage: Arc<StorageEngine>,
    request: Request<CopyObjectRequest>,
) -> Result<Response<CopyObjectResponse>, Status> {
    let req = request.into_inner();

    let metadata_directive = req.metadata_directive.as_deref();

    let new_metadata = if metadata_directive == Some("REPLACE") {
        Some(req.metadata)
    } else {
        None
    };

    // Note: CopyObjectRequest doesn't have content_type field in proto
    // Using None for now - can be enhanced if needed
    let new_content_type = None;

    let result_metadata = storage
        .copy_object(
            &req.source_bucket,
            &req.source_key,
            &req.dest_bucket,
            &req.dest_key,
            metadata_directive,
            new_metadata,
            new_content_type,
        )
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(CopyObjectResponse {
        etag: result_metadata.etag,
        last_modified: Some(prost_types::Timestamp {
            seconds: result_metadata.last_modified.timestamp(),
            nanos: result_metadata.last_modified.timestamp_subsec_nanos() as i32,
        }),
        version_id: None,
    }))
}

/// Get object attributes
pub async fn get_object_attributes(
    storage: Arc<StorageEngine>,
    request: Request<GetObjectAttributesRequest>,
) -> Result<Response<GetObjectAttributesResponse>, Status> {
    let req = request.into_inner();

    let metadata = storage
        .head_object(&req.bucket, &req.key)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(GetObjectAttributesResponse {
        etag: metadata.etag,
        size: metadata.size,
        storage_class: "STANDARD".to_string(),
        last_modified: Some(prost_types::Timestamp {
            seconds: metadata.last_modified.timestamp(),
            nanos: metadata.last_modified.timestamp_subsec_nanos() as i32,
        }),
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    }))
}

/// Put object tagging
pub async fn put_object_tagging(
    storage: Arc<StorageEngine>,
    request: Request<PutObjectTaggingRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    let tagging = ObjectTagging { tags: req.tags };
    storage
        .put_object_tagging(&req.bucket, &req.key, &tagging)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

/// Get object tagging
pub async fn get_object_tagging(
    storage: Arc<StorageEngine>,
    request: Request<GetObjectTaggingRequest>,
) -> Result<Response<GetObjectTaggingResponse>, Status> {
    let req = request.into_inner();

    let tagging = storage
        .get_object_tagging(&req.bucket, &req.key)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(GetObjectTaggingResponse {
        tags: tagging.tags,
        version_id: None,
    }))
}

/// Delete object tagging
pub async fn delete_object_tagging(
    storage: Arc<StorageEngine>,
    request: Request<DeleteObjectTaggingRequest>,
) -> Result<Response<DeleteObjectTaggingResponse>, Status> {
    let req = request.into_inner();

    storage
        .delete_object_tagging(&req.bucket, &req.key)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(DeleteObjectTaggingResponse {
        version_id: None,
    }))
}
