//! End-to-end smoke tests for rs3gw
//!
//! Exercises the full object lifecycle: create bucket, PUT, HEAD, GET,
//! list, DELETE object, delete bucket, and verify cleanup.

mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::setup_test_server;

/// Full lifecycle smoke test:
/// 1. Create bucket
/// 2. PUT object with metadata
/// 3. HEAD object - verify metadata
/// 4. GET object - verify content
/// 5. List objects - verify object in list
/// 6. DELETE object
/// 7. Verify object is gone (GET returns error)
/// 8. Delete bucket
/// 9. Verify bucket is gone (HEAD returns error)
#[tokio::test]
async fn test_full_lifecycle_smoke() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    let bucket = "smoke-lifecycle";
    let key = "hello.txt";
    let body_bytes = b"Hello, rs3gw smoke test!";

    // 1. Create bucket
    let create_result = client.create_bucket().bucket(bucket).send().await;
    assert!(
        create_result.is_ok(),
        "CreateBucket failed: {:?}",
        create_result.err()
    );

    // 2. PUT object with user metadata
    let put_result = client
        .put_object()
        .bucket(bucket)
        .key(key)
        .content_type("text/plain")
        .metadata("smoke-key", "smoke-value")
        .body(ByteStream::from_static(body_bytes))
        .send()
        .await;
    assert!(
        put_result.is_ok(),
        "PutObject failed: {:?}",
        put_result.err()
    );
    let put_output = put_result.expect("put should succeed");
    assert!(
        put_output.e_tag().is_some(),
        "PutObject should return an ETag"
    );

    // 3. HEAD object - verify metadata
    let head_result = client.head_object().bucket(bucket).key(key).send().await;
    assert!(
        head_result.is_ok(),
        "HeadObject failed: {:?}",
        head_result.err()
    );
    let head_output = head_result.expect("head should succeed");
    assert_eq!(
        head_output.content_length().unwrap_or(0),
        body_bytes.len() as i64,
        "Content-Length mismatch"
    );
    assert_eq!(
        head_output.content_type(),
        Some("text/plain"),
        "Content-Type mismatch"
    );
    let meta = head_output
        .metadata()
        .unwrap_or(&std::collections::HashMap::new())
        .clone();
    assert_eq!(
        meta.get("smoke-key").map(|s| s.as_str()),
        Some("smoke-value"),
        "User metadata mismatch"
    );

    // 4. GET object - verify content
    let get_result = client.get_object().bucket(bucket).key(key).send().await;
    assert!(
        get_result.is_ok(),
        "GetObject failed: {:?}",
        get_result.err()
    );
    let get_output = get_result.expect("get should succeed");
    let got_bytes = get_output
        .body
        .collect()
        .await
        .expect("body collect failed")
        .into_bytes();
    assert_eq!(got_bytes.as_ref(), body_bytes, "Object content mismatch");

    // 5. List objects - verify object is present
    let list_result = client.list_objects_v2().bucket(bucket).send().await;
    assert!(
        list_result.is_ok(),
        "ListObjectsV2 failed: {:?}",
        list_result.err()
    );
    let list_output = list_result.expect("list should succeed");
    let keys: Vec<&str> = list_output
        .contents()
        .iter()
        .filter_map(|obj| obj.key())
        .collect();
    assert!(
        keys.contains(&key),
        "Object key '{}' not found in listing: {:?}",
        key,
        keys
    );

    // 6. DELETE object
    let delete_result = client.delete_object().bucket(bucket).key(key).send().await;
    assert!(
        delete_result.is_ok(),
        "DeleteObject failed: {:?}",
        delete_result.err()
    );

    // 7. Verify object is gone (GET should fail)
    let get_gone = client.get_object().bucket(bucket).key(key).send().await;
    assert!(get_gone.is_err(), "GetObject should fail after deletion");

    // 8. Delete bucket
    let delete_bucket_result = client.delete_bucket().bucket(bucket).send().await;
    assert!(
        delete_bucket_result.is_ok(),
        "DeleteBucket failed: {:?}",
        delete_bucket_result.err()
    );

    // 9. Verify bucket is gone (HEAD should fail)
    let head_bucket_gone = client.head_bucket().bucket(bucket).send().await;
    assert!(
        head_bucket_gone.is_err(),
        "HeadBucket should fail after deletion"
    );
}

/// Smoke test: multiple objects in one bucket with prefix listing
#[tokio::test]
async fn test_prefix_listing_smoke() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    let bucket = "smoke-prefix";

    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    // Upload objects with different prefixes
    for (key, content) in [
        ("docs/readme.txt", "readme"),
        ("docs/guide.txt", "guide"),
        ("images/logo.png", "logo-bytes"),
        ("root.txt", "root"),
    ] {
        client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(ByteStream::from_static(content.as_bytes()))
            .send()
            .await
            .unwrap_or_else(|e| panic!("put {} failed: {:?}", key, e));
    }

    // List with prefix "docs/"
    let list = client
        .list_objects_v2()
        .bucket(bucket)
        .prefix("docs/")
        .send()
        .await
        .expect("list with prefix");

    let keys: Vec<&str> = list.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(
        keys.len(),
        2,
        "Expected 2 objects under docs/, got {:?}",
        keys
    );
    assert!(keys.contains(&"docs/readme.txt"));
    assert!(keys.contains(&"docs/guide.txt"));

    // Cleanup
    for key in [
        "docs/readme.txt",
        "docs/guide.txt",
        "images/logo.png",
        "root.txt",
    ] {
        let _ = client.delete_object().bucket(bucket).key(key).send().await;
    }
    let _ = client.delete_bucket().bucket(bucket).send().await;
}
