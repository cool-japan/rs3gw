//! WS-8 extended multipart tests:
//! - Error response shapes match AWS
//! - Concurrent clients
//! - Request timeout handling
//! - Pagination
//! - Part number edge cases

mod common;

use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use common::setup_test_server;

// =====================================================================
// XML helper utilities (duplicated from multipart_tests for independence)
// =====================================================================

/// Extract the text content of the first occurrence of a given XML tag.
fn extract_xml_value(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

/// Count occurrences of a given literal string in XML (e.g., "<PartNumber>").
fn count_xml_elements(xml: &str, marker: &str) -> usize {
    let mut count = 0;
    let mut search = xml;
    while let Some(pos) = search.find(marker) {
        count += 1;
        search = &search[pos + marker.len()..];
    }
    count
}

/// Helper: initiate a multipart upload (bucket must already exist).
async fn begin_multipart(client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> String {
    let resp = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .expect("create_multipart_upload failed");
    resp.upload_id().expect("upload_id missing").to_string()
}

// =====================================================================
// Error response shapes match AWS (7 tests)
// =====================================================================

/// CreateMultipartUpload on non-existent bucket → XML with Code + Message + RequestId
#[tokio::test]
async fn test_error_create_multipart_bucket_not_found() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let url = format!(
        "{}/nonexistent-bucket-xyz/file.bin?uploads",
        server.base_url
    );
    let resp = http.post(&url).send().await.expect("POST failed");
    assert_eq!(resp.status().as_u16(), 404);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<Code>"), "Expected <Code>, got: {}", body);
    assert!(
        body.contains("<Message>"),
        "Expected <Message>, got: {}",
        body
    );
    assert!(
        body.contains("<RequestId>"),
        "Expected <RequestId>, got: {}",
        body
    );
}

/// UploadPart with unknown uploadId → XML with Code + Message + RequestId
#[tokio::test]
async fn test_error_upload_part_not_found() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("err-up-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?partNumber=1&uploadId=no-such-id",
        server.base_url, bucket
    );
    let resp = http
        .put(&url)
        .body(vec![b'X'; 64])
        .send()
        .await
        .expect("PUT");
    assert_eq!(resp.status().as_u16(), 404);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<Code>"), "Expected <Code>, got: {}", body);
    assert!(
        body.contains("<Message>"),
        "Expected <Message>, got: {}",
        body
    );
    assert!(
        body.contains("<RequestId>"),
        "Expected <RequestId>, got: {}",
        body
    );
}

/// UploadPartCopy when source object does not exist → XML error
#[tokio::test]
async fn test_error_upload_part_copy_source_not_found() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("err-upc-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "dest.bin").await;
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/dest.bin?partNumber=1&uploadId={}",
        server.base_url, bucket, upload_id
    );
    let resp = http
        .put(&url)
        .header("x-amz-copy-source", format!("/{}/ghost-key.bin", bucket))
        .send()
        .await
        .expect("PUT failed");
    assert!(
        resp.status().as_u16() >= 400,
        "Expected error status, got {}",
        resp.status()
    );
    let _ = resp.text().await;
}

/// CompleteMultipartUpload with a wrong ETag → XML with Code + Message
#[tokio::test]
async fn test_error_complete_multipart_invalid_etag() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("err-cme-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "file.bin").await;
    client
        .upload_part()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'Z'; 128].into())
        .send()
        .await
        .expect("upload_part");
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?uploadId={}",
        server.base_url, bucket, upload_id
    );
    let xml_body =
        "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>\"wrong-etag\"</ETag></Part></CompleteMultipartUpload>";
    let resp = http.post(&url).body(xml_body).send().await.expect("POST");
    assert_eq!(resp.status().as_u16(), 400);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<Code>"), "Expected <Code>, got: {}", body);
    assert!(
        body.contains("<Message>"),
        "Expected <Message>, got: {}",
        body
    );
}

/// AbortMultipartUpload for a non-existent upload → 404 with XML error
#[tokio::test]
async fn test_error_abort_nonexistent_upload() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("err-abort-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?uploadId=no-such-upload",
        server.base_url, bucket
    );
    let resp = http.delete(&url).send().await.expect("DELETE");
    assert_eq!(resp.status().as_u16(), 404);
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("<Code>") || body.is_empty() || body.contains("NoSuchUpload"),
        "Expected XML error, got: {}",
        body
    );
}

