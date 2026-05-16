//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::api::utils::{error_response, parse_delete_objects_xml, parse_restore_request_xml};
use crate::api::xml_responses::{DeleteResult, ListMultipartUploadsResult, Owner, UploadElement};
use crate::storage::archival::ArchivalStatus;
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
    headers: HeaderMap,
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

    // Extract bypass-governance header once; applies to all objects in the batch.
    let bypass_governance = headers
        .get("x-amz-bypass-governance-retention")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    const MAX_CONCURRENT_DELETES: usize = 32;
    let storage = state.storage.clone();
    let bucket_clone = bucket.clone();
    let lock_mgr = state.storage.object_lock_manager.clone();
    let results: Vec<_> = stream::iter(keys)
        .map(|key| {
            let storage = storage.clone();
            let bucket = bucket_clone.clone();
            let lock_mgr = lock_mgr.clone();
            async move {
                // Check Object Lock protection before deleting.
                // Bulk delete uses version "null" (no versioning support at this layer yet).
                match lock_mgr.get(&bucket, &key, "null").await {
                    Ok(meta) => {
                        use chrono::Utc;
                        // LegalHold blocks deletion unconditionally.
                        if meta.legal_hold_status.as_deref() == Some("ON") {
                            return (
                                key,
                                Err(StorageError::ObjectLocked(
                                    "Object has a legal hold in place".to_string(),
                                )),
                            );
                        }
                        // Active retention period.
                        if let Some(until) = meta.retain_until_date {
                            if until > Utc::now() {
                                let mode = meta.retention_mode.as_deref().unwrap_or("");
                                if mode == "COMPLIANCE" {
                                    return (
                                        key,
                                        Err(StorageError::ObjectLocked(
                                            "Object is under COMPLIANCE retention".to_string(),
                                        )),
                                    );
                                }
                                // GOVERNANCE: deletable only with bypass header.
                                if mode == "GOVERNANCE" && !bypass_governance {
                                    return (
                                        key,
                                        Err(StorageError::ObjectLocked(
                                            "Object is under GOVERNANCE retention; \
                                             x-amz-bypass-governance-retention header required"
                                                .to_string(),
                                        )),
                                    );
                                }
                            }
                        }
                    }
                    // No sidecar → not protected; proceed.
                    Err(StorageError::NotFound(_)) => {}
                    Err(e) => return (key, Err(e)),
                }
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
            Err(StorageError::ObjectLocked(msg)) => {
                warn!(
                    bucket = % bucket, key = % key, reason = % msg,
                    "Object Lock blocked batch delete"
                );
                result.add_error(key, "AccessDenied", "Object is locked against deletion");
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
/// Restore an archived object from Glacier/DeepArchive.
///
/// Parses the `<RestoreRequest>` XML body, checks the object's archival status,
/// and dispatches to `ArchivalManager::restore_object`. Returns 202 Accepted for
/// new restores, 200 OK when the object is already active.
pub async fn restore_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    info!(bucket = %bucket, key = %key, "RestoreObject");

    // 1. Check bucket exists
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

    // 2. Check object exists
    match state.storage.head_object(&bucket, &key).await {
        Ok(_) => {}
        Err(StorageError::NotFound(_)) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchKey",
                "The specified key does not exist.",
                &format!("/{}/{}", bucket, key),
            );
        }
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }

    // 3. Parse RestoreRequest body
    let xml_str = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "Request body is not valid UTF-8",
                &format!("/{}/{}", bucket, key),
            );
        }
    };
    let restore_req = match parse_restore_request_xml(xml_str) {
        Ok(r) => r,
        Err(msg) => {
            if msg.contains("Invalid tier") {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    &msg,
                    &format!("/{}/{}", bucket, key),
                );
            }
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                &msg,
                &format!("/{}/{}", bucket, key),
            );
        }
    };
    debug!(bucket = %bucket, key = %key, days = restore_req.days, tier = %restore_req.tier, "RestoreObject parsed");

    // 4. Check archival status
    let status = state.storage.get_archival_status(&bucket, &key).await;
    match status {
        None => {
            return error_response(
                StatusCode::CONFLICT,
                "InvalidObjectState",
                "Object is not in an archive tier and cannot be restored.",
                &format!("/{}/{}", bucket, key),
            );
        }
        Some(ArchivalStatus::Active) => {
            // Already restored — return 200
            return StatusCode::OK.into_response();
        }
        Some(ArchivalStatus::Restoring { .. }) => {
            // Already restoring — return 202
            return StatusCode::ACCEPTED.into_response();
        }
        Some(_) => {
            // Archived / Archiving / Failed — proceed with restore
        }
    }

    // 5. Initiate restore
    match state.storage.archive_object_restore(&bucket, &key).await {
        Ok(_) => {
            let req_id = uuid::Uuid::new_v4().to_string();
            Response::builder()
                .status(StatusCode::ACCEPTED)
                .header("x-amz-request-id", req_id)
                .body(Body::empty())
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
