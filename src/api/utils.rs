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

/// Extract the text content of the first occurrence of an XML tag.
///
/// Returns `None` if the tag is not present or malformed.
fn extract_xml_tag(s: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = s.find(&open)? + open.len();
    let end = s.find(&close)?;
    if end < start {
        return None;
    }
    Some(s[start..end].trim().to_string())
}

/// Extract the text content of every occurrence of an XML tag.
fn extract_all_xml_tags(s: &str, tag: &str) -> Vec<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let mut results = Vec::new();
    let mut remaining = s;
    while let Some(start_idx) = remaining.find(&open) {
        remaining = &remaining[start_idx + open.len()..];
        if let Some(end_idx) = remaining.find(&close) {
            results.push(remaining[..end_idx].trim().to_string());
            remaining = &remaining[end_idx + close.len()..];
        } else {
            break;
        }
    }
    results
}

/// Build a MalformedXML 400 error response.
pub fn malformed_xml_response(msg: &str) -> Response {
    error_response(StatusCode::BAD_REQUEST, "MalformedXML", msg, "/")
}

/// Parse `ServerSideEncryptionConfiguration` XML from a PutBucketEncryption request body.
///
/// Returns the parsed [`crate::storage::EncryptionConfig`] on success, or the validation
/// error message on failure (caller should construct a MalformedXML response).
pub fn parse_encryption_xml(xml: &str) -> Result<crate::storage::EncryptionConfig, String> {
    let mut rules = Vec::new();
    for rule_section in xml.split("<Rule>").skip(1) {
        let algo = extract_xml_tag(rule_section, "SSEAlgorithm").unwrap_or_default();
        if algo.is_empty() {
            continue;
        }
        if !matches!(algo.as_str(), "AES256" | "aws:kms" | "aws:kms:dsse") {
            return Err("Invalid SSEAlgorithm value".to_string());
        }
        let kms_key = extract_xml_tag(rule_section, "KMSMasterKeyID");
        let bucket_key_str = extract_xml_tag(rule_section, "BucketKeyEnabled").unwrap_or_default();
        let bucket_key_enabled = bucket_key_str.eq_ignore_ascii_case("true");
        rules.push(crate::storage::SseRule {
            sse_algorithm: algo,
            kms_master_key_id: kms_key,
            bucket_key_enabled,
        });
    }
    if rules.is_empty() {
        return Err("ServerSideEncryptionConfiguration must contain at least one Rule".to_string());
    }
    Ok(crate::storage::EncryptionConfig { rules })
}

