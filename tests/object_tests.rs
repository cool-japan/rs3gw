//! Object operation tests for rs3gw

mod common;

use common::setup_test_server;

#[tokio::test]
async fn test_object_operations() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket first
    client
        .create_bucket()
        .bucket("objects-test")
        .send()
        .await
        .unwrap();

    // Put object
    let content = b"Hello, rs3gw!";
    let put_result = client
        .put_object()
        .bucket("objects-test")
        .key("test-file.txt")
        .content_type("text/plain")
        .body(content.to_vec().into())
        .send()
        .await;
    assert!(
        put_result.is_ok(),
        "Failed to put object: {:?}",
        put_result.err()
    );

    // Head object
    let head_result = client
        .head_object()
        .bucket("objects-test")
        .key("test-file.txt")
        .send()
        .await;
    assert!(head_result.is_ok(), "Failed to head object");
    let head = head_result.unwrap();
    assert_eq!(head.content_length(), Some(content.len() as i64));

    // Get object
    let get_result = client
        .get_object()
        .bucket("objects-test")
        .key("test-file.txt")
        .send()
        .await;
    assert!(get_result.is_ok(), "Failed to get object");
    let body = get_result.unwrap().body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), content);

    // Delete object
    let delete_result = client
        .delete_object()
        .bucket("objects-test")
        .key("test-file.txt")
        .send()
        .await;
    assert!(delete_result.is_ok(), "Failed to delete object");

    // Verify object is deleted
    let get_result = client
        .get_object()
        .bucket("objects-test")
        .key("test-file.txt")
        .send()
        .await;
    assert!(get_result.is_err(), "Object should have been deleted");
}

#[tokio::test]
async fn test_list_objects_v2() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("list-test")
        .send()
        .await
        .unwrap();

    // Put multiple objects
    for key in ["foo/a.txt", "foo/b.txt", "bar/c.txt", "root.txt"] {
        client
            .put_object()
            .bucket("list-test")
            .key(key)
            .body(b"test content".to_vec().into())
            .send()
            .await
            .unwrap();
    }

    // List all objects
    let list_result = client.list_objects_v2().bucket("list-test").send().await;
    assert!(list_result.is_ok(), "Failed to list objects");
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    assert_eq!(contents.len(), 4);

    // List with prefix
    let list_result = client
        .list_objects_v2()
        .bucket("list-test")
        .prefix("foo/")
        .send()
        .await;
    assert!(list_result.is_ok());
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    assert_eq!(contents.len(), 2);

    // List with delimiter
    let list_result = client
        .list_objects_v2()
        .bucket("list-test")
        .delimiter("/")
        .send()
        .await;
    assert!(list_result.is_ok());
    let result = list_result.unwrap();
    let contents = result.contents();
    let prefixes = result.common_prefixes();
    assert_eq!(contents.len(), 1); // root.txt
    assert_eq!(prefixes.len(), 2); // foo/ and bar/

    // Test pagination with start_after
    let list_result = client
        .list_objects_v2()
        .bucket("list-test")
        .start_after("foo/a.txt")
        .send()
        .await;
    assert!(list_result.is_ok());
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    // Should return objects after "foo/a.txt" (alphabetically: foo/b.txt, root.txt)
    assert_eq!(contents.len(), 2);

    // Test pagination with max_keys and verify is_truncated/continuation_token
    let list_result = client
        .list_objects_v2()
        .bucket("list-test")
        .max_keys(2)
        .send()
        .await;
    assert!(list_result.is_ok());
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    assert_eq!(contents.len(), 2);
    assert_eq!(list_output.is_truncated(), Some(true));
    assert!(list_output.next_continuation_token().is_some());

    // Use continuation_token to get next page
    let continuation_token = list_output.next_continuation_token().unwrap().to_string();
    let list_result = client
        .list_objects_v2()
        .bucket("list-test")
        .continuation_token(&continuation_token)
        .send()
        .await;
    assert!(list_result.is_ok());
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    // Should return remaining objects
    assert_eq!(contents.len(), 2);
}

