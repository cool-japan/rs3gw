#![cfg(feature = "server")]
//! Multipart upload tests for rs3gw

mod common;

use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use common::setup_test_server;

#[tokio::test]
async fn test_multipart_upload() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("multipart-test")
        .send()
        .await
        .unwrap();

    // Initiate multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket("multipart-test")
        .key("large-file.bin")
        .content_type("application/octet-stream")
        .send()
        .await;
    assert!(
        create_result.is_ok(),
        "Failed to create multipart upload: {:?}",
        create_result.err()
    );
    let upload = create_result.unwrap();
    let upload_id = upload.upload_id().unwrap();

    // Upload parts (minimum 5MB per part except last, but for test we use smaller)
    let part1 = vec![b'A'; 1024];
    let part2 = vec![b'B'; 1024];
    let part3 = vec![b'C'; 512];

    let part1_result = client
        .upload_part()
        .bucket("multipart-test")
        .key("large-file.bin")
        .upload_id(upload_id)
        .part_number(1)
        .body(part1.clone().into())
        .send()
        .await;
    assert!(part1_result.is_ok(), "Failed to upload part 1");
    let part1_etag = part1_result.unwrap().e_tag.unwrap();

    let part2_result = client
        .upload_part()
        .bucket("multipart-test")
        .key("large-file.bin")
        .upload_id(upload_id)
        .part_number(2)
        .body(part2.clone().into())
        .send()
        .await;
    assert!(part2_result.is_ok(), "Failed to upload part 2");
    let part2_etag = part2_result.unwrap().e_tag.unwrap();

    let part3_result = client
        .upload_part()
        .bucket("multipart-test")
        .key("large-file.bin")
        .upload_id(upload_id)
        .part_number(3)
        .body(part3.clone().into())
        .send()
        .await;
    assert!(part3_result.is_ok(), "Failed to upload part 3");
    let part3_etag = part3_result.unwrap().e_tag.unwrap();

    // List parts
    let list_parts_result = client
        .list_parts()
        .bucket("multipart-test")
        .key("large-file.bin")
        .upload_id(upload_id)
        .send()
        .await;
    assert!(list_parts_result.is_ok(), "Failed to list parts");
    let parts_list = list_parts_result.unwrap();
    assert_eq!(parts_list.parts().len(), 3);

    // Complete multipart upload
    use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
    let completed = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&part1_etag)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(2)
                .e_tag(&part2_etag)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(3)
                .e_tag(&part3_etag)
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket("multipart-test")
        .key("large-file.bin")
        .upload_id(upload_id)
        .multipart_upload(completed)
        .send()
        .await;
    assert!(
        complete_result.is_ok(),
        "Failed to complete multipart upload: {:?}",
        complete_result.err()
    );

    // Verify the final object
    let get_result = client
        .get_object()
        .bucket("multipart-test")
        .key("large-file.bin")
        .send()
        .await;
    assert!(get_result.is_ok());
    let body = get_result.unwrap().body.collect().await.unwrap();
    let body_bytes = body.into_bytes();

    // Verify combined content
    let expected: Vec<u8> = [part1, part2, part3].concat();
    assert_eq!(body_bytes.len(), expected.len());
    assert_eq!(body_bytes.as_ref(), expected.as_slice());
}

#[tokio::test]
async fn test_abort_multipart_upload() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("abort-test")
        .send()
        .await
        .unwrap();

    // Initiate multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket("abort-test")
        .key("aborted-file.bin")
        .send()
        .await;
    assert!(create_result.is_ok());
    let upload = create_result.unwrap();
    let upload_id = upload.upload_id().unwrap();

    // Upload one part
    let part1 = vec![b'X'; 1024];
    let part1_result = client
        .upload_part()
        .bucket("abort-test")
        .key("aborted-file.bin")
        .upload_id(upload_id)
        .part_number(1)
        .body(part1.into())
        .send()
        .await;
    assert!(part1_result.is_ok());

    // Abort the upload
    let abort_result = client
        .abort_multipart_upload()
        .bucket("abort-test")
        .key("aborted-file.bin")
        .upload_id(upload_id)
        .send()
        .await;
    assert!(abort_result.is_ok(), "Failed to abort multipart upload");

    // Verify upload was aborted (listing parts should fail)
    let list_parts_result = client
        .list_parts()
        .bucket("abort-test")
        .key("aborted-file.bin")
        .upload_id(upload_id)
        .send()
        .await;
    assert!(
        list_parts_result.is_err(),
        "Upload should have been aborted"
    );
}

