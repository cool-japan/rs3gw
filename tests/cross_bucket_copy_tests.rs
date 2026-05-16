//! Integration tests for cross-bucket copy operations

use bytes::Bytes;
use tempfile::TempDir;

#[tokio::test]
async fn test_cross_bucket_copy() {
    let temp_dir = TempDir::new().unwrap();
    let storage = rs3gw::storage::StorageEngine::new(temp_dir.path().to_path_buf()).unwrap();

    // Create two buckets
    storage.create_bucket("source-bucket").await.unwrap();
    storage.create_bucket("dest-bucket").await.unwrap();

    // Put an object in the source bucket
    let data = Bytes::from("Hello, cross-bucket copy!");
    let metadata = std::collections::HashMap::new();
    storage
        .put_object(
            "source-bucket",
            "test-file.txt",
            "text/plain",
            metadata,
            data,
        )
        .await
        .unwrap();

    // Copy to destination bucket
    let result = storage
        .copy_object(
            "source-bucket",
            "test-file.txt",
            "dest-bucket",
            "copied-file.txt",
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(result.key, "copied-file.txt");
    assert_eq!(result.size, 25);

    // Verify the copy exists and has the same content
    let copied_metadata = storage
        .head_object("dest-bucket", "copied-file.txt")
        .await
        .unwrap();
    assert_eq!(copied_metadata.size, 25);
    assert_eq!(copied_metadata.content_type, "text/plain");

    // Read the copied object
    let (_meta, mut stream) = storage
        .get_object("dest-bucket", "copied-file.txt")
        .await
        .unwrap();

    use futures::StreamExt;
    let mut copied_data = Vec::new();
    while let Some(chunk) = stream.next().await {
        copied_data.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(copied_data, b"Hello, cross-bucket copy!");
}

#[tokio::test]
async fn test_cross_bucket_copy_with_metadata_replace() {
    let temp_dir = TempDir::new().unwrap();
    let storage = rs3gw::storage::StorageEngine::new(temp_dir.path().to_path_buf()).unwrap();

    storage.create_bucket("source-bucket").await.unwrap();
    storage.create_bucket("dest-bucket").await.unwrap();

    // Put object with metadata
    let data = Bytes::from("Test data");
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("original-key".to_string(), "original-value".to_string());

    storage
        .put_object("source-bucket", "file.txt", "text/plain", metadata, data)
        .await
        .unwrap();

    // Copy with REPLACE directive and new metadata
    let mut new_metadata = std::collections::HashMap::new();
    new_metadata.insert("new-key".to_string(), "new-value".to_string());

    storage
        .copy_object(
            "source-bucket",
            "file.txt",
            "dest-bucket",
            "copy.txt",
            Some("REPLACE"),
            Some(new_metadata.clone()),
            Some("application/octet-stream"),
        )
        .await
        .unwrap();

    // Verify new metadata
    let copied = storage
        .head_object("dest-bucket", "copy.txt")
        .await
        .unwrap();
    assert_eq!(copied.content_type, "application/octet-stream");
    assert_eq!(copied.metadata.get("new-key").unwrap(), "new-value");
    assert!(!copied.metadata.contains_key("original-key"));
}

#[tokio::test]
async fn test_cross_bucket_copy_preserves_metadata() {
    let temp_dir = TempDir::new().unwrap();
    let storage = rs3gw::storage::StorageEngine::new(temp_dir.path().to_path_buf()).unwrap();

    storage.create_bucket("source-bucket").await.unwrap();
    storage.create_bucket("dest-bucket").await.unwrap();

    // Put object with custom metadata
    let data = Bytes::from("Test data");
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("x-custom-header".to_string(), "custom-value".to_string());

    storage
        .put_object(
            "source-bucket",
            "file.txt",
            "text/html",
            metadata.clone(),
            data,
        )
        .await
        .unwrap();

    // Copy without REPLACE (should preserve metadata)
    storage
        .copy_object(
            "source-bucket",
            "file.txt",
            "dest-bucket",
            "copy.txt",
            None, // No directive means COPY (preserve)
            None,
            None,
        )
        .await
        .unwrap();

    // Verify metadata is preserved
    let copied = storage
        .head_object("dest-bucket", "copy.txt")
        .await
        .unwrap();
    assert_eq!(copied.content_type, "text/html");
    assert_eq!(
        copied.metadata.get("x-custom-header").unwrap(),
        "custom-value"
    );
}

#[tokio::test]
async fn test_cross_bucket_copy_nonexistent_source() {
    let temp_dir = TempDir::new().unwrap();
    let storage = rs3gw::storage::StorageEngine::new(temp_dir.path().to_path_buf()).unwrap();

    storage.create_bucket("source-bucket").await.unwrap();
    storage.create_bucket("dest-bucket").await.unwrap();

    // Try to copy non-existent object
    let result = storage
        .copy_object(
            "source-bucket",
            "nonexistent.txt",
            "dest-bucket",
            "copy.txt",
            None,
            None,
            None,
        )
        .await;

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        rs3gw::storage::StorageError::NotFound(_)
    ));
}

#[tokio::test]
async fn test_cross_bucket_copy_same_bucket() {
    let temp_dir = TempDir::new().unwrap();
    let storage = rs3gw::storage::StorageEngine::new(temp_dir.path().to_path_buf()).unwrap();

    storage.create_bucket("test-bucket").await.unwrap();

    // Put an object
    let data = Bytes::from("Original data");
    storage
        .put_object(
            "test-bucket",
            "original.txt",
            "text/plain",
            std::collections::HashMap::new(),
            data,
        )
        .await
        .unwrap();

    // Copy within the same bucket
    storage
        .copy_object(
            "test-bucket",
            "original.txt",
            "test-bucket",
            "copy.txt",
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Verify both files exist
    let original = storage
        .head_object("test-bucket", "original.txt")
        .await
        .unwrap();
    let copy = storage
        .head_object("test-bucket", "copy.txt")
        .await
        .unwrap();

    assert_eq!(original.size, copy.size);
    assert_eq!(original.content_type, copy.content_type);
}
