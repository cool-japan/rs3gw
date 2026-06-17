//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::super::types::{
    ConditionalResult, ListObjectVersionsQuery, ListObjectsV1Query, ListObjectsV2Query,
};
use crate::api::xml_responses::{
    DeleteMarkerEntry, ListBucketResult, ListBucketResultV1, ListVersionsResult, ObjectVersion,
    Owner,
};
use crate::storage::encryption::EncryptedData;
use crate::storage::versioning::VersioningStatus;
use crate::storage::{ByteRange, ObjectSseSidecar, StorageError};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use futures::TryStreamExt;
use tracing::{debug, info, warn};

use super::core::{
    check_conditional_headers, metadata_to_contents, prefixes_to_common, storage_error_to_response,
};
use super::functions_3::build_object_headers_with_sci;
use crate::api::utils::error_response;

/// Parse SelectObjectContent XML request
#[cfg(feature = "formats")]
pub(super) fn parse_select_request_xml(
    body: &[u8],
) -> Result<crate::api::select::SelectRequest, String> {
    use crate::api::select::*;
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_reader(body);
    reader.config_mut().trim_text(true);
    let mut expression = String::new();
    let expression_type = ExpressionType::Sql;
    let mut input_format = InputFormat::Csv(CsvInput::new());
    let mut output_format = OutputFormat::Json(JsonOutput::default());
    let mut current_path = Vec::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                current_path.push(name.clone());
                if current_path.ends_with(&["InputSerialization".to_string(), "CSV".to_string()]) {
                    input_format = InputFormat::Csv(CsvInput::new());
                } else if current_path
                    .ends_with(&["InputSerialization".to_string(), "JSON".to_string()])
                {
                    input_format = InputFormat::Json(JsonInput::default());
                } else if current_path
                    .ends_with(&["InputSerialization".to_string(), "Parquet".to_string()])
                {
                    input_format = InputFormat::Parquet;
                } else if current_path
                    .ends_with(&["OutputSerialization".to_string(), "CSV".to_string()])
                {
                    output_format = OutputFormat::Csv(CsvOutput::default());
                } else if current_path
                    .ends_with(&["OutputSerialization".to_string(), "JSON".to_string()])
                {
                    output_format = OutputFormat::Json(JsonOutput::default());
                }
            }
            Ok(Event::End(_)) => {
                current_path.pop();
            }
            Ok(Event::Text(e)) => {
                let text = String::from_utf8_lossy(&e).to_string();
                if current_path.ends_with(&["Expression".to_string()]) {
                    expression = text;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("XML parse error: {}", e)),
            _ => {}
        }
        buf.clear();
    }
    if expression.is_empty() {
        return Err("No Expression provided".to_string());
    }
    Ok(SelectRequest {
        expression,
        expression_type,
        input_serialization: input_format,
        output_serialization: output_format,
        scan_range: None,
    })
}
/// List objects in a bucket (V2)
#[utoipa::path(
    get,
    path = "/{bucket}",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        (
            "prefix" = Option<String>,
            Query,
            description = "Limits the response to keys that begin with the specified prefix"
        ),
        (
            "delimiter" = Option<String>,
            Query,
            description = "Character used to group keys"
        ),
        (
            "max-keys" = Option<i32>,
            Query,
            description = "Maximum number of keys to return (default 1000)"
        ),
        (
            "continuation-token" = Option<String>,
            Query,
            description = "Token from previous response to continue listing"
        ),
        (
            "start-after" = Option<String>,
            Query,
            description = "Key to start listing after"
        )
    ),
    responses(
        (
            status = 200,
            description = "List of objects in the bucket",
            content_type = "application/xml"
        ),
        (status = 404, description = "Bucket not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_objects_v2(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<ListObjectsV2Query>,
) -> Response {
    use base64::Engine;
    let prefix = params.prefix.as_deref().unwrap_or("");
    let delimiter = params.delimiter.as_deref();
    let max_keys = params.max_keys.unwrap_or(1000).min(1000);
    let decoded_token: Option<String> = if params.start_after.is_some() {
        None
    } else if let Some(ref token) = params.continuation_token {
        base64::engine::general_purpose::STANDARD
            .decode(token)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
    } else {
        None
    };
    let start_after = params.start_after.as_deref().or(decoded_token.as_deref());
    info!(
        bucket = % bucket, prefix = % prefix, delimiter = ? delimiter, max_keys = %
        max_keys, start_after = ? start_after, continuation_token = ? params
        .continuation_token, "ListObjectsV2"
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
    match state
        .storage
        .list_objects_with_pagination(&bucket, prefix, delimiter, max_keys, start_after)
        .await
    {
        Ok((objects, common_prefixes, is_truncated)) => {
            let mut result = ListBucketResult::new(&bucket);
            result.prefix = prefix.to_string();
            result.delimiter = delimiter.map(String::from);
            result.max_keys = max_keys;
            result.key_count = objects.len() + common_prefixes.len();
            result.is_truncated = is_truncated;
            result.continuation_token = params.continuation_token.clone();
            if is_truncated {
                if let Some(last_obj) = objects.last() {
                    result.next_continuation_token = Some(
                        base64::engine::general_purpose::STANDARD.encode(last_obj.key.as_bytes()),
                    );
                }
            }
            result.contents = objects.into_iter().map(metadata_to_contents).collect();
            result.common_prefixes = prefixes_to_common(common_prefixes);
            (
                StatusCode::OK,
                [
                    ("Content-Type", "application/xml"),
                    ("x-amz-request-id", &uuid::Uuid::new_v4().to_string()),
                ],
                result.to_xml(),
            )
                .into_response()
        }
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}
/// List objects in a bucket (V1 - original S3 API)
pub async fn list_objects_v1(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(params): Query<ListObjectsV1Query>,
) -> Response {
    let prefix = params.prefix.as_deref().unwrap_or("");
    let delimiter = params.delimiter.as_deref();
    let max_keys = params.max_keys.unwrap_or(1000).min(1000);
    let marker = params.marker.as_deref();
    info!(
        bucket = % bucket, prefix = % prefix, delimiter = ? delimiter, max_keys = %
        max_keys, marker = ? marker, "ListObjectsV1"
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
    match state
        .storage
        .list_objects_with_pagination(&bucket, prefix, delimiter, max_keys, marker)
        .await
    {
        Ok((objects, common_prefixes, is_truncated)) => {
            let mut result = ListBucketResultV1::new(&bucket);
            result.prefix = prefix.to_string();
            result.marker = marker.unwrap_or("").to_string();
            result.delimiter = delimiter.map(String::from);
            result.max_keys = max_keys;
            result.is_truncated = is_truncated;
            if is_truncated {
                if let Some(last_obj) = objects.last() {
                    result.next_marker = Some(last_obj.key.clone());
                }
            }
            result.contents = objects.into_iter().map(metadata_to_contents).collect();
            result.common_prefixes = prefixes_to_common(common_prefixes);
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
/// List object versions
pub async fn list_object_versions(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectVersionsQuery>,
) -> Response {
    info!(bucket = %bucket, "ListObjectVersions");
    let prefix = query.prefix.as_deref().unwrap_or("");
    let delimiter = query.delimiter.as_deref();
    let max_keys = query.max_keys.unwrap_or(1000);
    let key_marker = query.key_marker.clone().unwrap_or_default();
    let version_id_marker = query.version_id_marker.clone().unwrap_or_default();

    let versioning_enabled = match state.storage.get_bucket_versioning(&bucket).await {
        Ok(cfg) => !matches!(cfg.status, VersioningStatus::Unversioned),
        Err(StorageError::BucketNotFound) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                "The specified bucket does not exist.",
                &format!("/{}", bucket),
            )
        }
        Err(e) => return storage_error_to_response(e, &format!("/{}", bucket)),
    };

    if versioning_enabled {
        match state
            .storage
            .versioning_manager()
            .list_all_versions(&bucket)
            .await
        {
            Ok(all_versions) => {
                let mut result = ListVersionsResult::new(&bucket);
                result.prefix = prefix.to_string();
                result.key_marker = key_marker.clone();
                result.version_id_marker = version_id_marker.clone();
                result.max_keys = max_keys as u32;
                result.delimiter = delimiter.unwrap_or("").to_string();

                let filtered: Vec<_> = all_versions
                    .into_iter()
                    .filter(|v| {
                        if !prefix.is_empty() && !v.key.starts_with(prefix) {
                            return false;
                        }
                        if key_marker.is_empty() {
                            return true;
                        }
                        if v.key > key_marker {
                            return true;
                        }
                        if v.key == key_marker && !version_id_marker.is_empty() {
                            return v.version_id > version_id_marker;
                        }
                        false
                    })
                    .take(max_keys + 1)
                    .collect();

                let is_truncated = filtered.len() > max_keys;
                let items: Vec<_> = filtered.into_iter().take(max_keys).collect();

                result.is_truncated = is_truncated;
                for v in &items {
                    if v.is_delete_marker {
                        result.delete_markers.push(DeleteMarkerEntry {
                            key: v.key.clone(),
                            version_id: v.version_id.clone(),
                            is_latest: v.is_latest,
                            last_modified: v
                                .created_at
                                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                                .to_string(),
                            owner: Owner::default(),
                        });
                    } else {
                        result.versions.push(ObjectVersion {
                            key: v.key.clone(),
                            version_id: v.version_id.clone(),
                            is_latest: v.is_latest,
                            last_modified: v
                                .created_at
                                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                                .to_string(),
                            etag: format!("\"{}\"", v.etag),
                            size: v.size,
                            storage_class: "STANDARD".to_string(),
                            owner: Owner::default(),
                        });
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
    } else {
        match state
            .storage
            .list_objects(&bucket, prefix, delimiter, max_keys)
            .await
        {
            Ok((objects, common_prefixes)) => {
                let mut result = ListVersionsResult::new(&bucket);
                result.prefix = prefix.to_string();
                result.key_marker = key_marker.clone();
                result.max_keys = max_keys as u32;
                let filtered_objects: Vec<_> = objects
                    .into_iter()
                    .filter(|obj| key_marker.is_empty() || obj.key > key_marker)
                    .take(max_keys)
                    .collect();
                result.is_truncated = filtered_objects.len() >= max_keys;
                result.versions = filtered_objects
                    .into_iter()
                    .map(|obj| ObjectVersion {
                        key: obj.key,
                        version_id: "null".to_string(),
                        is_latest: true,
                        last_modified: obj
                            .last_modified
                            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                            .to_string(),
                        etag: format!("\"{}\"", obj.etag),
                        size: obj.size,
                        storage_class: "STANDARD".to_string(),
                        owner: Owner::default(),
                    })
                    .collect();
                result.common_prefixes = prefixes_to_common(common_prefixes);
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
}
/// Get object metadata
#[utoipa::path(
    head,
    path = "/{bucket}/{key}",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        ("key" = String, Path, description = "Object key name")
    ),
    responses(
        (
            status = 200,
            description = "Object metadata retrieved successfully",
            headers(
                ("ETag" = String, description = "Entity tag for the object"),
                ("Content-Length" = i64, description = "Size of the object"),
                ("Content-Type" = String, description = "MIME type of the object"),
                ("Last-Modified" = String, description = "Last modification date")
            )
        ),
        (status = 304, description = "Not modified (conditional request)"),
        (status = 404, description = "Object or bucket not found"),
        (status = 412, description = "Precondition failed"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn head_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    info!(bucket = % bucket, key = % key, "HeadObject");
    let mut meta = match state.storage.head_object(&bucket, &key).await {
        Ok(m) => m,
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    };
    match check_conditional_headers(&headers, &meta.etag, meta.last_modified) {
        ConditionalResult::NotModified(etag) => {
            return (
                StatusCode::NOT_MODIFIED,
                [("ETag", format!("\"{}\"", etag))],
            )
                .into_response();
        }
        ConditionalResult::PreconditionFailed(etag) => {
            return (
                StatusCode::PRECONDITION_FAILED,
                [("ETag", format!("\"{}\"", etag))],
            )
                .into_response();
        }
        ConditionalResult::Proceed => {}
    }
    let sci_meta = state
        .storage
        .get_scientific_metadata(&bucket, &key)
        .await
        .ok()
        .flatten();

    // For SSE-C and SSE-KMS objects, augment the response with sidecar-derived headers.
    // The base builder (build_object_headers_with_sci) already emits the algorithm header.
    let sse_algo = meta.metadata.get("__sse_algorithm__").cloned();
    let is_sse_object = sse_algo.is_some();
    let is_sse_c = sse_algo.as_deref() == Some("AES256-SSE-C");
    let is_sse_kms = sse_algo.as_deref() == Some("aws:kms");

    // For SSE objects, `meta.size` is the on-disk size, which for single-PUT objects is the
    // ciphertext length (plaintext + per-chunk GCM tags) and is therefore larger than the
    // plaintext. AWS S3 reports the *plaintext* size in Content-Length for both GET and HEAD,
    // so derive it from the sidecar and overwrite `meta.size` before building headers — this
    // keeps HEAD consistent with the GET path (which sizes from the sidecar too). Best-effort:
    // if the sidecar can't be loaded, fall back to the on-disk size rather than failing HEAD.
    let sidecar_opt = if is_sse_object {
        state
            .storage
            .get_object_sse(&bucket, &key)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    if let Some(ref sidecar) = sidecar_opt {
        meta.size = if !sidecar.chunks.is_empty() {
            sidecar.chunks.iter().map(|c| c.plaintext_len).sum()
        } else {
            crate::storage::encryption::single_shot_plaintext_len(sidecar.ciphertext_len)
        };
    }

    let base_response = build_object_headers_with_sci(meta, sci_meta);

    if !is_sse_c && !is_sse_kms {
        return base_response;
    }

    let Some(sidecar) = sidecar_opt else {
        return base_response;
    };

    let (mut parts, body) = base_response.into_parts();

    if is_sse_c {
        // Append customer-key-MD5 for SSE-C.
        if let Some(ref key_md5) = sidecar.customer_key_md5 {
            parts.headers.insert(
                axum::http::header::HeaderName::from_static(
                    "x-amz-server-side-encryption-customer-key-md5",
                ),
                axum::http::HeaderValue::from_str(key_md5)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("")),
            );
        }
    } else if is_sse_kms {
        // Append KMS key ARN for SSE-KMS.
        if let Some(ref kms_key_id) = sidecar.kms_master_key_id {
            if let Ok(hv) = axum::http::HeaderValue::from_str(kms_key_id) {
                parts.headers.insert(
                    axum::http::header::HeaderName::from_static(
                        "x-amz-server-side-encryption-aws-kms-key-id",
                    ),
                    hv,
                );
            }
        }
    }

    Response::from_parts(parts, body)
}

/// Read the full ciphertext of an SSE object from disk and decrypt it to plaintext.
///
/// `state.storage.get_object` transparently decompresses ciphertext that was compressed on
/// disk, so the bytes handed to the cipher are always raw ciphertext. SSE-C objects use the
/// customer-supplied key (MD5-validated against the value stored at PUT time); SSE-S3 and
/// SSE-KMS objects use the server-managed KEK via the v1/v2 dispatch in `decrypt`. An empty
/// object (0 ciphertext bytes) decrypts to empty plaintext without invoking the cipher — an
/// empty single-shot payload has no nonce and would otherwise be rejected.
///
/// On any failure it returns a ready-to-send error [`Response`].
async fn read_full_sse_plaintext(
    state: &AppState,
    bucket: &str,
    key: &str,
    sidecar: &ObjectSseSidecar,
    is_sse_c: bool,
    customer_key: Option<[u8; 32]>,
) -> Result<Vec<u8>, Response> {
    let path_ctx = format!("/{}/{}", bucket, key);

    let (_m, stream) = match state.storage.get_object(bucket, key).await {
        Ok(v) => v,
        Err(e) => return Err(storage_error_to_response(e, &path_ctx)),
    };
    let ciphertext: Vec<u8> = {
        use futures::StreamExt;
        let mut collected = Vec::new();
        let mut s = stream;
        while let Some(chunk) = s.next().await {
            match chunk {
                Ok(bytes) => collected.extend_from_slice(&bytes),
                Err(e) => {
                    return Err(error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "InternalError",
                        &format!("Failed to read encrypted object: {}", e),
                        &path_ctx,
                    ));
                }
            }
        }
        collected
    };

    // Empty object: no ciphertext → empty plaintext (an empty single-shot payload has no nonce).
    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }

    if is_sse_c {
        let customer_key = match customer_key {
            Some(k) => k,
            None => {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidRequest",
                    "SSE-C object requires x-amz-server-side-encryption-customer-key on GET",
                    &path_ctx,
                ));
            }
        };
        // Validate the customer key MD5 against what was stored at PUT time.
        let computed_md5_b64 =
            base64::engine::general_purpose::STANDARD.encode(md5::compute(customer_key).0);
        if let Some(ref stored_md5) = sidecar.customer_key_md5 {
            if stored_md5 != &computed_md5_b64 {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    "The provided customer key does not match the key used to encrypt the object",
                    &path_ctx,
                ));
            }
        }
        let aad = format!("{}/{}", sidecar.aad_bucket, sidecar.aad_key);
        let enc_data = EncryptedData {
            algorithm: crate::storage::encryption::EncryptionAlgorithm::Aes256Gcm,
            encrypted_dek: sidecar.encrypted_dek.clone(),
            kek_id: sidecar.kek_id.clone(),
            dek_nonce: sidecar.dek_nonce.clone(),
            ciphertext,
            payload_nonce: sidecar.payload_nonce.clone(),
            aad: Some(aad.into_bytes()),
            chunks: vec![],
            chunk_size: 0,
        };
        match state
            .encryption
            .decrypt_with_customer_key(&enc_data, &customer_key)
            .await
        {
            Ok(p) => Ok(p),
            Err(_) => Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                "SSE-C decryption failed",
                &path_ctx,
            )),
        }
    } else {
        // SSE-S3 / SSE-KMS (v1 single-shot or v2 chunked): decrypt dispatches on `chunks`.
        let enc_data = sidecar.into_encrypted_data(ciphertext);
        match state.encryption.decrypt(&enc_data).await {
            Ok(p) => Ok(p),
            Err(e) => Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                &format!("Decryption failed: {}", e),
                &path_ctx,
            )),
        }
    }
}
/// Get an object
#[utoipa::path(
    get,
    path = "/{bucket}/{key}",
    tag = "Objects",
    params(
        ("bucket" = String, Path, description = "Name of the bucket"),
        ("key" = String, Path, description = "Object key name")
    ),
    responses(
        (
            status = 200,
            description = "Object retrieved successfully",
            content_type = "application/octet-stream"
        ),
        (status = 206, description = "Partial content (range request)"),
        (status = 304, description = "Not modified (conditional request)"),
        (status = 404, description = "Object or bucket not found"),
        (status = 412, description = "Precondition failed"),
        (status = 416, description = "Range not satisfiable")
    )
)]
pub async fn get_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    state.metrics_tracker.record_request();
    info!(bucket = % bucket, key = % key, "GetObject");
    let range_header = headers.get("Range").and_then(|v| v.to_str().ok());
    debug!(range = ? range_header, "Range request");
    let meta = match state.storage.head_object(&bucket, &key).await {
        Ok(m) => m,
        Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    };
    match check_conditional_headers(&headers, &meta.etag, meta.last_modified) {
        ConditionalResult::NotModified(etag) => {
            return (
                StatusCode::NOT_MODIFIED,
                [("ETag", format!("\"{}\"", etag))],
            )
                .into_response();
        }
        ConditionalResult::PreconditionFailed(etag) => {
            return (
                StatusCode::PRECONDITION_FAILED,
                [("ETag", format!("\"{}\"", etag))],
            )
                .into_response();
        }
        ConditionalResult::Proceed => {}
    }
    // SSE objects store ciphertext on disk, and `meta.size` is unreliable for them (ciphertext
    // size for single-PUT objects, plaintext size for multipart). Load the SSE sidecar up front
    // so we can size the range in plaintext coordinates and, for v2 (chunked) objects, read only
    // the ciphertext bytes covering the requested chunks on a range-GET.
    let sse_algo = meta.metadata.get("__sse_algorithm__").cloned();
    let is_sse_object = sse_algo.is_some();
    let is_sse_c = sse_algo.as_deref() == Some("AES256-SSE-C");

    let sse_sidecar: Option<ObjectSseSidecar> = if is_sse_object {
        match state.storage.get_object_sse(&bucket, &key).await {
            Ok(Some(s)) => Some(s),
            Ok(None) => {
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "SSE sidecar missing for encrypted object",
                    &format!("/{}/{}", bucket, key),
                );
            }
            Err(e) => return storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
        }
    } else {
        None
    };

    // Logical (plaintext) size: derived from the sidecar for SSE objects, else `meta.size`.
    let logical_size: u64 = match &sse_sidecar {
        Some(sc) if !sc.chunks.is_empty() => sc.chunks.iter().map(|c| c.plaintext_len).sum(),
        Some(sc) => crate::storage::encryption::single_shot_plaintext_len(sc.ciphertext_len),
        None => meta.size,
    };

    let range = if let Some(range_str) = range_header {
        match ByteRange::parse(range_str, logical_size) {
            Ok(r) => Some(r),
            Err(e) => {
                warn!(range = % range_str, "Invalid range header");
                return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
            }
        }
    } else {
        None
    };

    let status = if range.is_some() {
        StatusCode::PARTIAL_CONTENT
    } else {
        StatusCode::OK
    };

    // SSE-C objects require the client to supply the customer key on GET.
    // Validate the key before reading any bytes.
    let sse_c_customer_key: Option<[u8; 32]> = if is_sse_c {
        let key_b64 = match headers
            .get("x-amz-server-side-encryption-customer-key")
            .and_then(|v| v.to_str().ok())
        {
            Some(v) => v.to_string(),
            None => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidRequest",
                    "SSE-C object requires x-amz-server-side-encryption-customer-key on GET",
                    &format!("/{}/{}", bucket, key),
                );
            }
        };

        let key_bytes = match base64::engine::general_purpose::STANDARD.decode(&key_b64) {
            Ok(b) => b,
            Err(_) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    "x-amz-server-side-encryption-customer-key is not valid base64",
                    &format!("/{}/{}", bucket, key),
                );
            }
        };
        if key_bytes.len() != 32 {
            return error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                &format!(
                    "SSE-C customer key must be 32 bytes for AES256, got {}",
                    key_bytes.len()
                ),
                &format!("/{}/{}", bucket, key),
            );
        }

        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);
        Some(key_array)
    } else {
        None
    };

    // For SSE objects the on-disk bytes are ciphertext. For a v2 (chunked) range-GET on an
    // uncompressed object we read only the ciphertext span covering the requested chunks and
    // decrypt those chunks (seekable — no full-object read). Every other SSE case reads the
    // full ciphertext and decrypts it; range slicing then happens on the plaintext.
    let (final_body, final_content_length, final_content_range) =
        if let Some(sidecar) = sse_sidecar.as_ref() {
            // A raw seek into a compressed-on-disk ciphertext file is invalid, so the seekable
            // path only applies to uncompressed v2 chunked objects.
            let is_compressed = meta.metadata.contains_key("__compression__");
            let chunked_seekable = !is_sse_c && !is_compressed && !sidecar.chunks.is_empty();

            if let Some(ref r) = range {
                // r.end is inclusive; chunk helpers expect an exclusive end.
                let range_end_exclusive = r.end + 1;
                if chunked_seekable {
                    let span = match crate::storage::encryption::chunk_ciphertext_span(
                        &sidecar.chunks,
                        sidecar.chunk_size,
                        r.start,
                        range_end_exclusive,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            return error_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "InternalError",
                                &format!("Range computation failed: {}", e),
                                &format!("/{}/{}", bucket, key),
                            );
                        }
                    };
                    // Read ONLY the covering ciphertext bytes from disk.
                    let chunk_ct = match state
                        .storage
                        .read_object_ciphertext_range(&bucket, &key, span.file_start, span.file_end)
                        .await
                    {
                        Ok(b) => b,
                        Err(e) => {
                            return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
                        }
                    };
                    let aad_prefix = format!("{}/{}", sidecar.aad_bucket, sidecar.aad_key);
                    let slice = match state
                        .encryption
                        .decrypt_chunked_range_from_slice(
                            &sidecar.kek_id,
                            &sidecar.encrypted_dek,
                            &sidecar.dek_nonce,
                            &sidecar.chunks,
                            &span,
                            &chunk_ct,
                            r.start,
                            range_end_exclusive,
                            aad_prefix.as_bytes(),
                        )
                        .await
                    {
                        Ok(s) => s,
                        Err(e) => {
                            return error_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "InternalError",
                                &format!("Chunked range decryption failed: {}", e),
                                &format!("/{}/{}", bucket, key),
                            );
                        }
                    };
                    let slice_len = slice.len() as u64;
                    let cr = Some(format!(
                        "bytes {}-{}/{}",
                        r.start,
                        r.start + slice_len - 1,
                        logical_size,
                    ));
                    (Body::from(bytes::Bytes::from(slice)), slice_len, cr)
                } else {
                    // SSE-C, compressed, or v1 single-shot: full read + decrypt, then slice.
                    let plaintext = match read_full_sse_plaintext(
                        &state,
                        &bucket,
                        &key,
                        sidecar,
                        is_sse_c,
                        sse_c_customer_key,
                    )
                    .await
                    {
                        Ok(p) => p,
                        Err(resp) => return resp,
                    };
                    let start = r.start as usize;
                    if start >= plaintext.len() {
                        return storage_error_to_response(
                            StorageError::InvalidRange,
                            &format!("/{}/{}", bucket, key),
                        );
                    }
                    let end = (r.end as usize + 1).min(plaintext.len());
                    let slice = plaintext[start..end].to_vec();
                    let slice_len = slice.len() as u64;
                    let cr = Some(format!(
                        "bytes {}-{}/{}",
                        r.start,
                        r.start + slice_len - 1,
                        plaintext.len()
                    ));
                    (Body::from(bytes::Bytes::from(slice)), slice_len, cr)
                }
            } else {
                // Full-object GET on an SSE object.
                let plaintext = match read_full_sse_plaintext(
                    &state,
                    &bucket,
                    &key,
                    sidecar,
                    is_sse_c,
                    sse_c_customer_key,
                )
                .await
                {
                    Ok(p) => p,
                    Err(resp) => return resp,
                };
                // D2: decrypt-then-hash. Validate the stored plaintext SHA-256 digest against
                // the just-decrypted bytes (full-object GET only; range bodies are partial).
                // This is end-to-end integrity on top of the AEAD tags checked during decrypt.
                if let (Some(algo), Some(stored)) = (
                    meta.metadata.get("__checksum_algo__"),
                    meta.metadata.get("__checksum_value__"),
                ) {
                    if algo == "sha256" {
                        let computed = base64::engine::general_purpose::STANDARD
                            .encode(<sha2::Sha256 as sha2::Digest>::digest(&plaintext));
                        if &computed != stored {
                            return error_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "InternalError",
                                "checksum mismatch: object data corrupted",
                                &format!("/{}/{}", bucket, key),
                            );
                        }
                    }
                }
                let len = plaintext.len() as u64;
                (Body::from(bytes::Bytes::from(plaintext)), len, None)
            }
        } else {
            // Non-SSE object: stream from disk (seekable for range). Unchanged behavior.
            let (stream, content_length, content_range) = if let Some(ref r) = range {
                match state.storage.get_object_range(&bucket, &key, r).await {
                    Ok((m, s)) => {
                        let content_range = format!("bytes {}-{}/{}", r.start, r.end, m.size);
                        (s, r.length(), Some(content_range))
                    }
                    Err(e) => {
                        return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
                    }
                }
            } else {
                match state.storage.get_object(&bucket, &key).await {
                    Ok((m, s)) => {
                        let size = m.size;
                        (s, size, None)
                    }
                    Err(e) => {
                        return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
                    }
                }
            };
            let body = Body::from_stream(stream.map_err(|e| std::io::Error::other(e.to_string())));
            (body, content_length, content_range)
        };

    let response_meta = meta;

    state
        .metrics_tracker
        .record_bytes_downloaded(final_content_length);
    state
        .usage_tracker
        .record_get(&bucket, final_content_length)
        .await;

    let mut response = Response::builder()
        .status(status)
        .header("Content-Type", &response_meta.content_type)
        .header("Content-Length", final_content_length)
        .header("ETag", format!("\"{}\"", response_meta.etag))
        .header(
            "Last-Modified",
            response_meta
                .last_modified
                .format("%a, %d %b %Y %H:%M:%S GMT")
                .to_string(),
        )
        .header("Accept-Ranges", "bytes")
        .header("x-amz-storage-class", "STANDARD")
        .header("x-amz-version-id", "null")
        .header("x-amz-request-id", uuid::Uuid::new_v4().to_string());
    if let Some(ref cr) = final_content_range {
        response = response.header("Content-Range", cr.as_str());
    }
    // Emit SSE algorithm header when present.
    // SSE-C and SSE-KMS use different headers from SSE-S3.
    match sse_algo.as_deref() {
        Some("AES256") => {
            response = response.header("x-amz-server-side-encryption", "AES256");
        }
        Some("AES256-SSE-C") => {
            response = response.header("x-amz-server-side-encryption-customer-algorithm", "AES256");
            // Include customer-key-MD5 in GET response (from the sidecar loaded above).
            if let Some(ref sc) = sse_sidecar {
                if let Some(ref md5) = sc.customer_key_md5 {
                    response = response.header(
                        "x-amz-server-side-encryption-customer-key-md5",
                        md5.as_str(),
                    );
                }
            }
        }
        Some("aws:kms") => {
            response = response.header("x-amz-server-side-encryption", "aws:kms");
            // Include KMS key ARN in GET response (from the sidecar loaded above).
            if let Some(ref sc) = sse_sidecar {
                if let Some(ref kms_key_id) = sc.kms_master_key_id {
                    response = response.header(
                        "x-amz-server-side-encryption-aws-kms-key-id",
                        kms_key_id.as_str(),
                    );
                }
            }
        }
        _ => {}
    }
    for (k, v) in &response_meta.metadata {
        if !k.starts_with("__sys_") && !k.starts_with("__checksum_") && !k.starts_with("__sse_") {
            response = response.header(format!("x-amz-meta-{}", k), v);
        }
    }
    // Emit stored checksum header only for full-object responses.
    // For range requests the stored checksum covers the full object, so
    // sending it would cause the SDK to compare it against the partial body
    // and report a ChecksumMismatch.
    if final_content_range.is_none() {
        if let (Some(algo), Some(value)) = (
            response_meta.metadata.get("__checksum_algo__"),
            response_meta.metadata.get("__checksum_value__"),
        ) {
            response = response.header(format!("x-amz-checksum-{}", algo), value);
        }
    }
    // Emit stored caching/content headers
    const SYS_RESPONSE_HEADERS: &[(&str, &str)] = &[
        ("__sys_content_disposition__", "Content-Disposition"),
        ("__sys_cache_control__", "Cache-Control"),
        ("__sys_expires__", "Expires"),
        ("__sys_content_encoding__", "Content-Encoding"),
    ];
    for (reserved_key, response_header) in SYS_RESPONSE_HEADERS {
        if let Some(v) = response_meta.metadata.get(*reserved_key) {
            response = response.header(*response_header, v);
        }
    }
    if let Ok(Some(sci_meta)) = state.storage.get_scientific_metadata(&bucket, &key).await {
        for (k, v) in sci_meta.to_s3_metadata() {
            response = response.header(format!("x-amz-meta-{}", k), v);
        }
    }
    response.body(final_body).unwrap_or_else(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            "Failed to build response",
            &format!("/{}/{}", bucket, key),
        )
    })
}