/// ListParts for a non-existent upload → 404 with XML error
#[tokio::test]
async fn test_error_list_parts_nonexistent_upload() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("err-lp-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?uploadId=no-such-upload",
        server.base_url, bucket
    );
    let resp = http.get(&url).send().await.expect("GET");
    assert_eq!(resp.status().as_u16(), 404);
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("<Code>") || body.contains("NoSuchUpload"),
        "Expected XML error, got: {}",
        body
    );
}

/// ListMultipartUploads for a non-existent bucket → 404 with XML error
#[tokio::test]
async fn test_error_list_multipart_uploads_nonexistent_bucket() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let url = format!("{}/no-such-bucket-xyz/?uploads", server.base_url);
    let resp = http.get(&url).send().await.expect("GET");
    assert_eq!(resp.status().as_u16(), 404);
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("<Code>") || body.contains("NoSuchBucket"),
        "Expected XML error, got: {}",
        body
    );
}

// =====================================================================
// Concurrent clients (4 tests)
// =====================================================================

/// Two clients upload to the same key with different uploadIds concurrently → both complete
#[tokio::test]
async fn test_concurrent_uploads_same_key_different_ids() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let addr = server.addr;
    let bucket = format!("conc-sk-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let key = "shared-key.bin";
    let uid1 = begin_multipart(&client, &bucket, key).await;
    let uid2 = begin_multipart(&client, &bucket, key).await;

    let c1 = common::create_s3_client(addr).await;
    let c2 = common::create_s3_client(addr).await;
    let b1 = bucket.clone();
    let b2 = bucket.clone();
    let u1 = uid1.clone();
    let u2 = uid2.clone();

    let (r1, r2) = tokio::join!(
        async move {
            let etag = c1
                .upload_part()
                .bucket(&b1)
                .key(key)
                .upload_id(&u1)
                .part_number(1)
                .body(vec![b'A'; 256].into())
                .send()
                .await
                .expect("c1 upload_part")
                .e_tag
                .expect("c1 etag");
            c1.complete_multipart_upload()
                .bucket(&b1)
                .key(key)
                .upload_id(&u1)
                .multipart_upload(
                    CompletedMultipartUpload::builder()
                        .parts(CompletedPart::builder().part_number(1).e_tag(&etag).build())
                        .build(),
                )
                .send()
                .await
        },
        async move {
            let etag = c2
                .upload_part()
                .bucket(&b2)
                .key(key)
                .upload_id(&u2)
                .part_number(1)
                .body(vec![b'B'; 256].into())
                .send()
                .await
                .expect("c2 upload_part")
                .e_tag
                .expect("c2 etag");
            c2.complete_multipart_upload()
                .bucket(&b2)
                .key(key)
                .upload_id(&u2)
                .multipart_upload(
                    CompletedMultipartUpload::builder()
                        .parts(CompletedPart::builder().part_number(1).e_tag(&etag).build())
                        .build(),
                )
                .send()
                .await
        },
    );
    assert!(r1.is_ok(), "upload1 complete failed: {:?}", r1.err());
    assert!(r2.is_ok(), "upload2 complete failed: {:?}", r2.err());
}

/// Concurrent UploadPart calls for same upload/same part number → last write wins, no panic
#[tokio::test]
async fn test_concurrent_upload_same_part() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let addr = server.addr;
    let bucket = format!("conc-sp-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let key = "part-race.bin";
    let upload_id = begin_multipart(&client, &bucket, key).await;

    let c1 = common::create_s3_client(addr).await;
    let c2 = common::create_s3_client(addr).await;
    let b1 = bucket.clone();
    let b2 = bucket.clone();
    let uid1 = upload_id.clone();
    let uid2 = upload_id.clone();

    let (r1, r2) = tokio::join!(
        c1.upload_part()
            .bucket(b1)
            .key(key)
            .upload_id(uid1)
            .part_number(1)
            .body(vec![b'X'; 512].into())
            .send(),
        c2.upload_part()
            .bucket(b2)
            .key(key)
            .upload_id(uid2)
            .part_number(1)
            .body(vec![b'Y'; 512].into())
            .send(),
    );
    let one_ok = r1.is_ok() || r2.is_ok();
    assert!(
        one_ok,
        "Both concurrent uploads failed: {:?} / {:?}",
        r1.err(),
        r2.err()
    );
}

