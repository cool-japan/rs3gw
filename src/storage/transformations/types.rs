//! Core types and error definitions for transformations

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

// ============================================================================
// Error Types
// ============================================================================

/// Errors that can occur during transformations
#[derive(Error, Debug)]
pub enum TransformationError {
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),

    #[error("Transformation failed: {0}")]
    TransformationFailed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Image processing error: {0}")]
    ImageError(String),

    #[error("Video processing error: {0}")]
    VideoError(String),

    #[error("Compression error: {0}")]
    CompressionError(String),

    #[error("WASM plugin error: {0}")]
    WasmError(String),
}

// ============================================================================
// Transformation Types
// ============================================================================

/// Type of transformation to apply
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TransformationType {
    /// Image transformation
    Image(ImageTransformParams),
    /// Video transcoding
    Video(VideoTransformParams),
    /// Compression/decompression
    Compression(CompressionParams),
    /// WASM plugin transformation
    WasmPlugin {
        plugin_name: String,
        params: HashMap<String, String>,
    },
}

impl fmt::Display for TransformationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransformationType::Image(params) => write!(f, "Image({:?})", params),
            TransformationType::Video(params) => write!(f, "Video({:?})", params),
            TransformationType::Compression(params) => write!(f, "Compression({:?})", params),
            TransformationType::WasmPlugin { plugin_name, .. } => {
                write!(f, "WasmPlugin({})", plugin_name)
            }
        }
    }
}

// ============================================================================
// Image Transformation Types
// ============================================================================

/// Image transformation parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageTransformParams {
    /// Target image format
    pub format: Option<ImageFormat>,
    /// Resize width (None = keep original)
    pub width: Option<u32>,
    /// Resize height (None = keep original)
    pub height: Option<u32>,
    /// Quality for lossy formats (1-100)
    pub quality: Option<u8>,
    /// Resize mode
    pub resize_mode: ResizeMode,
}

impl Default for ImageTransformParams {
    fn default() -> Self {
        Self {
            format: None,
            width: None,
            height: None,
            quality: Some(85),
            resize_mode: ResizeMode::Fit,
        }
    }
}

/// Image format
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Gif,
    Bmp,
    Tiff,
}

impl ImageFormat {
    pub fn content_type(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
            ImageFormat::WebP => "image/webp",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Bmp => "image/bmp",
            ImageFormat::Tiff => "image/tiff",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
            ImageFormat::WebP => "webp",
            ImageFormat::Gif => "gif",
            ImageFormat::Bmp => "bmp",
            ImageFormat::Tiff => "tiff",
        }
    }
}

/// Image resize mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResizeMode {
    /// Scale to fit within bounds (maintain aspect ratio)
    Fit,
    /// Scale to fill bounds, cropping as needed
    Fill,
    /// Exact dimensions (may distort)
    Exact,
    /// Scale by width only
    ByWidth,
    /// Scale by height only
    ByHeight,
}

// ============================================================================
// Video Transformation Types
// ============================================================================

/// Video transformation parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoTransformParams {
    /// Target video codec
    pub codec: VideoCodec,
    /// Bitrate in kbps
    pub bitrate: Option<u32>,
    /// Frame rate
    pub fps: Option<u32>,
    /// Resolution width
    pub width: Option<u32>,
    /// Resolution height
    pub height: Option<u32>,
    /// Audio codec
    pub audio_codec: Option<String>,
}

impl Default for VideoTransformParams {
    fn default() -> Self {
        Self {
            codec: VideoCodec::H264,
            bitrate: Some(2000),
            fps: Some(30),
            width: None,
            height: None,
            audio_codec: Some("aac".to_string()),
        }
    }
}

/// Video codec
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    H264,
    H265,
    VP8,
    VP9,
    AV1,
}

impl VideoCodec {
    pub fn as_str(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "h265",
            VideoCodec::VP8 => "vp8",
            VideoCodec::VP9 => "vp9",
            VideoCodec::AV1 => "av1",
        }
    }
}

// ============================================================================
// Compression Types
// ============================================================================

/// Compression parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompressionParams {
    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,
    /// Compression level (algorithm-specific)
    pub level: Option<i32>,
}

impl Default for CompressionParams {
    fn default() -> Self {
        Self {
            algorithm: CompressionAlgorithm::Zstd,
            level: Some(3),
        }
    }
}

/// Compression algorithm
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CompressionAlgorithm {
    /// Zstandard compression (levels 1-22)
    Zstd,
    /// Gzip compression (levels 1-9)
    Gzip,
    /// LZ4 compression (fast)
    Lz4,
}

impl CompressionAlgorithm {
    pub fn content_encoding(&self) -> &'static str {
        match self {
            CompressionAlgorithm::Zstd => "zstd",
            CompressionAlgorithm::Gzip => "gzip",
            CompressionAlgorithm::Lz4 => "lz4",
        }
    }
}

// ============================================================================
// Transformation Result
// ============================================================================

/// Result of a transformation
#[derive(Debug, Clone)]
pub struct TransformationResult {
    /// Transformed data
    pub data: Bytes,
    /// Content type after transformation
    pub content_type: String,
    /// Metadata about the transformation
    pub metadata: HashMap<String, String>,
}

impl TransformationResult {
    pub fn new(data: Bytes, content_type: impl Into<String>) -> Self {
        Self {
            data,
            content_type: content_type.into(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}
