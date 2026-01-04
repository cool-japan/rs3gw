//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::core::storage_error_to_response;
use crate::api::utils::error_response;
use crate::api::websocket::{S3Event, S3EventType};
use crate::storage::ObjectMetadata;
use crate::AppState;
use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use tracing::{debug, info};

/// Put an object
#[utoipa::path(
    put,
    path = "/{bucket}/{key}",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        ("key" = String, Path, description = "Object key name")
    ),
    request_body(content = Vec<u8>, content_type = "application/octet-stream"),
    responses(
        (
            status = 200,
            description = "Object created successfully",
            headers(
                ("ETag" = String, description = "Entity tag for the object"),
                (
                    "x-amz-version-id" = String,
                    description = "Version ID (if versioning enabled)"
                )
            )
        ),
        (status = 404, description = "Bucket not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn put_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    state.metrics_tracker.record_request();
    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    info!(
        bucket = % bucket, key = % key, size = % body.len(), content_type = %
        content_type, "PutObject"
    );
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
    let mut metadata = std::collections::HashMap::new();
    for (name, value) in headers.iter() {
        if let Some(meta_key) = name.as_str().strip_prefix("x-amz-meta-") {
            if let Ok(v) = value.to_str() {
                metadata.insert(meta_key.to_string(), v.to_string());
            }
        }
    }
    if let Some(model_format) = crate::storage::ml_models::detect_ml_model_format(&body).await {
        if let Some(model_metadata) =
            crate::storage::ml_models::extract_ml_metadata(model_format, &body).await
        {
            let ml_headers = model_metadata.to_headers();
            for (header_key, header_value) in ml_headers {
                if let Some(meta_key) = header_key.strip_prefix("x-amz-meta-") {
                    metadata.insert(meta_key.to_string(), header_value);
                }
            }
            debug!(
                "Detected ML model: {:?}, {} tensors, {} parameters",
                model_metadata.format,
                model_metadata.tensors.len(),
                model_metadata.parameter_count.unwrap_or(0)
            );
        }
    }
    let body_size = body.len() as u64;
    match state
        .storage
        .put_object(&bucket, &key, content_type, metadata, body)
        .await
    {
        Ok(etag) => {
            state.metrics_tracker.record_bytes_uploaded(body_size);
            let event = S3Event::new(S3EventType::ObjectCreated, bucket.clone())
                .with_key(key.clone())
                .with_size(body_size)
                .with_etag(etag.clone());
            state.event_broadcaster.broadcast(event);
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("ETag", format!("\"{}\"", etag))
                .header("x-amz-version-id", "null")
                .header("x-amz-request-id", uuid::Uuid::new_v4().to_string());
            response.body(Body::empty()).unwrap_or_else(|_| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}/{}", bucket, key),
                )
            })
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
/// POST object (browser-based upload using multipart/form-data)
pub async fn post_object(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    mut multipart: Multipart,
) -> Response {
    info!(bucket = % bucket, "PostObject");
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
    let mut key: Option<String> = None;
    let mut content_type = "application/octet-stream".to_string();
    let mut file_data: Option<Bytes> = None;
    let mut metadata = std::collections::HashMap::new();
    let mut success_action_redirect: Option<String> = None;
    let mut success_action_status: Option<u16> = None;
    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_lowercase();
        match field_name.as_str() {
            "key" => {
                if let Ok(value) = field.text().await {
                    key = Some(value);
                }
            }
            "content-type" => {
                if let Ok(value) = field.text().await {
                    content_type = value;
                }
            }
            "success_action_redirect" => {
                if let Ok(value) = field.text().await {
                    success_action_redirect = Some(value);
                }
            }
            "success_action_status" => {
                if let Ok(value) = field.text().await {
                    if let Ok(status) = value.parse::<u16>() {
                        success_action_status = Some(status);
                    }
                }
            }
            "file" => {
                if let Some(ct) = field.content_type() {
                    content_type = ct.to_string();
                }
                if let Ok(data) = field.bytes().await {
                    file_data = Some(data);
                }
            }
            name if name.starts_with("x-amz-meta-") => {
                if let Ok(value) = field.text().await {
                    if let Some(meta_key) = name.strip_prefix("x-amz-meta-") {
                        metadata.insert(meta_key.to_string(), value);
                    }
                }
            }
            _ => {
                let _ = field.bytes().await;
            }
        }
    }
    let key = match key {
        Some(k) if !k.is_empty() => k,
        _ => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "The 'key' form field is required",
                &format!("/{}", bucket),
            );
        }
    };
    let file_data = match file_data {
        Some(data) => data,
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "The 'file' form field is required",
                &format!("/{}", bucket),
            );
        }
    };
    info!(
        bucket = % bucket, key = % key, size = % file_data.len(), content_type = %
        content_type, "PostObject uploading"
    );
    match state
        .storage
        .put_object(&bucket, &key, &content_type, metadata, file_data)
        .await
    {
        Ok(etag) => {
            if let Some(redirect_url) = success_action_redirect {
                let separator = if redirect_url.contains('?') { '&' } else { '?' };
                let redirect = format!(
                    "{}{}bucket={}&key={}&etag={}",
                    redirect_url,
                    separator,
                    percent_encoding::utf8_percent_encode(
                        &bucket,
                        percent_encoding::NON_ALPHANUMERIC
                    ),
                    percent_encoding::utf8_percent_encode(&key, percent_encoding::NON_ALPHANUMERIC),
                    percent_encoding::utf8_percent_encode(
                        &etag,
                        percent_encoding::NON_ALPHANUMERIC
                    ),
                );
                return Response::builder()
                    .status(StatusCode::SEE_OTHER)
                    .header("Location", redirect)
                    .header("ETag", format!("\"{}\"", etag))
                    .body(Body::empty())
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
            }
            let status = match success_action_status {
                Some(200) => StatusCode::OK,
                Some(201) => StatusCode::CREATED,
                _ => StatusCode::NO_CONTENT,
            };
            if status == StatusCode::CREATED {
                let xml = format!(
                    r#"<?xml version="1.0" encoding="UTF-8"?>
<PostResponse>
<Location>/{}/{}</Location>
<Bucket>{}</Bucket>
<Key>{}</Key>
<ETag>"{}"</ETag>
</PostResponse>"#,
                    bucket, key, bucket, key, etag
                );
                Response::builder()
                    .status(StatusCode::CREATED)
                    .header("Content-Type", "application/xml")
                    .header("ETag", format!("\"{}\"", etag))
                    .body(Body::from(xml))
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
            } else {
                Response::builder()
                    .status(status)
                    .header("ETag", format!("\"{}\"", etag))
                    .body(Body::empty())
                    .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
            }
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
/// Delete an object
#[utoipa::path(
    delete,
    path = "/{bucket}/{key}",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        ("key" = String, Path, description = "Object key name")
    ),
    responses(
        (
            status = 204,
            description = "Object deleted successfully",
            headers(
                (
                    "x-amz-version-id" = String,
                    description = "Version ID of the deleted object"
                ),
                (
                    "x-amz-delete-marker" = String,
                    description = "Whether a delete marker was created"
                )
            )
        ),
        (status = 404, description = "Object or bucket not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = % bucket, key = % key, "DeleteObject");
    match state.storage.delete_object(&bucket, &key).await {
        Ok(()) => {
            let event =
                S3Event::new(S3EventType::ObjectRemoved, bucket.clone()).with_key(key.clone());
            state.event_broadcaster.broadcast(event);
            (
                StatusCode::NO_CONTENT,
                [
                    ("x-amz-version-id", "null".to_string()),
                    ("x-amz-delete-marker", "false".to_string()),
                    ("x-amz-request-id", uuid::Uuid::new_v4().to_string()),
                ],
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}
#[allow(dead_code)]
fn build_object_headers(meta: ObjectMetadata) -> Response {
    build_object_headers_with_sci(meta, None)
}
pub(super) fn build_object_headers_with_sci(
    meta: ObjectMetadata,
    sci_meta: Option<crate::storage::SciMetadata>,
) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", &meta.content_type)
        .header("Content-Length", meta.size)
        .header("ETag", format!("\"{}\"", meta.etag))
        .header(
            "Last-Modified",
            meta.last_modified
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string(),
        )
        .header("Accept-Ranges", "bytes")
        .header("x-amz-storage-class", "STANDARD")
        .header("x-amz-version-id", "null")
        .header("x-amz-request-id", uuid::Uuid::new_v4().to_string());
    for (k, v) in &meta.metadata {
        builder = builder.header(format!("x-amz-meta-{}", k), v);
    }
    if let Some(sci) = sci_meta {
        for (k, v) in sci.to_s3_metadata() {
            builder = builder.header(format!("x-amz-meta-{}", k), v);
        }
    }
    builder
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
