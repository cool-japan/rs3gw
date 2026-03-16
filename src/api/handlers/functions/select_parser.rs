//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::super::types::{
    ConditionalResult, ListObjectVersionsQuery, ListObjectsV1Query, ListObjectsV2Query,
};
use crate::api::xml_responses::{
    ListBucketResult, ListBucketResultV1, ListVersionsResult, ObjectVersion, Owner,
};
use crate::storage::ByteRange;
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures::TryStreamExt;
use tracing::{debug, info, warn};

use super::core::{
    check_conditional_headers, metadata_to_contents, prefixes_to_common, storage_error_to_response,
};
use super::functions_3::build_object_headers_with_sci;
use crate::api::utils::error_response;

/// Parse SelectObjectContent XML request
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
/// List object versions (stub - versioning not fully implemented)
/// Returns current objects as the only "version" with version_id "null"
pub async fn list_object_versions(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<ListObjectVersionsQuery>,
) -> Response {
    info!(bucket = % bucket, "ListObjectVersions");
    let prefix = query.prefix.as_deref().unwrap_or("");
    let delimiter = query.delimiter.as_deref();
    let max_keys = query.max_keys.unwrap_or(1000);
    match state
        .storage
        .list_objects(&bucket, prefix, delimiter, max_keys)
        .await
    {
        Ok((objects, common_prefixes)) => {
            let mut result = ListVersionsResult::new(&bucket);
            result.prefix = prefix.to_string();
            result.key_marker = query.key_marker.clone().unwrap_or_default();
            result.max_keys = max_keys as u32;
            let filtered_objects: Vec<_> = objects
                .into_iter()
                .filter(|obj| {
                    if let Some(marker) = &query.key_marker {
                        obj.key > *marker
                    } else {
                        true
                    }
                })
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
    let sci_meta = state
        .storage
        .get_scientific_metadata(&bucket, &key)
        .await
        .ok()
        .flatten();
    build_object_headers_with_sci(meta, sci_meta)
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
    let range = if let Some(range_str) = range_header {
        match ByteRange::parse(range_str, meta.size) {
            Ok(r) => Some(r),
            Err(e) => {
                warn!(range = % range_str, "Invalid range header");
                return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
            }
        }
    } else {
        None
    };
    let (response_meta, stream, status, content_length, content_range) = if let Some(ref r) = range
    {
        match state.storage.get_object_range(&bucket, &key, r).await {
            Ok((m, s)) => {
                let content_range = format!("bytes {}-{}/{}", r.start, r.end, m.size);
                (
                    m,
                    s,
                    StatusCode::PARTIAL_CONTENT,
                    r.length(),
                    Some(content_range),
                )
            }
            Err(e) => {
                return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
            }
        }
    } else {
        match state.storage.get_object(&bucket, &key).await {
            Ok((m, s)) => {
                let size = m.size;
                (m, s, StatusCode::OK, size, None)
            }
            Err(e) => {
                return storage_error_to_response(e, &format!("/{}/{}", bucket, key));
            }
        }
    };
    state
        .metrics_tracker
        .record_bytes_downloaded(content_length);
    // Streaming: the object body is streamed chunk-by-chunk using axum::body::Body::from_stream,
    // which provides true backpressure — no full buffering in memory occurs here.
    // The storage layer yields chunks lazily via an async stream, so large objects do not cause
    // excess memory consumption.  If the storage backend were to buffer internally (e.g., via
    // `Bytes::copy_from_slice` on the entire file), that would be a known limitation and should
    // be replaced with an incremental `tokio::fs::File` reader wrapped in `ReaderStream`.
    let body = Body::from_stream(stream.map_err(|e| std::io::Error::other(e.to_string())));
    let mut response = Response::builder()
        .status(status)
        .header("Content-Type", &response_meta.content_type)
        .header("Content-Length", content_length)
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
    if let Some(ref cr) = content_range {
        response = response.header("Content-Range", cr.as_str());
    }
    for (k, v) in &response_meta.metadata {
        if !k.starts_with("__sys_") && !k.starts_with("__checksum_") {
            response = response.header(format!("x-amz-meta-{}", k), v);
        }
    }
    // Emit stored checksum header only for full-object responses.
    // For range requests the stored checksum covers the full object, so
    // sending it would cause the SDK to compare it against the partial body
    // and report a ChecksumMismatch.
    if content_range.is_none() {
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
    response.body(body).unwrap_or_else(|_| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            "Failed to build response",
            &format!("/{}/{}", bucket, key),
        )
    })
}
