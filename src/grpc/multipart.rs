//! gRPC Multipart Upload Operations Handlers

use bytes::Bytes;
use futures::StreamExt as FuturesStreamExt;
use std::sync::Arc;
use tonic::{Request, Response, Status, Streaming};
use tracing::debug;

use crate::storage::{ByteRange, StorageEngine};

use super::proto::multipart::*;

/// Convert storage errors to gRPC Status
fn map_storage_error(err: impl std::fmt::Display) -> Status {
    Status::internal(format!("Storage error: {}", err))
}

pub async fn create_multipart_upload(
    storage: Arc<StorageEngine>,
    request: Request<CreateMultipartUploadRequest>,
) -> Result<Response<CreateMultipartUploadResponse>, Status> {
    let req = request.into_inner();

    let content_type = req
        .content_type
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let upload_id = storage
        .create_multipart_upload(&req.bucket, &req.key, &content_type, req.metadata)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(CreateMultipartUploadResponse {
        upload_id,
        bucket: req.bucket,
        key: req.key,
    }))
}

pub async fn upload_part(
    storage: Arc<StorageEngine>,
    request: Request<UploadPartRequest>,
) -> Result<Response<UploadPartResponse>, Status> {
    let req = request.into_inner();

    let data = Bytes::from(req.data);

    let etag = storage
        .upload_part(
            &req.bucket,
            &req.key,
            &req.upload_id,
            req.part_number as u32,
            data,
        )
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(UploadPartResponse { etag }))
}

pub async fn upload_part_stream(
    storage: Arc<StorageEngine>,
    request: Request<Streaming<UploadPartStreamRequest>>,
) -> Result<Response<UploadPartResponse>, Status> {
    let mut stream = request.into_inner();

    // First message should contain metadata
    let first_msg = stream
        .message()
        .await
        .map_err(|e| Status::internal(format!("Failed to receive metadata: {}", e)))?
        .ok_or_else(|| Status::invalid_argument("Empty stream"))?;

    let metadata = match first_msg.request {
        Some(upload_part_stream_request::Request::Metadata(m)) => m,
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
            Some(upload_part_stream_request::Request::Chunk(chunk)) => {
                data.extend_from_slice(&chunk);
            }
            _ => return Err(Status::invalid_argument("Invalid message in stream")),
        }
    }

    debug!(
        "Received {} bytes for part {} of upload {}",
        data.len(),
        metadata.part_number,
        metadata.upload_id
    );

    let etag = storage
        .upload_part(
            &metadata.bucket,
            &metadata.key,
            &metadata.upload_id,
            metadata.part_number as u32,
            Bytes::from(data),
        )
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(UploadPartResponse { etag }))
}

pub async fn upload_part_copy(
    storage: Arc<StorageEngine>,
    request: Request<UploadPartCopyRequest>,
) -> Result<Response<UploadPartCopyResponse>, Status> {
    let req = request.into_inner();

    // Get source object (with or without range)
    let mut data = Vec::new();

    if let (Some(start), Some(end)) = (req.copy_source_range_start, req.copy_source_range_end) {
        // Range request
        let range = ByteRange {
            start: start as u64,
            end: end as u64,
        };
        let (_, mut stream) = storage
            .get_object_range(&req.source_bucket, &req.source_key, &range)
            .await
            .map_err(map_storage_error)?;

        // Collect stream into bytes
        while let Some(chunk_result) = FuturesStreamExt::next(&mut stream).await {
            let chunk = chunk_result.map_err(map_storage_error)?;
            data.extend_from_slice(&chunk);
        }
    } else {
        // Full object
        let (_, mut stream) = storage
            .get_object(&req.source_bucket, &req.source_key)
            .await
            .map_err(map_storage_error)?;

        // Collect stream into bytes
        while let Some(chunk_result) = FuturesStreamExt::next(&mut stream).await {
            let chunk = chunk_result.map_err(map_storage_error)?;
            data.extend_from_slice(&chunk);
        }
    }

    // Upload as part
    let etag = storage
        .upload_part(
            &req.dest_bucket,
            &req.dest_key,
            &req.upload_id,
            req.part_number as u32,
            Bytes::from(data),
        )
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(UploadPartCopyResponse {
        etag,
        last_modified: Some(prost_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        }),
    }))
}