#[tokio::test]
async fn test_list_objects_v1() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("list-v1-test")
        .send()
        .await
        .unwrap();

    // Put multiple objects
    for key in ["alpha/a.txt", "alpha/b.txt", "beta/c.txt", "root.txt"] {
        client
            .put_object()
            .bucket("list-v1-test")
            .key(key)
            .body(b"test content".to_vec().into())
            .send()
            .await
            .unwrap();
    }

    // List all objects using V1 API (list_objects without v2)
    #[allow(deprecated)]
    let list_result = client.list_objects().bucket("list-v1-test").send().await;
    assert!(
        list_result.is_ok(),
        "Failed to list objects V1: {:?}",
        list_result.err()
    );
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    assert_eq!(contents.len(), 4);

    // List with prefix using V1
    #[allow(deprecated)]
    let list_result = client
        .list_objects()
        .bucket("list-v1-test")
        .prefix("alpha/")
        .send()
        .await;
    assert!(list_result.is_ok());
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    assert_eq!(contents.len(), 2);

    // List with delimiter using V1
    #[allow(deprecated)]
    let list_result = client
        .list_objects()
        .bucket("list-v1-test")
        .delimiter("/")
        .send()
        .await;
    assert!(list_result.is_ok());
    let result = list_result.unwrap();
    let contents = result.contents();
    let prefixes = result.common_prefixes();
    assert_eq!(contents.len(), 1); // root.txt
    assert_eq!(prefixes.len(), 2); // alpha/ and beta/

    // Test with marker (pagination)
    #[allow(deprecated)]
    let list_result = client
        .list_objects()
        .bucket("list-v1-test")
        .marker("alpha/b.txt")
        .send()
        .await;
    assert!(list_result.is_ok());
    let list_output = list_result.unwrap();
    let contents = list_output.contents();
    // Should return objects after "alpha/b.txt" (alphabetically: beta/c.txt, root.txt)
    assert!(contents.len() >= 2);
}

#[tokio::test]
async fn test_object_with_metadata() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("meta-test")
        .send()
        .await
        .unwrap();

    // Put object with custom metadata
    let put_result = client
        .put_object()
        .bucket("meta-test")
        .key("with-meta.txt")
        .content_type("text/plain")
        .metadata("custom-key", "custom-value")
        .body(b"test content".to_vec().into())
        .send()
        .await;
    assert!(put_result.is_ok());

    // Head object and check metadata
    let head_result = client
        .head_object()
        .bucket("meta-test")
        .key("with-meta.txt")
        .send()
        .await;
    assert!(head_result.is_ok());
    let head = head_result.unwrap();
    assert_eq!(head.content_type(), Some("text/plain"));
    let metadata = head.metadata();
    assert!(metadata.is_some());
    assert_eq!(
        metadata.unwrap().get("custom-key"),
        Some(&"custom-value".to_string())
    );
}

#[tokio::test]
async fn test_copy_object() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("copy-test")
        .send()
        .await
        .unwrap();

    // Put source object with metadata
    let content = b"Hello, this is the source!";
    client
        .put_object()
        .bucket("copy-test")
        .key("source.txt")
        .content_type("text/plain")
        .metadata("original", "true")
        .body(content.to_vec().into())
        .send()
        .await
        .unwrap();

    // Copy object to new key
    let copy_result = client
        .copy_object()
        .bucket("copy-test")
        .key("destination.txt")
        .copy_source("copy-test/source.txt")
        .send()
        .await;
    assert!(
        copy_result.is_ok(),
        "Failed to copy object: {:?}",
        copy_result.err()
    );

    // Verify copied object
    let get_result = client
        .get_object()
        .bucket("copy-test")
        .key("destination.txt")
        .send()
        .await;
    assert!(get_result.is_ok());
    let body = get_result.unwrap().body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), content);

    // Verify metadata was copied
    let head_result = client
        .head_object()
        .bucket("copy-test")
        .key("destination.txt")
        .send()
        .await;
    assert!(head_result.is_ok());
    let head = head_result.unwrap();
    let metadata = head.metadata().unwrap();
    assert_eq!(metadata.get("original"), Some(&"true".to_string()));
}

