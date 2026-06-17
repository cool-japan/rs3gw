#![cfg(feature = "server")]
//! Golden tests for XML response serialization
//!
//! Verifies that all XML response structs produce well-formed, correct XML.

mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::setup_test_server;
use rs3gw::api::xml_responses::{
    BucketLoggingStatusXml, CompleteMultipartUploadResult, CorsConfigurationXml, CorsRuleXml,
    ErrorDocumentXml, ErrorResponse, IndexDocumentXml, InitiateMultipartUploadResult,
    LifecycleConfigurationXml, LifecycleExpirationXml, LifecycleRuleXml, ListAllMyBucketsResult,
    ListBucketResult, ListBucketResultV1, LoggingEnabledXml, OwnershipControlsXml,
    OwnershipRuleXml, PublicAccessBlockConfigurationXml, RequestPaymentConfigurationXml,
    SseConfigurationXml, SseDefaultXml, SseRuleXml, VersioningConfiguration,
    WebsiteConfigurationXml,
};

fn assert_valid_xml(xml: &str) {
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().check_end_names = true;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => panic!(
                "Invalid XML at position {}: {e}\nXML: {xml}",
                reader.error_position()
            ),
            _ => {}
        }
        buf.clear();
    }
}

fn extract_element_text(xml: &str, element: &str) -> Option<String> {
    let open_tag = format!("<{}>", element);
    let close_tag = format!("</{}>", element);
    let start = xml.find(&open_tag)? + open_tag.len();
    let end = xml.find(&close_tag)?;
    if start <= end {
        Some(xml[start..end].to_string())
    } else {
        None
    }
}

/// Test 1: ListAllMyBucketsResult serialization
#[test]
fn test_list_all_my_buckets_result_golden() {
    use chrono::TimeZone;
    let created_at = chrono::Utc
        .with_ymd_and_hms(2024, 1, 15, 10, 30, 0)
        .unwrap();
    let result = ListAllMyBucketsResult::new(vec![
        ("my-bucket".to_string(), created_at),
        ("another-bucket".to_string(), created_at),
    ]);
    let xml = result.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("ListAllMyBucketsResult"),
        "Must contain root element"
    );
    assert!(xml.contains("my-bucket"), "Must contain first bucket name");
    assert!(
        xml.contains("another-bucket"),
        "Must contain second bucket name"
    );
    assert!(
        xml.contains("s3.amazonaws.com"),
        "Must contain S3 namespace"
    );
    assert!(xml.contains("<Name>"), "Must contain Name elements");
    assert!(
        xml.contains("<CreationDate>"),
        "Must contain CreationDate elements"
    );
}

/// Test 2: ListBucketResult V2 serialization
#[test]
fn test_list_bucket_result_v2_golden() {
    use rs3gw::api::xml_responses::ObjectContents;
    let mut result = ListBucketResult::new("test-bucket");
    result.contents = vec![ObjectContents {
        key: "folder/object.txt".to_string(),
        last_modified: "2024-01-15T10:30:00.000Z".to_string(),
        etag: "\"abc123def456\"".to_string(),
        size: 1024,
        storage_class: "STANDARD".to_string(),
    }];
    result.key_count = 1;
    result.max_keys = 1000;
    result.is_truncated = false;

    let xml = result.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("ListBucketResult"),
        "Must contain root element"
    );
    assert!(
        xml.contains("<Name>test-bucket</Name>"),
        "Must contain bucket name"
    );
    assert!(xml.contains("folder/object.txt"), "Must contain object key");
    assert!(
        xml.contains("<Size>1024</Size>"),
        "Must contain object size"
    );
    assert!(
        xml.contains("<KeyCount>1</KeyCount>"),
        "Must contain key count"
    );
    assert!(
        xml.contains("<IsTruncated>false</IsTruncated>"),
        "Must contain truncation flag"
    );
}

/// Test 3: ListBucketResultV1 serialization
#[test]
fn test_list_bucket_result_v1_golden() {
    use rs3gw::api::xml_responses::ObjectContents;
    let mut result = ListBucketResultV1::new("legacy-bucket");
    result.contents = vec![ObjectContents {
        key: "file.dat".to_string(),
        last_modified: "2024-01-15T10:30:00.000Z".to_string(),
        etag: "\"deadbeef\"".to_string(),
        size: 512,
        storage_class: "STANDARD".to_string(),
    }];
    result.max_keys = 100;
    result.marker = "".to_string();

    let xml = result.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("ListBucketResult"),
        "Must contain root element"
    );
    assert!(
        xml.contains("<Name>legacy-bucket</Name>"),
        "Must contain bucket name"
    );
    assert!(xml.contains("file.dat"), "Must contain object key");
    // Marker element may be serialized as <Marker/> or <Marker></Marker> when empty
    assert!(
        xml.contains("<Marker>") || xml.contains("<Marker/>"),
        "V1 must contain Marker element, got: {xml}"
    );
    assert!(
        !xml.contains("KeyCount"),
        "V1 must NOT contain KeyCount (V2 only)"
    );
    assert!(
        xml.contains("<MaxKeys>100</MaxKeys>"),
        "Must contain max keys"
    );
}

