#![cfg(feature = "server")]
//! Tests for S3 SSE-S3 (AES-256 server-side encryption) operations.
//!
//! All tests use a single `setup_test_server()` call per test so that the
//! process-local `LocalKeyProvider` key is shared between the PUT and the
//! subsequent GET — the only way to successfully decrypt in a unit/integration
//! test environment.

mod common;

use base64::Engine as _;
use common::setup_test_server;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal well-formed `ServerSideEncryptionConfiguration` XML for AES256.
fn bucket_encryption_xml_aes256() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<ServerSideEncryptionConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <ApplyServerSideEncryptionByDefault>
      <SSEAlgorithm>AES256</SSEAlgorithm>
    </ApplyServerSideEncryptionByDefault>
  </Rule>
</ServerSideEncryptionConfiguration>"#
}

// ---------------------------------------------------------------------------
// Test 1: PUT with AES256 header -> GET round-trips plaintext
// ---------------------------------------------------------------------------

/// PUT an object with `x-amz-server-side-encryption: AES256`, verify the PUT
/// response echoes the header, then GET and verify the body is decrypted.
#[tokio::test]
async fn test_put_get_with_aes256_header_round_trips() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-rtrip-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();
    let plaintext = b"hello encrypted world";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT with SSE header
    let put_resp = http
        .put(format!("{}/{}/mykey", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT with SSE header should complete");

    assert_eq!(
        put_resp.status(),
        200,
        "PutObject with AES256 header should return 200"
    );
    assert_eq!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "PUT response must echo x-amz-server-side-encryption: AES256"
    );

    // GET and verify decrypted body
    let get_resp = http
        .get(format!("{}/{}/mykey", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");

    assert_eq!(
        get_resp.status(),
        200,
        "GET of SSE object should return 200"
    );
    assert_eq!(
        get_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "GET response must include x-amz-server-side-encryption: AES256"
    );

    let body = get_resp.bytes().await.expect("read GET body");
    assert_eq!(
        body.as_ref(),
        plaintext,
        "GET body must equal original plaintext (decryption round-trip)"
    );

    // Cleanup
    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 2: Bucket-default AES256 encrypts even without per-request header
// ---------------------------------------------------------------------------

/// Set bucket-default encryption to AES256, then PUT without any SSE header.
/// GET should return plaintext and emit `x-amz-server-side-encryption: AES256`.
#[tokio::test]
async fn test_put_without_header_bucket_default_aes256_encrypts() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-bktdef-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();
    let plaintext = b"bucket default encrypted data";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // Set bucket-default encryption
    let enc_resp = http
        .put(format!("{}/{}?encryption", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(bucket_encryption_xml_aes256())
        .send()
        .await
        .expect("PUT ?encryption should complete");
    assert_eq!(
        enc_resp.status(),
        200,
        "PutBucketEncryption(AES256) should return 200, got: {}",
        enc_resp.status()
    );

    // PUT object WITHOUT any SSE header
    let put_resp = http
        .put(format!("{}/{}/noheader", base_url, bucket))
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT without SSE header should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutObject (bucket-default encryption) should return 200"
    );
    assert_eq!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "PUT response must echo AES256 when bucket-default applies"
    );

    // GET — must return plaintext
    let get_resp = http
        .get(format!("{}/{}/noheader", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");
    assert_eq!(get_resp.status(), 200, "GET should return 200");
    assert_eq!(
        get_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "GET response must include x-amz-server-side-encryption: AES256"
    );
    let body = get_resp.bytes().await.expect("read GET body");
    assert_eq!(
        body.as_ref(),
        plaintext,
        "GET body must equal original plaintext"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 3: aws:kms:dsse still returns 501 NotImplemented
// ---------------------------------------------------------------------------

/// PUT with `x-amz-server-side-encryption: aws:kms:dsse` must return 501
/// (double-layer KMS is explicitly deferred).
#[tokio::test]
async fn test_put_with_aws_kms_dsse_returns_501() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-kms-dsse-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}/dsse", base_url, bucket))
        .header("x-amz-server-side-encryption", "aws:kms:dsse")
        .body(b"payload".as_ref())
        .send()
        .await
        .expect("PUT with aws:kms:dsse header should complete");

    assert_eq!(
        put_resp.status(),
        501,
        "PutObject with aws:kms:dsse should return 501 NotImplemented"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 10: SSE-KMS PUT/GET round-trip (default key, no explicit key ID)
// ---------------------------------------------------------------------------

/// PUT with `x-amz-server-side-encryption: aws:kms` (no key-id header),
/// verify PUT response has `x-amz-server-side-encryption: aws:kms` and the
/// kms-key-id header, then GET and verify the body equals the original plaintext.
#[tokio::test]
async fn test_sse_kms_put_get_round_trip() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-kms-rtrip-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();
    let plaintext = b"hello SSE-KMS world -- encrypted with local KMS shim";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT with SSE-KMS header (no explicit key ID — use default KEK).
    let put_resp = http
        .put(format!("{}/{}/kmsobj", base_url, bucket))
        .header("x-amz-server-side-encryption", "aws:kms")
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT with aws:kms header should complete");

    assert_eq!(
        put_resp.status(),
        200,
        "PutObject with aws:kms should return 200"
    );
    assert_eq!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("aws:kms"),
        "PUT response must echo x-amz-server-side-encryption: aws:kms"
    );
    assert!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption-aws-kms-key-id")
            .is_some(),
        "PUT response must include x-amz-server-side-encryption-aws-kms-key-id"
    );

    // GET — server decrypts and returns plaintext.
    let get_resp = http
        .get(format!("{}/{}/kmsobj", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");

    assert_eq!(
        get_resp.status(),
        200,
        "GET of SSE-KMS object should return 200"
    );
    assert_eq!(
        get_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("aws:kms"),
        "GET response must include x-amz-server-side-encryption: aws:kms"
    );

    let body = get_resp.bytes().await.expect("read GET body");
    assert_eq!(
        body.as_ref(),
        plaintext,
        "GET body must equal original plaintext (SSE-KMS round-trip)"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 11: SSE-KMS HeadObject emits both KMS headers
// ---------------------------------------------------------------------------

/// PUT with `x-amz-server-side-encryption: aws:kms`, then HEAD — the response
/// must include both `x-amz-server-side-encryption: aws:kms` and the KMS key ARN.
#[tokio::test]
async fn test_sse_kms_head_emits_kms_headers() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-kms-head-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT with SSE-KMS header.
    let put_resp = http
        .put(format!("{}/{}/headkms", base_url, bucket))
        .header("x-amz-server-side-encryption", "aws:kms")
        .body(b"content for SSE-KMS head test".as_ref())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PUT should return 200");

    // HEAD — verify SSE-KMS headers.
    let head_resp = http
        .head(format!("{}/{}/headkms", base_url, bucket))
        .send()
        .await
        .expect("HEAD should complete");
    assert_eq!(head_resp.status(), 200, "HEAD should return 200");
    assert_eq!(
        head_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("aws:kms"),
        "HEAD response must include x-amz-server-side-encryption: aws:kms"
    );
    assert!(
        head_resp
            .headers()
            .get("x-amz-server-side-encryption-aws-kms-key-id")
            .is_some(),
        "HEAD response must include x-amz-server-side-encryption-aws-kms-key-id"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 4: HeadObject emits SSE header when sidecar is present
// ---------------------------------------------------------------------------

/// PUT with AES256, then HEAD — response must include the SSE header.
#[tokio::test]
async fn test_head_object_emits_sse_header() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-head-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT with SSE
    let put_resp = http
        .put(format!("{}/{}/headobj", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(b"content for head".as_ref())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PUT should return 200");

    // HEAD
    let head_resp = http
        .head(format!("{}/{}/headobj", base_url, bucket))
        .send()
        .await
        .expect("HEAD should complete");
    assert_eq!(head_resp.status(), 200, "HEAD should return 200");
    assert_eq!(
        head_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "HEAD response must include x-amz-server-side-encryption: AES256"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 5: Range-GET on SSE object — full-decrypt-then-slice (D3 path)
// ---------------------------------------------------------------------------

/// PUT "Hello, World!" (13 bytes) with AES256. Range-GET bytes=0-4 must return "Hello".
#[tokio::test]
async fn test_get_range_on_sse_object() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-range-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();
    let plaintext = b"Hello, World!"; // 13 bytes

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}/rangeobj", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PUT should return 200");

    // Range-GET bytes=0-4 → "Hello"
    let range_resp = http
        .get(format!("{}/{}/rangeobj", base_url, bucket))
        .header("Range", "bytes=0-4")
        .send()
        .await
        .expect("Range-GET should complete");
    assert_eq!(
        range_resp.status(),
        206,
        "Range-GET should return 206 Partial Content"
    );

    let body = range_resp.bytes().await.expect("read range body");
    assert_eq!(
        body.as_ref(),
        b"Hello",
        "Range-GET[0-4] of SSE object must return first 5 bytes of plaintext"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 6: Plain object (no encryption) — SSE header absent in GET response
// ---------------------------------------------------------------------------

/// A fresh bucket with no default encryption + PUT without SSE header → GET
/// must NOT include `x-amz-server-side-encryption` in the response.
#[tokio::test]
async fn test_plain_object_not_encrypted_by_default() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-plain-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();
    let plaintext = b"plain unencrypted content";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT without SSE header
    let put_resp = http
        .put(format!("{}/{}/plainobj", base_url, bucket))
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT without SSE should complete");
    assert_eq!(put_resp.status(), 200, "PUT should return 200");
    assert!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .is_none(),
        "PUT response for plain object must NOT include SSE header"
    );

    // GET — no SSE header, body is plaintext as-is
    let get_resp = http
        .get(format!("{}/{}/plainobj", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");
    assert_eq!(get_resp.status(), 200, "GET should return 200");
    assert!(
        get_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .is_none(),
        "GET response for plain object must NOT include SSE header"
    );
    let body = get_resp.bytes().await.expect("read GET body");
    assert_eq!(
        body.as_ref(),
        plaintext,
        "GET body must equal original data"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 7: SSE-S3 multipart upload round-trip
// ---------------------------------------------------------------------------

/// Initiate a multipart upload with `x-amz-server-side-encryption: AES256`,
/// upload 2 parts, complete it, then GET and verify:
///   - The response body equals the concatenation of the two parts (plaintext).
///   - The GET response includes `x-amz-server-side-encryption: AES256`.
#[tokio::test]
async fn test_multipart_sse_s3_round_trip() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-mp-{}", uuid::Uuid::new_v4());
    let key = "multipart-sse-object";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    // Part data — must be distinguishable after concatenation.
    let part1: Vec<u8> = (0u8..=255u8).collect(); // 256 bytes, 0x00..0xFF
    let part2: Vec<u8> = (0u8..128u8).rev().collect(); // 128 bytes, 0x7F..0x00

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // --- CreateMultipartUpload ---
    let create_resp = http
        .post(format!("{}/{}/{}?uploads", base_url, bucket, key))
        .header("x-amz-server-side-encryption", "AES256")
        .send()
        .await
        .expect("CreateMultipartUpload should complete");

    assert_eq!(
        create_resp.status(),
        200,
        "CreateMultipartUpload with AES256 should return 200"
    );
    assert_eq!(
        create_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "CreateMultipartUpload response must echo x-amz-server-side-encryption: AES256"
    );

    // Extract uploadId from XML response body.
    let xml_body = create_resp
        .text()
        .await
        .expect("read CreateMultipartUpload body");
    let upload_id = {
        let start = xml_body
            .find("<UploadId>")
            .expect("UploadId tag in CreateMultipartUpload response")
            + "<UploadId>".len();
        let end = xml_body
            .find("</UploadId>")
            .expect("</UploadId> tag in CreateMultipartUpload response");
        xml_body[start..end].to_string()
    };

    // --- UploadPart 1 ---
    let up1_resp = http
        .put(format!(
            "{}/{}/{}?partNumber=1&uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .body(part1.clone())
        .send()
        .await
        .expect("UploadPart 1 should complete");
    assert_eq!(up1_resp.status(), 200, "UploadPart 1 should return 200");
    let etag1 = up1_resp
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .expect("ETag header on UploadPart 1")
        .trim_matches('"')
        .to_string();

    // --- UploadPart 2 ---
    let up2_resp = http
        .put(format!(
            "{}/{}/{}?partNumber=2&uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .body(part2.clone())
        .send()
        .await
        .expect("UploadPart 2 should complete");
    assert_eq!(up2_resp.status(), 200, "UploadPart 2 should return 200");
    let etag2 = up2_resp
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .expect("ETag header on UploadPart 2")
        .trim_matches('"')
        .to_string();

    // --- CompleteMultipartUpload ---
    let complete_xml = format!(
        r#"<CompleteMultipartUpload>
  <Part><PartNumber>1</PartNumber><ETag>"{}"</ETag></Part>
  <Part><PartNumber>2</PartNumber><ETag>"{}"</ETag></Part>
</CompleteMultipartUpload>"#,
        etag1, etag2
    );
    let complete_resp = http
        .post(format!(
            "{}/{}/{}?uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .header("Content-Type", "application/xml")
        .body(complete_xml)
        .send()
        .await
        .expect("CompleteMultipartUpload should complete");

    assert_eq!(
        complete_resp.status(),
        200,
        "CompleteMultipartUpload should return 200"
    );
    assert_eq!(
        complete_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "CompleteMultipartUpload response must include x-amz-server-side-encryption: AES256"
    );

    // --- GET and verify plaintext round-trip ---
    let get_resp = http
        .get(format!("{}/{}/{}", base_url, bucket, key))
        .send()
        .await
        .expect("GET should complete");

    assert_eq!(
        get_resp.status(),
        200,
        "GET of SSE multipart object should return 200"
    );
    assert_eq!(
        get_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "GET response must include x-amz-server-side-encryption: AES256"
    );

    let body = get_resp.bytes().await.expect("read GET body");

    // Expected plaintext = part1 || part2
    let mut expected = part1.clone();
    expected.extend_from_slice(&part2);

    assert_eq!(
        body.as_ref(),
        expected.as_slice(),
        "GET body must equal concatenation of part1 and part2 (SSE decryption round-trip)"
    );

    // Cleanup
    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 8 (orig Test 7): Sidecar AAD tamper (placeholder — requires filesystem manipulation)
// ---------------------------------------------------------------------------

/// Swapping sidecars between two SSE objects should cause decryption to fail
/// because the AAD (bucket+key) will not match.
///
/// The sidecar stores `aad_bucket` and `aad_key`, which are used verbatim as
/// the GCM AAD on every GET.  Swapping the sidecar of key-a with that of key-b
/// means the server will attempt to decrypt key-a's ciphertext using an AAD of
/// `"<bucket>/key-b"`, while the ciphertext was sealed with `"<bucket>/key-a"`.
/// GCM authentication will reject the tag → the server must return non-200.
#[tokio::test]
async fn test_sidecar_aad_tampering_fails_decrypt() {
    let (client, temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-tamper-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    let plaintext_a: &[u8] = b"plaintext-for-key-a-aaaaaaaaaaaa";
    let plaintext_b: &[u8] = b"plaintext-for-key-b-bbbbbbbbbbbb";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT key-a with AES256
    let put_a = http
        .put(format!("{}/{}/key-a", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext_a)
        .send()
        .await
        .expect("PUT key-a should complete");
    assert_eq!(put_a.status(), 200, "PUT key-a should return 200");

    // PUT key-b with AES256
    let put_b = http
        .put(format!("{}/{}/key-b", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext_b)
        .send()
        .await
        .expect("PUT key-b should complete");
    assert_eq!(put_b.status(), 200, "PUT key-b should return 200");

    // Build the on-disk sidecar paths.
    // sanitize_key_for_fs("key-a") == "key-a" (no special chars).
    let sidecar_a = temp_dir.path().join(&bucket).join("sse").join("key-a.json");
    let sidecar_b = temp_dir.path().join(&bucket).join("sse").join("key-b.json");

    assert!(
        sidecar_a.exists(),
        "sidecar for key-a must exist at {:?}",
        sidecar_a
    );
    assert!(
        sidecar_b.exists(),
        "sidecar for key-b must exist at {:?}",
        sidecar_b
    );

    // Swap the two sidecar files: A→tmp, B→A, tmp→B.
    let tmp_path = temp_dir.path().join("sse_swap_tmp.json");
    std::fs::copy(&sidecar_a, &tmp_path).expect("copy sidecar_a to tmp");
    std::fs::copy(&sidecar_b, &sidecar_a).expect("copy sidecar_b to sidecar_a");
    std::fs::copy(&tmp_path, &sidecar_b).expect("copy tmp to sidecar_b");
    std::fs::remove_file(&tmp_path).expect("remove tmp file");

    // GET key-a: sidecar now holds key-b's metadata, so AAD mismatch → decrypt fails.
    let get_resp = http
        .get(format!("{}/{}/key-a", base_url, bucket))
        .send()
        .await
        .expect("GET key-a after sidecar swap should complete");

    let status = get_resp.status();
    // The GCM authentication tag verification fails because the ciphertext was
    // sealed with AAD = "<bucket>/key-a" but the (swapped) sidecar reconstructs
    // AAD = "<bucket>/key-b".  The server should propagate the error as non-200.
    assert_ne!(
        status.as_u16(),
        200,
        "GET with swapped sidecar must NOT return 200 (got: {})",
        status
    );

    // Cleanup
    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// SSE-C helpers
// ---------------------------------------------------------------------------

/// Build the SSE-C `(key_b64, md5_b64)` pair for a 32-byte key.
///
/// Callers attach these as request headers:
///   x-amz-server-side-encryption-customer-algorithm: AES256
///   x-amz-server-side-encryption-customer-key: <key_b64>
///   x-amz-server-side-encryption-customer-key-MD5: <md5_b64>
fn sse_c_headers(key: &[u8; 32]) -> (String, String) {
    let key_b64 = base64::engine::general_purpose::STANDARD.encode(key);
    let md5_b64 = base64::engine::general_purpose::STANDARD.encode(md5::compute(key).0);
    (key_b64, md5_b64)
}

// ---------------------------------------------------------------------------
// Test 8: SSE-C PUT/GET round-trip
// ---------------------------------------------------------------------------

/// PUT an object with customer-provided key headers (SSE-C), then GET with the
/// same key headers — the server must return the original plaintext.
#[tokio::test]
async fn test_sse_c_put_get_round_trip() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;

    // Create bucket via raw HTTP (no AWS SDK; easier to control custom headers).
    let bucket = format!("ssec-rtrip-{}", uuid::Uuid::new_v4());
    let put_bucket_resp = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket should succeed");
    assert!(
        put_bucket_resp.status().is_success(),
        "create bucket returned {}",
        put_bucket_resp.status()
    );

    // Generate a deterministic 32-byte customer key.
    let customer_key: [u8; 32] = [0x42u8; 32];
    let (key_b64, md5_b64) = sse_c_headers(&customer_key);
    let plaintext = b"hello SSE-C world -- keep this secret!";

    // PUT with SSE-C headers.
    let put_resp = http
        .put(format!("{}/{}/obj1", base_url, bucket))
        .header("x-amz-server-side-encryption-customer-algorithm", "AES256")
        .header("x-amz-server-side-encryption-customer-key", &key_b64)
        .header("x-amz-server-side-encryption-customer-key-MD5", &md5_b64)
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT with SSE-C should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutObject with SSE-C should return 200"
    );
    // PUT response must echo the SSE-C algorithm and key-MD5 headers.
    assert_eq!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption-customer-algorithm")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "PUT response must echo x-amz-server-side-encryption-customer-algorithm"
    );
    assert_eq!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption-customer-key-md5")
            .and_then(|v| v.to_str().ok()),
        Some(md5_b64.as_str()),
        "PUT response must echo x-amz-server-side-encryption-customer-key-MD5"
    );
    // Must NOT emit the generic SSE-S3 header.
    assert!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .is_none(),
        "PUT response must NOT include x-amz-server-side-encryption for SSE-C"
    );

    // GET with same SSE-C key — server decrypts and returns plaintext.
    let get_resp = http
        .get(format!("{}/{}/obj1", base_url, bucket))
        .header("x-amz-server-side-encryption-customer-algorithm", "AES256")
        .header("x-amz-server-side-encryption-customer-key", &key_b64)
        .header("x-amz-server-side-encryption-customer-key-MD5", &md5_b64)
        .send()
        .await
        .expect("GET with SSE-C should complete");
    assert_eq!(
        get_resp.status(),
        200,
        "GET of SSE-C object with correct key should return 200"
    );
    let body = get_resp.bytes().await.expect("read GET body");
    assert_eq!(
        body.as_ref(),
        plaintext,
        "GET body must equal original plaintext (SSE-C round-trip)"
    );
}

// ---------------------------------------------------------------------------
// Test 9: SSE-C GET with wrong key returns error
// ---------------------------------------------------------------------------

/// PUT with key A, then GET with key B (different) — the server detects the
/// MD5 mismatch and must return non-200.
#[tokio::test]
async fn test_sse_c_get_wrong_key_returns_error() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;

    let bucket = format!("ssec-wrongkey-{}", uuid::Uuid::new_v4());
    let put_bucket_resp = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket should succeed");
    assert!(
        put_bucket_resp.status().is_success(),
        "create bucket returned {}",
        put_bucket_resp.status()
    );

    // Key A — used for PUT.
    let key_a: [u8; 32] = [0xAAu8; 32];
    let (key_a_b64, md5_a_b64) = sse_c_headers(&key_a);

    // Key B — different key used for GET.
    let key_b: [u8; 32] = [0xBBu8; 32];
    let (key_b_b64, md5_b_b64) = sse_c_headers(&key_b);

    let plaintext = b"secret data encrypted with key A";

    // PUT with key A.
    let put_resp = http
        .put(format!("{}/{}/wrongkeyobj", base_url, bucket))
        .header("x-amz-server-side-encryption-customer-algorithm", "AES256")
        .header("x-amz-server-side-encryption-customer-key", &key_a_b64)
        .header("x-amz-server-side-encryption-customer-key-MD5", &md5_a_b64)
        .body(plaintext.as_ref())
        .send()
        .await
        .expect("PUT with key A should complete");
    assert_eq!(put_resp.status(), 200, "PUT with key A should return 200");

    // GET with key B — MD5 mismatch detected server-side → must be non-200.
    let get_resp = http
        .get(format!("{}/{}/wrongkeyobj", base_url, bucket))
        .header("x-amz-server-side-encryption-customer-algorithm", "AES256")
        .header("x-amz-server-side-encryption-customer-key", &key_b_b64)
        .header("x-amz-server-side-encryption-customer-key-MD5", &md5_b_b64)
        .send()
        .await
        .expect("GET with wrong key should complete");

    assert_ne!(
        get_resp.status().as_u16(),
        200,
        "GET with wrong SSE-C key must NOT return 200 (got: {})",
        get_resp.status()
    );
}

// ---------------------------------------------------------------------------
// Test: Chunked SSE round-trip (12 MB object — multi-chunk v2 format)
// ---------------------------------------------------------------------------

/// PUT a 12 MB SSE-S3 object (3 chunks at 5 MiB each minus last),
/// GET the full object, verify the body is byte-for-byte identical.
#[tokio::test]
async fn test_chunked_sse_put_get_round_trip() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;

    let bucket = format!("chunked-rtrip-{}", uuid::Uuid::new_v4());
    let put_bucket_resp = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(
        put_bucket_resp.status().is_success(),
        "create bucket returned {}",
        put_bucket_resp.status()
    );

    // 12 MiB of pseudo-random bytes: 3 chunks (5MB + 5MB + 2MB).
    let size = 12 * 1024 * 1024usize;
    let plaintext: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();

    let put_resp = http
        .put(format!("{}/{}/big-obj", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext.clone())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PUT returned {}", put_resp.status());
    assert_eq!(
        put_resp
            .headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256"),
        "PUT response must echo x-amz-server-side-encryption: AES256"
    );

    let get_resp = http
        .get(format!("{}/{}/big-obj", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");
    assert_eq!(get_resp.status(), 200, "GET returned {}", get_resp.status());

    let body = get_resp.bytes().await.expect("read GET body");
    assert_eq!(
        body.len(),
        plaintext.len(),
        "GET body length mismatch: {} vs {}",
        body.len(),
        plaintext.len()
    );
    assert_eq!(
        body.as_ref(),
        plaintext.as_slice(),
        "GET body must equal original plaintext (chunked encrypt round-trip)"
    );
}

// ---------------------------------------------------------------------------
// Test: Chunked SSE range-GET (6 MB object, range spanning the 5 MB boundary)
// ---------------------------------------------------------------------------

/// PUT a 6 MB SSE-S3 object (first 3 MB = 0x00, last 3 MB = 0xFF),
/// GET with Range: bytes=4000000-5500000 (spans the 5 MiB chunk boundary),
/// verify the response is 206 and the body matches the expected slice.
#[tokio::test]
async fn test_chunked_sse_range_get() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;

    let bucket = format!("chunked-range-{}", uuid::Uuid::new_v4());
    let put_bucket_resp = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(
        put_bucket_resp.status().is_success(),
        "create bucket returned {}",
        put_bucket_resp.status()
    );

    // 6 MiB: first 3 MB = 0x00, last 3 MB = 0xFF.
    // Chunk boundary is at exactly 5 MiB (5_242_880 bytes).
    const THREE_MB: usize = 3 * 1024 * 1024;
    const SIX_MB: usize = 6 * 1024 * 1024;
    let mut plaintext = vec![0x00u8; THREE_MB];
    plaintext.extend(vec![0xFFu8; THREE_MB]);
    assert_eq!(plaintext.len(), SIX_MB);

    let put_resp = http
        .put(format!("{}/{}/range-obj", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext.clone())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put_resp.status(), 200, "PUT returned {}", put_resp.status());

    // Range: bytes=4000000-5500000 spans the 5 MiB (5_242_880) chunk boundary.
    // Expected slice = plaintext[4000000..=5500000].
    let range_start: usize = 4_000_000;
    let range_end_inclusive: usize = 5_500_000;
    let expected_slice = &plaintext[range_start..=range_end_inclusive];

    let get_resp = http
        .get(format!("{}/{}/range-obj", base_url, bucket))
        .header(
            "Range",
            format!("bytes={}-{}", range_start, range_end_inclusive),
        )
        .send()
        .await
        .expect("range-GET should complete");

    assert_eq!(
        get_resp.status(),
        206,
        "range-GET must return 206 Partial Content, got {}",
        get_resp.status()
    );

    let content_range = get_resp
        .headers()
        .get("Content-Range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert!(
        content_range.starts_with("bytes 4000000-5500000/"),
        "Content-Range header unexpected: {:?}",
        content_range
    );

    let body = get_resp.bytes().await.expect("read range-GET body");
    let expected_len = range_end_inclusive - range_start + 1;
    assert_eq!(
        body.len(),
        expected_len,
        "range-GET body length: expected {}, got {}",
        expected_len,
        body.len()
    );
    assert_eq!(
        body.as_ref(),
        expected_slice,
        "range-GET body must match expected plaintext slice"
    );
}

// ---------------------------------------------------------------------------
// Helper: PUT a deterministic multi-chunk (6 MiB) SSE-S3 object and return the
// plaintext for slice comparisons. 6 MiB = chunk 0 (5 MiB) + chunk 1 (1 MiB).
// ---------------------------------------------------------------------------
async fn put_6mib_sse_object(
    http: &reqwest::Client,
    base_url: &str,
    bucket: &str,
    key: &str,
) -> Vec<u8> {
    let put_bucket = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(put_bucket.status().is_success(), "create bucket");

    let size = 6 * 1024 * 1024usize; // 6_291_456
    let plaintext: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let put = http
        .put(format!("{}/{}/{}", base_url, bucket, key))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext.clone())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put.status(), 200, "PUT returned {}", put.status());
    plaintext
}

// ---------------------------------------------------------------------------
// Test: Suffix range (bytes=-N) on a chunked SSE object.
//
// Regression guard: `meta.size` for a single-PUT SSE object is the CIPHERTEXT
// size (plaintext + 16 per chunk). A suffix range resolved against ciphertext
// size would overshoot the plaintext end. The range must be sized in plaintext
// coordinates (from the sidecar), so bytes=-1000000 returns the last 1,000,000
// PLAINTEXT bytes.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_chunked_sse_suffix_range() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;
    let bucket = format!("sse-suffix-{}", uuid::Uuid::new_v4());

    let plaintext = put_6mib_sse_object(&http, base_url, &bucket, "obj").await;
    let total = plaintext.len(); // 6_291_456
    let suffix = 1_000_000usize;
    let expected_start = total - suffix; // 5_291_456

    let resp = http
        .get(format!("{}/{}/obj", base_url, bucket))
        .header("Range", format!("bytes=-{}", suffix))
        .send()
        .await
        .expect("suffix range-GET should complete");
    assert_eq!(resp.status(), 206, "suffix range must be 206");

    let content_range = resp
        .headers()
        .get("Content-Range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert_eq!(
        content_range,
        format!("bytes {}-{}/{}", expected_start, total - 1, total),
        "Content-Range must be in plaintext coordinates"
    );

    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), suffix, "suffix length mismatch");
    assert_eq!(
        body.as_ref(),
        &plaintext[expected_start..],
        "suffix body must equal the last {} plaintext bytes",
        suffix
    );
}

// ---------------------------------------------------------------------------
// Test: Open-ended range (bytes=A-) on a chunked SSE object.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_chunked_sse_open_ended_range() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;
    let bucket = format!("sse-openend-{}", uuid::Uuid::new_v4());

    let plaintext = put_6mib_sse_object(&http, base_url, &bucket, "obj").await;
    let total = plaintext.len();
    let start = 6_000_000usize;

    let resp = http
        .get(format!("{}/{}/obj", base_url, bucket))
        .header("Range", format!("bytes={}-", start))
        .send()
        .await
        .expect("open-ended range-GET should complete");
    assert_eq!(resp.status(), 206, "open-ended range must be 206");

    let content_range = resp
        .headers()
        .get("Content-Range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert_eq!(
        content_range,
        format!("bytes {}-{}/{}", start, total - 1, total)
    );

    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), total - start);
    assert_eq!(body.as_ref(), &plaintext[start..]);
}

// ---------------------------------------------------------------------------
// Test: Range entirely inside the SECOND chunk (non-zero file_start).
//
// This is the high-value seekable regression guard: only chunk 1 is read from
// disk, and `decrypt_chunked_range_from_slice` must index the ciphertext slice
// relative to chunk 1's file offset (not absolute 0).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_chunked_sse_second_chunk_only_range() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;
    let bucket = format!("sse-chunk1-{}", uuid::Uuid::new_v4());

    let plaintext = put_6mib_sse_object(&http, base_url, &bucket, "obj").await;
    let total = plaintext.len();
    // Both endpoints are past the 5 MiB (5_242_880) chunk boundary → chunk 1 only.
    let start = 5_300_000usize;
    let end_inclusive = 5_400_000usize;

    let resp = http
        .get(format!("{}/{}/obj", base_url, bucket))
        .header("Range", format!("bytes={}-{}", start, end_inclusive))
        .send()
        .await
        .expect("range-GET should complete");
    assert_eq!(resp.status(), 206);

    let content_range = resp
        .headers()
        .get("Content-Range")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    assert_eq!(
        content_range,
        format!("bytes {}-{}/{}", start, end_inclusive, total)
    );

    let body = resp.bytes().await.expect("read body");
    assert_eq!(body.len(), end_inclusive - start + 1);
    assert_eq!(
        body.as_ref(),
        &plaintext[start..=end_inclusive],
        "second-chunk range body must match plaintext slice"
    );
}

// ---------------------------------------------------------------------------
// Test: Empty (0-byte) SSE object — full GET returns 200 empty, any range 416.
//
// An empty SSE-S3 object encrypts to 0 ciphertext bytes with no chunks; the GET
// path must short-circuit to an empty body (the single-shot decrypt would
// otherwise be handed an empty nonce).
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_empty_sse_object_get_and_range() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;
    let bucket = format!("sse-empty-{}", uuid::Uuid::new_v4());

    let put_bucket = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(put_bucket.status().is_success());

    let put = http
        .put(format!("{}/{}/empty", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .body(Vec::<u8>::new())
        .send()
        .await
        .expect("PUT empty should complete");
    assert_eq!(put.status(), 200, "PUT empty SSE object should be 200");

    // Full GET → 200, empty body.
    let get = http
        .get(format!("{}/{}/empty", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");
    assert_eq!(get.status(), 200, "full GET of empty SSE object → 200");
    assert_eq!(
        get.headers()
            .get("x-amz-server-side-encryption")
            .and_then(|v| v.to_str().ok()),
        Some("AES256")
    );
    let body = get.bytes().await.expect("read body");
    assert_eq!(body.len(), 0, "empty SSE object body must be empty");

    // Any range on a 0-byte object → 416 Range Not Satisfiable.
    let range = http
        .get(format!("{}/{}/empty", base_url, bucket))
        .header("Range", "bytes=0-0")
        .send()
        .await
        .expect("range-GET should complete");
    assert_eq!(
        range.status(),
        416,
        "range on empty object → 416, got {}",
        range.status()
    );
}

// ---------------------------------------------------------------------------
// Test: D2 — full-object GET of a chunked SSE object with a stored SHA-256
// checksum runs decrypt-then-hash validation and succeeds for valid data.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_chunked_sse_full_get_checksum_validated() {
    use sha2::Digest;

    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let base_url = &server.base_url;
    let bucket = format!("sse-d2-{}", uuid::Uuid::new_v4());

    let put_bucket = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(put_bucket.status().is_success());

    let size = 6 * 1024 * 1024usize;
    let plaintext: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    let checksum_b64 =
        base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(&plaintext));

    let put = http
        .put(format!("{}/{}/obj", base_url, bucket))
        .header("x-amz-server-side-encryption", "AES256")
        .header("x-amz-checksum-sha256", &checksum_b64)
        .body(plaintext.clone())
        .send()
        .await
        .expect("PUT should complete");
    assert_eq!(put.status(), 200, "PUT returned {}", put.status());

    // Full GET: D2 decrypt-then-hash must pass and return the object.
    let get = http
        .get(format!("{}/{}/obj", base_url, bucket))
        .send()
        .await
        .expect("GET should complete");
    assert_eq!(
        get.status(),
        200,
        "full GET with valid SHA-256 checksum must pass D2 validation"
    );
    assert_eq!(
        get.headers()
            .get("x-amz-checksum-sha256")
            .and_then(|v| v.to_str().ok()),
        Some(checksum_b64.as_str()),
        "GET must echo the stored SHA-256 checksum"
    );
    let body = get.bytes().await.expect("read body");
    assert_eq!(body.as_ref(), plaintext.as_slice(), "body round-trips");
}

// ---------------------------------------------------------------------------
// Test: Range-GET on a multipart SSE-S3 object (v1 single-shot fallback).
//
// Multipart-SSE objects are encrypted single-shot (empty `chunks`), so the
// seekable path does not apply; this proves the v1 full-read-then-slice
// fallback returns the correct plaintext slice.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_multipart_sse_range_get() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-mp-range-{}", uuid::Uuid::new_v4());
    let key = "mp-range-object";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    let part1: Vec<u8> = (0u8..=255u8).collect(); // 256 bytes
    let part2: Vec<u8> = (0u8..128u8).rev().collect(); // 128 bytes
    let mut full = part1.clone();
    full.extend_from_slice(&part2);

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let create_resp = http
        .post(format!("{}/{}/{}?uploads", base_url, bucket, key))
        .header("x-amz-server-side-encryption", "AES256")
        .send()
        .await
        .expect("CreateMultipartUpload");
    assert_eq!(create_resp.status(), 200);
    let xml_body = create_resp.text().await.expect("read body");
    let upload_id = {
        let start = xml_body.find("<UploadId>").expect("UploadId") + "<UploadId>".len();
        let end = xml_body.find("</UploadId>").expect("/UploadId");
        xml_body[start..end].to_string()
    };

    let up1 = http
        .put(format!(
            "{}/{}/{}?partNumber=1&uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .body(part1.clone())
        .send()
        .await
        .expect("UploadPart 1");
    let etag1 = up1
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .expect("ETag 1")
        .trim_matches('"')
        .to_string();
    let up2 = http
        .put(format!(
            "{}/{}/{}?partNumber=2&uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .body(part2.clone())
        .send()
        .await
        .expect("UploadPart 2");
    let etag2 = up2
        .headers()
        .get("ETag")
        .and_then(|v| v.to_str().ok())
        .expect("ETag 2")
        .trim_matches('"')
        .to_string();

    let complete_xml = format!(
        r#"<CompleteMultipartUpload>
  <Part><PartNumber>1</PartNumber><ETag>"{}"</ETag></Part>
  <Part><PartNumber>2</PartNumber><ETag>"{}"</ETag></Part>
</CompleteMultipartUpload>"#,
        etag1, etag2
    );
    let complete = http
        .post(format!(
            "{}/{}/{}?uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .header("Content-Type", "application/xml")
        .body(complete_xml)
        .send()
        .await
        .expect("CompleteMultipartUpload");
    assert_eq!(
        complete.status(),
        200,
        "complete returned {}",
        complete.status()
    );

    // Range-GET bytes=10-20 on a small (single-chunk) multipart SSE object. Multipart
    // objects are now post-encrypted with the chunked (v2) format, so this goes through
    // the seekable chunk path; the 384-byte object has a single chunk, so the covering
    // read happens to be the whole (tiny) object. See `test_multipart_sse_seekable_multichunk`
    // for the multi-chunk case that actually skips chunks.
    let resp = http
        .get(format!("{}/{}/{}", base_url, bucket, key))
        .header("Range", "bytes=10-20")
        .send()
        .await
        .expect("range-GET should complete");
    assert_eq!(resp.status(), 206, "multipart SSE range must be 206");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(
        body.as_ref(),
        &full[10..=20],
        "multipart SSE range slice must match plaintext"
    );
}

// ---------------------------------------------------------------------------
// Helper: multipart-SSE upload of `n_parts` parts of `part_size` bytes each.
//
// Returns the full assembled plaintext. The object is initiated with AES256, so
// CompleteMultipartUpload post-encrypts the assembled plaintext with the chunked
// (v2) format — which is what makes multipart objects seekable.
// ---------------------------------------------------------------------------
async fn put_multipart_sse_object(
    http: &reqwest::Client,
    base_url: &str,
    bucket: &str,
    key: &str,
    part_size: usize,
    n_parts: usize,
) -> Vec<u8> {
    let put_bucket = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(put_bucket.status().is_success(), "create bucket");

    let create_resp = http
        .post(format!("{}/{}/{}?uploads", base_url, bucket, key))
        .header("x-amz-server-side-encryption", "AES256")
        .send()
        .await
        .expect("CreateMultipartUpload");
    assert_eq!(create_resp.status(), 200);
    let xml_body = create_resp.text().await.expect("read body");
    let upload_id = {
        let start = xml_body.find("<UploadId>").expect("UploadId") + "<UploadId>".len();
        let end = xml_body.find("</UploadId>").expect("/UploadId");
        xml_body[start..end].to_string()
    };

    let mut full = Vec::with_capacity(part_size * n_parts);
    let mut completed_parts = String::new();
    for part_number in 1..=n_parts {
        // Deterministic, part-distinct data so boundary slices are unambiguous.
        let part: Vec<u8> = (0..part_size)
            .map(|i| ((i + part_number * 37) % 251) as u8)
            .collect();
        full.extend_from_slice(&part);
        let up = http
            .put(format!(
                "{}/{}/{}?partNumber={}&uploadId={}",
                base_url, bucket, key, part_number, upload_id
            ))
            .body(part)
            .send()
            .await
            .expect("UploadPart");
        assert_eq!(up.status(), 200, "UploadPart {} status", part_number);
        let etag = up
            .headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .expect("ETag")
            .trim_matches('"')
            .to_string();
        completed_parts.push_str(&format!(
            "  <Part><PartNumber>{}</PartNumber><ETag>\"{}\"</ETag></Part>\n",
            part_number, etag
        ));
    }

    let complete_xml = format!(
        "<CompleteMultipartUpload>\n{}</CompleteMultipartUpload>",
        completed_parts
    );
    let complete = http
        .post(format!(
            "{}/{}/{}?uploadId={}",
            base_url, bucket, key, upload_id
        ))
        .header("Content-Type", "application/xml")
        .body(complete_xml)
        .send()
        .await
        .expect("CompleteMultipartUpload");
    assert_eq!(
        complete.status(),
        200,
        "CompleteMultipartUpload status {}",
        complete.status()
    );
    full
}

// ---------------------------------------------------------------------------
// Test: multipart-SSE objects are chunked (v2) and support seekable range-GET.
//
// A 6 MiB multipart upload (2 x 3 MiB parts) assembled and post-encrypted with the
// chunked format yields two AES-256-GCM chunks (5 MiB + 1 MiB). A range-GET landing
// entirely in the second chunk must read only that chunk's ciphertext (seekable),
// and a range crossing the 5 MiB chunk boundary must stitch the two chunks. HEAD and
// full-GET must report the PLAINTEXT size (6_291_456), not the larger ciphertext size.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_multipart_sse_seekable_multichunk() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-mp-multichunk-{}", uuid::Uuid::new_v4());
    let key = "mp-multichunk-object";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    const THREE_MB: usize = 3 * 1024 * 1024;
    let full = put_multipart_sse_object(&http, base_url, &bucket, key, THREE_MB, 2).await;
    assert_eq!(full.len(), 6 * 1024 * 1024, "assembled plaintext size");

    // HEAD reports the plaintext size (not the on-disk ciphertext size).
    let head = http
        .head(format!("{}/{}/{}", base_url, bucket, key))
        .send()
        .await
        .expect("HEAD");
    assert_eq!(head.status(), 200);
    assert_eq!(
        head.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()),
        Some("6291456"),
        "HEAD Content-Length must be the plaintext size"
    );

    // Range entirely within the second chunk ([5 MiB, 6 MiB)) — exercises a seekable
    // read of only the second chunk's ciphertext (the whole point of chunked multipart).
    let resp = http
        .get(format!("{}/{}/{}", base_url, bucket, key))
        .header("Range", "bytes=5400000-5400499")
        .send()
        .await
        .expect("range-GET within chunk 1");
    assert_eq!(resp.status(), 206);
    assert_eq!(
        resp.headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok()),
        Some("bytes 5400000-5400499/6291456"),
        "Content-Range denominator must be the plaintext size"
    );
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), &full[5400000..=5400499]);

    // Range crossing the 5 MiB chunk boundary (5_242_880) — stitches chunk 0 + chunk 1.
    let resp = http
        .get(format!("{}/{}/{}", base_url, bucket, key))
        .header("Range", "bytes=5242875-5242884")
        .send()
        .await
        .expect("boundary-crossing range-GET");
    assert_eq!(resp.status(), 206);
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.as_ref(), &full[5242875..=5242884]);

    // Full GET returns the whole plaintext.
    let resp = http
        .get(format!("{}/{}/{}", base_url, bucket, key))
        .send()
        .await
        .expect("full GET");
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()),
        Some("6291456"),
    );
    let body = resp.bytes().await.expect("body");
    assert_eq!(body.len(), full.len());
    assert_eq!(body.as_ref(), full.as_slice());
}

// ---------------------------------------------------------------------------
// Test: HEAD on a single-PUT SSE object reports the PLAINTEXT size.
//
// Regression guard for the HEAD/GET size inconsistency: PUT stores ciphertext
// (plaintext + 16-byte GCM tag per chunk) and `meta.size` is the on-disk ciphertext
// length. HEAD must report the plaintext size (sidecar-derived), matching GET.
// ---------------------------------------------------------------------------
#[tokio::test]
async fn test_head_sse_reports_plaintext_size() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("sse-head-size-{}", uuid::Uuid::new_v4());
    let key = "head-size-object";
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    let put_bucket = http
        .put(format!("{}/{}", base_url, bucket))
        .send()
        .await
        .expect("create bucket");
    assert!(put_bucket.status().is_success());

    let plaintext: Vec<u8> = (0..1000usize).map(|i| (i % 251) as u8).collect();
    let put = http
        .put(format!("{}/{}/{}", base_url, bucket, key))
        .header("x-amz-server-side-encryption", "AES256")
        .body(plaintext.clone())
        .send()
        .await
        .expect("PUT");
    assert_eq!(put.status(), 200);

    let head = http
        .head(format!("{}/{}/{}", base_url, bucket, key))
        .send()
        .await
        .expect("HEAD");
    assert_eq!(head.status(), 200);
    assert_eq!(
        head.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()),
        Some("1000"),
        "HEAD Content-Length must equal the plaintext size, not the ciphertext size"
    );

    // GET must agree on size and content.
    let get = http
        .get(format!("{}/{}/{}", base_url, bucket, key))
        .send()
        .await
        .expect("GET");
    assert_eq!(get.status(), 200);
    assert_eq!(
        get.headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()),
        Some("1000"),
    );
    let body = get.bytes().await.expect("body");
    assert_eq!(body.as_ref(), plaintext.as_slice());
}
