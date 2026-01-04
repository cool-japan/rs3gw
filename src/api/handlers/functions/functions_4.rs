//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::super::types::HealthResponse;
use super::core::storage_error_to_response;
use crate::api::utils::{error_response, etag_matches, parse_http_date, parse_tagging_xml};
use crate::api::xml_responses::{AccessControlPolicy, CopyObjectResult, TaggingResult};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use tracing::{info, warn};

/// Copy an object from source to destination
#[utoipa::path(
    put,
    path = "/{bucket}/{key}",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the destination bucket"),
        ("key" = String, Path, description = "Destination object key name")
    ),
    request_body(
        description = "Requires x-amz-copy-source header with source bucket/key"
    ),
    responses(
        (
            status = 200,
            description = "Object copied successfully",
            content_type = "application/xml"
        ),
        (status = 400, description = "Bad request - missing copy source header"),
        (status = 404, description = "Source object or bucket not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn copy_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let copy_source = match headers
        .get("x-amz-copy-source")
        .and_then(|v| v.to_str().ok())
    {
        Some(s) => s,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Missing x-amz-copy-source header",
                &format!("/{}/{}", bucket, key),
            );
        }
    };
    let decoded = percent_encoding::percent_decode_str(copy_source)
        .decode_utf8_lossy()
        .to_string();
    let source_path = decoded.trim_start_matches('/');
    let (src_bucket, src_key) = match source_path.split_once('/') {
        Some((b, k)) => (b, k),
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                "Invalid x-amz-copy-source format",
                &format!("/{}/{}", bucket, key),
            );
        }
    };
    info!(
        src_bucket = % src_bucket, src_key = % src_key, dst_bucket = % bucket, dst_key =
        % key, "CopyObject"
    );
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
        let src_meta = match state.storage.head_object(src_bucket, src_key).await {
            Ok(m) => m,
            Err(e) => {
                return storage_error_to_response(e, &format!("/{}/{}", src_bucket, src_key));
            }
        };
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
    let metadata_directive = headers
        .get("x-amz-metadata-directive")
        .and_then(|v| v.to_str().ok());
    let new_content_type = headers.get("Content-Type").and_then(|v| v.to_str().ok());
    let new_metadata: Option<std::collections::HashMap<String, String>> =
        if metadata_directive == Some("REPLACE") {
            let mut meta = std::collections::HashMap::new();
            for (name, value) in headers.iter() {
                if let Some(meta_key) = name.as_str().strip_prefix("x-amz-meta-") {
                    if let Ok(v) = value.to_str() {
                        meta.insert(meta_key.to_string(), v.to_string());
                    }
                }
            }
            Some(meta)
        } else {
            None
        };
    match state
        .storage
        .copy_object(
            src_bucket,
            src_key,
            &bucket,
            &key,
            metadata_directive,
            new_metadata,
            new_content_type,
        )
        .await
    {
        Ok(meta) => {
            let result = CopyObjectResult::new(
                &format!("\"{}\"", meta.etag),
                &meta
                    .last_modified
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string(),
            );
            (
                StatusCode::OK,
                [
                    ("Content-Type", "application/xml"),
                    ("x-amz-version-id", "null"),
                    ("x-amz-copy-source-version-id", "null"),
                ],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
/// Health check endpoint
#[utoipa::path(
    get,
    path = "/health",
    tag = "Admin",
    responses((status = 200, description = "Service is healthy", body = HealthResponse))
)]
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let compression = match state.config.compression {
        crate::storage::CompressionMode::None => "none".to_string(),
        crate::storage::CompressionMode::Zstd(level) => format!("zstd:{}", level),
        crate::storage::CompressionMode::Lz4 => "lz4".to_string(),
    };
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        compression,
    })
}
/// Metrics endpoint (Prometheus format)
pub async fn metrics(State(state): State<AppState>) -> Response {
    if let Ok(stats) = state.storage.get_storage_stats().await {
        crate::metrics::update_storage_stats(
            stats.bucket_count as usize,
            stats.object_count as usize,
            stats.total_size_bytes,
        );
    }
    let metrics_output = state.metrics_handle.render();
    (
        StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
        metrics_output,
    )
        .into_response()
}
/// Get object tagging
#[utoipa::path(
    get,
    path = "/{bucket}/{key}?tagging",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        ("key" = String, Path, description = "Object key name")
    ),
    responses(
        (
            status = 200,
            description = "Object tags retrieved successfully",
            content_type = "application/xml"
        ),
        (status = 404, description = "Object or bucket not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_object_tagging(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = % bucket, key = % key, "GetObjectTagging");
    match state.storage.get_object_tagging(&bucket, &key).await {
        Ok(tagging) => {
            let tags: Vec<(String, String)> = tagging.tags.into_iter().collect();
            let result = TaggingResult::new(tags);
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
/// Put object tagging
#[utoipa::path(
    put,
    path = "/{bucket}/{key}?tagging",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        ("key" = String, Path, description = "Object key name")
    ),
    request_body(description = "XML tagging document", content_type = "application/xml"),
    responses(
        (status = 200, description = "Tags applied successfully"),
        (status = 400, description = "Malformed XML"),
        (status = 404, description = "Object or bucket not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn put_object_tagging(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    info!(bucket = % bucket, key = % key, "PutObjectTagging");
    let body_str = String::from_utf8_lossy(&body);
    let tags = parse_tagging_xml(&body_str);
    if tags.is_empty() && !body_str.contains("<TagSet") {
        return error_response(
            StatusCode::BAD_REQUEST,
            "MalformedXML",
            "The XML you provided was not well-formed",
            &format!("/{}/{}", bucket, key),
        );
    }
    let tagging = crate::storage::ObjectTagging { tags };
    match state
        .storage
        .put_object_tagging(&bucket, &key, &tagging)
        .await
    {
        Ok(()) => (
            StatusCode::OK,
            [("x-amz-request-id", uuid::Uuid::new_v4().to_string())],
        )
            .into_response(),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
/// Delete object tagging
pub async fn delete_object_tagging(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = % bucket, key = % key, "DeleteObjectTagging");
    match state.storage.delete_object_tagging(&bucket, &key).await {
        Ok(()) => (
            StatusCode::NO_CONTENT,
            [("x-amz-request-id", uuid::Uuid::new_v4().to_string())],
        )
            .into_response(),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
/// Get object ACL
pub async fn get_object_acl(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = % bucket, key = % key, "GetObjectAcl");
    match state.storage.head_object(&bucket, &key).await {
        Ok(_meta) => {
            let result = AccessControlPolicy::new_full_control();
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
/// Put object ACL (stub - accepts but no-op)
pub async fn put_object_acl(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = % bucket, key = % key, "PutObjectAcl (stub)");
    match state.storage.head_object(&bucket, &key).await {
        Ok(_meta) => {
            warn!(
                bucket = % bucket, key = % key, "Object ACL accepted but not implemented"
            );
            StatusCode::OK.into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
/// Get object attributes (subset of metadata without fetching the body)
pub async fn get_object_attributes(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    info!(bucket = % bucket, key = % key, "GetObjectAttributes");
    let meta = match state.storage.head_object(&bucket, &key).await {
        Ok(m) => m,
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    };
    let requested_attrs: Vec<&str> = headers
        .get("x-amz-object-attributes")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').map(|a| a.trim()).collect())
        .unwrap_or_else(|| vec!["ETag", "ObjectSize", "StorageClass"]);
    let mut result = crate::api::xml_responses::GetObjectAttributesResult {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        etag: None,
        checksum: None,
        object_parts: None,
        storage_class: None,
        object_size: None,
    };
    for attr in requested_attrs {
        match attr {
            "ETag" => result.etag = Some(format!("\"{}\"", meta.etag)),
            "ObjectSize" => result.object_size = Some(meta.size),
            "StorageClass" => result.storage_class = Some("STANDARD".to_string()),
            "Checksum" => {
                result.checksum = Some(crate::api::xml_responses::ObjectChecksum {
                    checksum_sha256: Some(meta.etag.clone()),
                });
            }
            "ObjectParts" => {
                result.object_parts = None;
            }
            _ => {}
        }
    }
    (
        StatusCode::OK,
        [
            ("Content-Type", "application/xml"),
            ("x-amz-request-id", &uuid::Uuid::new_v4().to_string()),
            (
                "Last-Modified",
                &meta
                    .last_modified
                    .format("%a, %d %b %Y %H:%M:%S GMT")
                    .to_string(),
            ),
        ],
        result.to_xml(),
    )
        .into_response()
}
/// Get bucket tagging
pub async fn get_bucket_tagging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = % bucket, "GetBucketTagging");
    match state.storage.get_bucket_tagging(&bucket).await {
        Ok(tagging) => {
            let tags: Vec<(String, String)> = tagging.tags.into_iter().collect();
            let result = TaggingResult::new(tags);
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Put bucket tagging
pub async fn put_bucket_tagging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body: Bytes,
) -> Response {
    info!(bucket = % bucket, "PutBucketTagging");
    let body_str = String::from_utf8_lossy(&body);
    let tags = parse_tagging_xml(&body_str);
    if tags.is_empty() && !body_str.contains("<TagSet") {
        return error_response(
            StatusCode::BAD_REQUEST,
            "MalformedXML",
            "The XML you provided was not well-formed",
            &format!("/{}", bucket),
        );
    }
    let tagging = crate::storage::ObjectTagging { tags };
    match state.storage.put_bucket_tagging(&bucket, &tagging).await {
        Ok(()) => (
            StatusCode::NO_CONTENT,
            [("x-amz-request-id", uuid::Uuid::new_v4().to_string())],
        )
            .into_response(),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Delete bucket tagging
pub async fn delete_bucket_tagging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = % bucket, "DeleteBucketTagging");
    match state.storage.delete_bucket_tagging(&bucket).await {
        Ok(()) => (
            StatusCode::NO_CONTENT,
            [("x-amz-request-id", uuid::Uuid::new_v4().to_string())],
        )
            .into_response(),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Get bucket policy
pub async fn get_bucket_policy(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = % bucket, "GetBucketPolicy");
    match state.storage.get_bucket_policy(&bucket).await {
        Ok(policy) => (
            StatusCode::OK,
            [("Content-Type", "application/json")],
            policy,
        )
            .into_response(),
        Err(crate::storage::StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucketPolicy",
            "The bucket policy does not exist",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
