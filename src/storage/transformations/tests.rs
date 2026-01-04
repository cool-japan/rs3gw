//! Comprehensive tests for transformations module

use super::*;
use bytes::Bytes;

// ============================================================================
// Image Transformation Tests
// ============================================================================

#[tokio::test]
async fn test_image_resize_by_width() {
    let transformer = ImageTransformer;
    let test_image = create_test_image();

    let params = ImageTransformParams {
        width: Some(100),
        height: None,
        resize_mode: ResizeMode::ByWidth,
        ..Default::default()
    };

    let result = transformer
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    assert_eq!(
        result
            .metadata
            .get("transformed_width")
            .expect("transformed_width should be present"),
        "100"
    );
    assert!(!result.data.is_empty());
}

#[tokio::test]
async fn test_image_resize_by_height() {
    let transformer = ImageTransformer;
    let test_image = create_test_image();

    let params = ImageTransformParams {
        width: None,
        height: Some(100),
        resize_mode: ResizeMode::ByHeight,
        ..Default::default()
    };

    let result = transformer
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    assert_eq!(
        result
            .metadata
            .get("transformed_height")
            .expect("transformed_height should be present"),
        "100"
    );
}

#[tokio::test]
async fn test_image_resize_fit() {
    let transformer = ImageTransformer;
    let test_image = create_test_image();

    let params = ImageTransformParams {
        width: Some(100),
        height: Some(100),
        resize_mode: ResizeMode::Fit,
        ..Default::default()
    };

    let result = transformer
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    // Both dimensions should be <= 100
    let width: u32 = result
        .metadata
        .get("transformed_width")
        .expect("transformed_width should be present")
        .parse()
        .expect("transformed_width should be a valid u32");
    let height: u32 = result
        .metadata
        .get("transformed_height")
        .expect("transformed_height should be present")
        .parse()
        .expect("transformed_height should be a valid u32");

    assert!(width <= 100);
    assert!(height <= 100);
}

#[tokio::test]
async fn test_image_format_conversion() {
    let transformer = ImageTransformer;
    let test_image = create_test_image();

    let params = ImageTransformParams {
        format: Some(ImageFormat::Jpeg),
        quality: Some(85),
        ..Default::default()
    };

    let result = transformer
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    assert_eq!(result.content_type, "image/jpeg");
    assert_eq!(
        result
            .metadata
            .get("quality")
            .expect("quality should be present"),
        "85"
    );
}

#[tokio::test]
async fn test_image_resize_exact() {
    let transformer = ImageTransformer;
    let test_image = create_test_image();

    let params = ImageTransformParams {
        format: None,
        width: Some(100),
        height: Some(100),
        quality: Some(85),
        resize_mode: ResizeMode::Exact,
    };

    let result = transformer
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    // Should resize to exactly 100x100
    assert_eq!(
        result
            .metadata
            .get("transformed_width")
            .expect("transformed_width should be present"),
        "100"
    );
    assert_eq!(
        result
            .metadata
            .get("transformed_height")
            .expect("transformed_height should be present"),
        "100"
    );
}

#[tokio::test]
async fn test_image_resize_fill() {
    let transformer = ImageTransformer;
    let test_image = create_test_image();

    let params = ImageTransformParams {
        width: Some(100),
        height: Some(100),
        resize_mode: ResizeMode::Fill,
        ..Default::default()
    };

    let result = transformer
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    // Output should be exactly the requested dimensions
    assert_eq!(
        result
            .metadata
            .get("transformed_width")
            .expect("transformed_width should be present"),
        "100"
    );
    assert_eq!(
        result
            .metadata
            .get("transformed_height")
            .expect("transformed_height should be present"),
        "100"
    );
}

// ============================================================================
// Compression Tests
// ============================================================================

#[tokio::test]
async fn test_compression_zstd() {
    let transformer = CompressionTransformer;
    let test_data = b"Hello, world! This is test data for compression.";

    let params = CompressionParams {
        algorithm: CompressionAlgorithm::Zstd,
        level: Some(3),
    };

    let result = transformer
        .transform(test_data, &TransformationType::Compression(params))
        .await
        .expect("Compression should succeed");

    assert!(!result.data.is_empty());
    assert_eq!(
        result
            .metadata
            .get("algorithm")
            .expect("algorithm should be present"),
        "Zstd"
    );
    assert_eq!(
        result
            .metadata
            .get("original_size")
            .expect("original_size should be present"),
        &test_data.len().to_string()
    );
}

#[tokio::test]
async fn test_compression_gzip() {
    let transformer = CompressionTransformer;
    let test_data = b"Test data for gzip compression algorithm testing.";

    let params = CompressionParams {
        algorithm: CompressionAlgorithm::Gzip,
        level: Some(6),
    };

    let result = transformer
        .transform(test_data, &TransformationType::Compression(params))
        .await
        .expect("Compression should succeed");

    assert!(!result.data.is_empty());
    assert_eq!(
        result
            .metadata
            .get("algorithm")
            .expect("algorithm should be present"),
        "Gzip"
    );
    assert_eq!(
        result
            .metadata
            .get("compression_level")
            .expect("compression_level should be present"),
        "6"
    );
}

