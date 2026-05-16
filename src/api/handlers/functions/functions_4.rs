//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::super::types::HealthResponse;
use super::core::storage_error_to_response;
use crate::api::sse::{format_kms_arn, resolve_sse, resolve_sse_c_copy_source, SseDecision};
use crate::api::utils::{
    error_response, etag_matches, parse_acl_xml, parse_canned_acl_header, parse_http_date,
    parse_tagging_xml,
};
use crate::api::xml_responses::{AccessControlPolicy, CopyObjectResult, TaggingResult};
use crate::storage::encryption::EncryptedData;
use crate::storage::{AclConfig, ObjectSseSidecar, StorageError};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use bytes::Bytes;
use futures::StreamExt;
use sha2::Digest;
use tracing::info;

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

    // Resolve the SSE state of the source object and the desired SSE for the destination.
    let src_sse = match state.storage.get_object_sse(src_bucket, src_key).await {
        Ok(opt) => opt,
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", src_bucket, src_key)),
    };
    let dst_sse_decision = match resolve_sse(&state, &bucket, &headers).await {
        Ok(d) => d,
        Err(resp) => return resp,
    };

    // For Case 1 (plain → plain) we can delegate to storage::copy_object unchanged.
    // For all other cases we must read the source bytes, transform, and write explicitly
    // so that ETag = sha256(plaintext) is preserved regardless of encryption.
    let is_src_enc = src_sse.is_some();
    let is_dst_enc = matches!(
        &dst_sse_decision,
        SseDecision::Aes256 | SseDecision::SseC { .. } | SseDecision::SseKms { .. }
    );

    if !is_src_enc && !is_dst_enc {
        // Case 1: plain → plain — use existing storage copy (handles metadata directive too).
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
        // If the destination previously had an SSE sidecar, remove it (stale sidecar guard).
        let _ = state.storage.delete_object_sse(&bucket, &key).await;
        return match state
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
        };
    }

    // Cases 2, 3, 4: read source bytes and handle encrypt/decrypt.

    // Step A: read source metadata (needed for ETag and COPY metadata directive).
    let src_meta = match state.storage.head_object(src_bucket, src_key).await {
        Ok(m) => m,
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", src_bucket, src_key)),
    };

    // Step B: read the raw bytes stored on disk (ciphertext if SSE, plaintext otherwise).
    let raw_bytes_on_disk = {
        let (_, mut stream) = match state.storage.get_object(src_bucket, src_key).await {
            Ok(pair) => pair,
            Err(e) => return storage_error_to_response(e, &format!("/{}/{}", src_bucket, src_key)),
        };
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(b) => buf.extend_from_slice(&b),
                Err(e) => {
                    return storage_error_to_response(e, &format!("/{}/{}", src_bucket, src_key))
                }
            }
        }
        buf
    };

    // Step C: obtain plaintext.
    // If source is encrypted, decrypt it; otherwise raw bytes are already plaintext.
    let plaintext: Vec<u8> = if let Some(ref sidecar) = src_sse {
        if sidecar.algorithm == "AES256-SSE-C" {
            // SSE-C source: require copy-source customer key headers.
            let src_customer_key = match resolve_sse_c_copy_source(&headers).await {
                Ok(Some(k)) => k,
                Ok(None) => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "InvalidRequest",
                        "SSE-C source object requires x-amz-copy-source-server-side-encryption-customer-* headers",
                        &format!("/{}/{}", src_bucket, src_key),
                    );
                }
                Err(resp) => return resp,
            };

            // Validate the provided key MD5 matches what was stored at PUT time.
            let computed_md5_b64 = {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD
                    .encode(md5::compute(src_customer_key.as_ref()).0)
            };
            if let Some(ref stored_md5) = sidecar.customer_key_md5 {
                if stored_md5 != &computed_md5_b64 {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "InvalidArgument",
                        "The provided copy-source customer key does not match the key used to encrypt the source object",
                        &format!("/{}/{}", src_bucket, src_key),
                    );
                }
            }

            let aad = format!("{}/{}", sidecar.aad_bucket, sidecar.aad_key);
            let enc_data = EncryptedData {
                algorithm: crate::storage::encryption::EncryptionAlgorithm::Aes256Gcm,
                encrypted_dek: sidecar.encrypted_dek.clone(),
                kek_id: sidecar.kek_id.clone(),
                dek_nonce: sidecar.dek_nonce.clone(),
                ciphertext: raw_bytes_on_disk,
                payload_nonce: sidecar.payload_nonce.clone(),
                aad: Some(aad.into_bytes()),
                chunks: vec![],
                chunk_size: 0,
            };
            match state
                .encryption
                .decrypt_with_customer_key(&enc_data, src_customer_key.as_ref())
                .await
            {
                Ok(pt) => pt,
                Err(_) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        "SSE-C source decryption failed",
                        &format!("/{}/{}", src_bucket, src_key),
                    );
                }
            }
        } else {
            // SSE-S3 source: decrypt with server-managed KEK.
            let enc_data = sidecar.into_encrypted_data(raw_bytes_on_disk);
            match state.encryption.decrypt(&enc_data).await {
                Ok(pt) => pt,
                Err(e) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &format!("Decryption failed: {}", e),
                        &format!("/{}/{}", src_bucket, src_key),
                    );
                }
            }
        }
    } else {
        raw_bytes_on_disk
    };

    // Step D: build destination metadata.
    let mut dst_metadata: std::collections::HashMap<String, String> =
        if metadata_directive == Some("REPLACE") {
            let mut meta = std::collections::HashMap::new();
            for (name, value) in headers.iter() {
                if let Some(meta_key) = name.as_str().strip_prefix("x-amz-meta-") {
                    if let Ok(v) = value.to_str() {
                        meta.insert(meta_key.to_string(), v.to_string());
                    }
                }
            }
            meta
        } else {
            // COPY: inherit source metadata but strip the SSE stamp — we'll re-stamp below.
            let mut m = src_meta.metadata.clone();
            m.remove("__sse_algorithm__");
            m
        };
    // Stamp or clear __sse_algorithm__ based on dst decision.
    match &dst_sse_decision {
        SseDecision::Aes256 => {
            dst_metadata.insert("__sse_algorithm__".to_string(), "AES256".to_string());
        }
        SseDecision::SseC { .. } => {
            dst_metadata.insert("__sse_algorithm__".to_string(), "AES256-SSE-C".to_string());
        }
        SseDecision::SseKms { .. } => {
            dst_metadata.insert("__sse_algorithm__".to_string(), "aws:kms".to_string());
        }
        SseDecision::None => {
            dst_metadata.remove("__sse_algorithm__");
        }
    }
    let dst_content_type = if metadata_directive == Some("REPLACE") {
        new_content_type
            .unwrap_or("application/octet-stream")
            .to_string()
    } else {
        src_meta.content_type.clone()
    };

    // Step E: encrypt if destination requires SSE, otherwise write plaintext.
    let (write_bytes, sse_sidecar_opt): (Bytes, Option<ObjectSseSidecar>) = match &dst_sse_decision
    {
        SseDecision::Aes256 => {
            let aad = format!("{}/{}", bucket, key);
            match state
                .encryption
                .encrypt(&plaintext, Some(aad.as_bytes()))
                .await
            {
                Ok(enc) => {
                    let sidecar = ObjectSseSidecar::from_encrypted(&enc, &bucket, &key);
                    (Bytes::from(enc.ciphertext), Some(sidecar))
                }
                Err(e) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &format!("Encryption failed: {}", e),
                        &format!("/{}/{}", bucket, key),
                    );
                }
            }
        }
        SseDecision::SseC {
            key: customer_key,
            key_md5,
        } => {
            let aad = format!("{}/{}", bucket, key);
            match state
                .encryption
                .encrypt_with_customer_key(&plaintext, customer_key.as_ref(), Some(aad.as_bytes()))
                .await
            {
                Ok(enc) => {
                    let mut sidecar = ObjectSseSidecar::from_encrypted(&enc, &bucket, &key);
                    sidecar.algorithm = "AES256-SSE-C".to_string();
                    sidecar.customer_key_md5 = Some(key_md5.to_string());
                    (Bytes::from(enc.ciphertext), Some(sidecar))
                }
                Err(_) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        "SSE-C encryption failed",
                        &format!("/{}/{}", bucket, key),
                    );
                }
            }
        }
        SseDecision::SseKms { key_id } => {
            let aad = format!("{}/{}", bucket, key);
            match state
                .encryption
                .encrypt_with_kek_id(&plaintext, key_id, Some(aad.as_bytes()))
                .await
            {
                Ok(enc) => {
                    let mut sidecar = ObjectSseSidecar::from_encrypted(&enc, &bucket, &key);
                    sidecar.algorithm = "aws:kms".to_string();
                    sidecar.kms_master_key_id = Some(format_kms_arn(key_id));
                    (Bytes::from(enc.ciphertext), Some(sidecar))
                }
                Err(e) => {
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &format!("SSE-KMS encryption failed: {}", e),
                        &format!("/{}/{}", bucket, key),
                    );
                }
            }
        }
        SseDecision::None => {
            // encrypted → plain — no new sidecar needed, delete any stale one later.
            (Bytes::from(plaintext.clone()), None)
        }
    };

    // ETag is always sha256(plaintext) regardless of what goes on disk.
    let dst_etag = if is_src_enc {
        // Source was encrypted — its stored ETag was already sha256(plaintext).
        src_meta.etag.clone()
    } else {
        // Source was plain — compute from plaintext we just read.
        hex::encode(sha2::Sha256::digest(&plaintext))
    };

    // Step F: write to a temp file and use put_object_from_path for an atomic rename.
    let tmp_dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = tmp_dir.join(format!("rs3gw-copy-{}-{}.tmp", uuid::Uuid::new_v4(), nanos));
    {
        use tokio::io::AsyncWriteExt;
        let mut tmp_file = match tokio::fs::File::create(&tmp_path).await {
            Ok(f) => f,
            Err(e) => {
                return storage_error_to_response(
                    StorageError::from(e),
                    &format!("/{}/{}", bucket, key),
                );
            }
        };
        if let Err(e) = tmp_file.write_all(&write_bytes).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return storage_error_to_response(
                StorageError::from(e),
                &format!("/{}/{}", bucket, key),
            );
        }
        if let Err(e) = tmp_file.flush().await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return storage_error_to_response(
                StorageError::from(e),
                &format!("/{}/{}", bucket, key),
            );
        }
    }

    let write_size = write_bytes.len() as u64;
    match state
        .storage
        .put_object_from_path(
            &bucket,
            &key,
            Some(dst_content_type),
            dst_metadata,
            &tmp_path,
            write_size,
            dst_etag,
        )
        .await
    {
        Ok(etag) => {
            // Write SSE sidecar for encrypted destination, delete stale sidecar for plain.
            if let Some(sidecar) = sse_sidecar_opt {
                if let Err(e) = state.storage.put_object_sse(&bucket, &key, &sidecar).await {
                    tracing::warn!("Failed to write SSE sidecar for {}/{}: {}", bucket, key, e);
                }
            } else {
                // Destination is plaintext — ensure no stale sidecar survives.
                let _ = state.storage.delete_object_sse(&bucket, &key).await;
            }

            let result = CopyObjectResult::new(
                &format!("\"{}\"", etag),
                &chrono::Utc::now()
                    .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                    .to_string(),
            );
            let mut response = axum::response::Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .header("x-amz-version-id", "null")
                .header("x-amz-copy-source-version-id", "null");
            match &dst_sse_decision {
                SseDecision::Aes256 => {
                    response = response.header("x-amz-server-side-encryption", "AES256");
                }
                SseDecision::SseC { key_md5, .. } => {
                    response = response
                        .header("x-amz-server-side-encryption-customer-algorithm", "AES256")
                        .header(
                            "x-amz-server-side-encryption-customer-key-MD5",
                            key_md5.as_str(),
                        );
                }
                SseDecision::SseKms { key_id } => {
                    response = response
                        .header("x-amz-server-side-encryption", "aws:kms")
                        .header(
                            "x-amz-server-side-encryption-aws-kms-key-id",
                            format_kms_arn(key_id).as_str(),
                        );
                }
                SseDecision::None => {}
            }
            response
                .body(axum::body::Body::from(result.to_xml()))
                .unwrap_or_else(|_| {
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
    info!(bucket = %bucket, key = %key, "GetObjectAcl");
    match state.storage.head_object(&bucket, &key).await {
        Ok(_) => {}
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }

    let cfg = match state.storage.get_object_acl(&bucket, &key).await {
        Ok(cfg) => cfg,
        Err(StorageError::NotFound(_)) => AclConfig::canned_full_control("rs3gw", "rs3gw"),
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    };

    let policy = AccessControlPolicy::from_acl_config(&cfg);
    (
        StatusCode::OK,
        [("Content-Type", "application/xml")],
        policy.to_xml(),
    )
        .into_response()
}

/// Put object ACL
pub async fn put_object_acl(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    info!(bucket = %bucket, key = %key, "PutObjectAcl");
    match state.storage.head_object(&bucket, &key).await {
        Ok(_) => {}
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }

    let canned_header = headers
        .get("x-amz-acl")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let has_body = !body.is_empty();

    let cfg = match (canned_header.as_deref(), has_body) {
        (Some(_), true) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "UnexpectedContent",
                "You provided both a canned ACL header and an ACL body; provide only one.",
                &format!("/{}/{}", bucket, key),
            )
        }
        (None, false) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MissingSecurityHeader",
                "Your request was missing a required header.",
                &format!("/{}/{}", bucket, key),
            )
        }
        (Some(canned), false) => match parse_canned_acl_header(canned) {
            Ok(normalized) => match AclConfig::from_canned(normalized, "rs3gw", "rs3gw") {
                Ok(cfg) => cfg,
                Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
            },
            Err(msg) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    &msg,
                    &format!("/{}/{}", bucket, key),
                )
            }
        },
        (None, true) => {
            let xml_str = match std::str::from_utf8(&body) {
                Ok(s) => s,
                Err(_) => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "InvalidArgument",
                        "Request body is not valid UTF-8.",
                        &format!("/{}/{}", bucket, key),
                    )
                }
            };
            match parse_acl_xml(xml_str) {
                Ok(cfg) => cfg,
                Err(msg) => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        "MalformedXML",
                        &msg,
                        &format!("/{}/{}", bucket, key),
                    )
                }
            }
        }
    };

    match state.storage.put_object_acl(&bucket, &key, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
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