#[tokio::test]
async fn test_range_request() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("range-test")
        .send()
        .await
        .unwrap();

    // Put object with known content
    let content = b"0123456789ABCDEFGHIJ";
    client
        .put_object()
        .bucket("range-test")
        .key("range.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .unwrap();

    // Get partial content (first 5 bytes)
    let get_result = client
        .get_object()
        .bucket("range-test")
        .key("range.txt")
        .range("bytes=0-4")
        .send()
        .await;
    assert!(
        get_result.is_ok(),
        "Failed range request: {:?}",
        get_result.err()
    );
    let body = get_result.unwrap().body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), b"01234");

    // Get bytes from middle
    let get_result = client
        .get_object()
        .bucket("range-test")
        .key("range.txt")
        .range("bytes=10-14")
        .send()
        .await;
    assert!(get_result.is_ok());
    let body = get_result.unwrap().body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), b"ABCDE");

    // Get last 5 bytes
    let get_result = client
        .get_object()
        .bucket("range-test")
        .key("range.txt")
        .range("bytes=-5")
        .send()
        .await;
    assert!(get_result.is_ok());
    let body = get_result.unwrap().body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), b"FGHIJ");
}

#[tokio::test]
async fn test_object_tagging() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket and object
    client
        .create_bucket()
        .bucket("tagging-test")
        .send()
        .await
        .unwrap();
    client
        .put_object()
        .bucket("tagging-test")
        .key("tagged-file.txt")
        .body(b"content".to_vec().into())
        .send()
        .await
        .unwrap();

    // Get tagging (should be empty initially)
    let get_result = client
        .get_object_tagging()
        .bucket("tagging-test")
        .key("tagged-file.txt")
        .send()
        .await;
    assert!(
        get_result.is_ok(),
        "Failed to get tagging: {:?}",
        get_result.err()
    );
    let tagging = get_result.unwrap();
    assert!(tagging.tag_set().is_empty());

    // Put tagging
    use aws_sdk_s3::types::{Tag, Tagging};
    let tag1 = Tag::builder()
        .key("Environment")
        .value("Production")
        .build()
        .unwrap();
    let tag2 = Tag::builder()
        .key("Project")
        .value("rs3gw")
        .build()
        .unwrap();
    let tagging = Tagging::builder()
        .tag_set(tag1)
        .tag_set(tag2)
        .build()
        .unwrap();

    let put_result = client
        .put_object_tagging()
        .bucket("tagging-test")
        .key("tagged-file.txt")
        .tagging(tagging)
        .send()
        .await;
    assert!(
        put_result.is_ok(),
        "Failed to put tagging: {:?}",
        put_result.err()
    );

    // Get tagging (should have 2 tags now)
    let get_result = client
        .get_object_tagging()
        .bucket("tagging-test")
        .key("tagged-file.txt")
        .send()
        .await;
    assert!(get_result.is_ok());
    let tags = get_result.unwrap();
    let tag_set = tags.tag_set();
    assert_eq!(tag_set.len(), 2);

    // Delete tagging
    let delete_result = client
        .delete_object_tagging()
        .bucket("tagging-test")
        .key("tagged-file.txt")
        .send()
        .await;
    assert!(delete_result.is_ok(), "Failed to delete tagging");

    // Verify tagging is deleted
    let get_result = client
        .get_object_tagging()
        .bucket("tagging-test")
        .key("tagged-file.txt")
        .send()
        .await;
    assert!(get_result.is_ok());
    let tags = get_result.unwrap();
    assert!(tags.tag_set().is_empty());
}

