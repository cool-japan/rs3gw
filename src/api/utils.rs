//! API Utility Functions
//!
//! Common helper functions used across S3 API handlers.

use axum::{
    body::Body,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use base64::Engine;
use chrono::{DateTime, Utc};

use super::xml_responses::ErrorResponse;

/// Generate a unique request ID pair (x-amz-request-id and x-amz-id-2)
pub fn generate_request_ids() -> (String, String) {
    let request_id = uuid::Uuid::new_v4().to_string();
    // x-amz-id-2 is typically a base64-encoded extended request ID
    let id2 =
        base64::engine::general_purpose::STANDARD.encode(format!("rs3gw-{}", uuid::Uuid::new_v4()));
    (request_id, id2)
}

/// Build an S3 error response with proper XML formatting and headers
pub fn error_response(status: StatusCode, code: &str, message: &str, resource: &str) -> Response {
    let (request_id, id2) = generate_request_ids();
    let error = ErrorResponse {
        code: code.to_string(),
        message: message.to_string(),
        resource: resource.to_string(),
        request_id: request_id.clone(),
    };

    Response::builder()
        .status(status)
        .header("Content-Type", "application/xml")
        .header("x-amz-request-id", request_id)
        .header("x-amz-id-2", id2)
        .body(Body::from(error.to_xml()))
        .unwrap_or_else(|_| status.into_response())
}

/// Check if an ETag matches an expected value (handles quoted and unquoted ETags, wildcards)
pub fn etag_matches(actual: &str, expected: &str) -> bool {
    // Handle wildcard
    if expected.trim() == "*" {
        return true;
    }

    // Normalize ETags by removing surrounding quotes
    fn normalize(s: &str) -> &str {
        let s = s.trim();
        let s = s.strip_prefix('"').unwrap_or(s);
        let s = s.strip_suffix('"').unwrap_or(s);
        // Also handle W/ weak validator prefix
        let s = s.strip_prefix("W/").unwrap_or(s);
        let s = s.strip_prefix('"').unwrap_or(s);
        s.strip_suffix('"').unwrap_or(s)
    }

    // ETags can be comma-separated list
    for part in expected.split(',') {
        if normalize(actual) == normalize(part) {
            return true;
        }
    }
    false
}

/// Parse HTTP date format (RFC 7231)
#[allow(clippy::result_unit_err)]
pub fn parse_http_date(s: &str) -> Result<DateTime<Utc>, ()> {
    use chrono::NaiveDateTime;

    // Try RFC 2822 format: "Sun, 06 Nov 1994 08:49:37 GMT"
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Ok(dt.with_timezone(&Utc));
    }

    // Try alternative format: "Sunday, 06-Nov-94 08:49:37 GMT" (RFC 850)
    // Try ANSI C format: "Sun Nov  6 08:49:37 1994"
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%a, %d %b %Y %H:%M:%S GMT") {
        return Ok(dt.and_utc());
    }

    Err(())
}

/// Parse parts from CompleteMultipartUpload XML
pub fn parse_complete_multipart_parts(xml: &str) -> Vec<(u32, String)> {
    let mut parts = Vec::new();

    // Simple regex-free parsing
    for part in xml.split("<Part>").skip(1) {
        let part_end = part.find("</Part>").unwrap_or(part.len());
        let part_content = &part[..part_end];

        let part_number = part_content.find("<PartNumber>").and_then(|start| {
            let rest = &part_content[start + 12..];
            rest.find("</PartNumber>")
                .and_then(|end| rest[..end].parse::<u32>().ok())
        });

        let etag = part_content.find("<ETag>").and_then(|start| {
            let rest = &part_content[start + 6..];
            rest.find("</ETag>").map(|end| {
                // Decode XML entities first, then strip quotes
                rest[..end]
                    .replace("&quot;", "\"")  // Convert XML entity to actual quote
                    .replace("&amp;", "&")     // Convert other common entities
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .trim()
                    .trim_matches('"')         // Then strip quotes
                    .to_string()
            })
        });

        if let (Some(pn), Some(et)) = (part_number, etag) {
            parts.push((pn, et));
        }
    }

    parts.sort_by_key(|(pn, _)| *pn);
    parts
}