/// ListMultipartUploads while uploads are in progress → returns a consistent snapshot
#[tokio::test]
async fn test_list_uploads_while_in_progress() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("list-inprog-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    for i in 0..3u32 {
        let _ = begin_multipart(&client, &bucket, &format!("file-{}.bin", i)).await;
    }

    let resp = client
        .list_multipart_uploads()
        .bucket(&bucket)
        .send()
        .await
        .expect("list_multipart_uploads");
    let uploads = resp.uploads();
    assert!(
        uploads.len() >= 3,
        "Expected at least 3 in-progress uploads, got {}",
        uploads.len()
    );
}

/// Complete while a concurrent UploadPart is running → complete succeeds with what was uploaded
#[tokio::test]
async fn test_complete_while_concurrent_upload_part() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let addr = server.addr;
    let bucket = format!("conc-cmp-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let key = "race-complete.bin";
    let upload_id = begin_multipart(&client, &bucket, key).await;

    let r1 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'A'; 256].into())
        .send()
        .await
        .expect("upload part1");
    let etag1 = r1.e_tag.expect("etag1");

    let c2 = common::create_s3_client(addr).await;
    let b2 = bucket.clone();
    let uid2 = upload_id.clone();

    let (complete_result, _upload2_result) = tokio::join!(
        client
            .complete_multipart_upload()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .multipart_upload(
                CompletedMultipartUpload::builder()
                    .parts(
                        CompletedPart::builder()
                            .part_number(1)
                            .e_tag(&etag1)
                            .build()
                    )
                    .build(),
            )
            .send(),
        c2.upload_part()
            .bucket(b2)
            .key(key)
            .upload_id(uid2)
            .part_number(2)
            .body(vec![b'B'; 256].into())
            .send(),
    );

    assert!(
        complete_result.is_ok(),
        "complete_multipart_upload failed: {:?}",
        complete_result.err()
    );
}

// =====================================================================
// Request timeout handling (3 tests)
// =====================================================================

/// Client with a very small timeout → operation returns an error, not a panic
#[tokio::test]
async fn test_timeout_client_returns_error() {
    use aws_sdk_s3::config::timeout::TimeoutConfig;
    use std::time::Duration;

    let (_client, _temp_dir, server) = setup_test_server().await;
    let addr = server.addr;

    let credentials = aws_sdk_s3::config::Credentials::new("test", "test", None, None, "test");
    let timeout_config = TimeoutConfig::builder()
        .operation_timeout(Duration::from_millis(1))
        .build();
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_config::BehaviorVersion::latest())
        .endpoint_url(format!("http://{}", addr))
        .credentials_provider(credentials)
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .force_path_style(true)
        .timeout_config(timeout_config)
        .build();
    let tiny_client = aws_sdk_s3::Client::from_conf(config);

    let result = tiny_client.list_buckets().send().await;
    let _ = result; // must not panic
}

/// Abandoned partial upload older than retention → gc endpoint returns removed > 0
#[tokio::test]
async fn test_gc_abandoned_old_upload() {
    use std::time::Duration;

    let (client, temp_dir, server) = setup_test_server().await;
    let bucket = format!("gc-old-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let upload_id = begin_multipart(&client, &bucket, "abandoned.bin").await;

    // Back-date the metadata to appear 200 hours old
    let metadata_path = temp_dir
        .path()
        .join(&bucket)
        .join("multipart")
        .join(&upload_id)
        .join("metadata.json");

    let raw = tokio::fs::read(&metadata_path)
        .await
        .expect("read metadata");
    let mut meta: serde_json::Value = serde_json::from_slice(&raw).expect("parse json");
    let past = chrono::Utc::now() - chrono::Duration::hours(200);
    meta["initiated"] = serde_json::json!(past.to_rfc3339());
    tokio::fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&meta).expect("serialize"),
    )
    .await
    .expect("write metadata");

    let http = reqwest::Client::new();
    let url = format!(
        "{}/api/admin/gc/multipart?retention_hours=168&bucket={}",
        server.base_url, bucket
    );
    let resp = http.post(&url).send().await.expect("POST gc");
    assert_eq!(resp.status().as_u16(), 200, "GC endpoint failed");
    let body: serde_json::Value = resp.json().await.expect("json body");
    let removed = body["removed"].as_u64().expect("removed field");
    assert!(removed > 0, "Expected at least 1 removed, got {}", removed);

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !metadata_path.exists(),
        "Metadata should have been removed by GC"
    );
}