#[tokio::test]
async fn test_delete_objects() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket and multiple objects
    client
        .create_bucket()
        .bucket("batch-delete-test")
        .send()
        .await
        .unwrap();

    for i in 0..5 {
        client
            .put_object()
            .bucket("batch-delete-test")
            .key(format!("file{}.txt", i))
            .body(format!("content {}", i).into_bytes().into())
            .send()
            .await
            .unwrap();
    }

    // Verify all objects exist
    let list = client
        .list_objects_v2()
        .bucket("batch-delete-test")
        .send()
        .await
        .unwrap();
    assert_eq!(list.contents().len(), 5);

    // Delete 3 objects in batch
    use aws_sdk_s3::types::{Delete, ObjectIdentifier};
    let objects: Vec<ObjectIdentifier> = (0..3)
        .map(|i| {
            ObjectIdentifier::builder()
                .key(format!("file{}.txt", i))
                .build()
                .unwrap()
        })
        .collect();

    let delete = Delete::builder()
        .set_objects(Some(objects))
        .build()
        .unwrap();

    let delete_result = client
        .delete_objects()
        .bucket("batch-delete-test")
        .delete(delete)
        .send()
        .await;
    assert!(
        delete_result.is_ok(),
        "Failed to delete objects: {:?}",
        delete_result.err()
    );

    let result = delete_result.unwrap();
    assert_eq!(result.deleted().len(), 3);

    // Verify only 2 objects remain
    let list = client
        .list_objects_v2()
        .bucket("batch-delete-test")
        .send()
        .await
        .unwrap();
    assert_eq!(list.contents().len(), 2);

    // Verify the remaining objects are file3.txt and file4.txt
    let keys: Vec<&str> = list.contents().iter().filter_map(|o| o.key()).collect();
    assert!(keys.contains(&"file3.txt"));
    assert!(keys.contains(&"file4.txt"));
}

#[tokio::test]
async fn test_conditional_copy() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("cond-copy-test")
        .send()
        .await
        .unwrap();

    // Put source object
    let content = b"Conditional copy test content";
    client
        .put_object()
        .bucket("cond-copy-test")
        .key("source.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .unwrap();

    // Get the ETag and last modified time from source
    let head_result = client
        .head_object()
        .bucket("cond-copy-test")
        .key("source.txt")
        .send()
        .await
        .unwrap();
    let source_etag = head_result.e_tag().unwrap().to_string();

    // Use reqwest for raw HTTP calls to test conditional headers
    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    // Test 1: Copy with If-Match (matching ETag) - should succeed
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest1.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header("x-amz-copy-source-if-match", &source_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        200,
        "Copy with matching If-Match should succeed"
    );

    // Verify the copy was created
    let get_result = client
        .get_object()
        .bucket("cond-copy-test")
        .key("dest1.txt")
        .send()
        .await;
    assert!(get_result.is_ok(), "Copied object should exist");

    // Test 2: Copy with If-Match (non-matching ETag) - should fail with 412
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest2.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header("x-amz-copy-source-if-match", "\"wrong-etag\"")
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        412,
        "Copy with non-matching If-Match should fail with 412"
    );

    // Test 3: Copy with If-None-Match (non-matching ETag) - should succeed
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest3.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header("x-amz-copy-source-if-none-match", "\"different-etag\"")
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        200,
        "Copy with non-matching If-None-Match should succeed"
    );

    // Test 4: Copy with If-None-Match (matching ETag) - should fail with 412
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest4.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header("x-amz-copy-source-if-none-match", &source_etag)
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        412,
        "Copy with matching If-None-Match should fail with 412"
    );

    // Test 5: Copy with If-Modified-Since (old date) - should succeed
    // Note: Jan 1, 2020 is a Wednesday
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest5.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header(
            "x-amz-copy-source-if-modified-since",
            "Wed, 01 Jan 2020 00:00:00 GMT",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        200,
        "Copy with old If-Modified-Since should succeed"
    );

    // Test 6: Copy with If-Modified-Since (future date) - should fail with 412
    // Note: Jan 1, 2030 is a Tuesday
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest6.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header(
            "x-amz-copy-source-if-modified-since",
            "Tue, 01 Jan 2030 00:00:00 GMT",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        412,
        "Copy with future If-Modified-Since should fail with 412"
    );

    // Test 7: Copy with If-Unmodified-Since (future date) - should succeed
    // Note: Jan 1, 2030 is a Tuesday
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest7.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header(
            "x-amz-copy-source-if-unmodified-since",
            "Tue, 01 Jan 2030 00:00:00 GMT",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        200,
        "Copy with future If-Unmodified-Since should succeed"
    );

    // Test 8: Copy with If-Unmodified-Since (old date) - should fail with 412
    // Note: Jan 1, 2020 is a Wednesday
    let copy_response = http_client
        .put(format!("{}/cond-copy-test/dest8.txt", base_url))
        .header("x-amz-copy-source", "cond-copy-test/source.txt")
        .header(
            "x-amz-copy-source-if-unmodified-since",
            "Wed, 01 Jan 2020 00:00:00 GMT",
        )
        .send()
        .await
        .unwrap();
    assert_eq!(
        copy_response.status(),
        412,
        "Copy with old If-Unmodified-Since should fail with 412"
    );

    // Verify only the successful copies exist
    let list_result = client
        .list_objects_v2()
        .bucket("cond-copy-test")
        .send()
        .await
        .unwrap();

    let keys: Vec<&str> = list_result
        .contents()
        .iter()
        .filter_map(|obj| obj.key())
        .collect();

    // Should have: source.txt, dest1.txt, dest3.txt, dest5.txt, dest7.txt (5 objects)
    assert!(keys.contains(&"source.txt"));
    assert!(keys.contains(&"dest1.txt"));
    assert!(keys.contains(&"dest3.txt"));
    assert!(keys.contains(&"dest5.txt"));
    assert!(keys.contains(&"dest7.txt"));
    assert!(!keys.contains(&"dest2.txt")); // Failed copy
    assert!(!keys.contains(&"dest4.txt")); // Failed copy
    assert!(!keys.contains(&"dest6.txt")); // Failed copy
    assert!(!keys.contains(&"dest8.txt")); // Failed copy
}

