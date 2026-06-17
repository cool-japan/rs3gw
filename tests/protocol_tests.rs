#![cfg(feature = "server")]
//! Protocol correctness tests for rs3gw
//!
//! Tests covering:
//! - Conditional GET headers (If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since)
//! - Range requests (basic, suffix, single-byte, invalid, beyond-size)
//! - User metadata persistence (PUT → GET/HEAD parity)
//! - CopyObject with metadata directives (COPY/REPLACE) and cross-bucket
//! - Special object keys (URL-encoded spaces, plus, deep prefix, trailing slash)

mod common;

use common::setup_test_server;

// ─── Conditional Headers ─────────────────────────────────────────────────────

/// PUT an object, then GET with If-Match=<correct etag> → 200
#[tokio::test]
async fn test_conditional_get_if_match_hit() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cond-if-match-hit";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello".to_vec().into())
        .send()
        .await
        .expect("put object");

    let head = client
        .head_object()
        .bucket(bucket)
        .key("obj.txt")
        .send()
        .await
        .expect("head object");
    let etag = head.e_tag().expect("etag present").to_string();

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("If-Match", &etag)
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        200,
        "If-Match with correct ETag should return 200"
    );
}

/// GET with If-Match=wrong_etag → 412 PreconditionFailed
#[tokio::test]
async fn test_conditional_get_if_match_miss() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cond-if-match-miss";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello".to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("If-Match", "\"this-etag-will-never-match\"")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        412,
        "If-Match with wrong ETag should return 412"
    );
}

/// GET with If-None-Match=<correct etag> (etag matches → not modified) → 304
#[tokio::test]
async fn test_conditional_get_if_none_match_hit() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cond-if-none-match-hit";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello".to_vec().into())
        .send()
        .await
        .expect("put object");

    let head = client
        .head_object()
        .bucket(bucket)
        .key("obj.txt")
        .send()
        .await
        .expect("head object");
    let etag = head.e_tag().expect("etag present").to_string();

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("If-None-Match", &etag)
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        304,
        "If-None-Match with matching ETag should return 304"
    );
}

/// GET with If-None-Match=wrong_etag → 200 (etag differs → proceed)
#[tokio::test]
async fn test_conditional_get_if_none_match_miss() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cond-if-none-match-miss";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello".to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("If-None-Match", "\"totally-different-etag\"")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        200,
        "If-None-Match with non-matching ETag should return 200"
    );
}

/// GET with If-Modified-Since set to a far-future date → 304 (not modified since then)
#[tokio::test]
async fn test_conditional_get_if_modified_since_hit() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cond-if-modified-since-hit";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello".to_vec().into())
        .send()
        .await
        .expect("put object");

    // Object was created now; a far-future timestamp means "not modified since then"
    // → server should return 304
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("If-Modified-Since", "Thu, 01 Jan 2099 00:00:00 GMT")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        304,
        "If-Modified-Since with far-future timestamp: object not modified → 304"
    );
}

/// GET with If-Unmodified-Since set to an old timestamp → 412 (object was modified after)
#[tokio::test]
async fn test_conditional_get_if_unmodified_since_miss() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cond-if-unmod-since-miss";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello".to_vec().into())
        .send()
        .await
        .expect("put object");

    // Object was created now (2026); an old 2020 date means object was modified after that
    // → If-Unmodified-Since fails → 412
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("If-Unmodified-Since", "Wed, 01 Jan 2020 00:00:00 GMT")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        412,
        "If-Unmodified-Since with old timestamp: object was modified since then → 412"
    );
}

// ─── Range Requests ───────────────────────────────────────────────────────────

/// PUT 1000-byte object, GET with Range: bytes=0-99 → 206, body is 100 bytes
#[tokio::test]
async fn test_range_basic() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "range-basic";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    // Build a 1000-byte body with predictable content
    let content: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
    client
        .put_object()
        .bucket(bucket)
        .key("large.bin")
        .body(content.clone().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/large.bin", server.addr, bucket))
        .header("Range", "bytes=0-99")
        .send()
        .await
        .expect("send request");

    assert_eq!(resp.status(), 206, "Range request should return 206");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), 100, "Should return exactly 100 bytes");
    assert_eq!(
        &body[..],
        &content[0..100],
        "Range content should match source"
    );
}