#[tokio::test]
async fn test_upload_part_copy() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("part-copy-test")
        .send()
        .await
        .unwrap();

    // Create a source object that we'll copy from
    let source_content = vec![b'S'; 2048]; // 2KB source object
    client
        .put_object()
        .bucket("part-copy-test")
        .key("source-object.bin")
        .body(source_content.clone().into())
        .send()
        .await
        .unwrap();

    // Create another source object for range copy
    let source2_content = vec![b'R'; 4096]; // 4KB source object
    client
        .put_object()
        .bucket("part-copy-test")
        .key("source-object2.bin")
        .body(source2_content.clone().into())
        .send()
        .await
        .unwrap();

    // Initiate multipart upload
    let create_result = client
        .create_multipart_upload()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .content_type("application/octet-stream")
        .send()
        .await;
    assert!(create_result.is_ok());
    let upload = create_result.unwrap();
    let upload_id = upload.upload_id().unwrap();

    // Upload part 1 by copying the entire first source object
    let copy1_result = client
        .upload_part_copy()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .upload_id(upload_id)
        .part_number(1)
        .copy_source("part-copy-test/source-object.bin")
        .send()
        .await;
    assert!(
        copy1_result.is_ok(),
        "Failed to copy part 1: {:?}",
        copy1_result.err()
    );
    let part1 = copy1_result.unwrap();
    let part1_etag = part1
        .copy_part_result()
        .unwrap()
        .e_tag()
        .unwrap()
        .to_string();

    // Upload part 2 by copying a range from the second source object (bytes 1024-2047)
    let copy2_result = client
        .upload_part_copy()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .upload_id(upload_id)
        .part_number(2)
        .copy_source("part-copy-test/source-object2.bin")
        .copy_source_range("bytes=1024-2047")
        .send()
        .await;
    assert!(
        copy2_result.is_ok(),
        "Failed to copy part 2 with range: {:?}",
        copy2_result.err()
    );
    let part2 = copy2_result.unwrap();
    let part2_etag = part2
        .copy_part_result()
        .unwrap()
        .e_tag()
        .unwrap()
        .to_string();

    // Upload part 3 as regular part (not a copy)
    let part3_content = vec![b'P'; 512];
    let part3_result = client
        .upload_part()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .upload_id(upload_id)
        .part_number(3)
        .body(part3_content.clone().into())
        .send()
        .await;
    assert!(part3_result.is_ok());
    let part3_etag = part3_result.unwrap().e_tag.unwrap();

    // List parts to verify all 3 are there
    let list_parts = client
        .list_parts()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .upload_id(upload_id)
        .send()
        .await;
    assert!(list_parts.is_ok());
    assert_eq!(list_parts.unwrap().parts().len(), 3);

    // Complete multipart upload
    use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
    let completed = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&part1_etag)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(2)
                .e_tag(&part2_etag)
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(3)
                .e_tag(&part3_etag)
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .upload_id(upload_id)
        .multipart_upload(completed)
        .send()
        .await;
    assert!(
        complete_result.is_ok(),
        "Failed to complete multipart: {:?}",
        complete_result.err()
    );

    // Get the final object and verify its content
    let get_result = client
        .get_object()
        .bucket("part-copy-test")
        .key("composite-file.bin")
        .send()
        .await;
    assert!(get_result.is_ok());
    let body = get_result.unwrap().body.collect().await.unwrap();
    let body_bytes = body.into_bytes();

    // Expected: 2048 bytes of 'S' + 1024 bytes of 'R' (from range) + 512 bytes of 'P'
    let expected_size = 2048 + 1024 + 512;
    assert_eq!(body_bytes.len(), expected_size, "Unexpected file size");

    // Verify content segments
    assert!(
        body_bytes[..2048].iter().all(|&b| b == b'S'),
        "Part 1 content mismatch"
    );
    assert!(
        body_bytes[2048..3072].iter().all(|&b| b == b'R'),
        "Part 2 content mismatch"
    );
    assert!(
        body_bytes[3072..].iter().all(|&b| b == b'P'),
        "Part 3 content mismatch"
    );
}

