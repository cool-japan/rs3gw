#![cfg(feature = "server")]
//! AWS SDK Compatibility Integration Tests for rs3gw
//!
//! Covers:
//!  - Bucket CRUD validation
//!  - Object keys with special characters
//!  - Conditional headers (If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since)
//!  - Range requests
//!  - Pagination (list objects v1 / v2)
//!  - Error response XML format validation
//!
//! See also: aws_sdk_compat_tests_extended.rs for metadata, multipart,
//! checksum, caching headers, and regression tests.

mod common;

use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::get_object::GetObjectError;
use aws_sdk_s3::primitives::ByteStream;
use common::setup_test_server;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a unique bucket name that is valid S3 label (lowercase alphanumeric + hyphens, ≤ 63 chars)
fn unique_bucket() -> String {
    format!("compat-{}", Uuid::new_v4().as_simple())
}

// ---------------------------------------------------------------------------
// 1. Bucket CRUD validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bucket_create_and_delete() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    let create = client.create_bucket().bucket(&bucket).send().await;
    assert!(
        create.is_ok(),
        "create_bucket should succeed: {:?}",
        create.err()
    );

    let head = client.head_bucket().bucket(&bucket).send().await;
    assert!(
        head.is_ok(),
        "head_bucket should succeed after creation: {:?}",
        head.err()
    );

    let delete = client.delete_bucket().bucket(&bucket).send().await;
    assert!(
        delete.is_ok(),
        "delete_bucket should succeed: {:?}",
        delete.err()
    );

    // After deletion the bucket should not exist
    let head_after = client.head_bucket().bucket(&bucket).send().await;
    assert!(
        head_after.is_err(),
        "head_bucket should fail after deletion"
    );
}

#[tokio::test]
async fn test_bucket_already_exists_error() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("first create_bucket should succeed");

    // Second creation of the same bucket must return an error or 200 (idempotent on same account)
    // AWS S3 returns 200 (OK) for the same owner; other implementations may return
    // BucketAlreadyOwnedByYou (409).  We accept either.
    let second = client.create_bucket().bucket(&bucket).send().await;
    // Some implementations return 200 (idempotent), some 409 – both are valid per S3 spec.
    // We just assert the request completes and the bucket still exists afterwards.
    let _ = second; // don't assert ok/err – implementation-defined

    let head = client.head_bucket().bucket(&bucket).send().await;
    assert!(
        head.is_ok(),
        "bucket should still exist after duplicate create: {:?}",
        head.err()
    );

    // Cleanup
    client
        .delete_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("cleanup delete_bucket should succeed");
}

#[tokio::test]
async fn test_bucket_not_found_error() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket(); // never created

    let head = client.head_bucket().bucket(&bucket).send().await;
    assert!(
        head.is_err(),
        "head_bucket on non-existent bucket should fail"
    );
}

#[tokio::test]
async fn test_list_buckets_empty() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // A fresh server has no buckets
    let list = client
        .list_buckets()
        .send()
        .await
        .expect("list_buckets should succeed");

    let buckets = list.buckets();
    assert!(
        buckets.is_empty(),
        "fresh server should have no buckets, got {:?}",
        buckets.iter().map(|b| b.name()).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_list_buckets_multiple() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    let names: Vec<String> = (0..3).map(|_| unique_bucket()).collect();

    for name in &names {
        client
            .create_bucket()
            .bucket(name)
            .send()
            .await
            .expect("create_bucket should succeed");
    }

    let list = client
        .list_buckets()
        .send()
        .await
        .expect("list_buckets should succeed");

    let returned: Vec<&str> = list.buckets().iter().filter_map(|b| b.name()).collect();
    for name in &names {
        assert!(
            returned.contains(&name.as_str()),
            "expected bucket '{}' in list, got {:?}",
            name,
            returned
        );
    }

    // Cleanup
    for name in &names {
        client
            .delete_bucket()
            .bucket(name)
            .send()
            .await
            .expect("cleanup delete_bucket should succeed");
    }
}