/// GET with Range: bytes=-100 → 206, last 100 bytes
#[tokio::test]
async fn test_range_suffix() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "range-suffix";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
    client
        .put_object()
        .bucket(bucket)
        .key("large.bin")
        .body(content.clone().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/large.bin", server.addr, bucket))
        .header("Range", "bytes=-100")
        .send()
        .await
        .expect("send request");

    assert_eq!(resp.status(), 206, "Suffix range should return 206");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), 100, "Should return exactly 100 bytes");
    assert_eq!(
        &body[..],
        &content[900..1000],
        "Suffix range should return last 100 bytes"
    );
}

/// GET with Range: bytes=0-0 → 206, exactly 1 byte
#[tokio::test]
async fn test_range_single_byte() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "range-single-byte";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"ABCDEFGHIJ";
    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("Range", "bytes=0-0")
        .send()
        .await
        .expect("send request");

    assert_eq!(resp.status(), 206, "Single-byte range should return 206");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), 1, "Should return exactly 1 byte");
    assert_eq!(body[0], b'A', "Should return the first byte");
}

/// GET with Range: bytes=abc-def → 416 InvalidRange
#[tokio::test]
async fn test_range_invalid() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "range-invalid";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("obj.txt")
        .body(b"hello world".to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.txt", server.addr, bucket))
        .header("Range", "bytes=abc-def")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        416,
        "Invalid range header should return 416 Range Not Satisfiable"
    );
}

/// GET with Range: bytes=0-9999 on 100-byte object → 206, clamped to full content
#[tokio::test]
async fn test_range_beyond_size() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "range-beyond-size";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content: Vec<u8> = (b'A'..=b'Z').cycle().take(100).collect();
    client
        .put_object()
        .bucket(bucket)
        .key("obj.bin")
        .body(content.clone().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/obj.bin", server.addr, bucket))
        .header("Range", "bytes=0-9999")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        206,
        "Range beyond object size should clamp and return 206"
    );
    let body = resp.bytes().await.expect("read body");
    // Clamped to entire 100-byte object
    assert_eq!(
        body.len(),
        100,
        "Body should be clamped to full 100-byte content"
    );
    assert_eq!(&body[..], &content[..], "Content should match full object");
}

// ─── User Metadata Persistence ────────────────────────────────────────────────

/// PUT with x-amz-meta-foo/baz → GET and HEAD both return those headers
#[tokio::test]
async fn test_user_metadata_persistence() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "user-meta-persist";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("meta.txt")
        .content_type("text/plain")
        .metadata("foo", "bar")
        .metadata("baz", "qux")
        .body(b"metadata test".to_vec().into())
        .send()
        .await
        .expect("put object with metadata");

    // Verify via HEAD
    let head = client
        .head_object()
        .bucket(bucket)
        .key("meta.txt")
        .send()
        .await
        .expect("head object");
    let meta = head.metadata().expect("metadata present");
    assert_eq!(
        meta.get("foo"),
        Some(&"bar".to_string()),
        "HEAD: x-amz-meta-foo should be 'bar'"
    );
    assert_eq!(
        meta.get("baz"),
        Some(&"qux".to_string()),
        "HEAD: x-amz-meta-baz should be 'qux'"
    );

    // Verify via raw GET (check response headers)
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/meta.txt", server.addr, bucket))
        .send()
        .await
        .expect("send GET");
    assert_eq!(resp.status(), 200);
    let headers = resp.headers();
    assert_eq!(
        headers.get("x-amz-meta-foo").and_then(|v| v.to_str().ok()),
        Some("bar"),
        "GET: x-amz-meta-foo should be 'bar'"
    );
    assert_eq!(
        headers.get("x-amz-meta-baz").and_then(|v| v.to_str().ok()),
        Some("qux"),
        "GET: x-amz-meta-baz should be 'qux'"
    );
}