/// Test ListMultipartUploads operation
#[tokio::test]
async fn test_list_multipart_uploads() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket_name = format!("list-uploads-{}", uuid::Uuid::new_v4());

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Create a few multipart uploads
    let upload1 = client
        .create_multipart_upload()
        .bucket(&bucket_name)
        .key("file1.txt")
        .send()
        .await
        .unwrap();
    let upload1_id = upload1.upload_id().unwrap();

    let upload2 = client
        .create_multipart_upload()
        .bucket(&bucket_name)
        .key("file2.txt")
        .send()
        .await
        .unwrap();
    let upload2_id = upload2.upload_id().unwrap();

    let upload3 = client
        .create_multipart_upload()
        .bucket(&bucket_name)
        .key("subdir/file3.txt")
        .send()
        .await
        .unwrap();
    let upload3_id = upload3.upload_id().unwrap();

    // List all uploads
    let list_result = client
        .list_multipart_uploads()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    let uploads = list_result.uploads();
    assert_eq!(uploads.len(), 3, "Should have 3 uploads");

    // List with prefix
    let list_result = client
        .list_multipart_uploads()
        .bucket(&bucket_name)
        .prefix("subdir/")
        .send()
        .await
        .unwrap();

    let uploads = list_result.uploads();
    assert_eq!(uploads.len(), 1, "Should have 1 upload with prefix");
    assert_eq!(uploads[0].key(), Some("subdir/file3.txt"));

    // List with max-uploads
    let list_result = client
        .list_multipart_uploads()
        .bucket(&bucket_name)
        .max_uploads(2)
        .send()
        .await
        .unwrap();

    let uploads = list_result.uploads();
    assert_eq!(uploads.len(), 2, "Should have 2 uploads with max-uploads=2");
    assert!(
        list_result.is_truncated() == Some(true),
        "Should be truncated with max-uploads=2"
    );

    // Abort all uploads for cleanup
    client
        .abort_multipart_upload()
        .bucket(&bucket_name)
        .key("file1.txt")
        .upload_id(upload1_id)
        .send()
        .await
        .unwrap();

    client
        .abort_multipart_upload()
        .bucket(&bucket_name)
        .key("file2.txt")
        .upload_id(upload2_id)
        .send()
        .await
        .unwrap();

    client
        .abort_multipart_upload()
        .bucket(&bucket_name)
        .key("subdir/file3.txt")
        .upload_id(upload3_id)
        .send()
        .await
        .unwrap();

    // Verify all uploads are gone
    let list_result = client
        .list_multipart_uploads()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    assert!(
        list_result.uploads().is_empty(),
        "Should have no uploads after abort"
    );
}

// =====================================================================
// Edge case tests — WS-2
// =====================================================================

/// Helper: ensure bucket exists and initiate a multipart upload, returning upload_id.
/// Ignores BucketAlreadyExists so it is safe to call even if the bucket was pre-created.
async fn setup_multipart(client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> String {
    // Ignore BucketAlreadyExists — the caller may have already created the bucket.
    let _ = client.create_bucket().bucket(bucket).send().await;
    let resp = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .expect("create_multipart_upload failed");
    resp.upload_id().expect("upload_id missing").to_string()
}

/// Upload parts in order 3, 1, 2; complete in order 1,2,3; verify assembled body.
#[tokio::test]
async fn test_multipart_out_of_order_parts() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("oop-{}", uuid::Uuid::new_v4());
    let key = "out-of-order.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    let part3_data = vec![b'C'; 512];
    let part1_data = vec![b'A'; 1024];
    let part2_data = vec![b'B'; 1024];

    // Upload out of order: 3, then 1, then 2
    client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(3)
        .body(part3_data.clone().into())
        .send()
        .await
        .expect("upload part 3 failed");

    let resp1 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(part1_data.clone().into())
        .send()
        .await
        .expect("upload part 1 failed");
    let etag1 = resp1.e_tag.expect("etag1 missing");

    let resp2 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(2)
        .body(part2_data.clone().into())
        .send()
        .await
        .expect("upload part 2 failed");
    let etag2 = resp2.e_tag.expect("etag2 missing");

    // Fetch the ETag for part 3 via list_parts
    let list = client
        .list_parts()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("list_parts failed");
    let etag3 = list
        .parts()
        .iter()
        .find(|p| p.part_number() == Some(3))
        .and_then(|p| p.e_tag())
        .expect("part 3 etag missing")
        .to_string();

    // Complete in order 1, 2, 3
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

    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .expect("complete_multipart_upload failed");

    // Verify assembled body is part1 || part2 || part3
    let get_resp = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("get_object failed");
    let body_bytes = get_resp
        .body
        .collect()
        .await
        .expect("body collect failed")
        .into_bytes();

    let expected: Vec<u8> = [part1_data, part2_data, part3_data].concat();
    assert_eq!(
        body_bytes.as_ref(),
        expected.as_slice(),
        "Assembled body should be part1 || part2 || part3"
    );
}

/// Upload part 1 twice with different content; Complete with part 1 once.
/// The second upload should overwrite the first.
#[tokio::test]
async fn test_multipart_repeated_part_overwrite() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("rpo-{}", uuid::Uuid::new_v4());
    let key = "repeated-part.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    let first_data = vec![b'F'; 512];
    let second_data = vec![b'S'; 512];

    // Upload part 1 the first time
    client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(first_data.into())
        .send()
        .await
        .expect("first upload of part 1 failed");

    // Upload part 1 again with different content (overwrite)
    let resp2 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(second_data.clone().into())
        .send()
        .await
        .expect("second upload of part 1 failed");
    let etag_second = resp2.e_tag.expect("etag missing for second upload");

    // Complete with the second upload's ETag
    let completed = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&etag_second)
                .build(),
        )
        .build();

    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .expect("complete_multipart_upload failed");

    // Verify the object uses the second upload's content
    let get_resp = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("get_object failed");
    let body_bytes = get_resp
        .body
        .collect()
        .await
        .expect("body collect failed")
        .into_bytes();

    assert_eq!(
        body_bytes.as_ref(),
        second_data.as_slice(),
        "Should use second upload content"
    );
}

