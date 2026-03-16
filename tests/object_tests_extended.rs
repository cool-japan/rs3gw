//! Extended object operation tests for rs3gw (split from object_tests.rs)

mod common;

use common::setup_test_server;

// --- Edge cases (4 tests) ---

/// PUT 0-byte body → 200; GET → 200 empty body
#[tokio::test]
async fn test_empty_body_roundtrip() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    client
        .create_bucket()
        .bucket("empty-body-test")
        .send()
        .await
        .expect("create bucket");
    client
        .put_object()
        .bucket("empty-body-test")
        .key("empty.bin")
        .body(vec![].into())
        .send()
        .await
        .expect("PUT 0-byte");
    let got = client
        .get_object()
        .bucket("empty-body-test")
        .key("empty.bin")
        .send()
        .await
        .expect("GET empty");
    let returned = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(
        returned.len(),
        0,
        "GET of 0-byte object should return empty body"
    );
}

/// Content-Type roundtrip: PUT custom → HEAD/GET return same
#[tokio::test]
async fn test_content_type_roundtrip() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    client
        .create_bucket()
        .bucket("ct-roundtrip-test")
        .send()
        .await
        .expect("create bucket");
    let ct = "application/x-custom-type";
    client
        .put_object()
        .bucket("ct-roundtrip-test")
        .key("typed.bin")
        .content_type(ct)
        .body(b"ct test".to_vec().into())
        .send()
        .await
        .expect("PUT");
    let head = client
        .head_object()
        .bucket("ct-roundtrip-test")
        .key("typed.bin")
        .send()
        .await
        .expect("HEAD");
    assert_eq!(head.content_type(), Some(ct), "HEAD content-type mismatch");
    let get = client
        .get_object()
        .bucket("ct-roundtrip-test")
        .key("typed.bin")
        .send()
        .await
        .expect("GET");
    assert_eq!(get.content_type(), Some(ct), "GET content-type mismatch");
}

/// User metadata roundtrip: PUT x-amz-meta-custom → HEAD/GET return it
#[tokio::test]
async fn test_user_metadata_roundtrip() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    client
        .create_bucket()
        .bucket("meta-rt-test")
        .send()
        .await
        .expect("create bucket");
    client
        .put_object()
        .bucket("meta-rt-test")
        .key("m.txt")
        .metadata("custom", "val-42")
        .body(b"x".to_vec().into())
        .send()
        .await
        .expect("PUT");
    let head = client
        .head_object()
        .bucket("meta-rt-test")
        .key("m.txt")
        .send()
        .await
        .expect("HEAD");
    assert_eq!(
        head.metadata().expect("meta").get("custom"),
        Some(&"val-42".to_string()),
        "HEAD should return user metadata"
    );
    let get = client
        .get_object()
        .bucket("meta-rt-test")
        .key("m.txt")
        .send()
        .await
        .expect("GET");
    assert_eq!(
        get.metadata().expect("meta").get("custom"),
        Some(&"val-42".to_string()),
        "GET should return user metadata"
    );
}

/// Overwrite: second PUT wins, GET returns second version
#[tokio::test]
async fn test_overwrite_object() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    client
        .create_bucket()
        .bucket("overwrite-test")
        .send()
        .await
        .expect("create bucket");
    client
        .put_object()
        .bucket("overwrite-test")
        .key("ow.txt")
        .body(b"v1".to_vec().into())
        .send()
        .await
        .expect("first PUT");
    client
        .put_object()
        .bucket("overwrite-test")
        .key("ow.txt")
        .body(b"v2".to_vec().into())
        .send()
        .await
        .expect("second PUT");
    let got = client
        .get_object()
        .bucket("overwrite-test")
        .key("ow.txt")
        .send()
        .await
        .expect("GET after overwrite");
    let body = got.body.collect().await.expect("collect").into_bytes();
    assert_eq!(
        body.as_ref(),
        b"v2",
        "GET should return second version after overwrite"
    );
}

/// Test HEAD object with conditional headers
#[tokio::test]
async fn test_head_object_conditional() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("head-conditional-{}", uuid::Uuid::new_v4());

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Create object
    let content = b"test content for conditional head";
    client
        .put_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .body(aws_sdk_s3::primitives::ByteStream::from_static(content))
        .send()
        .await
        .unwrap();

    // Get ETag from HEAD
    let head_result = client
        .head_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .send()
        .await
        .unwrap();

    let etag = head_result.e_tag().unwrap();

    // Test If-Match with correct ETag - should succeed
    let result = client
        .head_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .if_match(etag)
        .send()
        .await;
    assert!(result.is_ok(), "HEAD with matching If-Match should succeed");

    // Test If-Match with wrong ETag - should fail with 412 Precondition Failed
    let result = client
        .head_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .if_match("\"wrongetag\"")
        .send()
        .await;
    assert!(
        result.is_err(),
        "HEAD with non-matching If-Match should fail"
    );

    // Test If-None-Match with different ETag - should succeed
    let result = client
        .head_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .if_none_match("\"differentetag\"")
        .send()
        .await;
    assert!(
        result.is_ok(),
        "HEAD with non-matching If-None-Match should succeed"
    );

    // Test If-None-Match with same ETag - should return 304 Not Modified
    let result = client
        .head_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .if_none_match(etag)
        .send()
        .await;
    // This returns an error (304) which SDK treats as error
    assert!(
        result.is_err(),
        "HEAD with matching If-None-Match should return 304"
    );

    // Use HTTP client for direct header verification
    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    // Verify 412 status code directly
    let response = http_client
        .head(format!("{}/{}/test.txt", base_url, bucket_name))
        .header("If-Match", "\"wrongetag\"")
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        412,
        "If-Match with wrong ETag should return 412"
    );

    // Verify 304 status code directly
    let response = http_client
        .head(format!("{}/{}/test.txt", base_url, bucket_name))
        .header("If-None-Match", etag)
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        304,
        "If-None-Match with same ETag should return 304"
    );
}

