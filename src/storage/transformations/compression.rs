//! Compression transformation implementation

use super::image::Transformer;
use super::types::*;
use async_trait::async_trait;
use bytes::Bytes;
use std::io::{Read, Write};

/// Compression transformer
pub struct CompressionTransformer;

#[async_trait]
impl Transformer for CompressionTransformer {
    fn supports(&self, transformation: &TransformationType) -> bool {
        matches!(transformation, TransformationType::Compression(_))
    }

    async fn transform(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError> {
        let TransformationType::Compression(params) = transformation else {
            return Err(TransformationError::UnsupportedFormat(
                "Not a compression transformation".to_string(),
            ));
        };

        let original_size = data.len();

        let compressed_data = match params.algorithm {
            CompressionAlgorithm::Zstd => compress_zstd(data, params.level.unwrap_or(3))?,
            CompressionAlgorithm::Gzip => compress_gzip(data, params.level.unwrap_or(6))?,
            CompressionAlgorithm::Lz4 => compress_lz4(data)?,
        };

        let compressed_size = compressed_data.len();
        let ratio = if original_size > 0 {
            compressed_size as f64 / original_size as f64
        } else {
            1.0
        };

        let mut result =
            TransformationResult::new(Bytes::from(compressed_data), "application/octet-stream");
        result = result
            .with_metadata("algorithm", format!("{:?}", params.algorithm))
            .with_metadata("original_size", original_size.to_string())
            .with_metadata("compressed_size", compressed_size.to_string())
            .with_metadata("compression_ratio", format!("{:.2}", ratio))
            .with_metadata("content_encoding", params.algorithm.content_encoding());

        if let Some(level) = params.level {
            result = result.with_metadata("compression_level", level.to_string());
        }

        Ok(result)
    }
}

/// Compress data using Zstandard
fn compress_zstd(data: &[u8], level: i32) -> Result<Vec<u8>, TransformationError> {
    let level = level.clamp(1, 22);
    zstd::encode_all(data, level).map_err(|e| {
        TransformationError::CompressionError(format!("Zstd compression failed: {}", e))
    })
}

/// Compress data using Gzip
fn compress_gzip(data: &[u8], level: i32) -> Result<Vec<u8>, TransformationError> {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let level = level.clamp(1, 9) as u32;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::new(level));
    encoder
        .write_all(data)
        .map_err(|e| TransformationError::CompressionError(format!("Gzip write failed: {}", e)))?;
    encoder
        .finish()
        .map_err(|e| TransformationError::CompressionError(format!("Gzip finish failed: {}", e)))
}

/// Compress data using LZ4
fn compress_lz4(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    Ok(lz4_flex::compress_prepend_size(data))
}

/// Decompress data based on algorithm
pub fn decompress(
    data: &[u8],
    algorithm: CompressionAlgorithm,
) -> Result<Vec<u8>, TransformationError> {
    match algorithm {
        CompressionAlgorithm::Zstd => decompress_zstd(data),
        CompressionAlgorithm::Gzip => decompress_gzip(data),
        CompressionAlgorithm::Lz4 => decompress_lz4(data),
    }
}

/// Decompress Zstandard data
fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    zstd::decode_all(data).map_err(|e| {
        TransformationError::CompressionError(format!("Zstd decompression failed: {}", e))
    })
}

/// Decompress Gzip data
fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    use flate2::read::GzDecoder;

    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).map_err(|e| {
        TransformationError::CompressionError(format!("Gzip decompression failed: {}", e))
    })?;
    Ok(decompressed)
}

/// Decompress LZ4 data
fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    lz4_flex::decompress_size_prepended(data).map_err(|e| {
        TransformationError::CompressionError(format!("LZ4 decompression failed: {}", e))
    })
}