/// partNumber=0 should return 400 with InvalidPart code.
#[tokio::test]
async fn test_multipart_invalid_part_zero() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("ipz-{}", uuid::Uuid::new_v4());
    let key = "invalid-part-zero.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    // Use reqwest directly because the AWS SDK validates part numbers client-side
    let http_client = reqwest::Client::new();
    let url = format!(
        "{}/{}/{}?uploadId={}&partNumber=0",
        server.base_url, bucket, key, upload_id
    );
    let resp = http_client
        .put(&url)
        .body(vec![b'X'; 128])
        .send()
        .await
        .expect("reqwest PUT failed");

    assert_eq!(
        resp.status().as_u16(),
        400,
        "partNumber=0 should return 400"
    );
    let body = resp.text().await.expect("body read failed");
    assert!(
        body.contains("InvalidPart"),
        "Response should contain InvalidPart, got: {}",
        body
    );
}

/// partNumber=-1 should return 4xx (parse failure since u32 can't hold negatives).
#[tokio::test]
async fn test_multipart_invalid_part_negative() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("ipn-{}", uuid::Uuid::new_v4());
    let key = "invalid-part-neg.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    let http_client = reqwest::Client::new();
    let url = format!(
        "{}/{}/{}?uploadId={}&partNumber=-1",
        server.base_url, bucket, key, upload_id
    );
    let resp = http_client
        .put(&url)
        .body(vec![b'X'; 128])
        .send()
        .await
        .expect("reqwest PUT failed");

    let status = resp.status().as_u16();
    assert!(
        (400..500).contains(&status),
        "partNumber=-1 should return 4xx, got {}",
        status
    );
}

/// partNumber=10001 should return 400 with InvalidPart.
#[tokio::test]
async fn test_multipart_invalid_part_too_large() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("iptl-{}", uuid::Uuid::new_v4());
    let key = "invalid-part-toolarge.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    let http_client = reqwest::Client::new();
    let url = format!(
        "{}/{}/{}?uploadId={}&partNumber=10001",
        server.base_url, bucket, key, upload_id
    );
    let resp = http_client
        .put(&url)
        .body(vec![b'X'; 128])
        .send()
        .await
        .expect("reqwest PUT failed");

    assert_eq!(
        resp.status().as_u16(),
        400,
        "partNumber=10001 should return 400"
    );
    let body = resp.text().await.expect("body read failed");
    assert!(
        body.contains("InvalidPart"),
        "Response should contain InvalidPart, got: {}",
        body
    );
}

/// After abort, the temp multipart directory should be removed.
#[tokio::test]
async fn test_multipart_abort_removes_parts() {
    let (client, temp_dir, _server) = setup_test_server().await;
    let bucket = format!("abclean-{}", uuid::Uuid::new_v4());
    let key = "to-abort.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    // Upload 3 parts
    for part_num in 1i32..=3 {
        client
            .upload_part()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(part_num)
            .body(vec![b'X'; 512].into())
            .send()
            .await
            .unwrap_or_else(|e| panic!("upload part {} failed: {}", part_num, e));
    }

    // Abort the upload
    client
        .abort_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("abort_multipart_upload failed");

    // Verify the temp multipart directory is gone
    let multipart_dir = temp_dir
        .path()
        .join(&bucket)
        .join("multipart")
        .join(&upload_id);
    assert!(
        !multipart_dir.exists(),
        "Multipart directory should be removed after abort: {:?}",
        multipart_dir
    );
}