#[tokio::test]
async fn test_compression_lz4() {
    let transformer = CompressionTransformer;
    let test_data = b"LZ4 compression test data with some repetition repetition.";

    let params = CompressionParams {
        algorithm: CompressionAlgorithm::Lz4,
        level: None,
    };

    let result = transformer
        .transform(test_data, &TransformationType::Compression(params))
        .await
        .expect("Compression should succeed");

    assert!(!result.data.is_empty());
    assert_eq!(
        result
            .metadata
            .get("algorithm")
            .expect("algorithm should be present"),
        "Lz4"
    );
}

#[tokio::test]
async fn test_decompression_zstd() {
    let test_data = b"Data to compress and decompress";

    let params = CompressionParams {
        algorithm: CompressionAlgorithm::Zstd,
        level: Some(3),
    };

    let transformer = CompressionTransformer;
    let compressed = transformer
        .transform(test_data, &TransformationType::Compression(params))
        .await
        .expect("Compression should succeed");

    let decompressed = compression::decompress(&compressed.data, CompressionAlgorithm::Zstd)
        .expect("Decompression should succeed");

    assert_eq!(decompressed, test_data);
}

// ============================================================================
// Manager Tests
// ============================================================================

#[tokio::test]
async fn test_transformation_manager() {
    let manager = TransformationManager::new();
    let test_image = create_test_image();

    let params = ImageTransformParams {
        width: Some(50),
        resize_mode: ResizeMode::ByWidth,
        ..Default::default()
    };

    let result = manager
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    assert_eq!(
        result
            .metadata
            .get("transformed_width")
            .expect("transformed_width should be present"),
        "50"
    );
}

#[tokio::test]
async fn test_transformation_cache() {
    let manager = TransformationManager::with_cache(100, 60);
    let test_image = create_test_image();

    let params = ImageTransformParams {
        width: Some(75),
        resize_mode: ResizeMode::ByWidth,
        ..Default::default()
    };

    // First call - not cached
    let result1 = manager
        .transform(&test_image, &TransformationType::Image(params.clone()))
        .await
        .expect("Transform should succeed");

    // Second call - should be cached
    let result2 = manager
        .transform(&test_image, &TransformationType::Image(params))
        .await
        .expect("Transform should succeed");

    assert_eq!(result1.data, result2.data);

    // Check cache stats
    let stats = manager.cache_stats().await;
    let (entries, max) = stats.expect("Cache stats should be present");
    assert_eq!(entries, 1);
    assert_eq!(max, 100);
}

#[tokio::test]
async fn test_transformation_chain() {
    let manager = TransformationManager::new();
    let test_image = create_test_image();

    let transformations = vec![
        TransformationType::Image(ImageTransformParams {
            width: Some(200),
            resize_mode: ResizeMode::ByWidth,
            ..Default::default()
        }),
        TransformationType::Compression(CompressionParams {
            algorithm: CompressionAlgorithm::Zstd,
            level: Some(5),
        }),
    ];

    let result = manager
        .transform_chain(Bytes::from(test_image), &transformations)
        .await
        .expect("Chain transform should succeed");

    // Should have metadata from both transformations
    assert!(result.metadata.contains_key("transformed_width"));
    assert!(result.metadata.contains_key("algorithm"));
}

#[tokio::test]
async fn test_cache_cleanup() {
    let manager = TransformationManager::with_cache(10, 1); // 1 second TTL
    let test_data = b"test";

    let params = CompressionParams {
        algorithm: CompressionAlgorithm::Zstd,
        level: Some(1),
    };

    // Add entry to cache
    let _ = manager
        .transform(test_data, &TransformationType::Compression(params))
        .await
        .expect("Transform should succeed");

    // Cache should have 1 entry
    let (entries, _) = manager
        .cache_stats()
        .await
        .expect("Cache stats should be present");
    assert_eq!(entries, 1);

    // Wait for TTL to expire
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Cleanup
    manager.cleanup_cache().await;

    // Cache should be empty
    let (entries, _) = manager
        .cache_stats()
        .await
        .expect("Cache stats should be present");
    assert_eq!(entries, 0);
}

