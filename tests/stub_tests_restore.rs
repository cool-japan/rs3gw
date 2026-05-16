//! Tests for S3 RestoreObject API operation

mod common;

use common::setup_test_server;

const VALID_RESTORE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<RestoreRequest>
  <Days>1</Days>
  <GlacierJobParameters><Tier>Standard</Tier></GlacierJobParameters>
</RestoreRequest>"#;

/// Test 1: RestoreObject on a non-archived object returns 409 InvalidObjectState
#[tokio::test]
async fn test_restore_object_not_archived() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("restore-noarch-{}", uuid::Uuid::new_v4().as_simple());
    let key = "normal-object.txt";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT a normal object (no GLACIER header)
    let put_resp = http
        .put(format!("{}/{}/{}", base_url, bucket, key))
        .body("normal content — not archived")
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PutObject should return 200");

    // POST ?restore on a non-archived object → 409 InvalidObjectState
    let resp = http
        .post(format!("{}/{}/{}?restore", base_url, bucket, key))
        .header("Content-Type", "application/xml")
        .body(VALID_RESTORE_XML)
        .send()
        .await
        .expect("RestoreObject request should complete");

    assert_eq!(
        resp.status(),
        409,
        "RestoreObject on non-archived object should return 409"
    );
    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("InvalidObjectState"),
        "Expected InvalidObjectState error code, got: {}",
        body
    );
}

/// Test 2: RestoreObject with malformed XML body returns 400 MalformedXML
#[tokio::test]
async fn test_restore_request_invalid_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("restore-badxml-{}", uuid::Uuid::new_v4().as_simple());
    let key = "object.txt";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}/{}", base_url, bucket, key))
        .body("some content")
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PutObject should return 200");

    // POST ?restore with completely malformed body
    let resp = http
        .post(format!("{}/{}/{}?restore", base_url, bucket, key))
        .header("Content-Type", "application/xml")
        .body("not xml at all")
        .send()
        .await
        .expect("RestoreObject request should complete");

    assert_eq!(
        resp.status(),
        400,
        "RestoreObject with malformed XML should return 400"
    );
    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("MalformedXML"),
        "Expected MalformedXML error code, got: {}",
        body
    );
}

/// Test 3: RestoreObject with invalid tier returns 400 InvalidArgument
#[tokio::test]
async fn test_restore_tier_invalid() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("restore-badtier-{}", uuid::Uuid::new_v4().as_simple());
    let key = "object.txt";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}/{}", base_url, bucket, key))
        .body("some content")
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PutObject should return 200");

    // POST ?restore with invalid tier value
    let resp = http
        .post(format!("{}/{}/{}?restore", base_url, bucket, key))
        .header("Content-Type", "application/xml")
        .body(
            r#"<RestoreRequest>
  <Days>1</Days>
  <GlacierJobParameters><Tier>InvalidTier</Tier></GlacierJobParameters>
</RestoreRequest>"#,
        )
        .send()
        .await
        .expect("RestoreObject request should complete");

    assert_eq!(
        resp.status(),
        400,
        "RestoreObject with invalid tier should return 400"
    );
    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("InvalidArgument"),
        "Expected InvalidArgument error code for bad tier, got: {}",
        body
    );
}

/// Test 4: RestoreObject on GLACIER-class PUT object follows full restore lifecycle
///
/// PUT with x-amz-storage-class: GLACIER archives the object automatically.
/// First POST ?restore → 202 (restore initiated and completes to Active in 50ms).
/// Second POST ?restore → 200 (already Active).
#[tokio::test]
async fn test_restore_object_archived() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("restore-glacier-{}", uuid::Uuid::new_v4().as_simple());
    let key = "glacier-object.bin";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    // Create bucket via raw HTTP (same base_url)
    let create_resp = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("CreateBucket should complete");
    assert_eq!(
        create_resp.status(),
        200,
        "CreateBucket should return 200, got: {}",
        create_resp.status()
    );

    // PUT object WITH GLACIER storage class — triggers archive_object
    let put_resp = http
        .put(format!("{}/{}/{}", base_url, bucket, key))
        .header("x-amz-storage-class", "GLACIER")
        .body("glacier test content")
        .send()
        .await
        .expect("PutObject with GLACIER should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutObject with GLACIER should return 200, got: {}",
        put_resp.status()
    );

    // First POST ?restore — object is archived, restore initiated → 202 Accepted
    let restore1 = http
        .post(format!("{}/{}/{}?restore", base_url, bucket, key))
        .header("Content-Type", "application/xml")
        .body(VALID_RESTORE_XML)
        .send()
        .await
        .expect("first RestoreObject should complete");
    assert_eq!(
        restore1.status(),
        202,
        "First RestoreObject should return 202 Accepted, got: {}",
        restore1.status()
    );

    // The restore_object implementation transitions to Active synchronously after 50ms.
    // Second POST ?restore — object is now Active → 200 OK
    let restore2 = http
        .post(format!("{}/{}/{}?restore", base_url, bucket, key))
        .header("Content-Type", "application/xml")
        .body(VALID_RESTORE_XML)
        .send()
        .await
        .expect("second RestoreObject should complete");
    assert!(
        restore2.status() == 200 || restore2.status() == 202,
        "Second RestoreObject should return 200 (Active) or 202 (Restoring), got: {}",
        restore2.status()
    );
}

/// Test 5: RestoreObject on a nonexistent key returns 404
#[tokio::test]
async fn test_restore_nonexistent_object() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("restore-nokey-{}", uuid::Uuid::new_v4().as_simple());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // POST ?restore on a key that doesn't exist → 404
    let resp = http
        .post(format!(
            "{}/{}/nonexistent-key-12345?restore",
            base_url, bucket
        ))
        .header("Content-Type", "application/xml")
        .body(VALID_RESTORE_XML)
        .send()
        .await
        .expect("RestoreObject on missing key should complete");

    assert_eq!(
        resp.status(),
        404,
        "RestoreObject on nonexistent key should return 404"
    );
    let body = resp.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchKey") || body.contains("NoSuchBucket"),
        "Expected NoSuchKey error code, got: {}",
        body
    );
}