/// ListParts must always return parts sorted ascending by part number.
#[tokio::test]
async fn test_listparts_ordering() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("lpo-{}", uuid::Uuid::new_v4());
    let key = "ordering.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    // Upload parts in scrambled order: 5, 2, 4, 1, 3
    for &pn in &[5i32, 2, 4, 1, 3] {
        client
            .upload_part()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(pn)
            .body(vec![b'X'; 128].into())
            .send()
            .await
            .unwrap_or_else(|e| panic!("upload part {} failed: {}", pn, e));
    }

    let list = client
        .list_parts()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .send()
        .await
        .expect("list_parts failed");

    let part_numbers: Vec<i32> = list
        .parts()
        .iter()
        .map(|p| p.part_number().unwrap_or(0))
        .collect();

    assert_eq!(
        part_numbers,
        vec![1, 2, 3, 4, 5],
        "Parts must be returned in ascending order, got: {:?}",
        part_numbers
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

/// ListParts with max-parts=2 over 5 parts: three pages of 2, 2, 1.
#[tokio::test]
async fn test_listparts_pagination() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("lpp-{}", uuid::Uuid::new_v4());
    let key = "paginated.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    // Upload 5 parts
    for pn in 1i32..=5 {
        client
            .upload_part()
            .bucket(&bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(pn)
            .body(vec![b'X'; 128].into())
            .send()
            .await
            .unwrap_or_else(|e| panic!("upload part {} failed: {}", pn, e));
    }

    let http_client = reqwest::Client::new();

    // Page 1: max-parts=2, no marker
    let page1_url = format!(
        "{}/{}/{}?uploadId={}&max-parts=2",
        server.base_url, bucket, key, upload_id
    );
    let page1_resp = http_client
        .get(&page1_url)
        .send()
        .await
        .expect("page1 GET failed");
    assert_eq!(page1_resp.status().as_u16(), 200, "page1 should return 200");
    let page1_body = page1_resp.text().await.expect("page1 body failed");
    assert!(
        page1_body.contains("<IsTruncated>true</IsTruncated>"),
        "Page 1 should be truncated, body: {}",
        page1_body
    );
    let next_marker1 = extract_xml_value(&page1_body, "NextPartNumberMarker")
        .expect("NextPartNumberMarker missing from page1");
    assert_eq!(
        next_marker1, "2",
        "NextPartNumberMarker should be 2 after page1"
    );
    assert_eq!(
        count_xml_elements(&page1_body, "<PartNumber>"),
        2,
        "Page 1 should have 2 parts"
    );

    // Page 2: max-parts=2, marker=2
    let page2_url = format!(
        "{}/{}/{}?uploadId={}&max-parts=2&part-number-marker=2",
        server.base_url, bucket, key, upload_id
    );
    let page2_resp = http_client
        .get(&page2_url)
        .send()
        .await
        .expect("page2 GET failed");
    assert_eq!(page2_resp.status().as_u16(), 200, "page2 should return 200");
    let page2_body = page2_resp.text().await.expect("page2 body failed");
    assert!(
        page2_body.contains("<IsTruncated>true</IsTruncated>"),
        "Page 2 should be truncated, body: {}",
        page2_body
    );
    assert_eq!(
        count_xml_elements(&page2_body, "<PartNumber>"),
        2,
        "Page 2 should have 2 parts"
    );

    // Page 3: max-parts=2, marker=4
    let page3_url = format!(
        "{}/{}/{}?uploadId={}&max-parts=2&part-number-marker=4",
        server.base_url, bucket, key, upload_id
    );
    let page3_resp = http_client
        .get(&page3_url)
        .send()
        .await
        .expect("page3 GET failed");
    assert_eq!(page3_resp.status().as_u16(), 200, "page3 should return 200");
    let page3_body = page3_resp.text().await.expect("page3 body failed");
    assert!(
        page3_body.contains("<IsTruncated>false</IsTruncated>"),
        "Page 3 should not be truncated, body: {}",
        page3_body
    );
    assert_eq!(
        count_xml_elements(&page3_body, "<PartNumber>"),
        1,
        "Page 3 should have 1 part"
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

/// ListMultipartUploads with prefix=foo/ returns only foo/* uploads.
#[tokio::test]
async fn test_list_multipart_uploads_prefix() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("lmup-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket failed");

    let u1 = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key("foo/a.txt")
        .send()
        .await
        .expect("create upload1 failed");
    let u1_id = u1.upload_id().expect("upload_id missing").to_string();

    let u2 = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key("foo/b.txt")
        .send()
        .await
        .expect("create upload2 failed");
    let u2_id = u2.upload_id().expect("upload_id missing").to_string();

    let u3 = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key("bar/c.txt")
        .send()
        .await
        .expect("create upload3 failed");
    let u3_id = u3.upload_id().expect("upload_id missing").to_string();

    // List with prefix=foo/
    let list_resp = client
        .list_multipart_uploads()
        .bucket(&bucket)
        .prefix("foo/")
        .send()
        .await
        .expect("list_multipart_uploads failed");

    let uploads = list_resp.uploads();
    assert_eq!(uploads.len(), 2, "Should have 2 uploads with prefix foo/");
    for upload in uploads {
        let k = upload.key().unwrap_or("");
        assert!(
            k.starts_with("foo/"),
            "Upload key {:?} should start with foo/",
            k
        );
    }

    // Cleanup
    for (key, uid) in &[
        ("foo/a.txt", &u1_id),
        ("foo/b.txt", &u2_id),
        ("bar/c.txt", &u3_id),
    ] {
        client
            .abort_multipart_upload()
            .bucket(&bucket)
            .key(*key)
            .upload_id(*uid)
            .send()
            .await
            .ok();
    }
}

/// ListMultipartUploads with max-uploads=2 over 3 uploads returns 2 + IsTruncated=true.
#[tokio::test]
async fn test_list_multipart_uploads_max() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("lmum-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket failed");

    let mut upload_ids: Vec<(&str, String)> = Vec::new();
    let keys = ["alpha.bin", "beta.bin", "gamma.bin"];
    for key in &keys {
        let resp = client
            .create_multipart_upload()
            .bucket(&bucket)
            .key(*key)
            .send()
            .await
            .expect("create_multipart_upload failed");
        upload_ids.push((
            key,
            resp.upload_id().expect("upload_id missing").to_string(),
        ));
    }

    let list_resp = client
        .list_multipart_uploads()
        .bucket(&bucket)
        .max_uploads(2)
        .send()
        .await
        .expect("list_multipart_uploads failed");

    assert_eq!(
        list_resp.uploads().len(),
        2,
        "Should return 2 uploads with max-uploads=2"
    );
    assert_eq!(
        list_resp.is_truncated(),
        Some(true),
        "Should be truncated when max-uploads < total"
    );

    for (key, uid) in &upload_ids {
        client
            .abort_multipart_upload()
            .bucket(&bucket)
            .key(*key)
            .upload_id(uid)
            .send()
            .await
            .ok();
    }
}

/// Complete with a missing part (parts 1 and 3 uploaded, 2 listed) should return an error.
#[tokio::test]
async fn test_multipart_complete_missing_part() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("cmp-{}", uuid::Uuid::new_v4());
    let key = "missing-part.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    let r1 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'A'; 256].into())
        .send()
        .await
        .expect("upload part 1 failed");
    let etag1 = r1.e_tag.expect("etag1 missing");

    let r3 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(3)
        .body(vec![b'C'; 256].into())
        .send()
        .await
        .expect("upload part 3 failed");
    let etag3 = r3.e_tag.expect("etag3 missing");

    // Complete listing parts 1, 2 (not uploaded), 3
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
                .e_tag("\"fake-etag\"")
                .build(),
        )
        .parts(
            CompletedPart::builder()
                .part_number(3)
                .e_tag(&etag3)
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await;

    assert!(
        complete_result.is_err(),
        "Complete with missing part should fail"
    );
}

