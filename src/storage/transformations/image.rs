//! Image transformation implementation

use super::types::*;
use async_trait::async_trait;
use bytes::Bytes;
use image::{imageops::FilterType, DynamicImage, ImageFormat as ImgFormat, ImageReader};
use std::io::Cursor;

/// Trait for transforming data
#[async_trait]
pub trait Transformer: Send + Sync {
    /// Check if this transformer supports the given transformation type
    fn supports(&self, transformation: &TransformationType) -> bool;

    /// Transform the input data
    async fn transform(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError>;
}

/// Image transformer
pub struct ImageTransformer;

#[async_trait]
impl Transformer for ImageTransformer {
    fn supports(&self, transformation: &TransformationType) -> bool {
        matches!(transformation, TransformationType::Image(_))
    }

    async fn transform(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError> {
        let TransformationType::Image(params) = transformation else {
            return Err(TransformationError::UnsupportedFormat(
                "Not an image transformation".to_string(),
            ));
        };

        // Load the image
        let img = ImageReader::new(Cursor::new(data))
            .with_guessed_format()
            .map_err(|e| TransformationError::ImageError(format!("Failed to guess format: {}", e)))?
            .decode()
            .map_err(|e| {
                TransformationError::ImageError(format!("Failed to decode image: {}", e))
            })?;

        let original_width = img.width();
        let original_height = img.height();

        // Apply transformations
        let mut transformed = img;

        // Resize if dimensions are specified
        if params.width.is_some() || params.height.is_some() {
            transformed = resize_image(transformed, params)?;
        }

        // Convert format if specified
        let (output_format, content_type) = if let Some(fmt) = params.format {
            (img_format_to_image_format(fmt), fmt.content_type())
        } else {
            // Keep original format (default to PNG if unknown)
            (ImgFormat::Png, "image/png")
        };

        // Encode the image
        let mut output = Vec::new();
        transformed
            .write_to(&mut Cursor::new(&mut output), output_format)
            .map_err(|e| {
                TransformationError::ImageError(format!("Failed to encode image: {}", e))
            })?;

        // Build result with metadata
        let output_len = output.len();
        let mut result = TransformationResult::new(Bytes::from(output), content_type);
        result = result
            .with_metadata("original_width", original_width.to_string())
            .with_metadata("original_height", original_height.to_string())
            .with_metadata("transformed_width", transformed.width().to_string())
            .with_metadata("transformed_height", transformed.height().to_string());

        if let Some(quality) = params.quality {
            result = result.with_metadata("quality", quality.to_string());
        }

        result = result.with_metadata("resize_mode", format!("{:?}", params.resize_mode));
        result = result.with_metadata("output_size", output_len.to_string());

        Ok(result)
    }
}

/// Resize an image based on parameters
fn resize_image(
    img: DynamicImage,
    params: &ImageTransformParams,
) -> Result<DynamicImage, TransformationError> {
    let original_width = img.width();
    let original_height = img.height();

    let (target_width, target_height) = match params.resize_mode {
        ResizeMode::ByWidth => {
            let width = params.width.ok_or_else(|| {
                TransformationError::InvalidParameters(
                    "Width required for ByWidth mode".to_string(),
                )
            })?;
            let ratio = width as f64 / original_width as f64;
            let height = (original_height as f64 * ratio).round() as u32;
            (width, height)
        }
        ResizeMode::ByHeight => {
            let height = params.height.ok_or_else(|| {
                TransformationError::InvalidParameters(
                    "Height required for ByHeight mode".to_string(),
                )
            })?;
            let ratio = height as f64 / original_height as f64;
            let width = (original_width as f64 * ratio).round() as u32;
            (width, height)
        }
        ResizeMode::Exact => {
            let width = params.width.unwrap_or(original_width);
            let height = params.height.unwrap_or(original_height);
            (width, height)
        }
        ResizeMode::Fit => {
            let width = params.width.unwrap_or(original_width);
            let height = params.height.unwrap_or(original_height);

            // Calculate scaling to fit within bounds
            let width_ratio = width as f64 / original_width as f64;
            let height_ratio = height as f64 / original_height as f64;
            let ratio = width_ratio.min(height_ratio);

            let new_width = (original_width as f64 * ratio).round() as u32;
            let new_height = (original_height as f64 * ratio).round() as u32;
            (new_width, new_height)
        }
        ResizeMode::Fill => {
            let width = params.width.unwrap_or(original_width);
            let height = params.height.unwrap_or(original_height);

            // Calculate scaling to fill bounds (may crop)
            let width_ratio = width as f64 / original_width as f64;
            let height_ratio = height as f64 / original_height as f64;
            let ratio = width_ratio.max(height_ratio);

            let new_width = (original_width as f64 * ratio).round() as u32;
            let new_height = (original_height as f64 * ratio).round() as u32;

            // Resize then crop to exact dimensions
            let resized = img.resize(new_width, new_height, FilterType::Lanczos3);
            let x_offset = (new_width - width) / 2;
            let y_offset = (new_height - height) / 2;
            return Ok(resized.crop_imm(x_offset, y_offset, width, height));
        }
    };

    Ok(img.resize(target_width, target_height, FilterType::Lanczos3))
}

/// Convert our ImageFormat to image crate's ImageFormat
fn img_format_to_image_format(fmt: ImageFormat) -> ImgFormat {
    match fmt {
        ImageFormat::Jpeg => ImgFormat::Jpeg,
        ImageFormat::Png => ImgFormat::Png,
        ImageFormat::WebP => ImgFormat::WebP,
        ImageFormat::Gif => ImgFormat::Gif,
        ImageFormat::Bmp => ImgFormat::Bmp,
        ImageFormat::Tiff => ImgFormat::Tiff,
    }
}