/// Parse `CORSConfiguration` XML from a PutBucketCors request body.
///
/// Returns the parsed [`crate::storage::CorsConfig`] on success, or the validation
/// error message on failure (caller should construct a MalformedXML response).
pub fn parse_cors_xml(xml: &str) -> Result<crate::storage::CorsConfig, String> {
    const VALID_METHODS: &[&str] = &["GET", "PUT", "POST", "DELETE", "HEAD"];
    let mut rules = Vec::new();
    for rule_section in xml.split("<CORSRule>").skip(1) {
        let id = extract_xml_tag(rule_section, "ID");
        let origins = extract_all_xml_tags(rule_section, "AllowedOrigin");
        let methods = extract_all_xml_tags(rule_section, "AllowedMethod");
        let headers = extract_all_xml_tags(rule_section, "AllowedHeader");
        let expose = extract_all_xml_tags(rule_section, "ExposeHeader");
        let max_age: Option<u32> =
            extract_xml_tag(rule_section, "MaxAgeSeconds").and_then(|s| s.parse().ok());

        if origins.is_empty() {
            return Err("CORSRule must have at least one AllowedOrigin".to_string());
        }
        for m in &methods {
            if !VALID_METHODS.contains(&m.as_str()) {
                return Err(format!("Invalid AllowedMethod: {}", m));
            }
        }
        rules.push(crate::storage::CorsRule {
            id,
            allowed_origins: origins,
            allowed_methods: methods,
            allowed_headers: headers,
            expose_headers: expose,
            max_age_seconds: max_age,
        });
    }
    if rules.is_empty() {
        return Err("CORSConfiguration must contain at least one CORSRule".to_string());
    }
    Ok(crate::storage::CorsConfig { rules })
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

/// Parse versioning XML from PutBucketVersioning request body.
pub fn parse_versioning_xml(xml: &str) -> Result<Option<String>, String> {
    let status = extract_xml_tag(xml, "Status");
    match status.as_deref() {
        None | Some("") => Ok(None),
        Some("Enabled") => Ok(Some("Enabled".to_string())),
        Some("Suspended") => Ok(Some("Suspended".to_string())),
        Some(other) => Err(format!("Invalid Status value: '{}'", other)),
    }
}

/// Parse LifecycleConfiguration XML from PutBucketLifecycle request body.
pub fn parse_lifecycle_xml(xml: &str) -> Result<crate::storage::LifecycleConfig, String> {
    use crate::storage::{LifecycleConfig, LifecycleRule, LifecycleTransition};

    let rule_sections = extract_all_xml_tags(xml, "Rule");
    if rule_sections.is_empty() {
        return Err("LifecycleConfiguration must contain at least one Rule".to_string());
    }
    let mut rules = Vec::new();
    for section in &rule_sections {
        let id = extract_xml_tag(section, "ID");
        let status = extract_xml_tag(section, "Status")
            .ok_or_else(|| "Rule must contain Status".to_string())?;
        if !matches!(status.as_str(), "Enabled" | "Disabled") {
            return Err(format!("Invalid Rule Status: '{}'", status));
        }
        let filter_prefix = extract_xml_tag(section, "Prefix").filter(|s| !s.is_empty());
        let expiration_days: Option<u64> = extract_xml_tag(section, "Expiration")
            .and_then(|exp| extract_xml_tag(&exp, "Days"))
            .map(|d| {
                d.parse::<u64>()
                    .map_err(|_| format!("Invalid Expiration Days: '{}'", d))
            })
            .transpose()?;
        let abort_incomplete_mpu_days: Option<u64> =
            extract_xml_tag(section, "AbortIncompleteMultipartUpload")
                .and_then(|s| extract_xml_tag(&s, "DaysAfterInitiation"))
                .map(|d| {
                    d.parse::<u64>()
                        .map_err(|_| format!("Invalid DaysAfterInitiation: '{}'", d))
                })
                .transpose()?;
        let noncurrent_expiration_days: Option<u64> =
            extract_xml_tag(section, "NoncurrentVersionExpiration")
                .and_then(|s| extract_xml_tag(&s, "NoncurrentDays"))
                .map(|d| {
                    d.parse::<u64>()
                        .map_err(|_| format!("Invalid NoncurrentDays: '{}'", d))
                })
                .transpose()?;
        let mut transitions = Vec::new();
        for t_section in extract_all_xml_tags(section, "Transition") {
            let days: u64 = extract_xml_tag(&t_section, "Days")
                .ok_or_else(|| "Transition must contain Days".to_string())?
                .parse()
                .map_err(|_| "Invalid Transition Days".to_string())?;
            let storage_class = extract_xml_tag(&t_section, "StorageClass")
                .ok_or_else(|| "Transition must contain StorageClass".to_string())?;
            transitions.push(LifecycleTransition {
                days,
                storage_class,
            });
        }
        let mut noncurrent_transitions = Vec::new();
        for t_section in extract_all_xml_tags(section, "NoncurrentVersionTransition") {
            let days: u64 = extract_xml_tag(&t_section, "NoncurrentDays")
                .ok_or_else(|| {
                    "NoncurrentVersionTransition must contain NoncurrentDays".to_string()
                })?
                .parse()
                .map_err(|_| "Invalid NoncurrentDays".to_string())?;
            let storage_class = extract_xml_tag(&t_section, "StorageClass").ok_or_else(|| {
                "NoncurrentVersionTransition must contain StorageClass".to_string()
            })?;
            noncurrent_transitions.push(LifecycleTransition {
                days,
                storage_class,
            });
        }
        let has_action = expiration_days.is_some()
            || !transitions.is_empty()
            || abort_incomplete_mpu_days.is_some()
            || noncurrent_expiration_days.is_some()
            || !noncurrent_transitions.is_empty();
        if !has_action {
            return Err("Rule must contain at least one action".to_string());
        }
        rules.push(LifecycleRule {
            id,
            status,
            filter_prefix,
            expiration_days,
            transitions,
            abort_incomplete_mpu_days,
            noncurrent_expiration_days,
            noncurrent_transitions,
        });
    }
    Ok(LifecycleConfig { rules })
}

/// Parse WebsiteConfiguration XML from PutBucketWebsite request body.
pub fn parse_website_xml(xml: &str) -> Result<crate::storage::WebsiteConfig, String> {
    use crate::storage::{WebsiteConfig, WebsiteRedirectAll, WebsiteRoutingRule};

    let index_document =
        extract_xml_tag(xml, "IndexDocument").and_then(|s| extract_xml_tag(&s, "Suffix"));
    let error_document =
        extract_xml_tag(xml, "ErrorDocument").and_then(|s| extract_xml_tag(&s, "Key"));
    let redirect_all_requests_to = extract_xml_tag(xml, "RedirectAllRequestsTo")
        .map(|s| {
            let host_name = extract_xml_tag(&s, "HostName")
                .ok_or_else(|| "RedirectAllRequestsTo must contain HostName".to_string())?;
            let protocol = extract_xml_tag(&s, "Protocol");
            Ok::<_, String>(WebsiteRedirectAll {
                host_name,
                protocol,
            })
        })
        .transpose()?;
    let routing_rules: Vec<WebsiteRoutingRule> = extract_xml_tag(xml, "RoutingRules")
        .map(|rr_block| {
            extract_all_xml_tags(&rr_block, "RoutingRule")
                .into_iter()
                .map(|rule_block| {
                    let condition_block = extract_xml_tag(&rule_block, "Condition");
                    let condition_key_prefix_equals = condition_block
                        .as_ref()
                        .and_then(|c| extract_xml_tag(c, "KeyPrefixEquals"));
                    let condition_http_error_code = condition_block
                        .as_ref()
                        .and_then(|c| extract_xml_tag(c, "HttpErrorCodeReturnedEquals"));
                    let redirect_block =
                        extract_xml_tag(&rule_block, "Redirect").unwrap_or_default();
                    WebsiteRoutingRule {
                        condition_key_prefix_equals,
                        condition_http_error_code,
                        redirect_host: extract_xml_tag(&redirect_block, "HostName"),
                        redirect_protocol: extract_xml_tag(&redirect_block, "Protocol"),
                        redirect_replace_key_prefix_with: extract_xml_tag(
                            &redirect_block,
                            "ReplaceKeyPrefixWith",
                        ),
                        redirect_replace_key_with: extract_xml_tag(
                            &redirect_block,
                            "ReplaceKeyWith",
                        ),
                        redirect_http_redirect_code: extract_xml_tag(
                            &redirect_block,
                            "HttpRedirectCode",
                        ),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    if index_document.is_none() && redirect_all_requests_to.is_none() && routing_rules.is_empty() {
        return Err("WebsiteConfiguration must contain IndexDocument, RedirectAllRequestsTo, or RoutingRules".to_string());
    }
    Ok(WebsiteConfig {
        index_document,
        error_document,
        redirect_all_requests_to,
        routing_rules,
    })
}

/// Parse PublicAccessBlockConfiguration XML from PutPublicAccessBlock request body.
pub fn parse_public_access_block_xml(
    xml: &str,
) -> Result<crate::storage::PublicAccessBlockConfig, String> {
    use crate::storage::PublicAccessBlockConfig;

    let parse_bool = |tag: &str| -> bool {
        extract_xml_tag(xml, tag)
            .map(|s| s.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    };
    Ok(PublicAccessBlockConfig {
        block_public_acls: parse_bool("BlockPublicAcls"),
        ignore_public_acls: parse_bool("IgnorePublicAcls"),
        block_public_policy: parse_bool("BlockPublicPolicy"),
        restrict_public_buckets: parse_bool("RestrictPublicBuckets"),
    })
}

/// Parse OwnershipControls XML from PutBucketOwnershipControls request body.
pub fn parse_ownership_controls_xml(
    xml: &str,
) -> Result<crate::storage::OwnershipControlsConfig, String> {
    use crate::storage::{OwnershipControlsConfig, OwnershipRule};

    const VALID_VALUES: &[&str] = &[
        "BucketOwnerPreferred",
        "ObjectWriter",
        "BucketOwnerEnforced",
    ];
    let rule_sections = extract_all_xml_tags(xml, "Rule");
    if rule_sections.is_empty() {
        return Err("OwnershipControls must contain at least one Rule".to_string());
    }
    let mut rules = Vec::new();
    for section in &rule_sections {
        let ownership = extract_xml_tag(section, "ObjectOwnership")
            .ok_or_else(|| "Rule must contain ObjectOwnership".to_string())?;
        if !VALID_VALUES.contains(&ownership.as_str()) {
            return Err(format!("Invalid ObjectOwnership value: '{}'", ownership));
        }
        rules.push(OwnershipRule {
            object_ownership: ownership,
        });
    }
    Ok(OwnershipControlsConfig { rules })
}

/// Parse BucketLoggingStatus XML from PutBucketLogging request body.
pub fn parse_logging_xml(xml: &str) -> Result<crate::storage::LoggingConfig, String> {
    use crate::storage::LoggingConfig;

    let logging_block = extract_xml_tag(xml, "LoggingEnabled");
    match logging_block {
        None => Ok(LoggingConfig {
            target_bucket: None,
            target_prefix: None,
        }),
        Some(block) => {
            let target_bucket = extract_xml_tag(&block, "TargetBucket")
                .ok_or_else(|| "LoggingEnabled must contain TargetBucket".to_string())?;
            let target_prefix = extract_xml_tag(&block, "TargetPrefix");
            Ok(LoggingConfig {
                target_bucket: Some(target_bucket),
                target_prefix,
            })
        }
    }
}

/// Parse RequestPaymentConfiguration XML from PutBucketRequestPayment request body.
pub fn parse_request_payment_xml(
    xml: &str,
) -> Result<crate::storage::RequestPaymentConfig, String> {
    use crate::storage::RequestPaymentConfig;

    let payer = extract_xml_tag(xml, "Payer")
        .ok_or_else(|| "RequestPaymentConfiguration must contain Payer".to_string())?;
    if !matches!(payer.as_str(), "Requester" | "BucketOwner") {
        return Err(format!("Invalid Payer value: '{}'", payer));
    }
    Ok(RequestPaymentConfig { payer })
}

/// Parse NotificationConfiguration XML from PutBucketNotification request body.
///
/// An empty body or self-closing root element returns an empty `NotificationConfig`.
pub fn parse_notification_xml(xml: &str) -> Result<crate::storage::NotificationConfig, String> {
    use crate::storage::{
        FilterRule, LambdaFunctionConfiguration, NotificationConfig, QueueConfiguration,
        TopicConfiguration,
    };

    let trimmed = xml.trim();
    if trimmed.is_empty()
        || trimmed == "<NotificationConfiguration/>"
        || trimmed == "<NotificationConfiguration></NotificationConfiguration>"
    {
        return Ok(NotificationConfig::default());
    }

    let parse_filter_rules = |section: &str| -> Vec<FilterRule> {
        let s3_key = match extract_xml_tag(section, "S3Key") {
            Some(s) => s,
            None => return Vec::new(),
        };
        extract_all_xml_tags(&s3_key, "FilterRule")
            .into_iter()
            .filter_map(|rule| {
                let name = extract_xml_tag(&rule, "Name")?;
                let value = extract_xml_tag(&rule, "Value").unwrap_or_default();
                Some(FilterRule { name, value })
            })
            .collect()
    };

    let topic_configurations = extract_all_xml_tags(xml, "TopicConfiguration")
        .into_iter()
        .map(|section| {
            let id = extract_xml_tag(&section, "Id").unwrap_or_default();
            let topic_arn = extract_xml_tag(&section, "Topic").unwrap_or_default();
            let events = extract_all_xml_tags(&section, "Event");
            let filter_block = extract_xml_tag(&section, "Filter").unwrap_or_default();
            let filter_rules = parse_filter_rules(&filter_block);
            TopicConfiguration {
                id,
                topic_arn,
                events,
                filter_rules,
            }
        })
        .collect();

    let queue_configurations = extract_all_xml_tags(xml, "QueueConfiguration")
        .into_iter()
        .map(|section| {
            let id = extract_xml_tag(&section, "Id").unwrap_or_default();
            let queue_arn = extract_xml_tag(&section, "Queue").unwrap_or_default();
            let events = extract_all_xml_tags(&section, "Event");
            let filter_block = extract_xml_tag(&section, "Filter").unwrap_or_default();
            let filter_rules = parse_filter_rules(&filter_block);
            QueueConfiguration {
                id,
                queue_arn,
                events,
                filter_rules,
            }
        })
        .collect();

    let lambda_function_configurations = extract_all_xml_tags(xml, "CloudFunctionConfiguration")
        .into_iter()
        .chain(extract_all_xml_tags(xml, "LambdaFunctionConfiguration"))
        .map(|section| {
            let id = extract_xml_tag(&section, "Id").unwrap_or_default();
            let lambda_function_arn = extract_xml_tag(&section, "CloudFunction")
                .or_else(|| extract_xml_tag(&section, "LambdaFunctionArn"))
                .unwrap_or_default();
            let events = extract_all_xml_tags(&section, "Event");
            let filter_block = extract_xml_tag(&section, "Filter").unwrap_or_default();
            let filter_rules = parse_filter_rules(&filter_block);
            LambdaFunctionConfiguration {
                id,
                lambda_function_arn,
                events,
                filter_rules,
            }
        })
        .collect();

    Ok(NotificationConfig {
        topic_configurations,
        queue_configurations,
        lambda_function_configurations,
    })
}

/// Parse ReplicationConfiguration XML from PutBucketReplication request body.
pub fn parse_replication_xml(xml: &str) -> Result<crate::storage::ReplicationConfig, String> {
    use crate::storage::{ReplicationConfig, ReplicationDestination, ReplicationRule};

    let role = extract_xml_tag(xml, "Role")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing required Role element".to_string())?;

    let rule_sections = extract_all_xml_tags(xml, "Rule");
    if rule_sections.is_empty() {
        return Err("ReplicationConfiguration must contain at least one Rule".to_string());
    }

    let mut rules = Vec::new();
    for section in &rule_sections {
        let dest_block = extract_xml_tag(section, "Destination")
            .ok_or_else(|| "Rule must contain Destination".to_string())?;
        let dest_bucket = extract_xml_tag(&dest_block, "Bucket")
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Destination must contain Bucket".to_string())?;
        let dest_storage_class = extract_xml_tag(&dest_block, "StorageClass");

        let id = extract_xml_tag(section, "ID")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let status = extract_xml_tag(section, "Status")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Enabled".to_string());
        let priority = extract_xml_tag(section, "Priority").and_then(|s| s.parse::<u32>().ok());
        let filter_prefix = extract_xml_tag(section, "Filter")
            .and_then(|f| extract_xml_tag(&f, "Prefix"))
            .filter(|s| !s.is_empty());
        let delete_marker_replication = extract_xml_tag(section, "DeleteMarkerReplication")
            .and_then(|s| extract_xml_tag(&s, "Status"))
            .filter(|s| !s.is_empty());

        rules.push(ReplicationRule {
            id,
            status,
            priority,
            filter_prefix,
            destination: ReplicationDestination {
                bucket: dest_bucket,
                storage_class: dest_storage_class,
            },
            delete_marker_replication,
        });
    }

    Ok(ReplicationConfig { role, rules })
}

/// Parse AccelerateConfiguration XML from PutBucketAccelerateConfiguration request body.
pub fn parse_accelerate_xml(xml: &str) -> Result<crate::storage::AccelerateConfig, String> {
    use crate::storage::AccelerateConfig;

    let status = extract_xml_tag(xml, "Status")
        .ok_or_else(|| "AccelerateConfiguration must contain Status".to_string())?;
    if !matches!(status.as_str(), "Enabled" | "Suspended") {
        return Err(format!(
            "Invalid Status: '{}' (expected Enabled or Suspended)",
            status
        ));
    }
    Ok(AccelerateConfig { status })
}

/// Parse IntelligentTieringConfiguration XML from PutBucketIntelligentTiering request body.
pub fn parse_intelligent_tiering_xml(
    xml: &str,
) -> Result<crate::storage::IntelligentTieringConfig, String> {
    use crate::storage::{IntelligentTieringConfig, Tiering};

    let id = extract_xml_tag(xml, "Id")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "IntelligentTieringConfiguration must contain Id".to_string())?;
    if id.len() > 64 {
        return Err("Id must be 1-64 characters".to_string());
    }

    let status = extract_xml_tag(xml, "Status")
        .ok_or_else(|| "IntelligentTieringConfiguration must contain Status".to_string())?;
    if !matches!(status.as_str(), "Enabled" | "Disabled") {
        return Err(format!(
            "Invalid Status: '{}' (expected Enabled or Disabled)",
            status
        ));
    }

    let filter_prefix = extract_xml_tag(xml, "Filter")
        .and_then(|f| extract_xml_tag(&f, "Prefix"))
        .filter(|s| !s.is_empty());

    let tiering_sections = extract_all_xml_tags(xml, "Tiering");
    let mut tierings = Vec::new();
    for section in &tiering_sections {
        let days: u32 = extract_xml_tag(section, "Days")
            .ok_or_else(|| "Tiering must contain Days".to_string())?
            .parse()
            .map_err(|_| "Invalid Tiering Days value".to_string())?;
        if days < 90 {
            return Err(format!("Tiering Days must be >= 90, got {}", days));
        }
        let access_tier = extract_xml_tag(section, "AccessTier")
            .ok_or_else(|| "Tiering must contain AccessTier".to_string())?;
        if !matches!(
            access_tier.as_str(),
            "ARCHIVE_ACCESS" | "DEEP_ARCHIVE_ACCESS"
        ) {
            return Err(format!(
                "Invalid AccessTier: '{}' (expected ARCHIVE_ACCESS or DEEP_ARCHIVE_ACCESS)",
                access_tier
            ));
        }
        tierings.push(Tiering { days, access_tier });
    }

    Ok(IntelligentTieringConfig {
        id,
        status,
        tierings,
        filter_prefix,
    })
}

// === Object Lock XML parsers ===

/// Parse a PutObjectLegalHold request body.
///
/// Returns the status string (`"ON"` or `"OFF"`), or an error message.
pub fn parse_legal_hold_xml(xml: &str) -> Result<String, String> {
    let status = extract_xml_tag(xml, "Status")
        .ok_or_else(|| "Missing Status element in LegalHold XML".to_string())?;
    match status.as_str() {
        "ON" | "OFF" => Ok(status),
        _ => Err(format!(
            "Invalid LegalHold Status '{}'; must be ON or OFF",
            status
        )),
    }
}

/// Parse a PutObjectRetention request body.
///
/// Returns `(mode, retain_until_date)`, or an error message.
pub fn parse_retention_xml(xml: &str) -> Result<(String, chrono::DateTime<chrono::Utc>), String> {
    let mode = extract_xml_tag(xml, "Mode")
        .ok_or_else(|| "Missing Mode element in Retention XML".to_string())?;
    match mode.as_str() {
        "GOVERNANCE" | "COMPLIANCE" => {}
        _ => {
            return Err(format!(
                "Invalid Retention Mode '{}'; must be GOVERNANCE or COMPLIANCE",
                mode
            ))
        }
    }
    let date_str = extract_xml_tag(xml, "RetainUntilDate")
        .ok_or_else(|| "Missing RetainUntilDate element in Retention XML".to_string())?;
    let until = chrono::DateTime::parse_from_rfc3339(&date_str)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| format!("Invalid RetainUntilDate '{}': {}", date_str, e))?;
    Ok((mode, until))
}

/// Parse a PutObjectLockConfiguration request body.
///
/// Returns a [`crate::storage::ObjectLockConfig`], or an error message.
pub fn parse_object_lock_configuration_xml(
    xml: &str,
) -> Result<crate::storage::ObjectLockConfig, String> {
    use crate::storage::{DefaultRetention, ObjectLockConfig, ObjectLockRule};

    let enabled =
        extract_xml_tag(xml, "ObjectLockEnabled").unwrap_or_else(|| "Enabled".to_string());
    if enabled != "Enabled" {
        return Err(format!(
            "Invalid ObjectLockEnabled '{}'; must be 'Enabled'",
            enabled
        ));
    }

    let rule = if xml.contains("<Rule>") {
        let mode = extract_xml_tag(xml, "Mode")
            .ok_or_else(|| "Missing Mode in DefaultRetention".to_string())?;
        match mode.as_str() {
            "GOVERNANCE" | "COMPLIANCE" => {}
            _ => return Err(format!("Invalid Mode '{}' in DefaultRetention", mode)),
        }
        let days = extract_xml_tag(xml, "Days").and_then(|d| d.parse::<u32>().ok());
        let years = extract_xml_tag(xml, "Years").and_then(|y| y.parse::<u32>().ok());
        if days.is_none() && years.is_none() {
            return Err("DefaultRetention must specify Days or Years".to_string());
        }
        Some(ObjectLockRule {
            default_retention: Some(DefaultRetention { mode, days, years }),
        })
    } else {
        None
    };

    Ok(ObjectLockConfig {
        object_lock_enabled: "Enabled".to_string(),
        rule,
    })
}

// === ID-keyed configuration parsers ===

/// Parse a `MetricsConfiguration` XML body.
pub fn parse_metrics_xml(xml: &str) -> Result<crate::storage::MetricsConfig, String> {
    use crate::storage::{BucketMetricsFilter, MetricsConfig};
    let id = extract_xml_tag(xml, "Id")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing or empty Id element".to_string())?;
    if id.len() > 64 {
        return Err("Id must be 64 characters or fewer".to_string());
    }
    let filter = extract_xml_tag(xml, "Filter").map(|_| BucketMetricsFilter {
        prefix: extract_xml_tag(xml, "Prefix"),
    });
    Ok(MetricsConfig { id, filter })
}

/// Parse an `AnalyticsConfiguration` XML body.
pub fn parse_analytics_xml(xml: &str) -> Result<crate::storage::AnalyticsConfig, String> {
    use crate::storage::{
        AnalyticsConfig, AnalyticsDataExport, AnalyticsS3BucketDestination,
        BucketAnalyticsExportDestination, BucketAnalyticsFilter, BucketStorageClassAnalysis,
    };
    let id = extract_xml_tag(xml, "Id")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing or empty Id element".to_string())?;
    if id.len() > 64 {
        return Err("Id must be 64 characters or fewer".to_string());
    }
    // Scope filter prefix to the filter substring to avoid conflation with
    // destination prefix when both are present.
    let filter = extract_xml_tag(xml, "Filter").map(|filter_xml| BucketAnalyticsFilter {
        prefix: extract_xml_tag(&filter_xml, "Prefix"),
    });
    let storage_class_analysis = if xml.contains("<StorageClassAnalysis>") {
        let version =
            extract_xml_tag(xml, "OutputSchemaVersion").unwrap_or_else(|| "V_1".to_string());
        let s3_dest = if xml.contains("<S3BucketDestination>") {
            let bucket = extract_xml_tag(xml, "Bucket").unwrap_or_default();
            let format = extract_xml_tag(xml, "Format").unwrap_or_else(|| "CSV".to_string());
            // Use destination prefix (outside any Filter context).
            let dest_xml = extract_xml_tag(xml, "Destination");
            let prefix = dest_xml
                .as_deref()
                .and_then(|d| extract_xml_tag(d, "Prefix"));
            Some(AnalyticsS3BucketDestination {
                bucket,
                format,
                prefix,
            })
        } else {
            None
        };
        Some(BucketStorageClassAnalysis {
            data_export: Some(AnalyticsDataExport {
                output_schema_version: version,
                destination: Some(BucketAnalyticsExportDestination {
                    s3_bucket_destination: s3_dest,
                }),
            }),
        })
    } else {
        None
    };
    Ok(AnalyticsConfig {
        id,
        storage_class_analysis,
        filter,
    })
}

/// Parse an `InventoryConfiguration` XML body.
pub fn parse_inventory_xml(xml: &str) -> Result<crate::storage::InventoryConfig, String> {
    use crate::storage::{InventoryConfig, InventoryDestination, InventoryS3BucketDestination};
    let id = extract_xml_tag(xml, "Id")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Missing or empty Id element".to_string())?;
    if id.len() > 64 {
        return Err("Id must be 64 characters or fewer".to_string());
    }
    let is_enabled = extract_xml_tag(xml, "IsEnabled")
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(true);
    let included_object_versions =
        extract_xml_tag(xml, "IncludedObjectVersions").unwrap_or_else(|| "All".to_string());
    match included_object_versions.as_str() {
        "All" | "Current" => {}
        other => {
            return Err(format!(
                "Invalid IncludedObjectVersions '{}'; must be All or Current",
                other
            ))
        }
    }
    let schedule_xml = extract_xml_tag(xml, "Schedule");
    let schedule_frequency = schedule_xml
        .as_deref()
        .and_then(|s| extract_xml_tag(s, "Frequency"))
        .unwrap_or_else(|| "Daily".to_string());
    match schedule_frequency.as_str() {
        "Daily" | "Weekly" => {}
        other => {
            return Err(format!(
                "Invalid Schedule Frequency '{}'; must be Daily or Weekly",
                other
            ))
        }
    }
    let dest_xml = extract_xml_tag(xml, "Destination")
        .ok_or_else(|| "Missing Destination element".to_string())?;
    let s3_xml =
        extract_xml_tag(&dest_xml, "S3BucketDestination").unwrap_or_else(|| dest_xml.clone());
    let bucket = extract_xml_tag(&s3_xml, "Bucket")
        .or_else(|| extract_xml_tag(xml, "Bucket"))
        .ok_or_else(|| "Missing Bucket in Destination".to_string())?;
    let format = extract_xml_tag(&s3_xml, "Format")
        .or_else(|| extract_xml_tag(xml, "Format"))
        .unwrap_or_else(|| "CSV".to_string());
    let dest_prefix = extract_xml_tag(&s3_xml, "Prefix");
    let optional_fields = extract_all_xml_tags(xml, "Field");
    Ok(InventoryConfig {
        id,
        destination: InventoryDestination {
            s3_bucket_destination: InventoryS3BucketDestination {
                bucket,
                format,
                prefix: dest_prefix,
            },
        },
        is_enabled,
        included_object_versions,
        schedule_frequency,
        optional_fields,
    })
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

/// Parsed RestoreObject request body.
#[derive(Debug)]
pub struct RestoreRequest {
    /// Number of days to keep the restored object accessible.
    pub days: u32,
    /// Retrieval tier: "Standard", "Bulk", or "Expedited".
    pub tier: String,
}

/// Parse an `x-amz-acl` canned ACL header value.
///
/// Returns the normalised canned-ACL string or an error message.
pub fn parse_canned_acl_header(value: &str) -> Result<&'static str, String> {
    match value.trim() {
        "private" => Ok("private"),
        "public-read" => Ok("public-read"),
        "public-read-write" => Ok("public-read-write"),
        "authenticated-read" => Ok("authenticated-read"),
        "aws-exec-read" => Ok("aws-exec-read"),
        "bucket-owner-read" => Ok("bucket-owner-read"),
        "bucket-owner-full-control" => Ok("bucket-owner-full-control"),
        "log-delivery-write" => Ok("log-delivery-write"),
        other => Err(format!("Invalid canned ACL: {}", other)),
    }
}

/// Parse an `<AccessControlPolicy>` XML body into an `AclConfig`.
///
/// AWS XML format:
/// ```xml
/// <AccessControlPolicy xmlns="...">
///   <Owner><ID>...</ID><DisplayName>...</DisplayName></Owner>
///   <AccessControlList>
///     <Grant>
///       <Grantee xmlns:xsi="..." xsi:type="CanonicalUser">
///         <ID>...</ID><DisplayName>...</DisplayName>
///       </Grantee>
///       <Permission>FULL_CONTROL</Permission>
///     </Grant>
///   </AccessControlList>
/// </AccessControlPolicy>
/// ```
pub fn parse_acl_xml(xml: &str) -> Result<crate::storage::AclConfig, String> {
    use crate::storage::{AclConfig, AclGrant};

    // Scope owner extraction to the <Owner> block to avoid grabbing grantee fields.
    let owner_block =
        extract_xml_tag(xml, "Owner").ok_or_else(|| "Missing <Owner> element".to_string())?;
    let owner_id =
        extract_xml_tag(&owner_block, "ID").ok_or_else(|| "Missing <Owner><ID>".to_string())?;
    let owner_display_name = extract_xml_tag(&owner_block, "DisplayName").unwrap_or_default();

    // Parse the AccessControlList section.
    let acl_section = extract_xml_tag(xml, "AccessControlList").unwrap_or_default();

    // Extract all <Grant> blocks.
    let grant_blocks = extract_all_xml_tags(&acl_section, "Grant");

    let mut grants = Vec::new();
    for grant_block in grant_blocks {
        let grantee_block = extract_xml_tag(&grant_block, "Grantee").unwrap_or_default();
        let permission = extract_xml_tag(&grant_block, "Permission")
            .ok_or_else(|| "Grant missing <Permission>".to_string())?;

        // Determine grantee type from the xsi:type attribute value present in the block.
        let grantee_type = if grantee_block.contains("CanonicalUser") {
            "CanonicalUser"
        } else if grantee_block.contains("AmazonCustomerByEmail") {
            "AmazonCustomerByEmail"
        } else if grantee_block.contains("Group") {
            "Group"
        } else {
            "CanonicalUser" // fallback
        };

        let grant = match grantee_type {
            "CanonicalUser" => AclGrant {
                grantee_type: "CanonicalUser".to_string(),
                grantee_id: extract_xml_tag(&grantee_block, "ID"),
                grantee_display_name: extract_xml_tag(&grantee_block, "DisplayName"),
                grantee_uri: None,
                grantee_email: None,
                permission,
            },
            "Group" => AclGrant {
                grantee_type: "Group".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: extract_xml_tag(&grantee_block, "URI"),
                grantee_email: None,
                permission,
            },
            "AmazonCustomerByEmail" => AclGrant {
                grantee_type: "AmazonCustomerByEmail".to_string(),
                grantee_id: None,
                grantee_display_name: None,
                grantee_uri: None,
                grantee_email: extract_xml_tag(&grantee_block, "EmailAddress"),
                permission,
            },
            _ => unreachable!(),
        };
        grants.push(grant);
    }

    Ok(AclConfig {
        owner_id,
        owner_display_name,
        grants,
    })
}

/// Parse an S3 `<RestoreRequest>` XML body.
///
/// Expected shape:
/// ```xml
/// <RestoreRequest>
///   <Days>1</Days>
///   <GlacierJobParameters>
///     <Tier>Standard</Tier>
///   </GlacierJobParameters>
/// </RestoreRequest>
/// ```
///
/// Returns `Err(String)` with a descriptive message on invalid input.
pub fn parse_restore_request_xml(xml: &str) -> Result<RestoreRequest, String> {
    // Extract Days
    let days_str =
        extract_xml_tag(xml, "Days").ok_or_else(|| "Missing <Days> element".to_string())?;
    let days: u32 = days_str
        .parse()
        .map_err(|_| format!("Invalid <Days> value: {}", days_str))?;
    if days < 1 {
        return Err("Invalid <Days> value: must be >= 1".to_string());
    }

    // Extract Tier — may be nested inside <GlacierJobParameters> or at top level
    let tier = if let Some(glacier_section) = extract_xml_tag(xml, "GlacierJobParameters") {
        extract_xml_tag(&glacier_section, "Tier")
            .ok_or_else(|| "Missing <Tier> inside <GlacierJobParameters>".to_string())?
    } else {
        extract_xml_tag(xml, "Tier").unwrap_or_else(|| "Standard".to_string())
    };

    const VALID_TIERS: &[&str] = &["Standard", "Bulk", "Expedited"];
    if !VALID_TIERS.contains(&tier.as_str()) {
        return Err(format!(
            "Invalid tier '{}': must be one of Standard, Bulk, Expedited",
            tier
        ));
    }

    Ok(RestoreRequest { days, tier })
}