/// PUT object → GET and HEAD should return identical ETag, Content-Type, Content-Length, metadata
#[tokio::test]
async fn test_head_get_parity() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "head-get-parity";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"parity test content";
    client
        .put_object()
        .bucket(bucket)
        .key("parity.txt")
        .content_type("text/plain")
        .metadata("version", "42")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();

    // HEAD request
    let head_resp = http
        .head(format!("http://{}/{}/parity.txt", server.addr, bucket))
        .send()
        .await
        .expect("HEAD request");
    assert_eq!(head_resp.status(), 200);
    let head_etag = head_resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .expect("HEAD ETag")
        .to_string();
    let head_ct = head_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .expect("HEAD Content-Type")
        .to_string();
    let head_cl = head_resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("HEAD Content-Length")
        .to_string();
    let head_meta = head_resp
        .headers()
        .get("x-amz-meta-version")
        .and_then(|v| v.to_str().ok())
        .expect("HEAD x-amz-meta-version")
        .to_string();

    // GET request
    let get_resp = http
        .get(format!("http://{}/{}/parity.txt", server.addr, bucket))
        .send()
        .await
        .expect("GET request");
    assert_eq!(get_resp.status(), 200);
    let get_etag = get_resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .expect("GET ETag")
        .to_string();
    let get_ct = get_resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .expect("GET Content-Type")
        .to_string();
    let get_cl = get_resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("GET Content-Length")
        .to_string();
    let get_meta = get_resp
        .headers()
        .get("x-amz-meta-version")
        .and_then(|v| v.to_str().ok())
        .expect("GET x-amz-meta-version")
        .to_string();

    assert_eq!(
        head_etag, get_etag,
        "ETag must be identical in HEAD and GET"
    );
    assert_eq!(
        head_ct, get_ct,
        "Content-Type must be identical in HEAD and GET"
    );
    assert_eq!(
        head_cl, get_cl,
        "Content-Length must be identical in HEAD and GET"
    );
    assert_eq!(
        head_meta, get_meta,
        "x-amz-meta-version must be identical in HEAD and GET"
    );
}

// ─── CopyObject ───────────────────────────────────────────────────────────────

/// CopyObject without metadata-directive (defaults to COPY) → dst has same metadata
#[tokio::test]
async fn test_copy_object_metadata_directive_copy() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = "copy-meta-copy";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("src.txt")
        .content_type("text/plain")
        .metadata("original", "yes")
        .metadata("version", "1")
        .body(b"source content".to_vec().into())
        .send()
        .await
        .expect("put source");

    // Copy without specifying metadata-directive (should default to COPY)
    let copy_result = client
        .copy_object()
        .bucket(bucket)
        .key("dst.txt")
        .copy_source(format!("{}/src.txt", bucket))
        .send()
        .await;
    assert!(
        copy_result.is_ok(),
        "CopyObject should succeed: {:?}",
        copy_result.err()
    );

    let head = client
        .head_object()
        .bucket(bucket)
        .key("dst.txt")
        .send()
        .await
        .expect("head dst");
    let meta = head.metadata().expect("metadata present");
    assert_eq!(
        meta.get("original"),
        Some(&"yes".to_string()),
        "Metadata 'original' should be copied"
    );
    assert_eq!(
        meta.get("version"),
        Some(&"1".to_string()),
        "Metadata 'version' should be copied"
    );
    assert_eq!(
        head.content_type(),
        Some("text/plain"),
        "Content-Type should be copied"
    );
}

/// CopyObject with x-amz-metadata-directive: REPLACE and new metadata → dst has new metadata only
#[tokio::test]
async fn test_copy_object_metadata_directive_replace() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "copy-meta-replace";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("src.txt")
        .content_type("text/plain")
        .metadata("original", "yes")
        .metadata("version", "1")
        .body(b"source content".to_vec().into())
        .send()
        .await
        .expect("put source");

    // Copy with REPLACE directive and new metadata
    let http = reqwest::Client::new();
    let resp = http
        .put(format!("http://{}/{}/dst.txt", server.addr, bucket))
        .header("x-amz-copy-source", format!("{}/src.txt", bucket).as_str())
        .header("x-amz-metadata-directive", "REPLACE")
        .header("x-amz-meta-newkey", "newvalue")
        .header("Content-Type", "application/octet-stream")
        .send()
        .await
        .expect("copy with REPLACE");
    assert_eq!(resp.status(), 200, "CopyObject with REPLACE should succeed");

    let head = client
        .head_object()
        .bucket(bucket)
        .key("dst.txt")
        .send()
        .await
        .expect("head dst");
    let meta = head.metadata().expect("metadata present");
    assert_eq!(
        meta.get("newkey"),
        Some(&"newvalue".to_string()),
        "New metadata 'newkey' should be present"
    );
    assert_eq!(
        meta.get("original"),
        None,
        "Old metadata 'original' should NOT be present after REPLACE"
    );
    assert_eq!(
        meta.get("version"),
        None,
        "Old metadata 'version' should NOT be present after REPLACE"
    );
}