/// Test 4: ErrorResponse serialization
#[test]
fn test_error_response_golden() {
    let err = ErrorResponse {
        code: "NoSuchKey".to_string(),
        message: "The specified key does not exist.".to_string(),
        resource: "/my-bucket/missing-object.txt".to_string(),
        request_id: "test-request-id-12345".to_string(),
    };
    let xml = err.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(xml.contains("<Error>"), "Must contain Error root element");
    assert_eq!(
        extract_element_text(&xml, "Code").as_deref(),
        Some("NoSuchKey"),
        "Code element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "Message").as_deref(),
        Some("The specified key does not exist."),
        "Message element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "Resource").as_deref(),
        Some("/my-bucket/missing-object.txt"),
        "Resource element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "RequestId").as_deref(),
        Some("test-request-id-12345"),
        "RequestId element must match"
    );
}

/// Test 5: VersioningConfiguration with Enabled status
#[test]
fn test_versioning_configuration_enabled_golden() {
    let config = VersioningConfiguration::new(Some("Enabled"));
    let xml = config.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("VersioningConfiguration"),
        "Must contain root element"
    );
    assert_eq!(
        extract_element_text(&xml, "Status").as_deref(),
        Some("Enabled"),
        "Status must be Enabled"
    );
    assert!(
        xml.contains("s3.amazonaws.com"),
        "Must contain S3 namespace"
    );
}

/// Test 6: VersioningConfiguration with no versioning (None status)
#[test]
fn test_versioning_configuration_disabled_golden() {
    let config = VersioningConfiguration::new(None);
    let xml = config.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("VersioningConfiguration"),
        "Must contain root element"
    );
    assert!(
        !xml.contains("<Status>"),
        "Disabled versioning must not contain Status element (skip_serializing_if = None)"
    );
}

/// Test 7: InitiateMultipartUploadResult serialization
#[test]
fn test_initiate_multipart_upload_result_golden() {
    let result = InitiateMultipartUploadResult::new(
        "upload-bucket",
        "path/to/large-file.bin",
        "upload-id-xyz-9876",
    );
    let xml = result.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("InitiateMultipartUploadResult"),
        "Must contain root element"
    );
    assert_eq!(
        extract_element_text(&xml, "Bucket").as_deref(),
        Some("upload-bucket"),
        "Bucket element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "Key").as_deref(),
        Some("path/to/large-file.bin"),
        "Key element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "UploadId").as_deref(),
        Some("upload-id-xyz-9876"),
        "UploadId element must match"
    );
}

/// Test 8: CompleteMultipartUploadResult serialization
#[test]
fn test_complete_multipart_upload_result_golden() {
    let result = CompleteMultipartUploadResult::new(
        "https://s3.example.com/final-bucket/completed-object.bin",
        "final-bucket",
        "completed-object.bin",
        "\"etag-of-completed-object\"",
    );
    let xml = result.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration"
    );
    assert!(
        xml.contains("CompleteMultipartUploadResult"),
        "Must contain root element"
    );
    assert_eq!(
        extract_element_text(&xml, "Bucket").as_deref(),
        Some("final-bucket"),
        "Bucket element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "Key").as_deref(),
        Some("completed-object.bin"),
        "Key element must match"
    );
    assert_eq!(
        extract_element_text(&xml, "ETag").as_deref(),
        Some("\"etag-of-completed-object\""),
        "ETag element must match"
    );
    assert!(xml.contains("Location"), "Must contain Location element");
}

// ---------------------------------------------------------------------------
// Test 9: DeleteObjects partial failure (mix of existing and non-existing keys)
// ---------------------------------------------------------------------------

