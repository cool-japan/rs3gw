//! Multipart upload tests for rs3gw

mod common;

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