/// Cross-bucket copy: PUT in bucket1, CopyObject to bucket2/key → GET from bucket2 succeeds
#[tokio::test]
async fn test_copy_object_cross_bucket() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    client
        .create_bucket()
        .bucket("cross-src")
        .send()
        .await
        .expect("create src bucket");
    client
        .create_bucket()
        .bucket("cross-dst")
        .send()
        .await
        .expect("create dst bucket");

    let content = b"cross-bucket content";
    client
        .put_object()
        .bucket("cross-src")
        .key("src.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put into src bucket");

    let copy_result = client
        .copy_object()
        .bucket("cross-dst")
        .key("copied.txt")
        .copy_source("cross-src/src.txt")
        .send()
        .await;
    assert!(
        copy_result.is_ok(),
        "Cross-bucket copy should succeed: {:?}",
        copy_result.err()
    );

    let get_result = client
        .get_object()
        .bucket("cross-dst")
        .key("copied.txt")
        .send()
        .await
        .expect("get from dst bucket");
    let body = get_result
        .body
        .collect()
        .await
        .expect("collect body")
        .into_bytes();
    assert_eq!(
        &body[..],
        content,
        "Cross-bucket copied content should match"
    );
}

// ─── Special Keys ─────────────────────────────────────────────────────────────

/// PUT/GET/DELETE with URL-encoded key containing spaces (foo%20bar)
#[tokio::test]
async fn test_key_with_spaces() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "special-keys-spaces";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"content for key with spaces";

    // PUT via AWS SDK (key with literal space)
    client
        .put_object()
        .bucket(bucket)
        .key("foo bar")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object with space in key");

    // GET via raw HTTP with URL-encoded space
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/foo%20bar", server.addr, bucket))
        .send()
        .await
        .expect("GET with %20");
    assert_eq!(resp.status(), 200, "GET with %20 should return 200");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "Content should match");

    // DELETE via raw HTTP with URL-encoded space
    let resp = http
        .delete(format!("http://{}/{}/foo%20bar", server.addr, bucket))
        .send()
        .await
        .expect("DELETE with %20");
    assert_eq!(resp.status(), 204, "DELETE with %20 should return 204");

    // Verify gone
    let resp = http
        .get(format!("http://{}/{}/foo%20bar", server.addr, bucket))
        .send()
        .await
        .expect("GET after delete");
    assert_eq!(
        resp.status(),
        404,
        "Object should be deleted after DELETE with %20"
    );
}

/// PUT/GET key foo%2Bbar (literal plus in key name)
#[tokio::test]
async fn test_key_with_plus() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "special-keys-plus";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"content for key with plus sign";

    // PUT via AWS SDK with literal '+' in key
    client
        .put_object()
        .bucket(bucket)
        .key("foo+bar")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object with plus in key");

    // GET via raw HTTP — %2B should decode to '+' and fetch the correct object
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/foo%2Bbar", server.addr, bucket))
        .send()
        .await
        .expect("GET with %2B");
    assert_eq!(resp.status(), 200, "GET with %2B should return 200");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "Content should match");
}

/// PUT/GET/DELETE a/b/c/d/e/key (deep prefix)
#[tokio::test]
async fn test_deep_prefix_key() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = "deep-prefix";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let deep_key = "a/b/c/d/e/key.txt";
    let content = b"deeply nested content";

    client
        .put_object()
        .bucket(bucket)
        .key(deep_key)
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put deep prefix object");

    let get = client
        .get_object()
        .bucket(bucket)
        .key(deep_key)
        .send()
        .await
        .expect("get deep prefix object");
    let body = get.body.collect().await.expect("collect body").into_bytes();
    assert_eq!(&body[..], content, "Deep prefix content should match");

    client
        .delete_object()
        .bucket(bucket)
        .key(deep_key)
        .send()
        .await
        .expect("delete deep prefix object");

    let not_found = client
        .get_object()
        .bucket(bucket)
        .key(deep_key)
        .send()
        .await;
    assert!(not_found.is_err(), "Deep prefix object should be deleted");
}

