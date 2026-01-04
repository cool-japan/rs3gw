//! Multipart Upload Handlers
//!
//! Implements S3 multipart upload operations:
//! - CreateMultipartUpload
//! - UploadPart
//! - UploadPartCopy
//! - CompleteMultipartUpload
//! - AbortMultipartUpload
//! - ListParts

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use serde::Deserialize;
use tracing::info;

use crate::api::websocket::{S3Event, S3EventType};
use crate::storage::ByteRange;
use crate::AppState;

use super::handlers::storage_error_to_response;
use super::utils::{error_response, etag_matches, parse_complete_multipart_parts, parse_http_date};
use super::xml_responses::{
    CompleteMultipartUploadResult, CopyPartResult, InitiateMultipartUploadResult, ListPartsResult,
    PartElement,
};

/// Query parameters for multipart operations
#[derive(Debug, Deserialize, Default)]
pub struct MultipartQuery {
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,
    #[serde(rename = "partNumber")]
    pub part_number: Option<u32>,
    pub uploads: Option<String>,
}

/// Initiate a multipart upload
pub async fn create_multipart_upload(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    // Record metrics for this request
    state.metrics_tracker.record_request();

    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    info!(
        bucket = %bucket,
        key = %key,
        content_type = %content_type,
        "CreateMultipartUpload"
    );

    // Extract custom metadata
    let mut metadata = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        if let Some(meta_key) = name.as_str().strip_prefix("x-amz-meta-") {
            if let Ok(v) = value.to_str() {
                metadata.insert(meta_key.to_string(), v.to_string());
            }
        }
    }

    match state
        .storage
        .create_multipart_upload(&bucket, &key, content_type, metadata)
        .await
    {
        Ok(upload_id) => {
            // Broadcast multipart upload created event
            let event = S3Event::new(S3EventType::MultipartUploadCreated, bucket.clone())
                .with_key(key.clone())
                .with_metadata("uploadId".to_string(), upload_id.clone());
            state.event_broadcaster.broadcast(event);

            let result = InitiateMultipartUploadResult::new(&bucket, &key, &upload_id);
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Upload a part
pub async fn upload_part(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<MultipartQuery>,
    body: Bytes,
) -> Response {
    // Record metrics for this request
    state.metrics_tracker.record_request();

    let upload_id = match params.upload_id {
        Some(id) => id,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing uploadId parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    let part_number = match params.part_number {
        Some(n) => n,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing partNumber parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    info!(
        bucket = %bucket,
        key = %key,
        upload_id = %upload_id,
        part_number = %part_number,
        size = %body.len(),
        "UploadPart"
    );

    let body_size = body.len() as u64;
    match state
        .storage
        .upload_part(&bucket, &key, &upload_id, part_number, body)
        .await
    {
        Ok(etag) => {
            // Record bytes uploaded for metrics
            state.metrics_tracker.record_bytes_uploaded(body_size);

            Response::builder()
                .status(StatusCode::OK)
                .header("ETag", format!("\"{}\"", etag))
                .header("x-amz-request-id", uuid::Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Upload a part by copying from another object
pub async fn upload_part_copy(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<MultipartQuery>,
    headers: HeaderMap,
) -> Response {
    let upload_id = match params.upload_id {
        Some(id) => id,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing uploadId parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    let part_number = match params.part_number {
        Some(n) => n,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing partNumber parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    // Get copy source from header
    let copy_source = match headers.get("x-amz-copy-source") {
        Some(value) => match value.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidRequest",
                    "Invalid x-amz-copy-source header",
                    &format!("/{}/{}", bucket, key),
                );
            }
        },
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing x-amz-copy-source header",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    // Parse copy source (format: /bucket/key or bucket/key)
    let source = copy_source.trim_start_matches('/');
    let source_parts: Vec<&str> = source.splitn(2, '/').collect();
    if source_parts.len() != 2 {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "Invalid x-amz-copy-source format",
            &format!("/{}/{}", bucket, key),
        );
    }
    let source_bucket = source_parts[0];
    let source_key = source_parts[1];

    // Check conditional headers against source object
    let copy_if_match = headers
        .get("x-amz-copy-source-if-match")
        .and_then(|v| v.to_str().ok());
    let copy_if_none_match = headers
        .get("x-amz-copy-source-if-none-match")
        .and_then(|v| v.to_str().ok());
    let copy_if_modified_since = headers
        .get("x-amz-copy-source-if-modified-since")
        .and_then(|v| v.to_str().ok());
    let copy_if_unmodified_since = headers
        .get("x-amz-copy-source-if-unmodified-since")
        .and_then(|v| v.to_str().ok());

    if copy_if_match.is_some()
        || copy_if_none_match.is_some()
        || copy_if_modified_since.is_some()
        || copy_if_unmodified_since.is_some()
    {
        let src_meta = match state.storage.head_object(source_bucket, source_key).await {
            Ok(m) => m,
            Err(e) => {
                return storage_error_to_response(e, &format!("/{}/{}", source_bucket, source_key));
            }
        };

        // Check x-amz-copy-source-if-match
        if let Some(expected_etag) = copy_if_match {
            if !etag_matches(&src_meta.etag, expected_etag) {
                return error_response(
                    StatusCode::PRECONDITION_FAILED,
                    "PreconditionFailed",
                    "At least one of the pre-conditions you specified did not hold",
                    &format!("/{}/{}", bucket, key),
                );
            }
        }

        // Check x-amz-copy-source-if-none-match
        if let Some(expected_etag) = copy_if_none_match {
            if etag_matches(&src_meta.etag, expected_etag) {
                return error_response(
                    StatusCode::PRECONDITION_FAILED,
                    "PreconditionFailed",
                    "At least one of the pre-conditions you specified did not hold",
                    &format!("/{}/{}", bucket, key),
                );
            }
        }

        // Check x-amz-copy-source-if-modified-since
        if let Some(since_str) = copy_if_modified_since {
            if let Ok(since) = parse_http_date(since_str) {
                if src_meta.last_modified <= since {
                    return error_response(
                        StatusCode::PRECONDITION_FAILED,
                        "PreconditionFailed",
                        "At least one of the pre-conditions you specified did not hold",
                        &format!("/{}/{}", bucket, key),
                    );
                }
            }
        }

        // Check x-amz-copy-source-if-unmodified-since
        if let Some(since_str) = copy_if_unmodified_since {
            if let Ok(since) = parse_http_date(since_str) {
                if src_meta.last_modified > since {
                    return error_response(
                        StatusCode::PRECONDITION_FAILED,
                        "PreconditionFailed",
                        "At least one of the pre-conditions you specified did not hold",
                        &format!("/{}/{}", bucket, key),
                    );
                }
            }
        }
    }

    // Parse optional copy source range (format: bytes=start-end)
    let range = headers
        .get("x-amz-copy-source-range")
        .and_then(|v| v.to_str().ok())
        .and_then(|range_str| {
            // We need to get file size first, but for simplicity we'll parse the range
            // and let the storage engine handle validation
            let range_str = range_str.strip_prefix("bytes=")?;
            let parts: Vec<&str> = range_str.split('-').collect();
            if parts.len() != 2 {
                return None;
            }
            let start: u64 = parts[0].parse().ok()?;
            let end: u64 = parts[1].parse().ok()?;
            Some(ByteRange { start, end })
        });

    info!(
        bucket = %bucket,
        key = %key,
        upload_id = %upload_id,
        part_number = %part_number,
        source = %copy_source,
        range = ?range,
        "UploadPartCopy"
    );

    match state
        .storage
        .upload_part_copy(
            &bucket,
            &key,
            &upload_id,
            part_number,
            source_bucket,
            source_key,
            range,
        )
        .await
    {
        Ok((etag, last_modified)) => {
            let result = CopyPartResult::new(
                &format!("\"{}\"", etag),
                &last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
            );
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .header("x-amz-copy-source-version-id", "null")
                .header("x-amz-request-id", uuid::Uuid::new_v4().to_string())
                .body(Body::from(result.to_xml()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Complete a multipart upload
pub async fn complete_multipart_upload(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<MultipartQuery>,
    body: Bytes,
) -> Response {
    // Record metrics for this request
    state.metrics_tracker.record_request();

    let upload_id = match params.upload_id {
        Some(id) => id,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing uploadId parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    info!(
        bucket = %bucket,
        key = %key,
        upload_id = %upload_id,
        "CompleteMultipartUpload"
    );

    // Parse the XML body to get part list
    let body_str = String::from_utf8_lossy(&body);
    let parts = parse_complete_multipart_parts(&body_str);

    if parts.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "MalformedXML",
            "The XML you provided was not well-formed",
            &format!("/{}/{}", bucket, key),
        );
    }

    match state
        .storage
        .complete_multipart_upload(&bucket, &key, &upload_id, &parts)
        .await
    {
        Ok(etag) => {
            // Broadcast multipart upload completed event
            let event = S3Event::new(S3EventType::MultipartUploadCompleted, bucket.clone())
                .with_key(key.clone())
                .with_etag(etag.clone());
            state.event_broadcaster.broadcast(event);

            let location = format!("/{}/{}", bucket, key);
            let result = CompleteMultipartUploadResult::new(
                &location,
                &bucket,
                &key,
                &format!("\"{}\"", etag),
            );
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Abort a multipart upload
pub async fn abort_multipart_upload(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<MultipartQuery>,
) -> Response {
    let upload_id = match params.upload_id {
        Some(id) => id,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing uploadId parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    info!(
        bucket = %bucket,
        key = %key,
        upload_id = %upload_id,
        "AbortMultipartUpload"
    );

    match state
        .storage
        .abort_multipart_upload(&bucket, &key, &upload_id)
        .await
    {
        Ok(()) => {
            // Broadcast multipart upload aborted event
            let event = S3Event::new(S3EventType::MultipartUploadAborted, bucket.clone())
                .with_key(key.clone())
                .with_metadata("uploadId".to_string(), upload_id.clone());
            state.event_broadcaster.broadcast(event);

            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// List parts for a multipart upload
pub async fn list_parts(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<MultipartQuery>,
) -> Response {
    let upload_id = match params.upload_id {
        Some(id) => id,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing uploadId parameter",
                &format!("/{}/{}", bucket, key),
            );
        }
    };

    info!(
        bucket = %bucket,
        key = %key,
        upload_id = %upload_id,
        "ListParts"
    );

    match state.storage.list_parts(&bucket, &key, &upload_id).await {
        Ok(parts) => {
            let mut result = ListPartsResult::new(&bucket, &key, &upload_id);

            result.parts = parts
                .into_iter()
                .map(|p| PartElement {
                    part_number: p.part_number,
                    last_modified: p.last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                    etag: format!("\"{}\"", p.etag),
                    size: p.size,
                })
                .collect();

            if let Some(last) = result.parts.last() {
                result.next_part_number_marker = last.part_number;
            }

            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