/// Parse tagging XML from PutObjectTagging request
pub fn parse_tagging_xml(xml: &str) -> std::collections::HashMap<String, String> {
    let mut tags = std::collections::HashMap::new();

    for tag in xml.split("<Tag>").skip(1) {
        let tag_end = tag.find("</Tag>").unwrap_or(tag.len());
        let tag_content = &tag[..tag_end];

        let key = tag_content.find("<Key>").and_then(|start| {
            let rest = &tag_content[start + 5..];
            rest.find("</Key>").map(|end| rest[..end].to_string())
        });

        let value = tag_content.find("<Value>").and_then(|start| {
            let rest = &tag_content[start + 7..];
            rest.find("</Value>").map(|end| rest[..end].to_string())
        });

        if let (Some(k), Some(v)) = (key, value) {
            tags.insert(k, v);
        }
    }

    tags
}

/// Parse DeleteObjects request XML
pub fn parse_delete_objects_xml(xml: &str) -> Vec<String> {
    let mut keys = Vec::new();

    // Parse <Object><Key>...</Key></Object> elements
    for object in xml.split("<Object>").skip(1) {
        let object_end = object.find("</Object>").unwrap_or(object.len());
        let object_content = &object[..object_end];

        if let Some(key) = object_content.find("<Key>").and_then(|start| {
            let rest = &object_content[start + 5..];
            rest.find("</Key>").map(|end| rest[..end].to_string())
        }) {
            keys.push(key);
        }
    }

    keys
}

/// Add common S3 response headers to a response builder
///
/// This helper function adds standard S3 headers that are commonly included in responses:
/// - x-amz-request-id: Unique request identifier
/// - x-amz-id-2: Extended request ID (base64 encoded)
/// - x-amz-storage-class: Storage class (default: STANDARD)
/// - x-amz-version-id: Object version (default: "null" for non-versioned buckets)
///
/// # Arguments
/// * `builder` - The response builder to add headers to
/// * `include_storage_class` - Whether to include storage class header (default for object operations)
/// * `include_version_id` - Whether to include version ID header (default for object operations)
///
/// # Returns
/// The response builder with common headers added
pub fn add_common_s3_headers(
    mut builder: axum::http::response::Builder,
    include_storage_class: bool,
    include_version_id: bool,
) -> axum::http::response::Builder {
    let (request_id, id2) = generate_request_ids();

    builder = builder
        .header("x-amz-request-id", request_id)
        .header("x-amz-id-2", id2);

    if include_storage_class {
        builder = builder.header("x-amz-storage-class", "STANDARD");
    }

    if include_version_id {
        builder = builder.header("x-amz-version-id", "null");
    }

    builder
}