/// PUT/GET object with key ending in /
#[tokio::test]
async fn test_trailing_slash_key() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "trailing-slash";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"trailing slash key content";

    // PUT via AWS SDK with trailing slash in key (simulates directory-like object)
    client
        .put_object()
        .bucket(bucket)
        .key("folder/")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object with trailing slash");

    // GET via raw HTTP
    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/folder/", server.addr, bucket))
        .send()
        .await
        .expect("GET trailing slash key");
    assert_eq!(
        resp.status(),
        200,
        "GET trailing slash key should return 200"
    );
    let body = resp.bytes().await.expect("read body");
    assert_eq!(
        &body[..],
        content,
        "Trailing slash key content should match"
    );
}

// === Malformed XML rejection ===

/// PUT /{bucket} with an unclosed-tag body — should return 400
#[tokio::test]
async fn test_create_bucket_malformed_xml() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket = "malformed-xml-bucket";

    let http = reqwest::Client::new();
    // Send a body with unclosed tags to PUT /{bucket} (CreateBucket)
    let resp = http
        .put(format!("http://{}/{}", server.addr, bucket))
        .header("Content-Type", "application/xml")
        .body("<CreateBucketConfiguration><LocationConstraint>us-west-2</LocationConstraint")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        400,
        "CreateBucket with malformed XML body should return 400"
    );
}

/// POST /{bucket}/{key}?uploadId=test-upload-id with garbage body — should return 400
#[tokio::test]
async fn test_complete_multipart_upload_invalid_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "complete-mpu-bad-xml";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let http = reqwest::Client::new();
    // POST with garbage XML body — CompleteMultipartUpload parses <Part> elements
    // and will return MalformedXML when none are found
    let resp = http
        .post(format!(
            "http://{}/{}/mykey?uploadId=nonexistent-upload-id",
            server.addr, bucket
        ))
        .header("Content-Type", "application/xml")
        .body("!!garbage!not-xml!!")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        400,
        "CompleteMultipartUpload with garbage body should return 400"
    );
}

/// PUT /{bucket}?tagging with partial/invalid XML — should return 400
#[tokio::test]
async fn test_put_bucket_tagging_malformed_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "tagging-malformed-xml";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let http = reqwest::Client::new();
    // Send partial/invalid XML (no <TagSet> element)
    let resp = http
        .put(format!("http://{}/{}?tagging", server.addr, bucket))
        .header("Content-Type", "application/xml")
        .body("<Tagging><unclosed_tag")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        400,
        "PutBucketTagging with malformed XML should return 400"
    );
}

/// POST /{bucket}?delete with empty body — should return 400
#[tokio::test]
async fn test_delete_objects_empty_body() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "delete-objects-empty";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let http = reqwest::Client::new();
    // POST with empty body to ?delete — DeleteObjects expects an XML list of objects
    let resp = http
        .post(format!("http://{}/{}?delete", server.addr, bucket))
        .header("Content-Type", "application/xml")
        .body("")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        400,
        "DeleteObjects with empty body should return 400"
    );
}

// === Query Parsing Hardening ===

/// PUT /{bucket}/{key}?partNumber=abc&uploadId=xyz — non-numeric partNumber should return 400
#[tokio::test]
async fn test_invalid_part_number_query_param() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "invalid-part-number";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let http = reqwest::Client::new();
    // partNumber=abc is not a valid u32 — Axum's Query extractor will reject it with 400
    let resp = http
        .put(format!(
            "http://{}/{}/mykey?partNumber=abc&uploadId=some-upload-id",
            server.addr, bucket
        ))
        .body("part data")
        .send()
        .await
        .expect("send request");

    assert_eq!(
        resp.status(),
        400,
        "PUT with non-numeric partNumber should return 400"
    );
}

// === Content-Length on list responses ===

/// ListBuckets response must include a numeric Content-Length header
#[tokio::test]
async fn test_list_buckets_response_has_content_length() {
    let (_client, _temp_dir, server) = setup_test_server().await;

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/", server.addr))
        .send()
        .await
        .expect("send ListBuckets request");

    assert_eq!(resp.status(), 200, "ListBuckets should return 200");
    let cl_header = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("ListBuckets response must have content-length header");
    let cl_value: u64 = cl_header
        .parse()
        .expect("content-length must be a numeric value");
    assert!(
        cl_value > 0,
        "ListBuckets content-length should be non-zero (XML body)"
    );
}