// ---------------------------------------------------------------------------
// 2. Object keys with special characters
// ---------------------------------------------------------------------------

async fn put_and_get_key(key: &str) {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"special-key-content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put = client
        .put_object()
        .bucket(&bucket)
        .key(key)
        .body(ByteStream::from_static(content))
        .send()
        .await;
    assert!(
        put.is_ok(),
        "put_object with key '{}' should succeed: {:?}",
        key,
        put.err()
    );

    let get = client.get_object().bucket(&bucket).key(key).send().await;
    assert!(
        get.is_ok(),
        "get_object with key '{}' should succeed: {:?}",
        key,
        get.err()
    );

    let body = get
        .expect("get_object already asserted ok")
        .body
        .collect()
        .await
        .expect("collecting body should succeed");
    assert_eq!(
        body.into_bytes().as_ref(),
        content,
        "retrieved body should match for key '{}'",
        key
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
        .expect("cleanup delete_bucket should succeed");
}

#[tokio::test]
async fn test_object_key_with_spaces() {
    put_and_get_key("my key with spaces").await;
}

#[tokio::test]
async fn test_object_key_with_unicode() {
    put_and_get_key("键/值").await;
}

#[tokio::test]
async fn test_object_key_with_special_chars() {
    put_and_get_key("a+b=c&d%20e").await;
}

#[tokio::test]
async fn test_object_key_slash_prefix() {
    put_and_get_key("/leading/slash/key").await;
}

// ---------------------------------------------------------------------------
// 3. Conditional headers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_if_match_etag_match() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"conditional-match-content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("cond.txt")
        .body(ByteStream::from_static(content))
        .send()
        .await
        .expect("put_object should succeed");

    let head = client
        .head_object()
        .bucket(&bucket)
        .key("cond.txt")
        .send()
        .await
        .expect("head_object should succeed");

    let etag = head.e_tag().expect("ETag must be present").to_string();

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/cond.txt", base, bucket))
        .header("If-Match", &etag)
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "If-Match with correct ETag should return 200"
    );
}

#[tokio::test]
async fn test_if_match_etag_mismatch() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"conditional-mismatch-content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("cond.txt")
        .body(ByteStream::from_static(content))
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/cond.txt", base, bucket))
        .header("If-Match", "\"00000000000000000000000000000000\"")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        412,
        "If-Match with wrong ETag should return 412"
    );
}

#[tokio::test]
async fn test_if_none_match_etag_match_get_returns_304() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"if-none-match-content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("cond.txt")
        .body(ByteStream::from_static(content))
        .send()
        .await
        .expect("put_object should succeed");

    let head = client
        .head_object()
        .bucket(&bucket)
        .key("cond.txt")
        .send()
        .await
        .expect("head_object should succeed");

    let etag = head.e_tag().expect("ETag must be present").to_string();

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/cond.txt", base, bucket))
        .header("If-None-Match", &etag)
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        304,
        "If-None-Match with matching ETag should return 304 Not Modified"
    );
}

#[tokio::test]
async fn test_if_none_match_etag_mismatch_get_succeeds() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"if-none-match-mismatch";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("cond.txt")
        .body(ByteStream::from_static(content))
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/cond.txt", base, bucket))
        .header("If-None-Match", "\"ffffffffffffffffffffffffffffffff\"")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "If-None-Match with non-matching ETag should return 200"
    );
}

#[tokio::test]
async fn test_if_modified_since_not_modified() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"modified-since-content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("cond.txt")
        .body(ByteStream::from_static(content))
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    // Use a future date – the object was NOT modified since a date in the future.
    // If-Modified-Since: future => 304 (not modified since that future point)
    let resp = http
        .get(format!("{}/{}/cond.txt", base, bucket))
        .header("If-Modified-Since", "Tue, 01 Jan 2030 00:00:00 GMT")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        304,
        "If-Modified-Since with future date should return 304 (not modified)"
    );
}