/// Complete with a wrong ETag for an uploaded part should return an error.
#[tokio::test]
async fn test_multipart_complete_wrong_etag() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("cwe-{}", uuid::Uuid::new_v4());
    let key = "wrong-etag.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'A'; 256].into())
        .send()
        .await
        .expect("upload part 1 failed");

    // Complete with a deliberately wrong ETag
    let completed = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag("\"wrongetagvalue\"")
                .build(),
        )
        .build();

    let complete_result = client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await;

    assert!(
        complete_result.is_err(),
        "Complete with wrong ETag should fail"
    );
}

/// Two separate multipart uploads to the same key (different upload IDs) should both succeed.
#[tokio::test]
async fn test_multipart_concurrent_same_key() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket = format!("concurrent-{}", uuid::Uuid::new_v4());
    let key = "shared-key.bin";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket failed");

    // Initiate two uploads to the same key
    let up1 = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("create upload1 failed");
    let uid1 = up1.upload_id().expect("uid1 missing").to_string();

    let up2 = client
        .create_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("create upload2 failed");
    let uid2 = up2.upload_id().expect("uid2 missing").to_string();

    assert_ne!(uid1, uid2, "Upload IDs must be distinct");

    // Upload and complete upload 1
    let r1 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&uid1)
        .part_number(1)
        .body(vec![b'1'; 256].into())
        .send()
        .await
        .expect("upload1 part1 failed");
    let etag1 = r1.e_tag.expect("etag1 missing");

    let completed1 = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&etag1)
                .build(),
        )
        .build();
    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&uid1)
        .multipart_upload(completed1)
        .send()
        .await
        .expect("complete upload1 failed");

    // Upload and complete upload 2
    let r2 = client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&uid2)
        .part_number(1)
        .body(vec![b'2'; 256].into())
        .send()
        .await
        .expect("upload2 part1 failed");
    let etag2 = r2.e_tag.expect("etag2 missing");

    let completed2 = CompletedMultipartUpload::builder()
        .parts(
            CompletedPart::builder()
                .part_number(1)
                .e_tag(&etag2)
                .build(),
        )
        .build();
    client
        .complete_multipart_upload()
        .bucket(&bucket)
        .key(key)
        .upload_id(&uid2)
        .multipart_upload(completed2)
        .send()
        .await
        .expect("complete upload2 failed");

    // Object should be accessible; last writer wins
    let get_resp = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .expect("get_object failed");
    let body = get_resp
        .body
        .collect()
        .await
        .expect("collect failed")
        .into_bytes();
    assert_eq!(body.len(), 256, "Object should have 256 bytes");
}