/// Add custom metadata headers to a response builder
///
/// Converts a hashmap of metadata key-value pairs into x-amz-meta-* headers.
///
/// # Arguments
/// * `builder` - The response builder to add headers to
/// * `metadata` - HashMap of metadata keys and values
///
/// # Returns
/// The response builder with metadata headers added
pub fn add_metadata_headers(
    mut builder: axum::http::response::Builder,
    metadata: &std::collections::HashMap<String, String>,
) -> axum::http::response::Builder {
    for (k, v) in metadata {
        builder = builder.header(format!("x-amz-meta-{}", k), v);
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_http_date() {
        // Test standard HTTP date format (RFC 7231 preferred format)
        // Note: Jan 1, 2020 is a Wednesday
        let result = parse_http_date("Wed, 01 Jan 2020 00:00:00 GMT");
        assert!(result.is_ok(), "Failed to parse 2020 date");
        let dt = result.expect("Failed to unwrap 2020 date");
        assert_eq!(dt.year(), 2020);

        // Test future date - Jan 1, 2030 is a Tuesday
        let result = parse_http_date("Tue, 01 Jan 2030 00:00:00 GMT");
        assert!(result.is_ok(), "Failed to parse 2030 date");
        let dt = result.expect("Failed to unwrap 2030 date");
        assert_eq!(dt.year(), 2030);

        // Test RFC 7231 example date (Nov 6, 1994 is a Sunday)
        let result = parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT");
        assert!(result.is_ok(), "Failed to parse 1994 date");
    }

    #[test]
    fn test_add_common_s3_headers() {
        use axum::http::Response;

        // Test with both storage class and version ID
        let builder = Response::builder();
        let builder = add_common_s3_headers(builder, true, true);
        let response = builder.body(()).expect("Failed to build response");

        assert!(response.headers().contains_key("x-amz-request-id"));
        assert!(response.headers().contains_key("x-amz-id-2"));
        assert_eq!(
            response
                .headers()
                .get("x-amz-storage-class")
                .and_then(|v| v.to_str().ok()),
            Some("STANDARD")
        );
        assert_eq!(
            response
                .headers()
                .get("x-amz-version-id")
                .and_then(|v| v.to_str().ok()),
            Some("null")
        );

        // Test without storage class and version ID
        let builder = Response::builder();
        let builder = add_common_s3_headers(builder, false, false);
        let response = builder.body(()).expect("Failed to build response");

        assert!(response.headers().contains_key("x-amz-request-id"));
        assert!(response.headers().contains_key("x-amz-id-2"));
        assert!(!response.headers().contains_key("x-amz-storage-class"));
        assert!(!response.headers().contains_key("x-amz-version-id"));
    }

    #[test]
    fn test_add_metadata_headers() {
        use axum::http::Response;
        use std::collections::HashMap;

        let mut metadata = HashMap::new();
        metadata.insert("author".to_string(), "test-user".to_string());
        metadata.insert("category".to_string(), "documents".to_string());

        let builder = Response::builder();
        let builder = add_metadata_headers(builder, &metadata);
        let response = builder.body(()).expect("Failed to build response");

        assert_eq!(
            response
                .headers()
                .get("x-amz-meta-author")
                .and_then(|v| v.to_str().ok()),
            Some("test-user")
        );
        assert_eq!(
            response
                .headers()
                .get("x-amz-meta-category")
                .and_then(|v| v.to_str().ok()),
            Some("documents")
        );
    }

    #[test]
    fn test_etag_matches() {
        // Test exact match
        assert!(etag_matches("abc123", "abc123"));
        assert!(etag_matches("abc123", "\"abc123\""));
        assert!(etag_matches("\"abc123\"", "abc123"));
        assert!(etag_matches("\"abc123\"", "\"abc123\""));

        // Test wildcard
        assert!(etag_matches("anything", "*"));

        // Test weak validators
        assert!(etag_matches("abc123", "W/\"abc123\""));

        // Test comma-separated list
        assert!(etag_matches("abc123", "def456,abc123,xyz789"));
        assert!(etag_matches("abc123", "\"def456\",\"abc123\",\"xyz789\""));

        // Test non-match
        assert!(!etag_matches("abc123", "def456"));
    }

    #[test]
    fn test_generate_request_ids() {
        let (request_id, id2) = generate_request_ids();

        // Request ID should be a valid UUID
        assert!(uuid::Uuid::parse_str(&request_id).is_ok());

        // ID2 should be base64 encoded and non-empty
        assert!(!id2.is_empty());
        assert!(base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &id2).is_ok());
    }

    #[test]
    fn test_parse_tagging_xml() {
        let xml = r#"<Tagging><TagSet><Tag><Key>env</Key><Value>prod</Value></Tag><Tag><Key>team</Key><Value>backend</Value></Tag></TagSet></Tagging>"#;
        let tags = parse_tagging_xml(xml);

        assert_eq!(tags.len(), 2);
        assert_eq!(tags.get("env"), Some(&"prod".to_string()));
        assert_eq!(tags.get("team"), Some(&"backend".to_string()));
    }

    #[test]
    fn test_parse_delete_objects_xml() {
        let xml = r#"<Delete><Object><Key>file1.txt</Key></Object><Object><Key>file2.txt</Key></Object><Object><Key>dir/file3.txt</Key></Object></Delete>"#;
        let keys = parse_delete_objects_xml(xml);

        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], "file1.txt");
        assert_eq!(keys[1], "file2.txt");
        assert_eq!(keys[2], "dir/file3.txt");
    }
}