pub async fn complete_multipart_upload(
    storage: Arc<StorageEngine>,
    request: Request<CompleteMultipartUploadRequest>,
) -> Result<Response<CompleteMultipartUploadResponse>, Status> {
    let req = request.into_inner();

    let parts: Vec<(u32, String)> = req
        .parts
        .into_iter()
        .map(|p| (p.part_number as u32, p.etag))
        .collect();

    let etag = storage
        .complete_multipart_upload(&req.bucket, &req.key, &req.upload_id, &parts)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(CompleteMultipartUploadResponse {
        location: format!("/{}/{}", req.bucket, req.key),
        bucket: req.bucket,
        key: req.key,
        etag,
        version_id: None,
    }))
}

pub async fn abort_multipart_upload(
    storage: Arc<StorageEngine>,
    request: Request<AbortMultipartUploadRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    storage
        .abort_multipart_upload(&req.bucket, &req.key, &req.upload_id)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

pub async fn list_parts(
    storage: Arc<StorageEngine>,
    request: Request<ListPartsRequest>,
) -> Result<Response<ListPartsResponse>, Status> {
    let req = request.into_inner();

    let max_parts = req.max_parts.unwrap_or(1000) as usize;
    let part_number_marker = req.part_number_marker.unwrap_or(0) as usize;

    let all_parts = storage
        .list_parts(&req.bucket, &req.key, &req.upload_id)
        .await
        .map_err(map_storage_error)?;

    // Apply pagination
    let start_index = part_number_marker;
    let end_index = (start_index + max_parts).min(all_parts.len());
    let is_truncated = end_index < all_parts.len();

    let parts: Vec<Part> = all_parts[start_index..end_index]
        .iter()
        .map(|p| Part {
            part_number: p.part_number as i32,
            etag: p.etag.clone(),
            size: p.size,
            last_modified: Some(prost_types::Timestamp {
                seconds: p.last_modified.timestamp(),
                nanos: p.last_modified.timestamp_subsec_nanos() as i32,
            }),
        })
        .collect();

    Ok(Response::new(ListPartsResponse {
        bucket: req.bucket,
        key: req.key,
        upload_id: req.upload_id,
        parts,
        is_truncated,
        next_part_number_marker: if is_truncated {
            Some(end_index as i32)
        } else {
            None
        },
        max_parts: max_parts as i32,
        initiated: Some(prost_types::Timestamp {
            seconds: chrono::Utc::now().timestamp(),
            nanos: 0,
        }),
        storage_class: Some("STANDARD".to_string()),
    }))
}

pub async fn list_multipart_uploads(
    storage: Arc<StorageEngine>,
    request: Request<ListMultipartUploadsRequest>,
) -> Result<Response<ListMultipartUploadsResponse>, Status> {
    let req = request.into_inner();

    let max_uploads = req.max_uploads.unwrap_or(1000) as usize;

    let all_uploads = storage
        .list_multipart_uploads(&req.bucket, req.prefix.as_deref())
        .await
        .map_err(map_storage_error)?;

    // Apply simple pagination (basic implementation)
    let is_truncated = all_uploads.len() > max_uploads;
    let page_uploads: Vec<_> = all_uploads.into_iter().take(max_uploads).collect();

    let uploads: Vec<MultipartUpload> = page_uploads
        .iter()
        .map(|u| MultipartUpload {
            upload_id: u.upload_id.clone(),
            key: u.key.clone(),
            initiated: Some(prost_types::Timestamp {
                seconds: u.initiated.timestamp(),
                nanos: u.initiated.timestamp_subsec_nanos() as i32,
            }),
            storage_class: Some("STANDARD".to_string()),
        })
        .collect();

    Ok(Response::new(ListMultipartUploadsResponse {
        bucket: req.bucket,
        uploads,
        common_prefixes: Vec::new(), // Not implemented in storage layer
        is_truncated,
        next_key_marker: None,       // Simplified - not tracking markers
        next_upload_id_marker: None, // Simplified - not tracking markers
        delimiter: req.delimiter,
        prefix: req.prefix,
        max_uploads: max_uploads as i32,
    }))
}
