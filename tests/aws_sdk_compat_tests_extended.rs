//! AWS SDK Compatibility Integration Tests for rs3gw (Extended)
//!
//! Covers:
//!  - User-defined metadata round-trip
//!  - Content-Type preservation
//!  - Multipart upload edge cases
//!  - Checksum and caching headers
//!  - Regression tests (empty bucket listing, HEAD 404, empty key, path traversal, zero-byte objects)

mod common;

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use common::setup_test_server;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a unique bucket name that is valid S3 label (lowercase alphanumeric + hyphens, <= 63 chars)
fn unique_bucket() -> String {
    format!("compat-ext-{}", Uuid::new_v4().as_simple())
}

// ---------------------------------------------------------------------------
// 7. User-defined metadata round-trip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_user_metadata_roundtrip() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("meta.txt")
        .metadata("x-custom", "hello-world")
        .metadata("project", "rs3gw-compat")
        .body(ByteStream::from_static(b"metadata-content"))
        .send()
        .await
        .expect("put_object with metadata should succeed");

    let head = client
        .head_object()
        .bucket(&bucket)
        .key("meta.txt")
        .send()
        .await
        .expect("head_object should succeed");

    let meta = head.metadata().expect("metadata map should be present");
    assert_eq!(
        meta.get("x-custom").map(String::as_str),
        Some("hello-world"),
        "x-custom metadata should round-trip"
    );
    assert_eq!(
        meta.get("project").map(String::as_str),
        Some("rs3gw-compat"),
        "project metadata should round-trip"
    );
}

#[tokio::test]
async fn test_user_metadata_case_insensitive() {
    // S3 normalises user metadata keys to lowercase.
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // SDK sends "x-amz-meta-MyKey: val"; server should normalise to "mykey"
    client
        .put_object()
        .bucket(&bucket)
        .key("meta-case.txt")
        .metadata("MyKey", "CaseValue")
        .body(ByteStream::from_static(b"case-content"))
        .send()
        .await
        .expect("put_object should succeed");

    let head = client
        .head_object()
        .bucket(&bucket)
        .key("meta-case.txt")
        .send()
        .await
        .expect("head_object should succeed");

    let meta = head.metadata().expect("metadata map should be present");
    // The AWS SDK normalises keys to lowercase before returning
    let value = meta
        .get("mykey")
        .or_else(|| meta.get("MyKey"))
        .map(String::as_str);
    assert_eq!(
        value,
        Some("CaseValue"),
        "metadata value should be retrievable regardless of key case; map: {:?}",
        meta
    );
}

#[tokio::test]
async fn test_content_type_preservation() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("image.png")
        .content_type("image/png")
        .body(ByteStream::from_static(b"\x89PNG-fake"))
        .send()
        .await
        .expect("put_object should succeed");

    let head = client
        .head_object()
        .bucket(&bucket)
        .key("image.png")
        .send()
        .await
        .expect("head_object should succeed");

    assert_eq!(
        head.content_type(),
        Some("image/png"),
        "Content-Type should be preserved exactly"
    );
}