/// Verifies that a DeleteObjects request with a mix of existing and
/// non-existing keys returns a well-formed XML response. Note: AWS S3
/// returns `<Deleted>` for non-existing keys too (idempotent delete),
/// but our implementation currently returns `<Error>` with `NoSuchKey`
/// for keys that do not exist on disk.
#[tokio::test]
async fn test_delete_objects_partial_failure() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "delete-partial-golden";

    // Create bucket
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT 3 objects
    for key in &["key1", "key2", "key3"] {
        client
            .put_object()
            .bucket(bucket)
            .key(*key)
            .body(ByteStream::from_static(b"hello"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    // Send DeleteObjects XML with key1, key2, and key_nonexistent
    let delete_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Delete>
  <Object><Key>key1</Key></Object>
  <Object><Key>key2</Key></Object>
  <Object><Key>key_nonexistent</Key></Object>
</Delete>"#;

    let http = reqwest::Client::new();
    let resp = http
        .post(format!("http://{}/{}?delete", server.addr, bucket))
        .header("Content-Type", "application/xml")
        .body(delete_xml)
        .send()
        .await
        .expect("send delete_objects request");

    assert_eq!(resp.status(), 200, "DeleteObjects should return 200");

    let body = resp.text().await.expect("read response body");
    assert_valid_xml(&body);

    // Must contain DeleteResult root element
    assert!(
        body.contains("DeleteResult"),
        "Response must contain DeleteResult root element, got: {body}"
    );

    // key1 and key2 existed — must appear as <Deleted>
    // Count occurrences of <Deleted> blocks containing each key
    let deleted_key1 = body.contains("<Key>key1</Key>") && body.contains("<Deleted>");
    let deleted_key2 = body.contains("<Key>key2</Key>") && body.contains("<Deleted>");
    assert!(deleted_key1, "key1 should appear in response, got: {body}");
    assert!(deleted_key2, "key2 should appear in response, got: {body}");

    // key_nonexistent — our implementation returns it as an <Error> with NoSuchKey
    // (AWS S3 would return <Deleted> for idempotent behavior)
    assert!(
        body.contains("<Key>key_nonexistent</Key>"),
        "key_nonexistent should appear in response, got: {body}"
    );

    // Verify key3 was NOT mentioned (it was not in the delete request)
    assert!(
        !body.contains("<Key>key3</Key>"),
        "key3 should not appear in DeleteObjects response, got: {body}"
    );

    // Verify key3 still exists
    let head = client.head_object().bucket(bucket).key("key3").send().await;
    assert!(head.is_ok(), "key3 should still exist after partial delete");
}

// ---------------------------------------------------------------------------
// Test 10: SseConfigurationXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_sse_configuration_xml() {
    let cfg = SseConfigurationXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        rules: vec![SseRuleXml {
            apply_default: SseDefaultXml {
                sse_algorithm: "AES256".to_string(),
                kms_master_key_id: None,
            },
            bucket_key_enabled: None,
        }],
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.contains("ServerSideEncryptionConfiguration"),
        "xml: {}",
        xml
    );
    assert!(xml.contains("AES256"), "xml: {}", xml);
    assert!(xml.contains("SSEAlgorithm"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 11: CorsConfigurationXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_cors_configuration_xml() {
    let cfg = CorsConfigurationXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        rules: vec![CorsRuleXml {
            id: None,
            allowed_origins: vec!["https://example.com".to_string()],
            allowed_methods: vec!["GET".to_string(), "PUT".to_string()],
            allowed_headers: vec![],
            expose_headers: vec![],
            max_age_seconds: Some(3600),
        }],
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("CORSConfiguration"), "xml: {}", xml);
    assert!(xml.contains("AllowedOrigin"), "xml: {}", xml);
    assert!(xml.contains("https://example.com"), "xml: {}", xml);
    assert!(xml.contains("3600"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 12: LifecycleConfigurationXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_lifecycle_configuration_xml() {
    let cfg = LifecycleConfigurationXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        rules: vec![LifecycleRuleXml {
            id: Some("rule1".to_string()),
            status: "Enabled".to_string(),
            filter: None,
            expiration: Some(LifecycleExpirationXml { days: 30 }),
            transitions: vec![],
            abort_incomplete_mpu: None,
            noncurrent_expiration: None,
            noncurrent_transitions: vec![],
        }],
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("LifecycleConfiguration"), "xml: {}", xml);
    assert!(xml.contains("Rule"), "xml: {}", xml);
    assert!(xml.contains("Enabled"), "xml: {}", xml);
    assert!(xml.contains("30"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 13: WebsiteConfigurationXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_website_configuration_xml() {
    let cfg = WebsiteConfigurationXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        index_document: Some(IndexDocumentXml {
            suffix: "index.html".to_string(),
        }),
        error_document: Some(ErrorDocumentXml {
            key: "error.html".to_string(),
        }),
        redirect_all: None,
        routing_rules: None,
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("WebsiteConfiguration"), "xml: {}", xml);
    assert!(xml.contains("IndexDocument"), "xml: {}", xml);
    assert!(xml.contains("index.html"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 14: PublicAccessBlockConfigurationXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_public_access_block_configuration_xml() {
    let cfg = PublicAccessBlockConfigurationXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        block_public_acls: true,
        ignore_public_acls: true,
        block_public_policy: true,
        restrict_public_buckets: true,
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.contains("PublicAccessBlockConfiguration"),
        "xml: {}",
        xml
    );
    assert!(xml.contains("BlockPublicAcls"), "xml: {}", xml);
    assert!(xml.contains("true"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 15: OwnershipControlsXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_ownership_controls_xml() {
    let cfg = OwnershipControlsXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        rules: vec![OwnershipRuleXml {
            object_ownership: "BucketOwnerEnforced".to_string(),
        }],
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("OwnershipControls"), "xml: {}", xml);
    assert!(xml.contains("ObjectOwnership"), "xml: {}", xml);
    assert!(xml.contains("BucketOwnerEnforced"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 16: BucketLoggingStatusXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_bucket_logging_status_xml() {
    let cfg = BucketLoggingStatusXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        logging_enabled: Some(LoggingEnabledXml {
            target_bucket: "mylogbucket".to_string(),
            target_prefix: "logs/".to_string(),
        }),
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("BucketLoggingStatus"), "xml: {}", xml);
    assert!(xml.contains("TargetBucket"), "xml: {}", xml);
    assert!(xml.contains("mylogbucket"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Test 17: RequestPaymentConfigurationXml golden serialization
// ---------------------------------------------------------------------------

#[test]
fn test_request_payment_configuration_xml() {
    let cfg = RequestPaymentConfigurationXml {
        xmlns: "http://s3.amazonaws.com/doc/2006-03-01/".to_string(),
        payer: "Requester".to_string(),
    };
    let xml = cfg.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("RequestPaymentConfiguration"), "xml: {}", xml);
    assert!(xml.contains("Payer"), "xml: {}", xml);
    assert!(xml.contains("Requester"), "xml: {}", xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Phase A Tests: Notification, Replication, Accelerate, IntelligentTiering
// ---------------------------------------------------------------------------

/// Test 18: NotificationConfigXml golden serialization
#[test]
fn test_notification_config_xml() {
    use rs3gw::api::xml_responses::NotificationConfigXml;
    use rs3gw::storage::{FilterRule, NotificationConfig, QueueConfiguration, TopicConfiguration};

    let cfg = NotificationConfig {
        topic_configurations: vec![TopicConfiguration {
            id: "topic-1".to_string(),
            topic_arn: "arn:aws:sns:us-east-1:123456789012:my-topic".to_string(),
            events: vec!["s3:ObjectCreated:*".to_string()],
            filter_rules: vec![FilterRule {
                name: "prefix".to_string(),
                value: "logs/".to_string(),
            }],
        }],
        queue_configurations: vec![QueueConfiguration {
            id: "queue-1".to_string(),
            queue_arn: "arn:aws:sqs:us-east-1:123456789012:my-queue".to_string(),
            events: vec!["s3:ObjectRemoved:*".to_string()],
            filter_rules: vec![],
        }],
        lambda_function_configurations: vec![],
    };
    let xml = NotificationConfigXml::from_config(&cfg).to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("NotificationConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("TopicConfiguration"),
        "Must contain TopicConfiguration: {}",
        xml
    );
    assert!(
        xml.contains("QueueConfiguration"),
        "Must contain QueueConfiguration: {}",
        xml
    );
    assert!(xml.contains("topic-1"), "Must contain topic id: {}", xml);
    assert!(
        xml.contains("arn:aws:sns:us-east-1:123456789012:my-topic"),
        "Must contain topic ARN: {}",
        xml
    );
    assert!(
        xml.contains("s3:ObjectCreated:*"),
        "Must contain event: {}",
        xml
    );
    assert!(
        xml.contains("logs/"),
        "Must contain filter prefix value: {}",
        xml
    );
    assert!(xml.contains("queue-1"), "Must contain queue id: {}", xml);
}

/// Test 19: NotificationConfigXml empty config (no topics, queues, or lambdas)
#[test]
fn test_notification_config_xml_empty() {
    use rs3gw::api::xml_responses::NotificationConfigXml;
    use rs3gw::storage::NotificationConfig;

    let cfg = NotificationConfig {
        topic_configurations: vec![],
        queue_configurations: vec![],
        lambda_function_configurations: vec![],
    };
    let xml = NotificationConfigXml::from_config(&cfg).to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("NotificationConfiguration"), "xml: {}", xml);
    // Empty config should not contain sub-elements
    assert!(
        !xml.contains("TopicConfiguration"),
        "Empty should not have TopicConfiguration: {}",
        xml
    );
    assert!(
        !xml.contains("QueueConfiguration"),
        "Empty should not have QueueConfiguration: {}",
        xml
    );
}

/// Test 20: ReplicationConfigurationXml golden serialization
#[test]
fn test_replication_configuration_xml() {
    use rs3gw::api::xml_responses::ReplicationConfigurationXml;
    use rs3gw::storage::{ReplicationConfig, ReplicationDestination, ReplicationRule};

    let cfg = ReplicationConfig {
        role: "arn:aws:iam::123456789012:role/replication-role".to_string(),
        rules: vec![ReplicationRule {
            id: "rule-1".to_string(),
            status: "Enabled".to_string(),
            priority: Some(1),
            filter_prefix: Some("important/".to_string()),
            destination: ReplicationDestination {
                bucket: "arn:aws:s3:::dest-bucket".to_string(),
                storage_class: Some("STANDARD_IA".to_string()),
            },
            delete_marker_replication: Some("Enabled".to_string()),
        }],
    };
    let xml = ReplicationConfigurationXml::from_config(&cfg).to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("ReplicationConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(xml.contains("Role"), "Must contain Role: {}", xml);
    assert!(
        xml.contains("arn:aws:iam::123456789012:role/replication-role"),
        "Must contain role ARN: {}",
        xml
    );
    assert!(xml.contains("Rule"), "Must contain Rule: {}", xml);
    assert!(xml.contains("rule-1"), "Must contain rule id: {}", xml);
    assert!(xml.contains("Enabled"), "Must contain status: {}", xml);
    assert!(
        xml.contains("important/"),
        "Must contain filter prefix: {}",
        xml
    );
    assert!(
        xml.contains("arn:aws:s3:::dest-bucket"),
        "Must contain dest bucket: {}",
        xml
    );
    assert!(
        xml.contains("STANDARD_IA"),
        "Must contain storage class: {}",
        xml
    );
}

/// Test 21: AccelerateConfigurationXml golden serialization
#[test]
fn test_accelerate_configuration_xml() {
    use rs3gw::api::xml_responses::AccelerateConfigurationXml;
    use rs3gw::storage::AccelerateConfig;

    let cfg = AccelerateConfig {
        status: "Enabled".to_string(),
    };
    let xml = AccelerateConfigurationXml::from_config(&cfg).to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("AccelerateConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(xml.contains("Status"), "Must contain Status: {}", xml);
    assert!(
        xml.contains("Enabled"),
        "Must contain Enabled status: {}",
        xml
    );
}

/// Test 22: AccelerateConfigurationXml suspended
#[test]
fn test_accelerate_configuration_xml_suspended() {
    use rs3gw::api::xml_responses::AccelerateConfigurationXml;
    use rs3gw::storage::AccelerateConfig;

    let cfg = AccelerateConfig {
        status: "Suspended".to_string(),
    };
    let xml = AccelerateConfigurationXml::from_config(&cfg).to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("AccelerateConfiguration"), "xml: {}", xml);
    assert!(
        xml.contains("Suspended"),
        "Must contain Suspended status: {}",
        xml
    );
}

/// Test 23: IntelligentTieringConfigurationXml golden serialization
#[test]
fn test_intelligent_tiering_configuration_xml() {
    use rs3gw::api::xml_responses::IntelligentTieringConfigurationXml;
    use rs3gw::storage::{IntelligentTieringConfig, Tiering};

    let cfg = IntelligentTieringConfig {
        id: "tiering-config-1".to_string(),
        status: "Enabled".to_string(),
        tierings: vec![
            Tiering {
                days: 90,
                access_tier: "ARCHIVE_ACCESS".to_string(),
            },
            Tiering {
                days: 180,
                access_tier: "DEEP_ARCHIVE_ACCESS".to_string(),
            },
        ],
        filter_prefix: Some("data/".to_string()),
    };
    let xml = IntelligentTieringConfigurationXml::from_config(&cfg).to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("IntelligentTieringConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(xml.contains("tiering-config-1"), "Must contain id: {}", xml);
    assert!(xml.contains("Enabled"), "Must contain status: {}", xml);
    assert!(xml.contains("data/"), "Must contain filter prefix: {}", xml);
    assert!(xml.contains("Tiering"), "Must contain Tiering: {}", xml);
    assert!(xml.contains("90"), "Must contain days: {}", xml);
    assert!(
        xml.contains("ARCHIVE_ACCESS"),
        "Must contain access tier: {}",
        xml
    );
    assert!(xml.contains("180"), "Must contain second days: {}", xml);
    assert!(
        xml.contains("DEEP_ARCHIVE_ACCESS"),
        "Must contain second access tier: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Phase B Tests: LegalHold, Retention, ObjectLockConfiguration
// ---------------------------------------------------------------------------

/// Test 24: LegalHoldXml ON golden serialization
#[test]
fn test_legal_hold_xml_on() {
    use rs3gw::api::xml_responses::LegalHoldXml;

    let xml_obj = LegalHoldXml::from_status("ON");
    let xml = xml_obj.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("LegalHold"),
        "Must contain LegalHold element: {}",
        xml
    );
    assert!(
        xml.contains("Status"),
        "Must contain Status element: {}",
        xml
    );
    assert!(xml.contains("ON"), "Must contain ON status: {}", xml);
}

/// Test 25: LegalHoldXml OFF golden serialization
#[test]
fn test_legal_hold_xml_off() {
    use rs3gw::api::xml_responses::LegalHoldXml;

    let xml_obj = LegalHoldXml::from_status("OFF");
    let xml = xml_obj.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.contains("LegalHold"),
        "Must contain LegalHold element: {}",
        xml
    );
    assert!(xml.contains("OFF"), "Must contain OFF status: {}", xml);
}

/// Test 26: RetentionXml golden serialization
#[test]
fn test_retention_xml() {
    use chrono::TimeZone;
    use rs3gw::api::xml_responses::RetentionXml;

    let until = chrono::Utc
        .with_ymd_and_hms(2025, 12, 31, 23, 59, 59)
        .unwrap();
    let xml_obj = RetentionXml::new("GOVERNANCE", &until);
    let xml = xml_obj.to_xml();
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("Retention"),
        "Must contain Retention element: {}",
        xml
    );
    assert!(xml.contains("Mode"), "Must contain Mode element: {}", xml);
    assert!(
        xml.contains("GOVERNANCE"),
        "Must contain GOVERNANCE mode: {}",
        xml
    );
    assert!(
        xml.contains("RetainUntilDate"),
        "Must contain RetainUntilDate: {}",
        xml
    );
    assert!(xml.contains("2025-12-31"), "Must contain date: {}", xml);
}

/// Test 27: RetentionXml COMPLIANCE mode
#[test]
fn test_retention_xml_compliance() {
    use chrono::TimeZone;
    use rs3gw::api::xml_responses::RetentionXml;

    let until = chrono::Utc.with_ymd_and_hms(2030, 6, 15, 0, 0, 0).unwrap();
    let xml_obj = RetentionXml::new("COMPLIANCE", &until);
    let xml = xml_obj.to_xml();
    assert_valid_xml(&xml);
    assert!(xml.contains("Retention"), "xml: {}", xml);
    assert!(
        xml.contains("COMPLIANCE"),
        "Must contain COMPLIANCE mode: {}",
        xml
    );
    assert!(xml.contains("2030-06-15"), "Must contain date: {}", xml);
}

/// Test 28: ObjectLockConfigurationXml with rule and days
#[test]
fn test_object_lock_configuration_xml_with_rule() {
    use rs3gw::api::xml_responses::ObjectLockConfigurationXml;
    use rs3gw::storage::{DefaultRetention, ObjectLockConfig, ObjectLockRule};

    let cfg = ObjectLockConfig {
        object_lock_enabled: "Enabled".to_string(),
        rule: Some(ObjectLockRule {
            default_retention: Some(DefaultRetention {
                mode: "GOVERNANCE".to_string(),
                days: Some(30),
                years: None,
            }),
        }),
    };
    let xml = ObjectLockConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("ObjectLockConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("ObjectLockEnabled"),
        "Must contain ObjectLockEnabled: {}",
        xml
    );
    assert!(xml.contains("Enabled"), "Must contain Enabled: {}", xml);
    assert!(xml.contains("Rule"), "Must contain Rule: {}", xml);
    assert!(
        xml.contains("DefaultRetention"),
        "Must contain DefaultRetention: {}",
        xml
    );
    assert!(
        xml.contains("GOVERNANCE"),
        "Must contain GOVERNANCE: {}",
        xml
    );
    assert!(
        xml.contains("<Days>30</Days>"),
        "Must contain Days: {}",
        xml
    );
}

/// Test 29: ObjectLockConfigurationXml enabled without rule
#[test]
fn test_object_lock_configuration_xml_no_rule() {
    use rs3gw::api::xml_responses::ObjectLockConfigurationXml;
    use rs3gw::storage::ObjectLockConfig;

    let cfg = ObjectLockConfig {
        object_lock_enabled: "Enabled".to_string(),
        rule: None,
    };
    let xml = ObjectLockConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(xml.contains("ObjectLockConfiguration"), "xml: {}", xml);
    assert!(xml.contains("Enabled"), "xml: {}", xml);
    assert!(
        !xml.contains("<Rule>"),
        "Should not contain Rule when None: {}",
        xml
    );
}

/// Test 30: ObjectLockConfigurationXml with years instead of days
#[test]
fn test_object_lock_configuration_xml_years() {
    use rs3gw::api::xml_responses::ObjectLockConfigurationXml;
    use rs3gw::storage::{DefaultRetention, ObjectLockConfig, ObjectLockRule};

    let cfg = ObjectLockConfig {
        object_lock_enabled: "Enabled".to_string(),
        rule: Some(ObjectLockRule {
            default_retention: Some(DefaultRetention {
                mode: "COMPLIANCE".to_string(),
                days: None,
                years: Some(7),
            }),
        }),
    };
    let xml = ObjectLockConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(
        xml.contains("COMPLIANCE"),
        "Must contain COMPLIANCE: {}",
        xml
    );
    assert!(
        xml.contains("<Years>7</Years>"),
        "Must contain Years: {}",
        xml
    );
    assert!(
        !xml.contains("<Days>"),
        "Should not contain Days when using years: {}",
        xml
    );
}

// ---------------------------------------------------------------------------
// Phase C Tests: Metrics, Analytics, Inventory (unit struct static builders)
// ---------------------------------------------------------------------------

/// Test 31: MetricsConfigurationXml with filter prefix
#[test]
fn test_metrics_configuration_xml_with_filter() {
    use rs3gw::api::xml_responses::MetricsConfigurationXml;
    use rs3gw::storage::{BucketMetricsFilter, MetricsConfig};

    let cfg = MetricsConfig {
        id: "test-metrics".to_string(),
        filter: Some(BucketMetricsFilter {
            prefix: Some("logs/".to_string()),
        }),
    };
    let xml = MetricsConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("MetricsConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("<Id>test-metrics</Id>"),
        "Must contain Id: {}",
        xml
    );
    assert!(xml.contains("Filter"), "Must contain Filter: {}", xml);
    assert!(
        xml.contains("<Prefix>logs/</Prefix>"),
        "Must contain Prefix: {}",
        xml
    );
}

/// Test 32: MetricsConfigurationXml without filter
#[test]
fn test_metrics_configuration_xml_no_filter() {
    use rs3gw::api::xml_responses::MetricsConfigurationXml;
    use rs3gw::storage::MetricsConfig;

    let cfg = MetricsConfig {
        id: "all-objects-metrics".to_string(),
        filter: None,
    };
    let xml = MetricsConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(xml.contains("MetricsConfiguration"), "xml: {}", xml);
    assert!(
        xml.contains("<Id>all-objects-metrics</Id>"),
        "Must contain Id: {}",
        xml
    );
    assert!(!xml.contains("<Filter>"), "No filter when None: {}", xml);
}

/// Test 33: ListMetricsConfigurationsResultXml with multiple configs
#[test]
fn test_list_metrics_configurations_result_xml() {
    use rs3gw::api::xml_responses::ListMetricsConfigurationsResultXml;
    use rs3gw::storage::MetricsConfig;

    let configs = vec![
        MetricsConfig {
            id: "m1".to_string(),
            filter: None,
        },
        MetricsConfig {
            id: "m2".to_string(),
            filter: None,
        },
    ];
    let xml = ListMetricsConfigurationsResultXml::from_configs(&configs);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("ListMetricsConfigurationsResult"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("IsTruncated"),
        "Must contain IsTruncated: {}",
        xml
    );
    assert!(xml.contains("m1"), "Must contain first id: {}", xml);
    assert!(xml.contains("m2"), "Must contain second id: {}", xml);
}

/// Test 34: ListMetricsConfigurationsResultXml empty list
#[test]
fn test_list_metrics_configurations_result_xml_empty() {
    use rs3gw::api::xml_responses::ListMetricsConfigurationsResultXml;

    let xml = ListMetricsConfigurationsResultXml::from_configs(&[]);
    assert_valid_xml(&xml);
    assert!(
        xml.contains("ListMetricsConfigurationsResult"),
        "xml: {}",
        xml
    );
    assert!(
        xml.contains("IsTruncated"),
        "Must contain IsTruncated: {}",
        xml
    );
}

/// Test 35: AnalyticsConfigurationXml with storage class analysis
#[test]
fn test_analytics_configuration_xml_with_export() {
    use rs3gw::api::xml_responses::AnalyticsConfigurationXml;
    use rs3gw::storage::{
        AnalyticsConfig, AnalyticsDataExport, AnalyticsS3BucketDestination,
        BucketAnalyticsExportDestination, BucketAnalyticsFilter, BucketStorageClassAnalysis,
    };

    let cfg = AnalyticsConfig {
        id: "analytics-config-1".to_string(),
        filter: Some(BucketAnalyticsFilter {
            prefix: Some("images/".to_string()),
        }),
        storage_class_analysis: Some(BucketStorageClassAnalysis {
            data_export: Some(AnalyticsDataExport {
                output_schema_version: "V_1".to_string(),
                destination: Some(BucketAnalyticsExportDestination {
                    s3_bucket_destination: Some(AnalyticsS3BucketDestination {
                        format: "CSV".to_string(),
                        bucket: "arn:aws:s3:::my-analytics-bucket".to_string(),
                        prefix: Some("analytics-output/".to_string()),
                    }),
                }),
            }),
        }),
    };
    let xml = AnalyticsConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("AnalyticsConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("<Id>analytics-config-1</Id>"),
        "Must contain Id: {}",
        xml
    );
    assert!(
        xml.contains("<Prefix>images/</Prefix>"),
        "Must contain filter prefix: {}",
        xml
    );
    assert!(
        xml.contains("StorageClassAnalysis"),
        "Must contain StorageClassAnalysis: {}",
        xml
    );
    assert!(
        xml.contains("DataExport"),
        "Must contain DataExport: {}",
        xml
    );
    assert!(xml.contains("V_1"), "Must contain schema version: {}", xml);
    assert!(
        xml.contains("Destination"),
        "Must contain Destination: {}",
        xml
    );
    assert!(xml.contains("CSV"), "Must contain format: {}", xml);
    assert!(
        xml.contains("arn:aws:s3:::my-analytics-bucket"),
        "Must contain bucket ARN: {}",
        xml
    );
    assert!(
        xml.contains("analytics-output/"),
        "Must contain output prefix: {}",
        xml
    );
}

/// Test 36: AnalyticsConfigurationXml minimal (no filter, no analysis)
#[test]
fn test_analytics_configuration_xml_minimal() {
    use rs3gw::api::xml_responses::AnalyticsConfigurationXml;
    use rs3gw::storage::AnalyticsConfig;

    let cfg = AnalyticsConfig {
        id: "minimal-analytics".to_string(),
        filter: None,
        storage_class_analysis: None,
    };
    let xml = AnalyticsConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(xml.contains("AnalyticsConfiguration"), "xml: {}", xml);
    assert!(
        xml.contains("<Id>minimal-analytics</Id>"),
        "Must contain Id: {}",
        xml
    );
    assert!(!xml.contains("<Filter>"), "No filter when None: {}", xml);
    assert!(
        !xml.contains("StorageClassAnalysis"),
        "No analysis when None: {}",
        xml
    );
}

/// Test 37: ListBucketAnalyticsConfigurationsResultXml with entries
#[test]
fn test_list_analytics_configurations_result_xml() {
    use rs3gw::api::xml_responses::ListBucketAnalyticsConfigurationsResultXml;
    use rs3gw::storage::AnalyticsConfig;

    let configs = vec![
        AnalyticsConfig {
            id: "a1".to_string(),
            filter: None,
            storage_class_analysis: None,
        },
        AnalyticsConfig {
            id: "a2".to_string(),
            filter: None,
            storage_class_analysis: None,
        },
    ];
    let xml = ListBucketAnalyticsConfigurationsResultXml::from_configs(&configs);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("ListBucketAnalyticsConfigurationsResult"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("IsTruncated"),
        "Must contain IsTruncated: {}",
        xml
    );
    assert!(xml.contains("a1"), "Must contain first id: {}", xml);
    assert!(xml.contains("a2"), "Must contain second id: {}", xml);
}

/// Test 38: ListBucketAnalyticsConfigurationsResultXml empty list
#[test]
fn test_list_analytics_configurations_result_xml_empty() {
    use rs3gw::api::xml_responses::ListBucketAnalyticsConfigurationsResultXml;

    let xml = ListBucketAnalyticsConfigurationsResultXml::from_configs(&[]);
    assert_valid_xml(&xml);
    assert!(
        xml.contains("ListBucketAnalyticsConfigurationsResult"),
        "xml: {}",
        xml
    );
    assert!(
        xml.contains("IsTruncated"),
        "Must contain IsTruncated: {}",
        xml
    );
}

/// Test 39: InventoryConfigurationXml full config
#[test]
fn test_inventory_configuration_xml() {
    use rs3gw::api::xml_responses::InventoryConfigurationXml;
    use rs3gw::storage::{InventoryConfig, InventoryDestination, InventoryS3BucketDestination};

    let cfg = InventoryConfig {
        id: "daily-inventory".to_string(),
        destination: InventoryDestination {
            s3_bucket_destination: InventoryS3BucketDestination {
                bucket: "arn:aws:s3:::my-inventory-bucket".to_string(),
                format: "CSV".to_string(),
                prefix: Some("inventory/".to_string()),
            },
        },
        is_enabled: true,
        included_object_versions: "All".to_string(),
        schedule_frequency: "Daily".to_string(),
        optional_fields: vec!["Size".to_string(), "StorageClass".to_string()],
    };
    let xml = InventoryConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("InventoryConfiguration"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("<Id>daily-inventory</Id>"),
        "Must contain Id: {}",
        xml
    );
    assert!(
        xml.contains("<IsEnabled>true</IsEnabled>"),
        "Must contain IsEnabled: {}",
        xml
    );
    assert!(
        xml.contains("<IncludedObjectVersions>All</IncludedObjectVersions>"),
        "Must contain versions: {}",
        xml
    );
    assert!(xml.contains("Schedule"), "Must contain Schedule: {}", xml);
    assert!(
        xml.contains("<Frequency>Daily</Frequency>"),
        "Must contain frequency: {}",
        xml
    );
    assert!(
        xml.contains("Destination"),
        "Must contain Destination: {}",
        xml
    );
    assert!(
        xml.contains("S3BucketDestination"),
        "Must contain S3BucketDestination: {}",
        xml
    );
    assert!(
        xml.contains("arn:aws:s3:::my-inventory-bucket"),
        "Must contain bucket ARN: {}",
        xml
    );
    assert!(xml.contains("CSV"), "Must contain format: {}", xml);
    assert!(
        xml.contains("<Prefix>inventory/</Prefix>"),
        "Must contain prefix: {}",
        xml
    );
    assert!(
        xml.contains("OptionalFields"),
        "Must contain OptionalFields: {}",
        xml
    );
    assert!(
        xml.contains("<Field>Size</Field>"),
        "Must contain Size field: {}",
        xml
    );
    assert!(
        xml.contains("<Field>StorageClass</Field>"),
        "Must contain StorageClass field: {}",
        xml
    );
}

/// Test 40: InventoryConfigurationXml without optional fields or prefix
#[test]
fn test_inventory_configuration_xml_minimal() {
    use rs3gw::api::xml_responses::InventoryConfigurationXml;
    use rs3gw::storage::{InventoryConfig, InventoryDestination, InventoryS3BucketDestination};

    let cfg = InventoryConfig {
        id: "weekly-inventory".to_string(),
        destination: InventoryDestination {
            s3_bucket_destination: InventoryS3BucketDestination {
                bucket: "arn:aws:s3:::backup-bucket".to_string(),
                format: "ORC".to_string(),
                prefix: None,
            },
        },
        is_enabled: false,
        included_object_versions: "Current".to_string(),
        schedule_frequency: "Weekly".to_string(),
        optional_fields: vec![],
    };
    let xml = InventoryConfigurationXml::from_config(&cfg);
    assert_valid_xml(&xml);
    assert!(xml.contains("InventoryConfiguration"), "xml: {}", xml);
    assert!(
        xml.contains("<Id>weekly-inventory</Id>"),
        "Must contain Id: {}",
        xml
    );
    assert!(
        xml.contains("<IsEnabled>false</IsEnabled>"),
        "Must contain false: {}",
        xml
    );
    assert!(xml.contains("Weekly"), "Must contain Weekly: {}", xml);
    assert!(xml.contains("ORC"), "Must contain ORC format: {}", xml);
    assert!(
        !xml.contains("OptionalFields"),
        "No OptionalFields when empty: {}",
        xml
    );
    assert!(!xml.contains("<Prefix>"), "No Prefix when None: {}", xml);
}

/// Test 41: ListInventoryConfigurationsResultXml with entries
#[test]
fn test_list_inventory_configurations_result_xml() {
    use rs3gw::api::xml_responses::ListInventoryConfigurationsResultXml;
    use rs3gw::storage::{InventoryConfig, InventoryDestination, InventoryS3BucketDestination};

    let configs = vec![
        InventoryConfig {
            id: "inv-1".to_string(),
            destination: InventoryDestination {
                s3_bucket_destination: InventoryS3BucketDestination {
                    bucket: "arn:aws:s3:::inv-bucket".to_string(),
                    format: "CSV".to_string(),
                    prefix: None,
                },
            },
            is_enabled: true,
            included_object_versions: "Current".to_string(),
            schedule_frequency: "Daily".to_string(),
            optional_fields: vec![],
        },
        InventoryConfig {
            id: "inv-2".to_string(),
            destination: InventoryDestination {
                s3_bucket_destination: InventoryS3BucketDestination {
                    bucket: "arn:aws:s3:::inv-bucket".to_string(),
                    format: "Parquet".to_string(),
                    prefix: None,
                },
            },
            is_enabled: false,
            included_object_versions: "All".to_string(),
            schedule_frequency: "Weekly".to_string(),
            optional_fields: vec![],
        },
    ];
    let xml = ListInventoryConfigurationsResultXml::from_configs(&configs);
    assert_valid_xml(&xml);
    assert!(
        xml.starts_with(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        "Must have XML declaration: {}",
        xml
    );
    assert!(
        xml.contains("ListInventoryConfigurationsResult"),
        "Must contain root element: {}",
        xml
    );
    assert!(
        xml.contains("IsTruncated"),
        "Must contain IsTruncated: {}",
        xml
    );
    assert!(xml.contains("inv-1"), "Must contain first id: {}", xml);
    assert!(xml.contains("inv-2"), "Must contain second id: {}", xml);
}

/// Test 42: ListInventoryConfigurationsResultXml empty list
#[test]
fn test_list_inventory_configurations_result_xml_empty() {
    use rs3gw::api::xml_responses::ListInventoryConfigurationsResultXml;

    let xml = ListInventoryConfigurationsResultXml::from_configs(&[]);
    assert_valid_xml(&xml);
    assert!(
        xml.contains("ListInventoryConfigurationsResult"),
        "xml: {}",
        xml
    );
    assert!(
        xml.contains("IsTruncated"),
        "Must contain IsTruncated: {}",
        xml
    );
}