/// CreateMultipartUpload response must have Bucket, Key, UploadId XML elements.
#[tokio::test]
async fn test_multipart_create_response_shape() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("crs-{}", uuid::Uuid::new_v4());
    let key = "shape-test.bin";

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket failed");

    let http_client = reqwest::Client::new();
    let url = format!("{}/{}/{}?uploads", server.base_url, bucket, key);
    let resp = http_client.post(&url).send().await.expect("POST failed");

    assert_eq!(
        resp.status().as_u16(),
        200,
        "CreateMultipartUpload should return 200"
    );
    let body = resp.text().await.expect("body read failed");

    assert!(
        body.contains("<Bucket>"),
        "Response should have <Bucket> element, got: {}",
        body
    );
    assert!(
        body.contains("<Key>"),
        "Response should have <Key> element, got: {}",
        body
    );
    assert!(
        body.contains("<UploadId>"),
        "Response should have <UploadId> element, got: {}",
        body
    );
}

/// ListParts response must contain UploadId, Bucket, Key, and Part elements.
#[tokio::test]
async fn test_multipart_list_parts_response_shape() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("lprs-{}", uuid::Uuid::new_v4());
    let key = "parts-shape.bin";

    let upload_id = setup_multipart(&client, &bucket, key).await;

    client
        .upload_part()
        .bucket(&bucket)
        .key(key)
        .upload_id(&upload_id)
        .part_number(1)
        .body(vec![b'X'; 128].into())
        .send()
        .await
        .expect("upload part failed");

    let http_client = reqwest::Client::new();
    let url = format!(
        "{}/{}/{}?uploadId={}",
        server.base_url, bucket, key, upload_id
    );
    let resp = http_client.get(&url).send().await.expect("GET failed");

    assert_eq!(resp.status().as_u16(), 200, "ListParts should return 200");
    let body = resp.text().await.expect("body read failed");

    assert!(
        body.contains("<UploadId>"),
        "Response should have <UploadId>, got: {}",
        body
    );
    assert!(
        body.contains("<Bucket>"),
        "Response should have <Bucket>, got: {}",
        body
    );
    assert!(
        body.contains("<Key>"),
        "Response should have <Key>, got: {}",
        body
    );
    assert!(
        body.contains("<Part>"),
        "Response should have <Part>, got: {}",
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

// =====================================================================
// XML helper utilities
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

// =====================================================================
//  Background Multipart GC Tests
// =====================================================================

/// Helper: create a StorageEngine in a temp directory and return both.
async fn create_test_storage() -> (
    std::sync::Arc<rs3gw::storage::StorageEngine>,
    std::path::PathBuf,
) {
    let tmp = std::env::temp_dir().join(format!("rs3gw-gc-{}", uuid::Uuid::new_v4()));
    let storage =
        rs3gw::storage::StorageEngine::new(tmp.clone()).expect("failed to create storage engine");
    (std::sync::Arc::new(storage), tmp)
}

/// Helper: write a fake multipart metadata JSON with a custom `initiated` timestamp.
/// This lets us simulate uploads that are arbitrarily old without waiting.
async fn write_fake_multipart_metadata(
    root: &std::path::Path,
    bucket: &str,
    upload_id: &str,
    key: &str,
    initiated: chrono::DateTime<chrono::Utc>,
) {
    let multipart_dir = root.join(bucket).join("multipart").join(upload_id);
    tokio::fs::create_dir_all(&multipart_dir)
        .await
        .expect("failed to create multipart dir");
    let metadata = serde_json::json!({
        "bucket": bucket,
        "key": key,
        "upload_id": upload_id,
        "content_type": "application/octet-stream",
        "metadata": {},
        "initiated": initiated.to_rfc3339(),
        "parts": {}
    });
    let metadata_path = multipart_dir.join("metadata.json");
    tokio::fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).expect("json"),
    )
    .await
    .expect("failed to write metadata");
}

