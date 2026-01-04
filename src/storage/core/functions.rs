//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

#[cfg(test)]
mod tests {
    use super::super::types::{ByteRange, StorageEngine};
    use bytes::Bytes;
    use std::collections::HashMap;
    #[test]
    fn test_byte_range_parse() {
        let range = ByteRange::parse("bytes=0-99", 1000).expect("Failed to parse byte range");
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 99);
        assert_eq!(range.length(), 100);
        let range = ByteRange::parse("bytes=500-", 1000).expect("Failed to parse open-ended range");
        assert_eq!(range.start, 500);
        assert_eq!(range.end, 999);
        let range = ByteRange::parse("bytes=-200", 1000).expect("Failed to parse suffix range");
        assert_eq!(range.start, 800);
        assert_eq!(range.end, 999);
        assert!(ByteRange::parse("bytes=1000-2000", 1000).is_err());
        assert!(ByteRange::parse("invalid", 1000).is_err());
    }
    #[tokio::test]
    async fn test_storage_engine_basic() {
        let temp_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
        let engine = StorageEngine::new(temp_dir.path().to_path_buf())
            .expect("Failed to create storage engine");
        engine
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create bucket");
        assert!(engine
            .bucket_exists("test-bucket")
            .await
            .expect("Failed to check bucket existence"));
        let data = Bytes::from("hello world");
        let etag = engine
            .put_object(
                "test-bucket",
                "test.txt",
                "text/plain",
                HashMap::new(),
                data,
            )
            .await
            .expect("Failed to put object");
        assert!(!etag.is_empty());
        let (metadata, _stream) = engine
            .get_object("test-bucket", "test.txt")
            .await
            .expect("Failed to get object");
        assert_eq!(metadata.size, 11);
        assert_eq!(metadata.content_type, "text/plain");
        engine
            .delete_object("test-bucket", "test.txt")
            .await
            .expect("Failed to delete object");
        engine
            .delete_bucket("test-bucket")
            .await
            .expect("Failed to delete bucket");
        assert!(!engine
            .bucket_exists("test-bucket")
            .await
            .expect("Failed to check bucket existence"));
    }
}
