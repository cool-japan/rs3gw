//! Compression transformation implementation

use super::image::Transformer;
use super::types::*;
use async_trait::async_trait;
use bytes::Bytes;

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

fn compress_zstd(data: &[u8], level: i32) -> Result<Vec<u8>, TransformationError> {
    let level = level.clamp(1, 22);
    oxiarc_zstd::encode_all(data, level).map_err(|e| {
        TransformationError::CompressionError(format!("Zstd compression failed: {}", e))
    })
}

fn compress_gzip(data: &[u8], level: i32) -> Result<Vec<u8>, TransformationError> {
    let level = level.clamp(1, 9) as u8;
    oxiarc_deflate::gzip_compress(data, level).map_err(|e| {
        TransformationError::CompressionError(format!("Gzip compression failed: {}", e))
    })
}

/// Compress using LZ4 block format with size prefix (preserves wire compat with lz4_flex::compress_prepend_size)
fn compress_lz4(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    let compressed = oxiarc_lz4::compress_block(data).map_err(|e| {
        TransformationError::CompressionError(format!("LZ4 compression failed: {}", e))
    })?;
    let mut out = Vec::with_capacity(4 + compressed.len());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
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

fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    oxiarc_zstd::decode_all(data).map_err(|e| {
        TransformationError::CompressionError(format!("Zstd decompression failed: {}", e))
    })
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    oxiarc_deflate::gzip_decompress(data).map_err(|e| {
        TransformationError::CompressionError(format!("Gzip decompression failed: {}", e))
    })
}

/// Decompress LZ4 block data with size prefix (preserves wire compat with lz4_flex::decompress_size_prepended)
fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>, TransformationError> {
    if data.len() < 4 {
        return Err(TransformationError::CompressionError(
            "LZ4 data too short: missing size prefix".to_string(),
        ));
    }
    let size_bytes: [u8; 4] = data[..4].try_into().map_err(|_| {
        TransformationError::CompressionError("LZ4 size prefix read failed".to_string())
    })?;
    let max_output = u32::from_le_bytes(size_bytes) as usize;
    oxiarc_lz4::decompress_block(&data[4..], max_output).map_err(|e| {
        TransformationError::CompressionError(format!("LZ4 decompression failed: {}", e))
    })
}