#[tokio::test]
async fn test_get_object_attributes() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("attrs-test")
        .send()
        .await
        .unwrap();

    // Put an object with known content
    let content = b"Test content for GetObjectAttributes";
    client
        .put_object()
        .bucket("attrs-test")
        .key("test-file.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .unwrap();

    // Use reqwest to call GetObjectAttributes (GET ?attributes)
    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    // Test 1: Get all default attributes
    let response = http_client
        .get(format!("{}/attrs-test/test-file.txt?attributes", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(
        body.contains("GetObjectAttributesResponse"),
        "Response should be GetObjectAttributesResponse XML"
    );
    assert!(body.contains("ETag"), "Response should contain ETag");
    assert!(
        body.contains("ObjectSize"),
        "Response should contain ObjectSize"
    );
    assert!(
        body.contains("<ObjectSize>36</ObjectSize>"),
        "ObjectSize should be 36 bytes"
    );
    assert!(
        body.contains("StorageClass"),
        "Response should contain StorageClass"
    );
    assert!(body.contains("STANDARD"), "StorageClass should be STANDARD");

    // Test 2: Request specific attributes with header
    let response = http_client
        .get(format!("{}/attrs-test/test-file.txt?attributes", base_url))
        .header("x-amz-object-attributes", "ETag, ObjectSize")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("ETag"), "Response should contain ETag");
    assert!(
        body.contains("ObjectSize"),
        "Response should contain ObjectSize"
    );

    // Test 3: Request Checksum attribute
    let response = http_client
        .get(format!("{}/attrs-test/test-file.txt?attributes", base_url))
        .header("x-amz-object-attributes", "Checksum")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(
        body.contains("Checksum"),
        "Response should contain Checksum"
    );
    assert!(
        body.contains("ChecksumSHA256"),
        "Response should contain ChecksumSHA256"
    );

    // Test 4: Non-existent object should return 404
    let response = http_client
        .get(format!(
            "{}/attrs-test/nonexistent.txt?attributes",
            base_url
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_list_object_versions() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("versions-test")
        .send()
        .await
        .unwrap();

    // Put some objects
    for i in 0..3 {
        client
            .put_object()
            .bucket("versions-test")
            .key(format!("file{}.txt", i))
            .body(format!("Content {}", i).into_bytes().into())
            .send()
            .await
            .unwrap();
    }

    // Use reqwest to call ListObjectVersions (GET ?versions)
    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    // Test 1: List all versions
    let response = http_client
        .get(format!("{}/versions-test?versions", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(
        body.contains("ListVersionsResult"),
        "Response should be ListVersionsResult XML"
    );
    assert!(
        body.contains("<Name>versions-test</Name>"),
        "Response should contain bucket name"
    );
    // Each object should appear as a Version with version_id "null"
    assert!(
        body.contains("<VersionId>null</VersionId>"),
        "Should have version_id null"
    );
    assert!(
        body.contains("<IsLatest>true</IsLatest>"),
        "Should be marked as latest"
    );
    assert!(body.contains("file0.txt"), "Should contain file0.txt");
    assert!(body.contains("file1.txt"), "Should contain file1.txt");
    assert!(body.contains("file2.txt"), "Should contain file2.txt");

    // Test 2: List with prefix
    let response = http_client
        .get(format!("{}/versions-test?versions&prefix=file1", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("file1.txt"), "Should contain file1.txt");
    assert!(!body.contains("file0.txt"), "Should NOT contain file0.txt");
    assert!(!body.contains("file2.txt"), "Should NOT contain file2.txt");

    // Test 3: Non-existent bucket should return 404
    let response = http_client
        .get(format!("{}/nonexistent-bucket?versions", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_post_object() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("post-test")
        .send()
        .await
        .unwrap();

    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    // Test 1: Basic POST object upload with multipart/form-data
    let form = reqwest::multipart::Form::new()
        .text("key", "uploaded/file.txt")
        .text("Content-Type", "text/plain")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"Hello from POST upload!".to_vec())
                .file_name("file.txt")
                .mime_str("text/plain")
                .unwrap(),
        );

    let response = http_client
        .post(format!("{}/post-test", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204, "Basic POST should return 204");
    assert!(
        response.headers().contains_key("etag"),
        "Should have ETag header"
    );

    // Verify the object was created
    let get_result = client
        .get_object()
        .bucket("post-test")
        .key("uploaded/file.txt")
        .send()
        .await;
    assert!(
        get_result.is_ok(),
        "Should be able to get the uploaded object"
    );
    let body = get_result.unwrap().body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), b"Hello from POST upload!");

    // Test 2: POST with success_action_status=201 should return XML
    let form = reqwest::multipart::Form::new()
        .text("key", "xml-response.txt")
        .text("success_action_status", "201")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"Content for 201 response".to_vec())
                .file_name("test.txt"),
        );

    let response = http_client
        .post(format!("{}/post-test", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201, "Should return 201 when requested");
    let body = response.text().await.unwrap();
    assert!(
        body.contains("<PostResponse>"),
        "Should return XML response"
    );
    assert!(
        body.contains("<Bucket>post-test</Bucket>"),
        "Should contain bucket name"
    );
    assert!(
        body.contains("<Key>xml-response.txt</Key>"),
        "Should contain key"
    );

    // Test 3: POST with success_action_status=200 should return 200
    let form = reqwest::multipart::Form::new()
        .text("key", "status-200.txt")
        .text("success_action_status", "200")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"Content for 200 response".to_vec())
                .file_name("test.txt"),
        );

    let response = http_client
        .post(format!("{}/post-test", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "Should return 200 when requested");

    // Test 4: POST with custom metadata
    let form = reqwest::multipart::Form::new()
        .text("key", "with-metadata.txt")
        .text("x-amz-meta-custom-key", "custom-value")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"Content with metadata".to_vec())
                .file_name("test.txt"),
        );

    let response = http_client
        .post(format!("{}/post-test", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // Verify metadata was stored
    let head_result = client
        .head_object()
        .bucket("post-test")
        .key("with-metadata.txt")
        .send()
        .await;
    assert!(head_result.is_ok());
    let metadata = head_result.unwrap().metadata().unwrap().clone();
    assert_eq!(
        metadata.get("custom-key"),
        Some(&"custom-value".to_string())
    );

    // Test 5: POST to non-existent bucket should return 404
    let form = reqwest::multipart::Form::new()
        .text("key", "test.txt")
        .part(
            "file",
            reqwest::multipart::Part::bytes(b"test".to_vec()).file_name("test.txt"),
        );

    let response = http_client
        .post(format!("{}/nonexistent-bucket", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);

    // Test 6: POST without key field should return 400
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"test".to_vec()).file_name("test.txt"),
    );

    let response = http_client
        .post(format!("{}/post-test", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400, "Missing key should return 400");

    // Test 7: POST without file field should return 400
    let form = reqwest::multipart::Form::new().text("key", "test.txt");

    let response = http_client
        .post(format!("{}/post-test", base_url))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400, "Missing file should return 400");
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