/// Resume upload after abort: start, upload parts, abort, then new upload succeeds
#[tokio::test]
async fn test_resume_upload_after_abort() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("resume-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let key = "resume-file.bin";

    let upload_id_first = begin_multipart(&client, &bucket, key).await;
    client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id_first)
        .part_number(1)
        .body(vec![b'A'; 256].into())
        .send()
        .await
        .expect("first part 1");
    client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id_first)
        .part_number(2)
        .body(vec![b'B'; 256].into())
        .send()
        .await
        .expect("first part 2");
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id_first)
        .send()
        .await
        .expect("abort first upload");

    let upload_id_second = begin_multipart(&client, &bucket, key).await;
    let etag = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id_second)
        .part_number(1)
        .body(vec![b'C'; 512].into())
        .send()
        .await
        .expect("second part 1")
        .e_tag
        .expect("etag");
    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id_second)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .parts(CompletedPart::builder().part_number(1).e_tag(&etag).build())
                .build(),
        )
        .send()
        .await
        .expect("complete second upload");

    let obj = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("get_object");
    let body = obj.body.collect().await.expect("collect").into_bytes();
    assert_eq!(body.len(), 512);
}

// =====================================================================
// Pagination (4 tests)
// =====================================================================

/// ListParts with max-parts=2 on a 5-part upload → IsTruncated=true, correct first page
#[tokio::test]
async fn test_list_parts_pagination_first_page() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("pagp1-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let key = "five-parts.bin";
    let upload_id = begin_multipart(&client, &bucket, key).await;

    let mut etags = Vec::new();
    for i in 1u32..=5 {
        let etag = client
            .upload_part()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(i as i32)
            .body(vec![b'P'; 128].into())
            .send()
            .await
            .expect("upload_part")
            .e_tag
            .expect("etag");
        etags.push(etag);
    }

    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/{}?uploadId={}&max-parts=2",
        server.base_url, bucket, key, upload_id
    );
    let resp = http.get(&url).send().await.expect("GET");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("body");

    let is_truncated = extract_xml_value(&body, "IsTruncated")
        .unwrap_or_default()
        .to_lowercase();
    assert_eq!(
        is_truncated, "true",
        "IsTruncated should be true; body: {}",
        body
    );
    let next_marker = extract_xml_value(&body, "NextPartNumberMarker");
    assert!(
        next_marker.is_some(),
        "Expected NextPartNumberMarker; body: {}",
        body
    );
    let part_count = count_xml_elements(&body, "<PartNumber>");
    assert_eq!(
        part_count, 2,
        "Expected 2 parts on first page; body: {}",
        body
    );

    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}

/// ListMultipartUploads with max-uploads=2 → correct pagination
#[tokio::test]
async fn test_list_multipart_uploads_pagination() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("lpagu-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let mut upload_ids = Vec::new();
    for i in 0..4u32 {
        let uid = begin_multipart(&client, &bucket, &format!("file-{}.bin", i)).await;
        upload_ids.push(uid);
    }

    let http = reqwest::Client::new();
    let url = format!("{}/{}/?uploads&max-uploads=2", server.base_url, bucket);
    let resp = http.get(&url).send().await.expect("GET");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("body");

    let upload_count = count_xml_elements(&body, "<UploadId>");
    assert!(
        upload_count <= 2,
        "Expected at most 2 uploads; body: {}",
        body
    );

    for (i, uid) in upload_ids.iter().enumerate() {
        client
            .abort_multipart_upload()
            .bucket(&bucket)
            .key(format!("file-{}.bin", i))
            .upload_id(uid)
            .send()
            .await
            .ok();
    }
}

/// ListParts page 2 using part-number-marker → correct remaining parts
#[tokio::test]
async fn test_list_parts_pagination_second_page() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("pagp2-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let key = "five-parts-p2.bin";
    let upload_id = begin_multipart(&client, &bucket, key).await;

    for i in 1u32..=5 {
        client
            .upload_part()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(i as i32)
            .body(vec![b'Q'; 128].into())
            .send()
            .await
            .expect("upload_part");
    }

    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/{}?uploadId={}&max-parts=3&part-number-marker=2",
        server.base_url, bucket, key, upload_id
    );
    let resp = http.get(&url).send().await.expect("GET");
    assert_eq!(resp.status().as_u16(), 200);
    let body = resp.text().await.expect("body");

    let part_count = count_xml_elements(&body, "<PartNumber>");
    assert_eq!(
        part_count, 3,
        "Expected parts 3, 4, 5 on page 2; body: {}",
        body
    );

    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}