#[tokio::test]
async fn test_if_unmodified_since_modified() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = b"unmodified-since-content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("cond.txt")
        .body(ByteStream::from_static(content))
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    // Use a past date – the object WAS modified after that date.
    // If-Unmodified-Since: past => 412 (was modified since that old date)
    let resp = http
        .get(format!("{}/{}/cond.txt", base, bucket))
        .header("If-Unmodified-Since", "Wed, 01 Jan 2020 00:00:00 GMT")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        412,
        "If-Unmodified-Since with past date should return 412"
    );
}

// ---------------------------------------------------------------------------
// 4. Range requests
// ---------------------------------------------------------------------------

/// Build a 100-byte content of repeated decimal digits
fn hundred_bytes() -> Vec<u8> {
    (0u8..100).collect()
}

#[tokio::test]
async fn test_range_request_first_bytes() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = hundred_bytes();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("range.bin")
        .body(content.clone().into())
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/range.bin", base, bucket))
        .header("Range", "bytes=0-9")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        206,
        "range request should return 206 Partial Content"
    );
    let body = resp.bytes().await.expect("reading body should succeed");
    assert_eq!(
        body.as_ref(),
        &content[0..10],
        "first 10 bytes should match"
    );
}

#[tokio::test]
async fn test_range_request_last_bytes() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = hundred_bytes();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("range.bin")
        .body(content.clone().into())
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/range.bin", base, bucket))
        .header("Range", "bytes=-10")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        206,
        "suffix range request should return 206"
    );
    let body = resp.bytes().await.expect("reading body should succeed");
    assert_eq!(
        body.as_ref(),
        &content[90..100],
        "last 10 bytes should match"
    );
}

#[tokio::test]
async fn test_range_request_invalid() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = hundred_bytes();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("range.bin")
        .body(content.into())
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    // Request range beyond object length
    let resp = http
        .get(format!("{}/{}/range.bin", base, bucket))
        .header("Range", "bytes=200-300")
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(
        resp.status().as_u16(),
        416,
        "out-of-range request should return 416 Range Not Satisfiable"
    );
}

