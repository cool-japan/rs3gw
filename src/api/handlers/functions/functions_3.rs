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
use base64::Engine as _;
use bytes::Bytes;
use sha2::Digest;
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
    body: Body,
) -> Response {
    state.metrics_tracker.record_request();
    let content_type = headers
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");
    info!(
        bucket = % bucket, key = % key, content_type = %
        content_type, "PutObject (streaming)"
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

    // Parse checksum headers and store under reserved keys
    const CHECKSUM_HEADERS: &[(&str, &str)] = &[
        ("x-amz-checksum-sha256", "sha256"),
        ("x-amz-checksum-crc32", "crc32"),
        ("x-amz-checksum-crc32c", "crc32c"),
        ("x-amz-checksum-sha1", "sha1"),
    ];
    for (header_name, algo_name) in CHECKSUM_HEADERS {
        if let Some(hv) = headers.get(*header_name) {
            match hv.to_str() {
                Ok(raw) => {
                    // Validate that the value is valid base64
                    if base64::engine::general_purpose::STANDARD
                        .decode(raw)
                        .is_err()
                        && base64::engine::general_purpose::URL_SAFE_NO_PAD
                            .decode(raw)
                            .is_err()
                    {
                        return error_response(
                            StatusCode::BAD_REQUEST,
                            "InvalidRequest",
                            "Checksum value is not valid base64",
                            &format!("/{}/{}", bucket, key),
                        );
                    }
                    metadata.insert("__checksum_algo__".to_string(), algo_name.to_string());
                    metadata.insert("__checksum_value__".to_string(), raw.to_string());
                }
                Err(_) => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "InvalidRequest",
                        "Checksum header value is not valid UTF-8",
                        &format!("/{}/{}", bucket, key),
                    );
                }
            }
            // Only the first matching checksum header is used
            break;
        }
    }

    // Persist caching / content headers under reserved keys
    const SYS_HEADERS: &[(&str, &str)] = &[
        ("content-disposition", "__sys_content_disposition__"),
        ("cache-control", "__sys_cache_control__"),
        ("expires", "__sys_expires__"),
        ("content-encoding", "__sys_content_encoding__"),
    ];
    for (header_name, reserved_key) in SYS_HEADERS {
        if let Some(hv) = headers.get(*header_name) {
            if let Ok(v) = hv.to_str() {
                metadata.insert(reserved_key.to_string(), v.to_string());
            }
        }
    }

    // Stream the body incrementally: collect chunks while computing SHA-256
    // and MD5 hashes, then pass the data through to the storage engine.
    let mut sha256_hasher = sha2::Sha256::new();
    let mut md5_hasher = md5::Context::new();
    let mut body_data: Vec<u8> = Vec::new();

    let mut body_stream = body.into_data_stream();
    use futures::StreamExt;
    while let Some(chunk_result) = body_stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                sha256_hasher.update(&chunk);
                md5_hasher.consume(&chunk);
                body_data.extend_from_slice(&chunk);
            }
            Err(e) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    &format!("Failed to read request body: {}", e),
                    &format!("/{}/{}", bucket, key),
                );
            }
        }
    }

    let sha256_hex = format!("{:x}", sha256_hasher.finalize());
    let _md5_digest = md5_hasher.finalize();
    let body_bytes = Bytes::from(body_data);

    info!(
        bucket = % bucket, key = % key, size = % body_bytes.len(),
        "PutObject body received"
    );

    if let Some(model_format) = crate::storage::ml_models::detect_ml_model_format(&body_bytes).await
    {
        if let Some(model_metadata) =
            crate::storage::ml_models::extract_ml_metadata(model_format, &body_bytes).await
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
    let body_size = body_bytes.len() as u64;

    // Write to a temp file and use put_object_from_path for atomic rename
    let tmp_dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = tmp_dir.join(format!("rs3gw-put-{}-{}.tmp", uuid::Uuid::new_v4(), nanos));

    // Write data to temp file
    {
        use tokio::io::AsyncWriteExt;
        let mut tmp_file = match tokio::fs::File::create(&tmp_path).await {
            Ok(f) => f,
            Err(e) => {
                return storage_error_to_response(
                    crate::storage::StorageError::from(e),
                    &format!("/{}/{}", bucket, key),
                );
            }
        };
        if let Err(e) = tmp_file.write_all(&body_bytes).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return storage_error_to_response(
                crate::storage::StorageError::from(e),
                &format!("/{}/{}", bucket, key),
            );
        }
        if let Err(e) = tmp_file.flush().await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return storage_error_to_response(
                crate::storage::StorageError::from(e),
                &format!("/{}/{}", bucket, key),
            );
        }
    }

    match state
        .storage
        .put_object_from_path(
            &bucket,
            &key,
            Some(content_type.to_string()),
            metadata,
            &tmp_path,
            body_size,
            sha256_hex,
        )
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
        Err(e) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            storage_error_to_response(e, &format!("/{}/{}", bucket, key))
        }
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
        if !k.starts_with("__sys_") && !k.starts_with("__checksum_") {
            builder = builder.header(format!("x-amz-meta-{}", k), v);
        }
    }
    // Emit stored checksum header
    if let (Some(algo), Some(value)) = (
        meta.metadata.get("__checksum_algo__"),
        meta.metadata.get("__checksum_value__"),
    ) {
        builder = builder.header(format!("x-amz-checksum-{}", algo), value);
    }
    // Emit stored caching/content headers
    const SYS_RESPONSE_HEADERS: &[(&str, &str)] = &[
        ("__sys_content_disposition__", "Content-Disposition"),
        ("__sys_cache_control__", "Cache-Control"),
        ("__sys_expires__", "Expires"),
        ("__sys_content_encoding__", "Content-Encoding"),
    ];
    for (reserved_key, response_header) in SYS_RESPONSE_HEADERS {
        if let Some(v) = meta.metadata.get(*reserved_key) {
            builder = builder.header(*response_header, v);
        }
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