#[tokio::test]
async fn test_cache_lru_eviction() {
    let manager = TransformationManager::with_cache(2, 60); // Max 2 entries

    let params1 = CompressionParams {
        algorithm: CompressionAlgorithm::Zstd,
        level: Some(1),
    };
    let params2 = CompressionParams {
        algorithm: CompressionAlgorithm::Gzip,
        level: Some(1),
    };
    let params3 = CompressionParams {
        algorithm: CompressionAlgorithm::Lz4,
        level: None,
    };

    // Add 3 entries (should evict LRU)
    let _ = manager
        .transform(b"test1", &TransformationType::Compression(params1))
        .await;
    let _ = manager
        .transform(b"test2", &TransformationType::Compression(params2))
        .await;
    let _ = manager
        .transform(b"test3", &TransformationType::Compression(params3))
        .await;

    // Should only have 2 entries
    let (entries, max) = manager
        .cache_stats()
        .await
        .expect("Cache stats should be present");
    assert_eq!(entries, 2);
    assert_eq!(max, 2);
}

// ============================================================================
// WASM Plugin Tests
// ============================================================================

#[tokio::test]
async fn test_wasm_plugin_registration() {
    let transformer = WasmPluginTransformer::new();

    // Try to register a plugin (will fail without feature, but shouldn't panic)
    let result = transformer.register_plugin("test", &[]).await;

    #[cfg(not(feature = "wasm-plugins"))]
    assert!(result.is_err());

    #[cfg(feature = "wasm-plugins")]
    {
        // With feature enabled, it should fail due to invalid WASM
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn test_wasm_plugin_transform_without_feature() {
    let transformer = WasmPluginTransformer::new();
    let test_data = b"test data";

    let mut params = std::collections::HashMap::new();
    params.insert("key".to_string(), "value".to_string());

    let transformation = TransformationType::WasmPlugin {
        plugin_name: "test_plugin".to_string(),
        params,
    };

    let result = transformer.transform(test_data, &transformation).await;

    #[cfg(not(feature = "wasm-plugins"))]
    assert!(result.is_err());

    #[cfg(feature = "wasm-plugins")]
    {
        // Should fail because plugin not registered
        assert!(result.is_err());
    }
}

// ============================================================================
// Video Tests
// ============================================================================

#[tokio::test]
async fn test_video_transformer_without_feature() {
    let transformer = VideoTransformer;
    let test_data = b"fake video data";

    let params = VideoTransformParams::default();
    let transformation = TransformationType::Video(params);

    let result = transformer.transform(test_data, &transformation).await;

    #[cfg(not(feature = "video-transcoding"))]
    assert!(result.is_err());

    #[cfg(feature = "video-transcoding")]
    {
        // Should fail due to invalid video data
        assert!(result.is_err());
    }
}

// ============================================================================
// Type Tests
// ============================================================================

#[test]
fn test_image_format_content_type() {
    assert_eq!(ImageFormat::Jpeg.content_type(), "image/jpeg");
    assert_eq!(ImageFormat::Png.content_type(), "image/png");
    assert_eq!(ImageFormat::WebP.content_type(), "image/webp");
}

#[test]
fn test_compression_algorithm_encoding() {
    assert_eq!(CompressionAlgorithm::Zstd.content_encoding(), "zstd");
    assert_eq!(CompressionAlgorithm::Gzip.content_encoding(), "gzip");
    assert_eq!(CompressionAlgorithm::Lz4.content_encoding(), "lz4");
}

#[test]
fn test_video_codec_names() {
    assert_eq!(VideoCodec::H264.as_str(), "h264");
    assert_eq!(VideoCodec::H265.as_str(), "h265");
    assert_eq!(VideoCodec::VP8.as_str(), "vp8");
    assert_eq!(VideoCodec::VP9.as_str(), "vp9");
    assert_eq!(VideoCodec::AV1.as_str(), "av1");
}

#[test]
fn test_transformation_result_metadata() {
    let result = TransformationResult::new(Bytes::from("test"), "text/plain")
        .with_metadata("key1", "value1")
        .with_metadata("key2", "value2");

    assert_eq!(result.content_type, "text/plain");
    assert_eq!(
        result.metadata.get("key1").expect("key1 should be present"),
        "value1"
    );
    assert_eq!(
        result.metadata.get("key2").expect("key2 should be present"),
        "value2"
    );
}

#[test]
fn test_transformation_type_display() {
    let img_transform = TransformationType::Image(ImageTransformParams::default());
    let display = format!("{}", img_transform);
    assert!(display.contains("Image"));

    let comp_transform = TransformationType::Compression(CompressionParams::default());
    let display = format!("{}", comp_transform);
    assert!(display.contains("Compression"));
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a simple test image (1x1 red pixel PNG)
fn create_test_image() -> Vec<u8> {
    // Use explicit paths to avoid conflict with our `image` module
    let img = ::image::ImageBuffer::from_fn(200, 200, |x, y| {
        let r = ((x as f32 / 200.0) * 255.0) as u8;
        let g = ((y as f32 / 200.0) * 255.0) as u8;
        ::image::Rgb([r, g, 128])
    });

    let mut buffer = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buffer, ::image::ImageFormat::Png)
        .expect("Failed to encode test image");
    buffer.into_inner()
}