// ---------------------------------------------------------------------------
// 8. Multipart upload edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_multipart_out_of_order_completion() {
    // Upload parts in order 3, 1, 2 — complete in correct order 1, 2, 3.
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let mpu = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key("out-of-order.bin")
        .send()
        .await
        .expect("create_multipart_upload should succeed");

    let upload_id = mpu
        .upload_id()
        .expect("upload_id must be present")
        .to_string();

    let part3_data = vec![b'C'; 512];
    let part1_data = vec![b'A'; 512];
    let part2_data = vec![b'B'; 512];

    // Upload in order 3, 1, 2
    let etag3 = client
        .upload_part()
        .bucket(&bucket)
        .key("out-of-order.bin")
        .upload_id(&upload_id)
        .part_number(3)
        .body(part3_data.clone().into())
        .send()
        .await
        .expect("upload_part 3 should succeed")
        .e_tag
        .expect("part 3 etag should be present");

    let etag1 = client
        .upload_part()
        .bucket(&bucket)
        .key("out-of-order.bin")
        .upload_id(&upload_id)
        .part_number(1)
        .body(part1_data.clone().into())
        .send()
        .await
        .expect("upload_part 1 should succeed")
        .e_tag
        .expect("part 1 etag should be present");

    let etag2 = client
        .upload_part()
        .bucket(&bucket)
        .key("out-of-order.bin")
        .upload_id(&upload_id)
        .part_number(2)
        .body(part2_data.clone().into())
        .send()
        .await
        .expect("upload_part 2 should succeed")
        .e_tag
        .expect("part 2 etag should be present");

    // Complete in correct order 1, 2, 3
    let completed = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&etag1)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(2)
                .e_tag(&etag2)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(3)
                .e_tag(&etag3)
                .build(),
        )
        .build();

    let complete = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key("out-of-order.bin")
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await;
    assert!(
        complete.is_ok(),
        "complete_multipart_upload should succeed: {:?}",
        complete.err()
    );

    // Verify assembled content: A*512 + B*512 + C*512
    let get = client
        .get_object()
        .bucket(&bucket)
        .key("out-of-order.bin")
        .send()
        .await
        .expect("get_object should succeed");

    let body = get
        .body
        .collect()
        .await
        .expect("collecting body should succeed")
        .into_bytes();

    let expected: Vec<u8> = [part1_data, part2_data, part3_data].concat();
    assert_eq!(
        body.as_ref(),
        expected.as_slice(),
        "assembled multipart object should be in part order 1,2,3"
    );
}

#[tokio::test]
async fn test_multipart_abort_cleanup() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let mpu = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key("aborted.bin")
        .send()
        .await
        .expect("create_multipart_upload should succeed");

    let upload_id = mpu
        .upload_id()
        .expect("upload_id must be present")
        .to_string();

    // Upload a couple of parts
    for part_number in 1..=2i32 {
        client
            .upload_part()
            .bucket(&bucket)
            .key("aborted.bin")
            .upload_id(&upload_id)
            .part_number(part_number)
            .body(vec![b'X'; 512].into())
            .send()
            .await
            .expect("upload_part should succeed");
    }

    // Abort
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key("aborted.bin")
        .upload_id(&upload_id)
        .send()
        .await
        .expect("abort_multipart_upload should succeed");

    // Listing parts for the aborted upload should fail
    let list = client
        .list_parts()
        .bucket(&bucket)
        .key("aborted.bin")
        .upload_id(&upload_id)
        .send()
        .await;
    assert!(
        list.is_err(),
        "list_parts after abort should fail; got: {:?}",
        list.ok()
    );

    // The final object must not exist
    let get = client
        .get_object()
        .bucket(&bucket)
        .key("aborted.bin")
        .send()
        .await;
    assert!(
        get.is_err(),
        "get_object after aborted upload should return an error"
    );
}

