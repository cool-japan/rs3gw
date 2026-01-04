//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::core::storage_error_to_response;
use crate::api::utils::error_response;
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
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