// ============================================================
// WS-1 Sprint 4: PUT Streaming Backpressure tests
// ============================================================

/// PUT empty (0-byte) object via streaming, verify 200 + ETag
#[tokio::test]
async fn test_put_object_streaming_zero_bytes() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base = &server.base_url;
    let bucket = "stream-zero-test";

    // Create bucket
    let resp = http
        .put(format!("{}/{}", base, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(resp.status().is_success(), "bucket creation failed");

    // PUT empty body
    let resp = http
        .put(format!("{}/{}/empty.bin", base, bucket))
        .body(Vec::<u8>::new())
        .send()
        .await
        .expect("PUT empty");
    assert_eq!(resp.status().as_u16(), 200, "PUT 0-byte should return 200");
    assert!(
        resp.headers().contains_key("etag"),
        "PUT 0-byte should return ETag"
    );
}

/// PUT 1-byte object via streaming, GET it back, verify content
#[tokio::test]
async fn test_put_object_streaming_small() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base = &server.base_url;
    let bucket = "stream-small-test";

    let resp = http
        .put(format!("{}/{}", base, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(resp.status().is_success());

    // PUT 1-byte body
    let resp = http
        .put(format!("{}/{}/tiny.bin", base, bucket))
        .body(vec![0x42u8])
        .send()
        .await
        .expect("PUT 1-byte");
    assert_eq!(resp.status().as_u16(), 200, "PUT 1-byte should return 200");

    // GET it back
    let resp = http
        .get(format!("{}/{}/tiny.bin", base, bucket))
        .send()
        .await
        .expect("GET 1-byte");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.bytes().await.expect("bytes");
    assert_eq!(body.as_ref(), &[0x42u8], "GET body should match PUT body");
}

/// PUT 10MB object via streaming, GET it back, verify content matches
#[tokio::test]
async fn test_put_object_streaming_medium() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base = &server.base_url;
    let bucket = "stream-medium-test";

    let resp = http
        .put(format!("{}/{}", base, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(resp.status().is_success());

    let size = 10 * 1024 * 1024;
    let data = vec![0xABu8; size];
    let resp = http
        .put(format!("{}/{}/medium.bin", base, bucket))
        .body(data.clone())
        .send()
        .await
        .expect("PUT 10MB");
    assert_eq!(resp.status().as_u16(), 200, "PUT 10MB should return 200");

    // GET it back
    let resp = http
        .get(format!("{}/{}/medium.bin", base, bucket))
        .send()
        .await
        .expect("GET 10MB");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.bytes().await.expect("bytes");
    assert_eq!(body.len(), size, "GET should return exactly 10MB");
    assert!(
        body.iter().all(|&b| b == 0xAB),
        "GET data should match PUT data"
    );
}

/// PUT with x-amz-meta-* headers via streaming, verify metadata preserved
#[tokio::test]
async fn test_put_object_streaming_with_metadata() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base = &server.base_url;
    let bucket = "stream-meta-test";

    let resp = http
        .put(format!("{}/{}", base, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(resp.status().is_success());

    // PUT with custom metadata
    let resp = http
        .put(format!("{}/{}/meta.txt", base, bucket))
        .header("x-amz-meta-custom-key", "custom-value-123")
        .header("x-amz-meta-another", "another-value")
        .body("metadata test body")
        .send()
        .await
        .expect("PUT with metadata");
    assert_eq!(resp.status().as_u16(), 200);

    // HEAD to verify metadata
    let resp = http
        .head(format!("{}/{}/meta.txt", base, bucket))
        .send()
        .await
        .expect("HEAD");
    assert_eq!(resp.status().as_u16(), 200);
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-custom-key")
            .and_then(|v| v.to_str().ok()),
        Some("custom-value-123"),
        "custom metadata should be preserved"
    );
    assert_eq!(
        resp.headers()
            .get("x-amz-meta-another")
            .and_then(|v| v.to_str().ok()),
        Some("another-value"),
        "second metadata key should be preserved"
    );
}

/// PUT with specific content-type via streaming, verify preserved
#[tokio::test]
async fn test_put_object_streaming_content_type() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base = &server.base_url;
    let bucket = "stream-ct-test";

    let resp = http
        .put(format!("{}/{}", base, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(resp.status().is_success());

    // PUT with custom content-type
    let resp = http
        .put(format!("{}/{}/typed.json", base, bucket))
        .header("Content-Type", "application/json")
        .body(r#"{"key": "value"}"#)
        .send()
        .await
        .expect("PUT with content-type");
    assert_eq!(resp.status().as_u16(), 200);

    // HEAD to verify content-type
    let resp = http
        .head(format!("{}/{}/typed.json", base, bucket))
        .send()
        .await
        .expect("HEAD");
    assert_eq!(resp.status().as_u16(), 200);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    assert_eq!(
        ct,
        Some("application/json"),
        "content-type should be preserved"
    );
}
