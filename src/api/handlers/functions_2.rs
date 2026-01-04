//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::api::utils::{error_response, parse_delete_objects_xml};
use crate::api::xml_responses::{DeleteResult, ListMultipartUploadsResult, Owner, UploadElement};
use crate::storage::StorageError;
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{debug, info, warn};

use super::functions::storage_error_to_response;
use super::types::{ListMultipartUploadsQuery, PresignQuery, PresignedUrlResponse};

/// Delete multiple objects in a single request
pub async fn delete_objects(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body: Bytes,
) -> Response {
    use futures::stream::{self, StreamExt};
    info!(bucket = % bucket, "DeleteObjects");
    let body_str = String::from_utf8_lossy(&body);
    let keys = parse_delete_objects_xml(&body_str);
    if keys.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "MalformedXML",
            "The XML you provided was not well-formed",
            &format!("/{}", bucket),
        );
    }
    const MAX_CONCURRENT_DELETES: usize = 32;
    let storage = state.storage.clone();
    let bucket_clone = bucket.clone();
    let results: Vec<_> = stream::iter(keys)
        .map(|key| {
            let storage = storage.clone();
            let bucket = bucket_clone.clone();
            async move {
                let result = storage.delete_object(&bucket, &key).await;
                (key, result)
            }
        })
        .buffer_unordered(MAX_CONCURRENT_DELETES)
        .collect()
        .await;
    let mut result = DeleteResult::new();
    for (key, delete_result) in results {
        match delete_result {
            Ok(()) => {
                debug!(bucket = % bucket, key = % key, "Object deleted in batch");
                result.add_deleted(key);
            }
            Err(e) => {
                let (code, message) = match &e {
                    StorageError::NotFound(_) => ("NoSuchKey", "The specified key does not exist."),
                    StorageError::BucketNotFound => {
                        ("NoSuchBucket", "The specified bucket does not exist.")
                    }
                    _ => ("InternalError", "We encountered an internal error."),
                };
                warn!(
                    bucket = % bucket, key = % key, error = % e,
                    "Failed to delete object in batch"
                );
                result.add_error(key, code, message);
            }
        }
    }
    info!(
        bucket = % bucket, deleted = % result.deleted.len(), errors = % result.errors
        .len(), "DeleteObjects completed"
    );
    (
        StatusCode::OK,
        [("Content-Type", "application/xml")],
        result.to_xml(),
    )
        .into_response()
}
/// List in-progress multipart uploads for a bucket
pub async fn list_multipart_uploads(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListMultipartUploadsQuery>,
) -> Response {
    info!(bucket = % bucket, "ListMultipartUploads");
    let prefix = query.prefix.as_deref();
    match state.storage.list_multipart_uploads(&bucket, prefix).await {
        Ok(uploads) => {
            let mut result = ListMultipartUploadsResult::new(&bucket);
            result.prefix = query.prefix;
            result.delimiter = query.delimiter;
            let max_uploads = query.max_uploads.unwrap_or(1000) as usize;
            let total_uploads = uploads.len();
            let is_truncated = total_uploads > max_uploads;
            result.max_uploads = max_uploads as u32;
            result.is_truncated = is_truncated;
            for upload in uploads.into_iter().take(max_uploads) {
                result.uploads.push(UploadElement {
                    key: upload.key,
                    upload_id: upload.upload_id,
                    initiator: Owner::default(),
                    owner: Owner::default(),
                    storage_class: "STANDARD".to_string(),
                    initiated: upload.initiated.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
                });
            }
            if is_truncated {
                if let Some(last) = result.uploads.last() {
                    result.next_key_marker = Some(last.key.clone());
                    result.next_upload_id_marker = Some(last.upload_id.clone());
                }
            }
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
/// Generate a presigned URL for an object
pub async fn generate_presigned_url(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(params): Query<PresignQuery>,
    headers: HeaderMap,
) -> Response {
    let method = params.method.as_deref().unwrap_or("GET").to_uppercase();
    let expires = params.expires.unwrap_or(3600).min(604800);
    if method != "GET" && method != "PUT" {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "Method must be GET or PUT",
            &format!("/{}/{}", bucket, key),
        );
    }
    if state.config.access_key.is_empty() || state.config.secret_key.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "Authentication must be configured to generate presigned URLs",
            &format!("/{}/{}", bucket, key),
        );
    }
    match state.storage.bucket_exists(&bucket).await {
        Ok(false) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                "The specified bucket does not exist.",
                &format!("/{}", bucket),
            );
        }
        Err(e) => return storage_error_to_response(e, &format!("/{}", bucket)),
        Ok(true) => {}
    }
    if method == "GET" {
        match state.storage.head_object(&bucket, &key).await {
            Err(crate::storage::StorageError::NotFound(_)) => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    "NoSuchKey",
                    "The specified key does not exist.",
                    &format!("/{}/{}", bucket, key),
                );
            }
            Err(e) => {
                return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
            }
            Ok(_) => {}
        }
    }
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| state.config.bind_addr.to_string());
    info!(
        bucket = % bucket, key = % key, method = % method, expires = % expires,
        "GeneratePresignedUrl"
    );
    let generator = crate::auth::PresignedUrlGenerator::new(
        state.config.access_key.clone(),
        state.config.secret_key.clone(),
        "us-east-1".to_string(),
        host,
    );
    let url = if method == "GET" {
        generator.generate_presigned_get_url(&bucket, &key, expires)
    } else {
        generator.generate_presigned_put_url(&bucket, &key, expires, None)
    };
    let response = PresignedUrlResponse {
        url,
        expires_in: expires,
        method,
    };
    (
        StatusCode::OK,
        [("Content-Type", "application/json")],
        serde_json::to_string(&response).unwrap_or_default(),
    )
        .into_response()
}
/// Restore an archived object from Glacier (stub - returns success for compatibility)
///
/// Since we don't implement Glacier storage classes, this stub simply returns
/// success (200 OK) to maintain compatibility with S3 SDKs and tools that may
/// attempt to restore objects.
pub async fn restore_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = % bucket, key = % key, "RestoreObject (stub)");
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => {}
        Ok(false) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                "The specified bucket does not exist.",
                &format!("/{}/{}", bucket, key),
            );
        }
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
    match state.storage.head_object(&bucket, &key).await {
        Ok(_) => {
            warn!(
                bucket = % bucket, key = % key,
                "RestoreObject accepted but Glacier storage class not implemented"
            );
            Response::builder()
                .status(StatusCode::OK)
                .header("x-amz-request-id", uuid::Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(crate::storage::StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchKey",
            "The specified key does not exist.",
            &format!("/{}/{}", bucket, key),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