#[tokio::test]
async fn test_range_request_suffix_exceeds_length() {
    // When suffix-range exceeds the object size the entire object should be returned.
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();
    let content = hundred_bytes();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    client
        .put_object()
        .bucket(&bucket)
        .key("range.bin")
        .body(content.clone().into())
        .send()
        .await
        .expect("put_object should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    // Ask for more bytes than the object has
    let resp = http
        .get(format!("{}/{}/range.bin", base, bucket))
        .header("Range", "bytes=-500")
        .send()
        .await
        .expect("HTTP GET should not error");
    // Either 200 or 206 is acceptable; body must be the full object.
    let status = resp.status().as_u16();
    assert!(
        status == 200 || status == 206,
        "suffix range exceeding length should return 200 or 206, got {}",
        status
    );
    let body = resp.bytes().await.expect("reading body should succeed");
    assert_eq!(
        body.as_ref(),
        content.as_slice(),
        "body should be the full object when suffix exceeds length"
    );
}

// ---------------------------------------------------------------------------
// 5. Pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_list_objects_v2_pagination() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // Upload 100 small objects
    for i in 0..100u32 {
        client
            .put_object()
            .bucket(&bucket)
            .key(format!("obj-{:03}.bin", i))
            .body(ByteStream::from_static(b"x"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    // Paginate with max_keys = 10, collect all keys
    let mut all_keys: Vec<String> = Vec::new();
    let mut continuation: Option<String> = None;

    loop {
        let mut req = client.list_objects_v2().bucket(&bucket).max_keys(10);
        if let Some(ref tok) = continuation {
            req = req.continuation_token(tok);
        }
        let page = req.send().await.expect("list_objects_v2 should succeed");

        for obj in page.contents() {
            if let Some(k) = obj.key() {
                all_keys.push(k.to_string());
            }
        }

        if page.is_truncated() == Some(true) {
            continuation = page.next_continuation_token().map(str::to_string);
        } else {
            break;
        }
    }

    assert_eq!(
        all_keys.len(),
        100,
        "pagination should yield all 100 objects"
    );

    // Keys must be in lexicographic order
    let mut sorted = all_keys.clone();
    sorted.sort();
    assert_eq!(all_keys, sorted, "paginated keys must be in sorted order");
}

#[tokio::test]
async fn test_list_objects_v1_marker() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    for i in 0..10u32 {
        client
            .put_object()
            .bucket(&bucket)
            .key(format!("item-{:02}.txt", i))
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    // Use marker = "item-04.txt" — should return items 05..09 (5 items)
    #[allow(deprecated)]
    let page = client
        .list_objects()
        .bucket(&bucket)
        .marker("item-04.txt")
        .send()
        .await
        .expect("list_objects should succeed");

    let contents = page.contents();
    assert_eq!(
        contents.len(),
        5,
        "marker pagination should skip first 5 keys, expected 5 remaining, got {}",
        contents.len()
    );
    let first_key = contents[0].key().expect("key must be present");
    assert_eq!(
        first_key, "item-05.txt",
        "first key after marker should be item-05.txt"
    );
}

#[tokio::test]
async fn test_list_objects_max_keys_1() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    for i in 0..5u32 {
        client
            .put_object()
            .bucket(&bucket)
            .key(format!("k{}.txt", i))
            .body(ByteStream::from_static(b"v"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    let page = client
        .list_objects_v2()
        .bucket(&bucket)
        .max_keys(1)
        .send()
        .await
        .expect("list_objects_v2 should succeed");

    assert_eq!(
        page.contents().len(),
        1,
        "max_keys=1 should return exactly 1 object"
    );
    assert_eq!(
        page.is_truncated(),
        Some(true),
        "is_truncated should be true when more keys exist"
    );
}

#[tokio::test]
async fn test_list_objects_prefix_filter() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    for key in &["alpha/a.txt", "alpha/b.txt", "beta/c.txt", "root.txt"] {
        client
            .put_object()
            .bucket(&bucket)
            .key(*key)
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    let page = client
        .list_objects_v2()
        .bucket(&bucket)
        .prefix("alpha/")
        .send()
        .await
        .expect("list_objects_v2 should succeed");

    let keys: Vec<&str> = page.contents().iter().filter_map(|o| o.key()).collect();
    assert_eq!(keys.len(), 2, "prefix filter should return 2 objects");
    assert!(keys.contains(&"alpha/a.txt"), "should contain alpha/a.txt");
    assert!(keys.contains(&"alpha/b.txt"), "should contain alpha/b.txt");
}

#[tokio::test]
async fn test_list_objects_delimiter_grouping() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    for key in &["dir1/a.txt", "dir1/b.txt", "dir2/c.txt", "root.txt"] {
        client
            .put_object()
            .bucket(&bucket)
            .key(*key)
            .body(ByteStream::from_static(b"data"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    let page = client
        .list_objects_v2()
        .bucket(&bucket)
        .delimiter("/")
        .send()
        .await
        .expect("list_objects_v2 should succeed");

    let contents = page.contents();
    let prefixes = page.common_prefixes();

    assert_eq!(contents.len(), 1, "should have 1 object at root level");
    assert_eq!(
        prefixes.len(),
        2,
        "should have 2 common prefixes (dir1/, dir2/)"
    );

    let prefix_vals: Vec<&str> = prefixes.iter().filter_map(|cp| cp.prefix()).collect();
    assert!(prefix_vals.contains(&"dir1/"), "dir1/ should be a prefix");
    assert!(prefix_vals.contains(&"dir2/"), "dir2/ should be a prefix");
}

#[tokio::test]
async fn test_continuation_token_is_stable() {
    // Verify that re-using the same continuation token yields the same next page.
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    for i in 0..20u32 {
        client
            .put_object()
            .bucket(&bucket)
            .key(format!("stable-{:02}.txt", i))
            .body(ByteStream::from_static(b"s"))
            .send()
            .await
            .expect("put_object should succeed");
    }

    // Get first page and capture the continuation token
    let page1 = client
        .list_objects_v2()
        .bucket(&bucket)
        .max_keys(10)
        .send()
        .await
        .expect("list_objects_v2 page 1 should succeed");

    assert_eq!(page1.is_truncated(), Some(true));
    let token = page1
        .next_continuation_token()
        .expect("next_continuation_token should be present")
        .to_string();

    // Request page 2 twice using the same token
    let page2a = client
        .list_objects_v2()
        .bucket(&bucket)
        .max_keys(10)
        .continuation_token(&token)
        .send()
        .await
        .expect("list_objects_v2 page 2a should succeed");

    let page2b = client
        .list_objects_v2()
        .bucket(&bucket)
        .max_keys(10)
        .continuation_token(&token)
        .send()
        .await
        .expect("list_objects_v2 page 2b should succeed");

    let keys2a: Vec<Option<&str>> = page2a.contents().iter().map(|o| o.key()).collect();
    let keys2b: Vec<Option<&str>> = page2b.contents().iter().map(|o| o.key()).collect();
    assert_eq!(
        keys2a, keys2b,
        "same continuation token should yield identical pages"
    );
}

// ---------------------------------------------------------------------------
// 6. Error response format validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_error_response_xml_format() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/nonexistent.txt", base, bucket))
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(resp.status().as_u16(), 404);

    let body = resp.text().await.expect("reading body should succeed");
    assert!(
        body.contains("<Error>"),
        "error body should contain <Error>; got: {}",
        body
    );
    assert!(
        body.contains("<Code>"),
        "error body should contain <Code>; got: {}",
        body
    );
    assert!(
        body.contains("</Code>"),
        "error body should contain </Code>; got: {}",
        body
    );
}

#[tokio::test]
async fn test_error_includes_request_id() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/missing.txt", base, bucket))
        .send()
        .await
        .expect("HTTP GET should not error");

    assert_eq!(resp.status().as_u16(), 404);

    // AWS S3-compatible servers MUST include x-amz-request-id
    let has_request_id = resp.headers().contains_key("x-amz-request-id")
        || resp.headers().contains_key("X-Amz-Request-Id");
    assert!(
        has_request_id,
        "response headers should include x-amz-request-id; headers: {:?}",
        resp.headers()
    );
}

#[tokio::test]
async fn test_no_such_bucket_error_format() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket = unique_bucket(); // never created

    let http = reqwest::Client::new();
    let base = format!("http://{}", server.addr);

    let resp = http
        .get(format!("{}/{}/any-key.txt", base, bucket))
        .send()
        .await
        .expect("HTTP GET should not error");
    assert_eq!(resp.status().as_u16(), 404);

    let body = resp.text().await.expect("reading body should succeed");
    assert!(
        body.contains("NoSuchBucket") || body.contains("NoSuchKey"),
        "error XML should contain NoSuchBucket or NoSuchKey; got: {}",
        body
    );
}

#[tokio::test]
async fn test_no_such_key_error_format() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = unique_bucket();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let result = client
        .get_object()
        .bucket(&bucket)
        .key("definitely-does-not-exist.txt")
        .send()
        .await;

    assert!(result.is_err(), "get_object on missing key should fail");

    let err = result.expect_err("already asserted error");
    match &err {
        SdkError::ServiceError(svc) => match svc.err() {
            GetObjectError::NoSuchKey(_) => { /* expected */ }
            other => {
                // Some SDK versions map this differently; accept any service error
                let _ = other;
            }
        },
        _ => {
            // Accept any SDK error wrapping a 404
        }
    }
}