/// ListObjectsV1 response must include a numeric Content-Length header
#[tokio::test]
async fn test_list_objects_response_has_content_length() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "list-cl-check";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    // Put one object so the response body is non-trivial
    client
        .put_object()
        .bucket(bucket)
        .key("test.txt")
        .body(b"content".to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}", server.addr, bucket))
        .send()
        .await
        .expect("send ListObjectsV1 request");

    assert_eq!(resp.status(), 200, "ListObjectsV1 should return 200");
    let cl_header = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("ListObjectsV1 response must have content-length header");
    let _cl_value: u64 = cl_header
        .parse()
        .expect("content-length must be a numeric value");
}

// ─── Content-Length Audit / Verification ──────────────────────────────────────

/// PUT a known-size object, GET it, verify Content-Length header equals actual body length
#[tokio::test]
async fn test_get_content_length_matches_body() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cl-get-body-match";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"The quick brown fox jumps over the lazy dog.";
    client
        .put_object()
        .bucket(bucket)
        .key("cl-test.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/cl-test.txt", server.addr, bucket))
        .send()
        .await
        .expect("GET request");

    assert_eq!(resp.status(), 200);
    let cl_header: u64 = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("Content-Length header must be present")
        .parse()
        .expect("Content-Length must be numeric");

    let body = resp.bytes().await.expect("read body");
    assert_eq!(
        cl_header,
        body.len() as u64,
        "Content-Length header must equal actual body length"
    );
    assert_eq!(
        cl_header,
        content.len() as u64,
        "Content-Length must match the original object size"
    );
}

/// PUT object, do both HEAD and GET, verify Content-Length headers match
#[tokio::test]
async fn test_head_content_length_matches_get() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cl-head-get-match";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content = b"content-length parity check between HEAD and GET";
    client
        .put_object()
        .bucket(bucket)
        .key("parity-cl.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();

    // HEAD request
    let head_resp = http
        .head(format!("http://{}/{}/parity-cl.txt", server.addr, bucket))
        .send()
        .await
        .expect("HEAD request");
    assert_eq!(head_resp.status(), 200);
    let head_cl: u64 = head_resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("HEAD Content-Length must be present")
        .parse()
        .expect("HEAD Content-Length must be numeric");

    // GET request
    let get_resp = http
        .get(format!("http://{}/{}/parity-cl.txt", server.addr, bucket))
        .send()
        .await
        .expect("GET request");
    assert_eq!(get_resp.status(), 200);
    let get_cl: u64 = get_resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("GET Content-Length must be present")
        .parse()
        .expect("GET Content-Length must be numeric");

    assert_eq!(
        head_cl, get_cl,
        "HEAD and GET Content-Length must be identical"
    );
    assert_eq!(
        head_cl,
        content.len() as u64,
        "Content-Length must match the original object size"
    );
}

/// PUT a 1000-byte object, GET with Range: bytes=100-199, verify Content-Length is 100
#[tokio::test]
async fn test_range_content_length_correct() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cl-range-check";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    let content: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
    client
        .put_object()
        .bucket(bucket)
        .key("range-cl.bin")
        .body(content.into())
        .send()
        .await
        .expect("put object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/range-cl.bin", server.addr, bucket))
        .header("Range", "bytes=100-199")
        .send()
        .await
        .expect("range GET request");

    assert_eq!(resp.status(), 206, "Range request should return 206");
    let cl_header: u64 = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("Content-Length header must be present on range response")
        .parse()
        .expect("Content-Length must be numeric");

    assert_eq!(
        cl_header, 100,
        "Content-Length for bytes=100-199 must be 100"
    );

    let body = resp.bytes().await.expect("read body");
    assert_eq!(
        body.len() as u64,
        cl_header,
        "Actual body length must match Content-Length header"
    );
}

/// PUT empty object, GET it, verify Content-Length is 0
#[tokio::test]
async fn test_zero_byte_object_content_length() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = "cl-zero-byte";
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create bucket");

    client
        .put_object()
        .bucket(bucket)
        .key("empty.txt")
        .body(Vec::new().into())
        .send()
        .await
        .expect("put empty object");

    let http = reqwest::Client::new();
    let resp = http
        .get(format!("http://{}/{}/empty.txt", server.addr, bucket))
        .send()
        .await
        .expect("GET empty object");

    assert_eq!(resp.status(), 200);
    let cl_header: u64 = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .expect("Content-Length header must be present for empty object")
        .parse()
        .expect("Content-Length must be numeric");

    assert_eq!(
        cl_header, 0,
        "Content-Length for a zero-byte object must be 0"
    );

    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), 0, "Body of empty object must be empty");
}