#[tokio::test]
async fn test_gc_removes_expired_uploads() {
    let (storage, tmp) = create_test_storage().await;

    // Create bucket
    storage
        .create_bucket("gc-test-1")
        .await
        .expect("create bucket");

    // Write a fake upload with an initiated timestamp 100 hours ago
    let old_time = chrono::Utc::now() - chrono::Duration::hours(100);
    write_fake_multipart_metadata(
        &tmp,
        "gc-test-1",
        "old-upload-001",
        "stale-file.bin",
        old_time,
    )
    .await;

    // Verify upload is listed
    let uploads = storage
        .list_multipart_uploads("gc-test-1", None)
        .await
        .expect("list");
    assert_eq!(uploads.len(), 1);

    // Run GC with 24-hour retention - should clean the 100-hour-old upload
    let cleaned = rs3gw::storage::gc::gc_sweep(&storage, None, 24)
        .await
        .expect("gc_sweep");
    assert_eq!(cleaned, 1);

    // Verify upload is gone
    let uploads_after = storage
        .list_multipart_uploads("gc-test-1", None)
        .await
        .expect("list");
    assert_eq!(uploads_after.len(), 0);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn test_gc_preserves_recent_uploads() {
    let (storage, tmp) = create_test_storage().await;

    storage
        .create_bucket("gc-test-2")
        .await
        .expect("create bucket");

    // Create a multipart upload via the real API (will have a recent timestamp)
    let upload_id = storage
        .create_multipart_upload(
            "gc-test-2",
            "recent-file.bin",
            "application/octet-stream",
            std::collections::HashMap::new(),
        )
        .await
        .expect("create multipart upload");

    // Run GC with long retention (168 hours = 1 week) - should NOT clean recent upload
    let cleaned = rs3gw::storage::gc::gc_sweep(&storage, None, 168)
        .await
        .expect("gc_sweep");
    assert_eq!(cleaned, 0);

    // Verify upload still exists
    let uploads = storage
        .list_multipart_uploads("gc-test-2", None)
        .await
        .expect("list");
    assert_eq!(uploads.len(), 1);
    assert_eq!(uploads[0].upload_id, upload_id);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn test_gc_handles_empty_buckets() {
    let (storage, tmp) = create_test_storage().await;

    // Run GC on completely empty storage (no buckets at all)
    let cleaned = rs3gw::storage::gc::gc_sweep(&storage, None, 1)
        .await
        .expect("gc_sweep");
    assert_eq!(cleaned, 0);

    // Create an empty bucket and run GC
    storage
        .create_bucket("empty-bucket")
        .await
        .expect("create bucket");
    let cleaned = rs3gw::storage::gc::gc_sweep(&storage, Some("empty-bucket"), 1)
        .await
        .expect("gc_sweep");
    assert_eq!(cleaned, 0);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn test_gc_multiple_buckets() {
    let (storage, tmp) = create_test_storage().await;

    // Create two buckets
    storage
        .create_bucket("gc-multi-a")
        .await
        .expect("create bucket a");
    storage
        .create_bucket("gc-multi-b")
        .await
        .expect("create bucket b");

    let old_time = chrono::Utc::now() - chrono::Duration::hours(200);
    let recent_time = chrono::Utc::now() - chrono::Duration::hours(1);

    // Bucket A: one expired upload, one recent
    write_fake_multipart_metadata(&tmp, "gc-multi-a", "expired-a1", "old-a.bin", old_time).await;
    write_fake_multipart_metadata(&tmp, "gc-multi-a", "recent-a1", "new-a.bin", recent_time).await;

    // Bucket B: one expired upload
    write_fake_multipart_metadata(&tmp, "gc-multi-b", "expired-b1", "old-b.bin", old_time).await;

    // Verify all uploads exist
    let uploads_a = storage
        .list_multipart_uploads("gc-multi-a", None)
        .await
        .expect("list a");
    assert_eq!(uploads_a.len(), 2);
    let uploads_b = storage
        .list_multipart_uploads("gc-multi-b", None)
        .await
        .expect("list b");
    assert_eq!(uploads_b.len(), 1);

    // Run GC with 24-hour retention across all buckets
    let cleaned = rs3gw::storage::gc::gc_sweep(&storage, None, 24)
        .await
        .expect("gc_sweep");
    assert_eq!(cleaned, 2); // expired-a1 and expired-b1

    // Verify: bucket A should have 1 remaining (recent), bucket B should be empty
    let uploads_a_after = storage
        .list_multipart_uploads("gc-multi-a", None)
        .await
        .expect("list a");
    assert_eq!(uploads_a_after.len(), 1);
    assert_eq!(uploads_a_after[0].upload_id, "recent-a1");

    let uploads_b_after = storage
        .list_multipart_uploads("gc-multi-b", None)
        .await
        .expect("list b");
    assert_eq!(uploads_b_after.len(), 0);

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}

#[tokio::test]
async fn test_gc_scheduler_runs() {
    let (storage, tmp) = create_test_storage().await;

    storage
        .create_bucket("gc-sched")
        .await
        .expect("create bucket");

    // Write an expired upload
    let old_time = chrono::Utc::now() - chrono::Duration::hours(50);
    write_fake_multipart_metadata(&tmp, "gc-sched", "sched-expired", "old.bin", old_time).await;

    // Verify upload exists
    let uploads = storage
        .list_multipart_uploads("gc-sched", None)
        .await
        .expect("list");
    assert_eq!(uploads.len(), 1);

    // Spawn GC scheduler with 1-second interval and 24-hour retention
    let handle = rs3gw::storage::gc::spawn_multipart_gc(std::sync::Arc::clone(&storage), 24, 1);

    // Wait enough for at least one GC tick (interval starts immediately, then every 1s)
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // The expired upload should have been cleaned by the scheduler
    let uploads_after = storage
        .list_multipart_uploads("gc-sched", None)
        .await
        .expect("list");
    assert_eq!(uploads_after.len(), 0);

    // Abort the scheduler
    handle.abort();

    let _ = tokio::fs::remove_dir_all(&tmp).await;
}
