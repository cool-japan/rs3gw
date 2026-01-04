# Object Transformations Guide

Comprehensive guide to rs3gw's object transformation capabilities.

## Table of Contents

- [Overview](#overview)
- [Transformation Types](#transformation-types)
- [Feature Flags](#feature-flags)
- [Usage Examples](#usage-examples)
- [API Reference](#api-reference)
- [Performance](#performance)
- [Best Practices](#best-practices)

## Overview

rs3gw provides powerful server-side object transformation capabilities allowing you to:

- **Process data on-the-fly**: Transform objects during upload or download
- **Reduce bandwidth**: Convert formats and compress before transfer
- **Standardize data**: Ensure consistent formats across your storage
- **Extend functionality**: Add custom transformations via WASM plugins

### Supported Transformations

| Type | Feature Flag | Status | Description |
|------|-------------|---------|-------------|
| Image Processing | *default* | ✅ Production | Resize, crop, format conversion |
| Compression | *default* | ✅ Production | Zstd, Gzip, LZ4 compression |
| Video Transcoding | `video-transcoding` | ✅ Production | Multi-codec video conversion |
| WASM Plugins | `wasm-plugins` | ✅ Production | Custom extensible transformations |

## Transformation Types

### 1. Image Processing

Transform images with high-quality resizing, format conversion, and optimization.

**Features**:
- Multiple resize modes (exact, fit, crop, by-width, by-height)
- Format conversion (JPEG, PNG, WebP, GIF, BMP, TIFF)
- Quality control for lossy formats
- Lanczos3 filtering for high-quality resizing
- Aspect ratio preservation

**Example**:
```rust
use rs3gw::storage::transformations::{
    TransformationType, ImageTransformParams, ImageFormat
};

let transform = TransformationType::Image {
    params: ImageTransformParams {
        width: Some(800),
        height: Some(600),
        format: Some(ImageFormat::Webp),
        quality: Some(85),
        maintain_aspect_ratio: true,
        crop_mode: None,
    }
};
```

**Supported Formats**:
- **Input**: JPEG, PNG, WebP, GIF, BMP, TIFF, ICO
- **Output**: JPEG, PNG, WebP, GIF, BMP, TIFF

**Resize Modes**:
- **Exact**: Resize to exact dimensions (may distort)
- **Fit**: Resize to fit within dimensions (preserves aspect ratio)
- **Crop**: Resize and crop to exact dimensions
- **By Width**: Resize by width, calculate height
- **By Height**: Resize by height, calculate width

### 2. Video Transcoding

Transcode videos between formats with configurable quality parameters.

**Requires**: `video-transcoding` feature flag

**Features**:
- Multi-codec support (H.264, H.265, VP8, VP9, AV1)
- Bitrate control
- Frame rate adjustment
- Resolution scaling
- Audio codec selection

**Example**:
```rust
use rs3gw::storage::transformations::{
    TransformationType, VideoTransformParams, VideoCodec
};

let transform = TransformationType::Video {
    params: VideoTransformParams {
        codec: VideoCodec::H264,
        bitrate: Some(2000),  // 2000 kbps
        fps: Some(30),
        width: Some(1920),
        height: Some(1080),
        audio_codec: Some("aac".to_string()),
        audio_bitrate: Some(128),
    }
};
```

**Supported Codecs**:
- **H.264**: Wide compatibility, good compression
- **H.265/HEVC**: Better compression, newer devices
- **VP8**: WebM format, royalty-free
- **VP9**: Improved WebM, better than VP8
- **AV1**: Next-generation, best compression

### 3. Compression/Decompression

Transparent compression for bandwidth and storage optimization.

**Features**:
- Multiple algorithms (Zstd, Gzip, LZ4)
- Configurable compression levels
- Automatic compression ratio tracking
- Skip compression for incompressible data

**Example**:
```rust
use rs3gw::storage::transformations::{
    TransformationType, CompressionParams, CompressionAlgorithm
};

// Compression
let compress = TransformationType::Compress {
    params: CompressionParams {
        algorithm: CompressionAlgorithm::Zstd,
        level: Some(3),  // 1-22 for Zstd
    }
};

// Decompression
let decompress = TransformationType::Decompress {
    algorithm: CompressionAlgorithm::Zstd
};
```

**Algorithms**:
- **Zstd**: Best compression ratio, configurable levels (1-22)
- **Gzip**: Standard deflate compression, levels (1-9)
- **LZ4**: Fastest compression, minimal CPU overhead

### 4. WASM Plugins

Extend rs3gw with custom transformations via WebAssembly plugins.

**Requires**: `wasm-plugins` feature flag

**Features**:
- Safe sandboxed execution
- Custom parameter passing
- Near-native performance
- Plugin versioning and management

**Example**:
```rust
use rs3gw::storage::transformations::{
    WasmPluginTransformer, TransformationType
};
use std::collections::HashMap;

// Register plugin
let transformer = WasmPluginTransformer::new();
let wasm_binary = std::fs::read("plugins/my-plugin.wasm")?;
transformer.register_plugin("my-plugin".to_string(), wasm_binary).await?;

// Use plugin
let mut params = HashMap::new();
params.insert("mode".to_string(), "fast".to_string());

let transform = TransformationType::WasmPlugin {
    plugin_name: "my-plugin".to_string(),
    params,
};
```

See [WASM Plugin Developer Guide](wasm_plugins.md) for creating custom plugins.

## Feature Flags

rs3gw uses Cargo feature flags to manage optional transformations, especially those with C/FFI dependencies.

### Default Features

Enabled by default (Pure Rust):
```toml
# Cargo.toml
[features]
default = []  # Pure Rust only

[dependencies]
image = "0.25.9"        # Image processing
zstd = "0.13"          # Compression
lz4_flex = "0.12"      # LZ4 compression
flate2 = "1.0"         # Gzip compression
```

### Optional Features

#### video-transcoding

Enable FFmpeg-based video transcoding:
```toml
[features]
video-transcoding = ["dep:ffmpeg-next"]

[dependencies]
ffmpeg-next = { version = "8.0", optional = true }
```

**Build with feature**:
```bash
cargo build --features video-transcoding
```

**Requirements**:
- FFmpeg 4.0+ installed on system
- C compiler (gcc/clang)

**Platform Notes**:
- Linux: `apt install ffmpeg libavcodec-dev libavformat-dev`
- macOS: `brew install ffmpeg`
- Windows: Download FFmpeg from official site

#### wasm-plugins

Enable WebAssembly plugin system:
```toml
[features]
wasm-plugins = ["dep:wasmtime"]

[dependencies]
wasmtime = { version = "40.0", optional = true }
```

**Build with feature**:
```bash
cargo build --features wasm-plugins
```

**Requirements**:
- None (Pure Rust)

#### All Features

Build with all features:
```bash
cargo build --all-features
```

### Runtime Behavior

When features are disabled, transformation requests return a placeholder result with a warning log:

```rust
// Without video-transcoding feature
tracing::warn!("Video transcoding requested but video-transcoding feature is not enabled");
// Returns input data unchanged
```

This allows graceful degradation without runtime errors.

## Usage Examples

### Via Rust API

```rust
use rs3gw::storage::transformations::{
    TransformationManager, TransformationType,
    ImageTransformParams, ImageFormat
};

// Create transformation manager
let manager = TransformationManager::new();

// Prepare image transformation
let transform = TransformationType::Image {
    params: ImageTransformParams {
        width: Some(800),
        height: None,
        format: Some(ImageFormat::Webp),
        quality: Some(85),
        maintain_aspect_ratio: true,
        crop_mode: None,
    }
};

// Execute transformation
let input_data = std::fs::read("input.jpg")?;
let result = manager.transform(&input_data, &transform).await?;

// Save result
std::fs::write("output.webp", &result.data)?;
println!("Content-Type: {}", result.content_type.unwrap());
```

### Via S3 API (Future)

Transformations can be requested via S3 metadata:

```python
import boto3

s3 = boto3.client('s3', endpoint_url='http://localhost:9000')

# Upload with transformation
s3.put_object(
    Bucket='mybucket',
    Key='image.jpg',
    Body=open('input.jpg', 'rb'),
    Metadata={
        'x-amz-meta-transform': 'image',
        'x-amz-meta-width': '800',
        'x-amz-meta-format': 'webp',
        'x-amz-meta-quality': '85'
    }
)

# Download with on-the-fly transformation
response = s3.get_object(
    Bucket='mybucket',
    Key='video.mp4',
    Metadata={
        'x-amz-meta-transform': 'video',
        'x-amz-meta-codec': 'h264',
        'x-amz-meta-bitrate': '2000'
    }
)
```

### Chaining Transformations

```rust
// Transform pipeline: decompress -> process -> compress
let decompressed = manager.transform(
    &compressed_data,
    &TransformationType::Decompress {
        algorithm: CompressionAlgorithm::Zstd
    }
).await?;

let processed = manager.transform(
    &decompressed.data,
    &TransformationType::Image {
        params: image_params
    }
).await?;

let final_result = manager.transform(
    &processed.data,
    &TransformationType::Compress {
        params: CompressionParams {
            algorithm: CompressionAlgorithm::Zstd,
            level: Some(5),
        }
    }
).await?;
```

## API Reference

### TransformationManager

Main interface for transformation operations.

```rust
pub struct TransformationManager {
    transformers: Vec<Arc<dyn Transformer>>,
}

impl TransformationManager {
    /// Create with default transformers
    pub fn new() -> Self;

    /// Register custom transformer
    pub fn register_transformer(&mut self, transformer: Arc<dyn Transformer>);

    /// Execute transformation
    pub async fn transform(
        &self,
        data: &[u8],
        transform_type: &TransformationType,
    ) -> Result<TransformationResult, TransformationError>;
}
```

### Transformer Trait

Implement for custom transformers:

```rust
#[async_trait]
pub trait Transformer: Send + Sync {
    async fn transform(
        &self,
        data: &[u8],
        params: &TransformationType,
    ) -> Result<TransformationResult, TransformationError>;

    fn supports(&self, transform_type: &TransformationType) -> bool;
}
```

### TransformationResult

```rust
pub struct TransformationResult {
    pub data: Bytes,
    pub content_type: Option<String>,
    pub metadata: HashMap<String, String>,
}
```

### Error Types

```rust
pub enum TransformationError {
    UnsupportedFormat(String),
    InvalidParameters(String),
    TransformationFailed(String),
    WasmPluginError(String),
    IoError(std::io::Error),
}
```

## Performance

### Benchmarks

Typical performance on modern hardware (i7/M1):

| Operation | Input Size | Time | Throughput |
|-----------|------------|------|------------|
| Image Resize (800x600) | 2MB JPEG | ~50ms | 40 MB/s |
| Image Format (JPEG→WebP) | 2MB | ~60ms | 33 MB/s |
| Video Transcode (H.264) | 100MB | ~15s | 6.7 MB/s |
| Zstd Compression (level 3) | 10MB | ~100ms | 100 MB/s |
| WASM Plugin (simple) | 1MB | ~5ms | 200 MB/s |

### Optimization Tips

1. **Use appropriate compression levels**
   - Zstd level 3 is good balance for most cases
   - Higher levels (10+) offer diminishing returns

2. **Choose right image format**
   - WebP: Best compression for photos
   - PNG: Lossless, good for graphics/screenshots
   - JPEG: Universal compatibility

3. **Optimize video settings**
   - Lower bitrate for streaming
   - Higher for archival
   - Consider VP9/AV1 for modern clients

4. **Cache transformed objects**
   - Store frequently accessed transformations
   - Use rs3gw's built-in caching layer

5. **Batch operations**
   - Process multiple objects in parallel
   - Use async/await for concurrency

## Best Practices

### 1. Choose Appropriate Transformations

```rust
// Good: Reasonable resize for web
let web_image = ImageTransformParams {
    width: Some(1200),
    height: None,
    format: Some(ImageFormat::Webp),
    quality: Some(85),
    maintain_aspect_ratio: true,
    crop_mode: None,
};

// Bad: Excessive resize wastes resources
let bad = ImageTransformParams {
    width: Some(10000),  // Unnecessarily large
    height: Some(10000),
    ...
};
```

### 2. Handle Errors Gracefully

```rust
match manager.transform(&data, &transform).await {
    Ok(result) => {
        // Process result
    }
    Err(TransformationError::UnsupportedFormat(fmt)) => {
        // Return original or different transformation
        tracing::warn!("Unsupported format: {}", fmt);
    }
    Err(e) => {
        // Log and return error to client
        tracing::error!("Transformation failed: {}", e);
    }
}
```

### 3. Validate Inputs

```rust
// Validate before transformation
if image_params.width.unwrap_or(0) > 10000 {
    return Err(TransformationError::InvalidParameters(
        "Width exceeds maximum allowed".to_string()
    ));
}
```

### 4. Monitor Performance

```rust
use std::time::Instant;

let start = Instant::now();
let result = manager.transform(&data, &transform).await?;
let duration = start.elapsed();

tracing::info!(
    "Transformation completed in {:?} ({} bytes -> {} bytes)",
    duration,
    data.len(),
    result.data.len()
);
```

### 5. Use Feature Flags Appropriately

```rust
#[cfg(feature = "video-transcoding")]
{
    // Video transcoding code
}

#[cfg(not(feature = "video-transcoding"))]
{
    return Err(TransformationError::UnsupportedFormat(
        "Video transcoding requires 'video-transcoding' feature".to_string()
    ));
}
```

## Troubleshooting

### Common Issues

**Problem**: "UnsupportedFormat error"
- **Solution**: Check if input format is supported, verify feature flags

**Problem**: Slow video transcoding
- **Solution**: Lower resolution/bitrate, use hardware acceleration (future)

**Problem**: WASM plugin not found
- **Solution**: Verify plugin is registered, check plugin name spelling

**Problem**: Out of memory during transformation
- **Solution**: Process in chunks, reduce quality settings, increase available memory

### Debug Logging

Enable detailed transformation logs:

```bash
RUST_LOG=rs3gw::storage::transformations=debug cargo run
```

## Future Enhancements

Planned improvements:

- [ ] Hardware-accelerated video encoding (NVENC, QuickSync, VideoToolbox)
- [ ] Transformation caching layer
- [ ] Streaming transformations for large objects
- [ ] WASI support for WASM plugins
- [ ] Additional image filters (blur, sharpen, color adjustment)
- [ ] Audio processing (format conversion, normalization)
- [ ] Document transformations (PDF generation, format conversion)

## Related Documentation

- [WASM Plugin Developer Guide](wasm_plugins.md)
- [Production Deployment Guide](production_deployment.md)
- [Performance Tuning Guide](performance.md)

## Support

For questions and issues:
- GitHub Issues: https://github.com/cool-japan/rs3gw/issues
- Documentation: https://github.com/cool-japan/rs3gw/docs
- Examples: https://github.com/cool-japan/rs3gw/examples
