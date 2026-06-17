#![cfg(feature = "server")]
//! Tests for S3 ACL API operations (GetBucketAcl, PutBucketAcl, GetObjectAcl, PutObjectAcl)

mod common;

use common::setup_test_server;

/// Test 1: Bucket ACL default — newly created bucket returns FULL_CONTROL for rs3gw owner
#[tokio::test]
async fn test_bucket_acl_default() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("acl-default-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let resp = http
        .get(format!("{}/{}?acl", base_url, bucket))
        .send()
        .await
        .expect("GET ?acl should complete");
    assert_eq!(resp.status(), 200, "GetBucketAcl should return 200");

    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("AccessControlPolicy"),
        "Expected AccessControlPolicy in default ACL response, got: {}",
        body
    );
    assert!(
        body.contains("FULL_CONTROL"),
        "Expected FULL_CONTROL in default ACL response, got: {}",
        body
    );
    assert!(
        body.contains("rs3gw"),
        "Expected rs3gw owner ID in default ACL response, got: {}",
        body
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 2: PutBucketAcl with canned public-read header, then GET to verify READ grant for AllUsers
#[tokio::test]
async fn test_bucket_acl_canned_public_read() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("acl-pubread-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT ?acl with canned header, no body
    let put_resp = http
        .put(format!("{}/{}?acl", base_url, bucket))
        .header("x-amz-acl", "public-read")
        .send()
        .await
        .expect("PUT ?acl (public-read) should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutBucketAcl public-read should return 200"
    );

    // GET to verify
    let get_resp = http
        .get(format!("{}/{}?acl", base_url, bucket))
        .send()
        .await
        .expect("GET ?acl should complete");
    assert_eq!(get_resp.status(), 200, "GetBucketAcl should return 200");

    let body = get_resp.text().await.expect("read response body");
    assert!(
        body.contains("FULL_CONTROL"),
        "Expected owner FULL_CONTROL grant, got: {}",
        body
    );
    assert!(
        body.contains("READ"),
        "Expected READ grant for public-read ACL, got: {}",
        body
    );
    assert!(
        body.contains("AllUsers"),
        "Expected AllUsers group URI in public-read ACL, got: {}",
        body
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 3: PutBucketAcl with explicit XML body
#[tokio::test]
async fn test_bucket_acl_explicit_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("acl-xml-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let acl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy>
  <Owner><ID>testowner</ID><DisplayName>Test Owner</DisplayName></Owner>
  <AccessControlList>
    <Grant>
      <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
        <ID>testowner</ID><DisplayName>Test Owner</DisplayName>
      </Grantee>
      <Permission>FULL_CONTROL</Permission>
    </Grant>
  </AccessControlList>
</AccessControlPolicy>"#;

    let put_resp = http
        .put(format!("{}/{}?acl", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(acl_xml)
        .send()
        .await
        .expect("PUT ?acl (XML body) should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutBucketAcl with explicit XML body should return 200"
    );

    let get_resp = http
        .get(format!("{}/{}?acl", base_url, bucket))
        .send()
        .await
        .expect("GET ?acl should complete");
    assert_eq!(get_resp.status(), 200, "GetBucketAcl should return 200");

    let body = get_resp.text().await.expect("read response body");
    assert!(
        body.contains("testowner"),
        "Expected testowner ID in ACL response, got: {}",
        body
    );
    assert!(
        body.contains("FULL_CONTROL"),
        "Expected FULL_CONTROL in ACL response, got: {}",
        body
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 4: PutBucketAcl with BOTH canned header AND XML body — should return 400 UnexpectedContent
#[tokio::test]
async fn test_bucket_acl_conflict() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("acl-conflict-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // Send both header and non-empty body — conflict
    let resp = http
        .put(format!("{}/{}?acl", base_url, bucket))
        .header("x-amz-acl", "public-read")
        .header("Content-Type", "application/xml")
        .body("<x/>")
        .send()
        .await
        .expect("PUT ?acl (conflict) should complete");
    assert_eq!(
        resp.status(),
        400,
        "PutBucketAcl with header+body conflict should return 400, got body: {}",
        resp.text().await.unwrap_or_default()
    );

    let body = http
        .put(format!("{}/{}?acl", base_url, bucket))
        .header("x-amz-acl", "public-read")
        .header("Content-Type", "application/xml")
        .body("<x/>")
        .send()
        .await
        .expect("PUT ?acl (conflict) second call should complete")
        .text()
        .await
        .expect("read response body");
    assert!(
        body.contains("UnexpectedContent"),
        "Expected UnexpectedContent error code, got: {}",
        body
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 5: PutBucketAcl with NO header and NO body — should return 400 MissingSecurityHeader
#[tokio::test]
async fn test_bucket_acl_missing_source() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("acl-missing-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // No x-amz-acl header, empty body
    let resp = http
        .put(format!("{}/{}?acl", base_url, bucket))
        .send()
        .await
        .expect("PUT ?acl (no header, no body) should complete");
    assert_eq!(
        resp.status(),
        400,
        "PutBucketAcl with no header and no body should return 400"
    );

    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("MissingSecurityHeader"),
        "Expected MissingSecurityHeader error code, got: {}",
        body
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 6: PutBucketAcl with invalid canned ACL value — should return 400 InvalidArgument
#[tokio::test]
async fn test_canned_acl_invalid() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("acl-invalid-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let resp = http
        .put(format!("{}/{}?acl", base_url, bucket))
        .header("x-amz-acl", "bogus-canned-acl")
        .send()
        .await
        .expect("PUT ?acl (invalid canned) should complete");
    assert_eq!(
        resp.status(),
        400,
        "PutBucketAcl with invalid canned ACL should return 400"
    );

    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("InvalidArgument"),
        "Expected InvalidArgument error code, got: {}",
        body
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 7: GetObjectAcl default — newly uploaded object returns FULL_CONTROL for rs3gw owner
#[tokio::test]
async fn test_object_acl_default() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("obj-acl-default-{}", uuid::Uuid::new_v4());
    let key = "test-object.txt";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT object
    client
        .put_object()
        .bucket(&bucket)
        .key(key)
        .body(aws_sdk_s3::primitives::ByteStream::from_static(
            b"hello world",
        ))
        .send()
        .await
        .expect("put_object should succeed");

    let resp = http
        .get(format!("{}/{}/{}?acl", base_url, bucket, key))
        .send()
        .await
        .expect("GET object ?acl should complete");
    assert_eq!(resp.status(), 200, "GetObjectAcl should return 200");

    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("FULL_CONTROL"),
        "Expected FULL_CONTROL in default object ACL response, got: {}",
        body
    );
    assert!(
        body.contains("rs3gw"),
        "Expected rs3gw owner ID in default object ACL response, got: {}",
        body
    );

    // Cleanup
    client
        .delete_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("delete_object should succeed");
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 8: PutObjectAcl with canned public-read header, then GET to verify READ grant for AllUsers
#[tokio::test]
async fn test_object_acl_canned_public_read() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("obj-acl-pubread-{}", uuid::Uuid::new_v4());
    let key = "public-object.txt";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT object first
    client
        .put_object()
        .bucket(&bucket)
        .key(key)
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"data"))
        .send()
        .await
        .expect("put_object should succeed");

    // PUT ?acl with canned header, no body
    let put_resp = http
        .put(format!("{}/{}/{}?acl", base_url, bucket, key))
        .header("x-amz-acl", "public-read")
        .send()
        .await
        .expect("PUT object ?acl (public-read) should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutObjectAcl public-read should return 200"
    );

    // GET to verify
    let get_resp = http
        .get(format!("{}/{}/{}?acl", base_url, bucket, key))
        .send()
        .await
        .expect("GET object ?acl should complete");
    assert_eq!(get_resp.status(), 200, "GetObjectAcl should return 200");

    let body = get_resp.text().await.expect("read response body");
    assert!(
        body.contains("READ"),
        "Expected READ grant for public-read object ACL, got: {}",
        body
    );
    assert!(
        body.contains("AllUsers"),
        "Expected AllUsers group URI in public-read object ACL, got: {}",
        body
    );

    // Cleanup
    client
        .delete_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("delete_object should succeed");
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 9: PutObjectAcl with explicit XML body, then GET to verify persisted data
#[tokio::test]
async fn test_object_acl_explicit_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("obj-acl-xml-{}", uuid::Uuid::new_v4());
    let key = "xml-acl-object.txt";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT object first
    client
        .put_object()
        .bucket(&bucket)
        .key(key)
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"content"))
        .send()
        .await
        .expect("put_object should succeed");

    let acl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy>
  <Owner><ID>testowner</ID><DisplayName>Test Owner</DisplayName></Owner>
  <AccessControlList>
    <Grant>
      <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
        <ID>testowner</ID><DisplayName>Test Owner</DisplayName>
      </Grantee>
      <Permission>FULL_CONTROL</Permission>
    </Grant>
  </AccessControlList>
</AccessControlPolicy>"#;

    let put_resp = http
        .put(format!("{}/{}/{}?acl", base_url, bucket, key))
        .header("Content-Type", "application/xml")
        .body(acl_xml)
        .send()
        .await
        .expect("PUT object ?acl (XML body) should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutObjectAcl with explicit XML body should return 200"
    );

    let get_resp = http
        .get(format!("{}/{}/{}?acl", base_url, bucket, key))
        .send()
        .await
        .expect("GET object ?acl should complete");
    assert_eq!(get_resp.status(), 200, "GetObjectAcl should return 200");

    let body = get_resp.text().await.expect("read response body");
    assert!(
        body.contains("testowner"),
        "Expected testowner in object ACL response, got: {}",
        body
    );
    assert!(
        body.contains("FULL_CONTROL"),
        "Expected FULL_CONTROL in object ACL response, got: {}",
        body
    );

    // Cleanup
    client
        .delete_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("delete_object should succeed");
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test 10: GetObjectAcl on nonexistent bucket/key — should return 404 or 400, not 200 or 500
#[tokio::test]
async fn test_object_acl_nonexistent_object() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    // No bucket created — directly request ACL for nonexistent bucket + key
    let resp = http
        .get(format!("{}/nonexistent-bucket-xyz/some-key?acl", base_url))
        .send()
        .await
        .expect("GET ?acl on nonexistent bucket should complete");

    let status = resp.status().as_u16();
    let body = resp.text().await.expect("read response body");

    assert!(
        status == 404 || status == 400,
        "GetObjectAcl on nonexistent bucket should return 404 or 400, got {}: {}",
        status,
        body
    );
    assert_ne!(
        status, 200,
        "GetObjectAcl on nonexistent bucket must not return 200"
    );
    assert_ne!(
        status, 500,
        "GetObjectAcl on nonexistent bucket must not return 500"
    );
}