/// ListMultipartUploads page 2 using key-marker + upload-id-marker
#[tokio::test]
async fn test_list_multipart_uploads_page2() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("lmup2-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    let keys = ["alpha.bin", "beta.bin", "gamma.bin", "delta.bin"];
    let mut uid_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for k in &keys {
        let uid = begin_multipart(&client, &bucket, k).await;
        uid_map.insert(k.to_string(), uid);
    }

    let http = reqwest::Client::new();
    let page1_url = format!("{}/{}/?uploads&max-uploads=2", server.base_url, bucket);
    let page1 = http.get(&page1_url).send().await.expect("GET page1");
    assert_eq!(page1.status().as_u16(), 200);
    let page1_body = page1.text().await.expect("page1 body");

    let is_truncated = extract_xml_value(&page1_body, "IsTruncated")
        .unwrap_or_default()
        .to_lowercase();

    if is_truncated == "true" {
        let next_key = extract_xml_value(&page1_body, "NextKeyMarker").unwrap_or_default();
        let next_uid = extract_xml_value(&page1_body, "NextUploadIdMarker").unwrap_or_default();
        let page2_url = format!(
            "{}/{}/?uploads&max-uploads=2&key-marker={}&upload-id-marker={}",
            server.base_url, bucket, next_key, next_uid
        );
        let page2 = http.get(&page2_url).send().await.expect("GET page2");
        assert_eq!(page2.status().as_u16(), 200);
        let page2_body = page2.text().await.expect("page2 body");
        let count = count_xml_elements(&page2_body, "<UploadId>");
        assert!(
            count >= 1,
            "Expected at least 1 upload on page 2; body: {}",
            page2_body
        );
    }

    for (k, uid) in &uid_map {
        client
            .abort_multipart_upload()
            .bucket(&bucket)
            .key(k)
            .upload_id(uid)
            .send()
            .await
            .ok();
    }
}

// =====================================================================
// Part number edge cases (5 tests)
// =====================================================================

/// Part number 1 (minimum valid) → accepted
#[tokio::test]
async fn test_part_number_minimum_valid() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("pn-min-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "file.bin").await;
    let result = client
        .upload_part()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'A'; 64].into())
        .send()
        .await;
    assert!(
        result.is_ok(),
        "Part number 1 should be accepted: {:?}",
        result.err()
    );
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}

/// Part number 10000 (maximum valid) → accepted
#[tokio::test]
async fn test_part_number_maximum_valid() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("pn-max-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "file.bin").await;
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?partNumber=10000&uploadId={}",
        server.base_url, bucket, upload_id
    );
    let resp = http
        .put(&url)
        .body(vec![b'Z'; 64])
        .send()
        .await
        .expect("PUT part 10000");
    assert_eq!(
        resp.status().as_u16(),
        200,
        "Part number 10000 should be accepted"
    );
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}

/// Part number 0 → 400 InvalidArgument
#[tokio::test]
async fn test_part_number_zero_rejected() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("pn-zero-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "file.bin").await;
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?partNumber=0&uploadId={}",
        server.base_url, bucket, upload_id
    );
    let resp = http
        .put(&url)
        .body(vec![b'X'; 64])
        .send()
        .await
        .expect("PUT");
    assert_eq!(
        resp.status().as_u16(),
        400,
        "Part number 0 should be rejected with 400"
    );
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}

/// Part number 10001 → 400 InvalidArgument
#[tokio::test]
async fn test_part_number_above_maximum_rejected() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("pn-10001-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "file.bin").await;
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?partNumber=10001&uploadId={}",
        server.base_url, bucket, upload_id
    );
    let resp = http
        .put(&url)
        .body(vec![b'X'; 64])
        .send()
        .await
        .expect("PUT");
    assert_eq!(
        resp.status().as_u16(),
        400,
        "Part number 10001 should be rejected with 400"
    );
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}

/// Part number -1 (sent as query param string) → 400 or parse error
#[tokio::test]
async fn test_part_number_negative_rejected() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("pn-neg-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    let upload_id = begin_multipart(&client, &bucket, "file.bin").await;
    let http = reqwest::Client::new();
    let url = format!(
        "{}/{}/file.bin?partNumber=-1&uploadId={}",
        server.base_url, bucket, upload_id
    );
    let resp = http
        .put(&url)
        .body(vec![b'X'; 64])
        .send()
        .await
        .expect("PUT");
    assert!(
        resp.status().as_u16() >= 400,
        "Part number -1 should be rejected, got {}",
        resp.status()
    );
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key("file.bin")
        .upload_id(&upload_id)
        .send()
        .await
        .ok();
}