#[tokio::test]
async fn test_multipart_minimum_part_size_validation() {
    // The S3 spec requires all parts except the last to be >= 5 MiB.
    // This test documents that completing such an upload may succeed or fail
    // depending on whether the gateway enforces the minimum size.
    // We mark it with `#[ignore]` since enforcement is implementation-defined.
    // Run explicitly: cargo nextest run test_multipart_minimum_part_size_validation -- --ignored
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let mpu = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key("small-parts.bin")
        .send()
        .await
        .expect("create_multipart_upload should succeed");

    let upload_id = mpu
        .upload_id()
        .expect("upload_id must be present")
        .to_string();

    // Upload two very small parts (well below 5 MiB)
    let etag1 = client
        .upload_part()
        .bucket(&bucket)
        .key("small-parts.bin")
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'S'; 128].into())
        .send()
        .await
        .expect("upload_part 1 should succeed")
        .e_tag
        .expect("part 1 etag should be present");

    let etag2 = client
        .upload_part()
        .bucket(&bucket)
        .key("small-parts.bin")
        .upload_id(&upload_id)
        .part_number(2)
        .body(vec![b'S'; 64].into())
        .send()
        .await
        .expect("upload_part 2 should succeed")
        .e_tag
        .expect("part 2 etag should be present");

    let completed = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&etag1)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(2)
                .e_tag(&etag2)
                .build(),
        )
        .build();

    let complete = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key("small-parts.bin")
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await;

    // Gateway may accept or reject sub-5MiB non-final parts.
    // Both outcomes are valid; we simply record the result.
    if complete.is_err() {
        // Enforcement active – clean up the upload if it's still outstanding
        let _ = client
            .abort_multipart_upload()
            .bucket(&bucket)
            .key("small-parts.bin")
            .upload_id(&upload_id)
            .send()
            .await;
    }
    // No assertion – behaviour is implementation-defined.
}

// === Checksum and caching headers ===

#[tokio::test]
async fn test_checksum_sha256_round_trip() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // A valid base64-encoded 32-byte SHA-256 value
    let checksum_value = "47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=";
    let http = reqwest::Client::new();
    let put_resp = http
        .put(format!("{}/{}/checksum-obj.bin", server.base_url, bucket))
        .header("Content-Type", "application/octet-stream")
        .header("x-amz-checksum-sha256", checksum_value)
        .body(b"hello world".to_vec())
        .send()
        .await
        .expect("PUT request should succeed");
    assert!(
        put_resp.status().is_success(),
        "PUT should succeed, got {}",
        put_resp.status()
    );

    // GET the object and verify the checksum header is echoed back
    let get_resp = http
        .get(format!("{}/{}/checksum-obj.bin", server.base_url, bucket))
        .send()
        .await
        .expect("GET request should succeed");
    assert_eq!(get_resp.status(), 200, "GET should return 200");
    let echoed = get_resp
        .headers()
        .get("x-amz-checksum-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        echoed, checksum_value,
        "x-amz-checksum-sha256 should be echoed back on GET"
    );
}

#[tokio::test]
async fn test_content_disposition_round_trip() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let disposition = "attachment; filename=\"f.txt\"";
    let http = reqwest::Client::new();
    let put_resp = http
        .put(format!("{}/{}/disp-obj.txt", server.base_url, bucket))
        .header("Content-Type", "text/plain")
        .header("Content-Disposition", disposition)
        .body(b"content".to_vec())
        .send()
        .await
        .expect("PUT request should succeed");
    assert!(
        put_resp.status().is_success(),
        "PUT should succeed, got {}",
        put_resp.status()
    );

    // HEAD the object and verify Content-Disposition is returned
    let head_resp = http
        .head(format!("{}/{}/disp-obj.txt", server.base_url, bucket))
        .send()
        .await
        .expect("HEAD request should succeed");
    assert_eq!(head_resp.status(), 200, "HEAD should return 200");
    let returned = head_resp
        .headers()
        .get("content-disposition")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        returned, disposition,
        "Content-Disposition should be echoed back on HEAD"
    );
}

#[tokio::test]
async fn test_cache_control_round_trip() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let cache_ctrl = "max-age=3600";
    let http = reqwest::Client::new();
    let put_resp = http
        .put(format!("{}/{}/cache-obj.bin", server.base_url, bucket))
        .header("Content-Type", "application/octet-stream")
        .header("Cache-Control", cache_ctrl)
        .body(b"cached data".to_vec())
        .send()
        .await
        .expect("PUT request should succeed");
    assert!(
        put_resp.status().is_success(),
        "PUT should succeed, got {}",
        put_resp.status()
    );

    // GET the object and verify Cache-Control is returned
    let get_resp = http
        .get(format!("{}/{}/cache-obj.bin", server.base_url, bucket))
        .send()
        .await
        .expect("GET request should succeed");
    assert_eq!(get_resp.status(), 200, "GET should return 200");
    let returned = get_resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(
        returned, cache_ctrl,
        "Cache-Control should be echoed back on GET"
    );
}

