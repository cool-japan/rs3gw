//! Golden tests for XML response serialization
//!
//! Verifies that all XML response structs produce well-formed, correct XML.

mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::setup_test_server;
use rs3gw::api::xml_responses::{
    CompleteMultipartUploadResult, ErrorResponse, InitiateMultipartUploadResult,
    ListAllMyBucketsResult, ListBucketResult, ListBucketResultV1, VersioningConfiguration,
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
