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

use crate::api::sse::{resolve_sse, SseDecision};
use crate::api::websocket::{S3Event, S3EventType};
use crate::storage::{ByteRange, ObjectSseSidecar};
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
    #[serde(rename = "max-parts")]
    pub max_parts: Option<u32>,
    #[serde(rename = "part-number-marker")]
    pub part_number_marker: Option<u32>,
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

    // Resolve SSE decision: keeps 501 for aws:kms / aws:kms:dsse; handles AES256.
    let sse_decision = match resolve_sse(&state, &bucket, &headers).await {
        Ok(d) => d,
        Err(err_response) => return err_response,
    };

    // Extract custom metadata
    let mut metadata = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        if let Some(meta_key) = name.as_str().strip_prefix("x-amz-meta-") {
            if let Ok(v) = value.to_str() {
                metadata.insert(meta_key.to_string(), v.to_string());
            }
        }
    }

    // SSE-KMS multipart is deferred — return 501 rather than silently storing plaintext.
    if matches!(&sse_decision, SseDecision::SseKms { .. }) {
        return error_response(
            StatusCode::NOT_IMPLEMENTED,
            "NotImplemented",
            "SSE-KMS for multipart uploads is not yet supported.",
            &format!("/{}/{}", bucket, key),
        );
    }

    // Capture the AES256 flag before consuming `sse_decision` in the match below,
    // so that the response header can be set without holding the enum across `.await`.
    let is_sse_aes256 = matches!(&sse_decision, SseDecision::Aes256);

    // Stamp SSE algorithm into metadata so that complete_multipart_upload can
    // pick it up later for post-encryption, and GetObject / HeadObject can emit
    // the correct header without loading the sidecar.
    match sse_decision {
        SseDecision::Aes256 => {
            metadata.insert("__sse_algorithm__".to_string(), "AES256".to_string());
        }
        SseDecision::None => {}
        // Forward-compat: ignore unknown variants (e.g., SseDecision::SseC from Phase 1A).
        _ => {}
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
            let mut builder = axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml");
            if is_sse_aes256 {
                builder = builder.header("x-amz-server-side-encryption", "AES256");
            }
            builder
                .body(axum::body::Body::from(result.to_xml()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
    // SSE on multipart is deferred to Session 6 — return 501 rather than silently
    // storing plaintext while the client believes the object is encrypted.
    if headers.get("x-amz-server-side-encryption").is_some() {
        return crate::api::utils::error_response(
            axum::http::StatusCode::NOT_IMPLEMENTED,
            "NotImplemented",
            "SSE-S3 for multipart uploads is not yet supported. Use single-part PutObject with x-amz-server-side-encryption: AES256.",
            &format!("/{}/{}", bucket, key),
        );
    }

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
            // SSE-S3 post-encryption: if the multipart was initiated with AES256,
            // the assembled object is currently plaintext. Encrypt it now using the
            // chunked (v2) format so the resulting object supports seekable range-GET
            // (a range read fetches only the covering ciphertext chunks from disk,
            // never the whole object).
            //
            // We read the __sse_algorithm__ flag from the object metadata that
            // complete_multipart_upload preserved from the MultipartMetadata.
            let sse_algorithm = state
                .storage
                .head_object(&bucket, &key)
                .await
                .ok()
                .and_then(|m| m.metadata.get("__sse_algorithm__").cloned());

            let is_sse = sse_algorithm.as_deref() == Some("AES256");

            if is_sse {
                // Read the assembled plaintext directly from disk using the canonical path.
                let obj_path = state.storage.object_path(&bucket, &key);
                match tokio::fs::read(&obj_path).await {
                    Ok(plaintext_bytes) => {
                        let aad = format!("{}/{}", bucket, key);
                        match state
                            .encryption
                            .encrypt_chunked(&plaintext_bytes, Some(aad.as_bytes()))
                            .await
                        {
                            Ok(enc) => {
                                let sidecar = ObjectSseSidecar::from_encrypted(&enc, &bucket, &key);
                                if let Err(e) = state
                                    .storage
                                    .overwrite_object_ciphertext(&bucket, &key, &enc.ciphertext)
                                    .await
                                {
                                    tracing::warn!(
                                        "Failed to overwrite multipart plaintext with ciphertext \
                                        for {}/{}: {}",
                                        bucket,
                                        key,
                                        e
                                    );
                                }
                                if let Err(e) =
                                    state.storage.put_object_sse(&bucket, &key, &sidecar).await
                                {
                                    tracing::warn!(
                                        "Failed to write SSE sidecar for multipart {}/{}: {}",
                                        bucket,
                                        key,
                                        e
                                    );
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Encryption failed for multipart {}/{}: {}; \
                                    object stored as plaintext",
                                    bucket,
                                    key,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Could not read assembled multipart object {}/{} for SSE: {}; \
                            object stored as plaintext",
                            bucket,
                            key,
                            e
                        );
                    }
                }
            }

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
            let mut builder = axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml");
            if is_sse {
                builder = builder.header("x-amz-server-side-encryption", "AES256");
            }
            builder
                .body(axum::body::Body::from(result.to_xml()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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

/// List parts for a multipart upload (supports max-parts and part-number-marker pagination)
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

    let max_parts = params.max_parts.unwrap_or(1000) as usize;
    let part_number_marker = params.part_number_marker.unwrap_or(0);

    info!(
        bucket = %bucket,
        key = %key,
        upload_id = %upload_id,
        max_parts = %max_parts,
        part_number_marker = %part_number_marker,
        "ListParts"
    );

    match state.storage.list_parts(&bucket, &key, &upload_id).await {
        Ok(all_parts) => {
            let mut result = ListPartsResult::new(&bucket, &key, &upload_id);
            result.max_parts = max_parts as u32;
            result.part_number_marker = part_number_marker;

            // Filter parts after the marker and apply max_parts limit
            let filtered: Vec<_> = all_parts
                .into_iter()
                .filter(|p| p.part_number > part_number_marker)
                .collect();

            let is_truncated = filtered.len() > max_parts;
            let page: Vec<_> = filtered.into_iter().take(max_parts).collect();

            if let Some(last) = page.last() {
                result.next_part_number_marker = last.part_number;
            }
            result.is_truncated = is_truncated;

            result.parts = page
                .into_iter()
                .map(|p| PartElement {
                    part_number: p.part_number,
                    last_modified: p.last_modified.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
                    etag: format!("\"{}\"", p.etag),
                    size: p.size,
                })
                .collect();

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
