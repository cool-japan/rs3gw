//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::super::types::ConditionalResult;
use crate::api::utils::{error_response, etag_matches, parse_http_date};
use crate::api::websocket::{S3Event, S3EventType};
use crate::api::xml_responses::{
    AccessControlPolicy, CommonPrefix, ListAllMyBucketsResult, LocationConstraint, ObjectContents,
    VersioningConfiguration,
};
use crate::storage::{ObjectMetadata, StorageError};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::TryStreamExt;
use tracing::{error, info, warn};

use super::select_parser::parse_select_request_xml;

/// Convert ObjectMetadata to ObjectContents for list responses
pub(super) fn metadata_to_contents(obj: ObjectMetadata) -> ObjectContents {
    ObjectContents {
        key: obj.key,
        last_modified: obj
            .last_modified
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string(),
        etag: format!("\"{}\"", obj.etag),
        size: obj.size,
        storage_class: "STANDARD".to_string(),
    }
}
/// Convert common prefix strings to CommonPrefix structs
pub(super) fn prefixes_to_common(prefixes: Vec<String>) -> Vec<CommonPrefix> {
    prefixes
        .into_iter()
        .map(|p| CommonPrefix { prefix: p })
        .collect()
}
/// Check conditional headers against object metadata
pub fn check_conditional_headers(
    headers: &HeaderMap,
    etag: &str,
    last_modified: chrono::DateTime<chrono::Utc>,
) -> ConditionalResult {
    let if_match = headers.get("If-Match").and_then(|v| v.to_str().ok());
    let if_none_match = headers.get("If-None-Match").and_then(|v| v.to_str().ok());
    let if_modified_since = headers
        .get("If-Modified-Since")
        .and_then(|v| v.to_str().ok());
    let if_unmodified_since = headers
        .get("If-Unmodified-Since")
        .and_then(|v| v.to_str().ok());
    if let Some(expected_etag) = if_match {
        if !etag_matches(etag, expected_etag) {
            return ConditionalResult::PreconditionFailed(etag.to_string());
        }
    }
    if let Some(expected_etag) = if_none_match {
        if etag_matches(etag, expected_etag) {
            return ConditionalResult::NotModified(etag.to_string());
        }
    }
    if let Some(since_str) = if_modified_since {
        if let Ok(since) = parse_http_date(since_str) {
            if last_modified <= since {
                return ConditionalResult::NotModified(etag.to_string());
            }
        }
    }
    if let Some(since_str) = if_unmodified_since {
        if let Ok(since) = parse_http_date(since_str) {
            if last_modified > since {
                return ConditionalResult::PreconditionFailed(etag.to_string());
            }
        }
    }
    ConditionalResult::Proceed
}
pub fn storage_error_to_response(err: StorageError, resource: &str) -> Response {
    match err {
        StorageError::NotFound(_) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchKey",
            "The specified key does not exist.",
            resource,
        ),
        StorageError::BucketNotFound => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            resource,
        ),
        StorageError::BucketAlreadyExists => error_response(
            StatusCode::CONFLICT,
            "BucketAlreadyExists",
            "The requested bucket name is not available.",
            resource,
        ),
        StorageError::BucketNotEmpty => error_response(
            StatusCode::CONFLICT,
            "BucketNotEmpty",
            "The bucket you tried to delete is not empty.",
            resource,
        ),
        StorageError::InvalidRange => error_response(
            StatusCode::RANGE_NOT_SATISFIABLE,
            "InvalidRange",
            "The requested range is not satisfiable.",
            resource,
        ),
        StorageError::MultipartNotFound => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchUpload",
            "The specified multipart upload does not exist.",
            resource,
        ),
        StorageError::InvalidPartNumber => error_response(
            StatusCode::BAD_REQUEST,
            "InvalidPart",
            "Part number must be between 1 and 10000.",
            resource,
        ),
        StorageError::Io(e) => {
            error!("Storage I/O error: {}", e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                "We encountered an internal error. Please try again.",
                resource,
            )
        }
        StorageError::Internal(msg) => {
            error!("Storage internal error: {}", msg);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                &msg,
                resource,
            )
        }
    }
}
/// List all buckets
#[utoipa::path(
    get,
    path = "/",
    tag = "Buckets",
    responses(
        (
            status = 200,
            description = "List of all buckets",
            content_type = "application/xml"
        ),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_buckets(State(state): State<AppState>) -> Response {
    info!("ListBuckets");
    match state.storage.list_buckets().await {
        Ok(buckets) => {
            let bucket_tuples: Vec<(String, chrono::DateTime<chrono::Utc>)> = buckets
                .into_iter()
                .map(|b| (b.name, b.creation_date))
                .collect();
            let result = ListAllMyBucketsResult::new(bucket_tuples);
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, "/"),
    }
}
/// Check if bucket exists
#[utoipa::path(
    head,
    path = "/{bucket}",
    tag = "Buckets",
    params(("bucket" = String, Path, description = "Name of the bucket to check")),
    responses(
        (status = 200, description = "Bucket exists"),
        (status = 404, description = "Bucket does not exist"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn head_bucket(State(state): State<AppState>, Path(bucket): Path<String>) -> Response {
    info!(bucket = % bucket, "HeadBucket");
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Create a bucket
#[utoipa::path(
    put,
    path = "/{bucket}",
    tag = "Buckets",
    params(("bucket" = String, Path, description = "Name of the bucket to create")),
    responses(
        (
            status = 200,
            description = "Bucket created successfully",
            headers(
                ("Location" = String, description = "Location of the created bucket")
            )
        ),
        (status = 409, description = "Bucket already exists"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_bucket(State(state): State<AppState>, Path(bucket): Path<String>) -> Response {
    info!(bucket = % bucket, "CreateBucket");
    match state.storage.create_bucket(&bucket).await {
        Ok(()) => {
            let event = S3Event::new(S3EventType::BucketCreated, bucket.clone());
            state.event_broadcaster.broadcast(event);
            (StatusCode::OK, [("Location", format!("/{}", bucket))]).into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Delete a bucket
#[utoipa::path(
    delete,
    path = "/{bucket}",
    tag = "Buckets",
    params(("bucket" = String, Path, description = "Name of the bucket to delete")),
    responses(
        (status = 204, description = "Bucket deleted successfully"),
        (status = 404, description = "Bucket not found"),
        (status = 409, description = "Bucket not empty"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_bucket(State(state): State<AppState>, Path(bucket): Path<String>) -> Response {
    info!(bucket = % bucket, "DeleteBucket");
    match state.storage.delete_bucket(&bucket).await {
        Ok(()) => {
            let event = S3Event::new(S3EventType::BucketRemoved, bucket.clone());
            state.event_broadcaster.broadcast(event);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Get bucket location
pub async fn get_bucket_location(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = % bucket, "GetBucketLocation");
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => {
            let result = LocationConstraint::new(None);
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Get bucket versioning configuration
pub async fn get_bucket_versioning(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = % bucket, "GetBucketVersioning");
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => {
            let result = VersioningConfiguration::new(None);
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Get bucket ACL
pub async fn get_bucket_acl(State(state): State<AppState>, Path(bucket): Path<String>) -> Response {
    info!(bucket = % bucket, "GetBucketAcl");
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => {
            let result = AccessControlPolicy::new_full_control();
            (
                StatusCode::OK,
                [("Content-Type", "application/xml")],
                result.to_xml(),
            )
                .into_response()
        }
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Put bucket versioning (stub - accepts request but versioning not supported)
pub async fn put_bucket_versioning(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body: Bytes,
) -> Response {
    info!(bucket = % bucket, "PutBucketVersioning");
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => {
            let body_str = String::from_utf8_lossy(&body);
            if body_str.contains("<Status>Enabled</Status>") {
                warn!(
                    bucket = % bucket,
                    "Versioning requested but not supported - request accepted"
                );
            }
            StatusCode::OK.into_response()
        }
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// SelectObjectContent - S3 Select API implementation
///
/// Runs SQL queries against CSV, JSON, or Parquet objects.
/// Results are cached using SelectResultCache for improved performance on repeated queries.
pub async fn select_object_content(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    use crate::api::select_optimizer::OptimizedSelectExecutor;
    info!(bucket = % bucket, key = % key, "SelectObjectContent");
    let request = match parse_select_request_xml(&body) {
        Ok(req) => req,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidRequest",
                &format!("Failed to parse SelectObjectContent request: {}", e),
                &format!("/{}/{}", bucket, key),
            );
        }
    };
    let (metadata, mut stream) = match state.storage.get_object(&bucket, &key).await {
        Ok(data) => data,
        Err(StorageError::NotFound(_)) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchKey",
                "The specified key does not exist.",
                &format!("/{}/{}", bucket, key),
            );
        }
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    };
    if let Some(cached_result) = state
        .select_result_cache
        .get(&request.expression, &metadata.etag)
        .await
    {
        info!(
            bucket = % bucket, key = % key, etag = % metadata.etag,
            "Cache hit for S3 Select query"
        );
        use std::sync::atomic::{AtomicU64, Ordering};
        static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);
        let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        return (
            StatusCode::OK,
            [
                ("Content-Type", "application/octet-stream"),
                ("x-amz-request-id", &format!("{:016x}", request_id)),
                ("x-amz-select-cache", "HIT"),
            ],
            cached_result,
        )
            .into_response();
    }
    let mut data_bytes = Vec::new();
    while let Some(chunk_result) = stream.try_next().await.transpose() {
        match chunk_result {
            Ok(chunk) => data_bytes.extend_from_slice(&chunk),
            Err(e) => {
                return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
            }
        }
    }
    let query = match crate::api::select::parse_sql(&request.expression) {
        Ok(query) => query,
        Err(e) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidExpression",
                &format!("SQL parse error: {}", e),
                &format!("/{}/{}", bucket, key),
            );
        }
    };
    let executor = OptimizedSelectExecutor::new(
        query,
        request.input_serialization.clone(),
        request.output_serialization.clone(),
        state.query_plan_cache.clone(),
    );
    match executor.execute(&data_bytes, &request.expression) {
        Ok(result) => {
            state
                .select_result_cache
                .put(
                    &request.expression,
                    &metadata.etag,
                    result.clone().into(),
                    3600,
                )
                .await;
            info!(
                bucket = % bucket, key = % key, etag = % metadata.etag,
                "Cached S3 Select query result"
            );
            use std::sync::atomic::{AtomicU64, Ordering};
            static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);
            let request_id = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
            (
                StatusCode::OK,
                [
                    ("Content-Type", "application/octet-stream"),
                    ("x-amz-request-id", &format!("{:016x}", request_id)),
                    ("x-amz-select-cache", "MISS"),
                ],
                result,
            )
                .into_response()
        }
        Err(e) => error_response(
            StatusCode::BAD_REQUEST,
            "SelectError",
            &format!("Query execution failed: {}", e),
            &format!("/{}/{}", bucket, key),
        ),
    }
}
