//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::core::storage_error_to_response;
use crate::api::utils::error_response;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::info;

/// Put bucket policy
pub async fn put_bucket_policy(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body: Bytes,
) -> Response {
    info!(bucket = % bucket, "PutBucketPolicy");
    let policy_str = String::from_utf8_lossy(&body);
    if serde_json::from_str::<serde_json::Value>(&policy_str).is_err() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "MalformedPolicy",
            "The policy is not valid JSON",
            &format!("/{}", bucket),
        );
    }
    match state.storage.put_bucket_policy(&bucket, &policy_str).await {
        Ok(()) => (
            StatusCode::NO_CONTENT,
            [("x-amz-request-id", uuid::Uuid::new_v4().to_string())],
        )
            .into_response(),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// Delete bucket policy
pub async fn delete_bucket_policy(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = % bucket, "DeleteBucketPolicy");
    match state.storage.delete_bucket_policy(&bucket).await {
        Ok(()) => (
            StatusCode::NO_CONTENT,
            [("x-amz-request-id", uuid::Uuid::new_v4().to_string())],
        )
            .into_response(),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// ── Admin: Garbage Collection ─────────────────────────────────────────────────

/// Query parameters for the multipart GC endpoint
#[derive(Debug, Deserialize)]
pub struct GcMultipartQuery {
    /// Only collect uploads from this bucket (optional; all buckets if absent)
    pub bucket: Option<String>,
    /// Retention window in hours (default: 168 = 7 days)
    #[serde(default = "default_retention_hours")]
    pub retention_hours: u64,
}

fn default_retention_hours() -> u64 {
    168
}

/// Response body returned by the admin GC endpoint
#[derive(Debug, Serialize)]
pub struct GcMultipartResponse {
    pub removed: u64,
    pub message: String,
}

/// `POST /api/admin/gc/multipart` — garbage-collect abandoned multipart uploads.
///
/// Query parameters:
/// - `bucket` (optional) — restrict GC to a single bucket
/// - `retention_hours` (optional, default 168) — age threshold in hours
pub async fn admin_gc_multipart(
    State(state): State<AppState>,
    Query(params): Query<GcMultipartQuery>,
) -> Response {
    info!(
        bucket = ?params.bucket,
        retention_hours = params.retention_hours,
        "AdminGcMultipart"
    );

    let bucket_ref = params.bucket.as_deref();
    match state
        .storage
        .gc_abandoned_multipart(bucket_ref, params.retention_hours)
        .await
    {
        Ok(removed) => {
            let body = GcMultipartResponse {
                removed,
                message: format!(
                    "Garbage collected {} abandoned multipart upload(s)",
                    removed
                ),
            };
            Json(body).into_response()
        }
        Err(e) => storage_error_to_response(e, "/api/admin/gc/multipart"),
    }
}

/// `GET /ready` — liveness/readiness probe; returns 200 when the server is ready.
pub async fn ready_check(State(_state): State<AppState>) -> Response {
    (StatusCode::OK, "ok").into_response()
}