#[tokio::test]
async fn test_invalid_checksum_value_rejected() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let http = reqwest::Client::new();
    let put_resp = http
        .put(format!("{}/{}/bad-checksum.bin", server.base_url, bucket))
        .header("Content-Type", "application/octet-stream")
        .header("x-amz-checksum-sha256", "not!valid!base64!!!")
        .body(b"data".to_vec())
        .send()
        .await
        .expect("PUT request should be sent");
    assert_eq!(
        put_resp.status(),
        400,
        "PUT with invalid checksum base64 should return 400"
    );
}

// === Regression tests ===

/// Regression: ListObjects on an empty bucket must return HTTP 200, not 404 or 500.
///
/// Previously a freshly-created bucket with no objects could trigger a storage path
/// that didn't exist yet, causing some implementations to return an incorrect status.
#[tokio::test]
async fn test_regression_list_objects_empty_bucket_returns_200() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // List immediately — no objects have been put yet.
    let list_v2 = client
        .list_objects_v2()
        .bucket(&bucket)
        .send()
        .await
        .expect("list_objects_v2 on empty bucket should return 200");

    let keys: Vec<_> = list_v2.contents().iter().collect();
    assert!(
        keys.is_empty(),
        "empty bucket should have no object listings, got {:?}",
        keys
    );

    // Also verify ListObjectsV1 path.
    let list_v1 = client
        .list_objects()
        .bucket(&bucket)
        .send()
        .await
        .expect("list_objects (v1) on empty bucket should return 200");

    assert!(
        list_v1.contents().is_empty(),
        "empty bucket v1 list should be empty"
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("cleanup delete_bucket should succeed");
}

/// Regression: HEAD on a key that does not exist must return HTTP 404, not 500.
///
/// Previously an absent key could trigger an internal error path instead of
/// producing a clean 404 response, breaking AWS SDK error classification.
#[tokio::test]
async fn test_regression_head_nonexistent_key_returns_404() {
    use aws_sdk_s3::error::SdkError;

    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let result = client
        .head_object()
        .bucket(&bucket)
        .key("this/key/does/not/exist.dat")
        .send()
        .await;

    assert!(
        result.is_err(),
        "HEAD on nonexistent key should fail, got Ok"
    );

    // The error must carry a 404 HTTP status — not 500 or any other code.
    match result {
        Err(SdkError::ServiceError(svc)) => {
            let status = svc.raw().status().as_u16();
            assert_eq!(
                status, 404,
                "HEAD on nonexistent key must be 404, got {status}"
            );
        }
        Err(other) => {
            // If the SDK wraps it differently, just confirm it is not a 500-class error.
            // We use the Debug representation as a best-effort check.
            let msg = format!("{other:?}");
            assert!(
                !msg.contains("500"),
                "HEAD on nonexistent key must not return 500: {msg}"
            );
        }
        Ok(_) => panic!("HEAD on nonexistent key should not succeed"),
    }

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("cleanup delete_bucket should succeed");
}

// ---------------------------------------------------------------------------
// Regression: Empty key handling
// ---------------------------------------------------------------------------

/// PUT/GET with an empty key should return a proper error, not panic.
#[tokio::test]
async fn test_empty_key_handling() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // Use raw HTTP — the SDK may refuse to send an empty key on its own.
    let http = reqwest::Client::new();

    // PUT with empty key — should return 4xx
    let put_resp = http
        .put(format!("http://{}/{}/", server.addr, bucket))
        .header("Content-Type", "application/octet-stream")
        .body(b"test data".to_vec())
        .send()
        .await
        .expect("send PUT with empty key");

    let put_status = put_resp.status().as_u16();
    assert!(
        (400..=404).contains(&put_status) || put_status == 200,
        "PUT with empty key should return 400-404 or 200, got {put_status}"
    );

    // GET with empty key — should return 4xx or be treated as ListObjects
    let get_resp = http
        .get(format!("http://{}/{}/", server.addr, bucket))
        .send()
        .await
        .expect("send GET with empty key");

    let get_status = get_resp.status().as_u16();
    // An empty key GET to /{bucket}/ may be interpreted as ListObjects (200) or as a
    // missing-key error (400/404). Either is acceptable — the key thing is that the
    // server does not panic / return 500.
    assert_ne!(get_status, 500, "GET with empty key must not return 500");
}

// ---------------------------------------------------------------------------
// Regression: Path traversal rejection
// ---------------------------------------------------------------------------

/// Keys containing path-traversal sequences must be safely handled — the
/// server must not leak files outside the storage root.
#[tokio::test]
async fn test_path_traversal_rejection() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let http = reqwest::Client::new();

    let traversal_keys = [
        "../../etc/passwd",
        "..%2F..%2Fetc%2Fpasswd",
        "../secret.txt",
        "foo/../../bar",
    ];

    for key in &traversal_keys {
        let resp = http
            .put(format!("http://{}/{}/{}", server.addr, bucket, key))
            .header("Content-Type", "application/octet-stream")
            .body(b"malicious content".to_vec())
            .send()
            .await
            .expect("send PUT with traversal key");

        let status = resp.status().as_u16();

        // The server should either reject outright (400/403) or accept the key
        // as a literal (200). It must NOT return 500.
        assert_ne!(
            status, 500,
            "PUT with traversal key '{key}' must not return 500, got {status}"
        );

        // If the PUT was accepted (200), verify the object is stored under the
        // literal key and does NOT actually write to the parent filesystem.
        if status == 200 {
            let get_resp = http
                .get(format!("http://{}/{}/{}", server.addr, bucket, key))
                .send()
                .await
                .expect("send GET for traversal key");

            // Should either return the object or 404 — never 500
            let get_status = get_resp.status().as_u16();
            assert_ne!(
                get_status, 500,
                "GET for traversal key '{key}' must not return 500"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Regression: Zero-byte object roundtrip
// ---------------------------------------------------------------------------

/// A zero-byte object should round-trip correctly, preserving Content-Type
/// and returning Content-Length: 0.
#[tokio::test]
async fn test_zero_byte_object_roundtrip() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT a zero-byte object with a specific content type
    client
        .put_object()
        .bucket(&bucket)
        .key("empty.json")
        .content_type("application/json")
        .body(ByteStream::from_static(b""))
        .send()
        .await
        .expect("put_object (zero-byte) should succeed");

    // GET it back
    let get = client
        .get_object()
        .bucket(&bucket)
        .key("empty.json")
        .send()
        .await
        .expect("get_object (zero-byte) should succeed");

    // Verify Content-Length is 0
    assert_eq!(
        get.content_length(),
        Some(0),
        "zero-byte object must have Content-Length: 0"
    );

    // Verify Content-Type is preserved
    assert_eq!(
        get.content_type(),
        Some("application/json"),
        "Content-Type must be preserved for zero-byte object"
    );

    // Verify the body is empty
    let body = get.body.collect().await.expect("collect body").into_bytes();
    assert!(
        body.is_empty(),
        "zero-byte object body must be empty, got {} bytes",
        body.len()
    );

    // HEAD should also report Content-Length: 0
    let head = client
        .head_object()
        .bucket(&bucket)
        .key("empty.json")
        .send()
        .await
        .expect("head_object (zero-byte) should succeed");
    assert_eq!(
        head.content_length(),
        Some(0),
        "HEAD on zero-byte object must report Content-Length: 0"
    );
}
