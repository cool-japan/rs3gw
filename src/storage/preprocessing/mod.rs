//! Dataset preprocessing pipeline for AI/ML workloads
//!
//! Provides on-the-fly data preprocessing capabilities including:
//! - Image normalization and resizing
//! - Data augmentation
//! - Text tokenization
//! - Audio/video processing
//! - Caching of preprocessing results
//! - Pipeline versioning and reproducibility

pub mod augmentation;
pub mod cache;
pub mod error;
mod image;
pub mod pipeline;
pub mod text;

pub use augmentation::AugmentationConfig;
pub use cache::PreprocessingCache;
pub use error::{PreprocessingError, PreprocessingResult};
pub use image::{
    DatasetResizeMode, ImageNormalizationConfig, ImageResizeConfig, PreprocessingStepType,
};
pub use pipeline::{PipelineBuilder, PipelineDefinition, PreprocessingStep};
pub use text::TokenizationConfig;

use bytes::Bytes;
// Image processing imports - used in public API implementation (apply_pipeline and helpers)
// Note: use ::image:: prefix to avoid ambiguity with our image submodule
#[cfg_attr(not(test), allow(unused_imports))]
use ::image::{
    imageops, load_from_memory, DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Rgb, Rgba,
};
use std::collections::HashMap;
#[cfg_attr(not(test), allow(unused_imports))]
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Preprocessing Pipeline Manager
// ============================================================================

/// Manager for preprocessing pipelines
pub struct PreprocessingManager {
    pipelines: Arc<RwLock<HashMap<String, PipelineDefinition>>>,
    cache: PreprocessingCache,
    storage_path: PathBuf,
}

impl PreprocessingManager {
    pub fn new(storage_path: PathBuf) -> Self {
        Self {
            pipelines: Arc::new(RwLock::new(HashMap::new())),
            cache: PreprocessingCache::new(1024 * 1024 * 1024), // 1GB default cache
            storage_path,
        }
    }

    pub fn with_cache_size(mut self, size_bytes: usize) -> Self {
        self.cache = PreprocessingCache::new(size_bytes);
        self
    }

    /// Register a new pipeline
    pub async fn register_pipeline(&self, pipeline: PipelineDefinition) -> PreprocessingResult<()> {
        let mut pipelines = self.pipelines.write().await;
        pipelines.insert(pipeline.id.clone(), pipeline);
        Ok(())
    }

    /// Get a pipeline by ID
    pub async fn get_pipeline(&self, id: &str) -> PreprocessingResult<PipelineDefinition> {
        let pipelines = self.pipelines.read().await;
        pipelines
            .get(id)
            .cloned()
            .ok_or_else(|| PreprocessingError::PipelineNotFound(id.to_string()))
    }

    /// List all registered pipelines
    pub async fn list_pipelines(&self) -> Vec<PipelineDefinition> {
        let pipelines = self.pipelines.read().await;
        pipelines.values().cloned().collect()
    }

    /// Delete a pipeline
    pub async fn delete_pipeline(&self, id: &str) -> PreprocessingResult<()> {
        let mut pipelines = self.pipelines.write().await;
        pipelines
            .remove(id)
            .ok_or_else(|| PreprocessingError::PipelineNotFound(id.to_string()))?;
        Ok(())
    }

    /// Save pipeline definition to file
    pub async fn save_pipeline_to_file(
        &self,
        pipeline: &PipelineDefinition,
        format: &str,
    ) -> PreprocessingResult<PathBuf> {
        let filename = format!("{}_{}.{}", pipeline.id, pipeline.version, format);
        let filepath = self.storage_path.join("pipelines").join(&filename);

        if let Some(parent) = filepath.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = match format {
            "json" => serde_json::to_string_pretty(pipeline)
                .map_err(|e| PreprocessingError::SerializationError(e.to_string()))?,
            "yaml" | "yml" => serde_yaml::to_string(pipeline)
                .map_err(|e| PreprocessingError::SerializationError(e.to_string()))?,
            _ => return Err(PreprocessingError::UnsupportedFormat(format.to_string())),
        };

        tokio::fs::write(&filepath, content).await?;
        Ok(filepath)
    }

    /// Load pipeline definition from file
    pub async fn load_pipeline_from_file(
        &self,
        path: &Path,
    ) -> PreprocessingResult<PipelineDefinition> {
        let content = tokio::fs::read_to_string(path).await?;

        let pipeline = if path.extension().and_then(|s| s.to_str()) == Some("json") {
            serde_json::from_str(&content)
                .map_err(|e| PreprocessingError::SerializationError(e.to_string()))?
        } else {
            serde_yaml::from_str(&content)
                .map_err(|e| PreprocessingError::SerializationError(e.to_string()))?
        };

        Ok(pipeline)
    }

    /// Apply image normalization
    fn apply_image_normalization(
        &self,
        img: &DynamicImage,
        config: &ImageNormalizationConfig,
    ) -> PreprocessingResult<DynamicImage> {
        let (width, height) = img.dimensions();
        let rgb_img = img.to_rgb8();

        let mut normalized = ImageBuffer::new(width, height);

        for (x, y, pixel) in rgb_img.enumerate_pixels() {
            let mut new_pixel = [0u8; 3];
            for (i, &channel) in pixel.0.iter().enumerate() {
                let mut value = channel as f32;

                // Normalize to [0, 1] if configured
                if config.normalize_range {
                    value /= 255.0;
                }

                // Apply mean and std normalization
                if i < config.mean.len() && i < config.std.len() {
                    value = (value - config.mean[i]) / config.std[i];
                }

                // Clamp and convert back to u8
                value = value.clamp(-1.0, 1.0);
                new_pixel[i] = ((value + 1.0) * 127.5) as u8;
            }

            normalized.put_pixel(x, y, Rgb(new_pixel));
        }

        Ok(DynamicImage::ImageRgb8(normalized))
    }

    /// Apply image resizing
    fn apply_image_resize(
        &self,
        img: &DynamicImage,
        config: &ImageResizeConfig,
    ) -> PreprocessingResult<DynamicImage> {
        let filter = match config.filter.as_str() {
            "nearest" => imageops::FilterType::Nearest,
            "bilinear" => imageops::FilterType::Triangle,
            "bicubic" => imageops::FilterType::CatmullRom,
            "lanczos3" => imageops::FilterType::Lanczos3,
            _ => imageops::FilterType::Triangle,
        };

        let resized = match config.mode {
            DatasetResizeMode::Exact => img.resize_exact(config.width, config.height, filter),
            DatasetResizeMode::Fit => img.resize(config.width, config.height, filter),
            DatasetResizeMode::Fill => {
                // Crop to fill the dimensions
                let (orig_width, orig_height) = img.dimensions();
                let scale_w = config.width as f32 / orig_width as f32;
                let scale_h = config.height as f32 / orig_height as f32;
                let scale = scale_w.max(scale_h);

                let scaled_w = (orig_width as f32 * scale) as u32;
                let scaled_h = (orig_height as f32 * scale) as u32;

                let scaled = img.resize_exact(scaled_w, scaled_h, filter);

                // Crop to target size
                let x_offset = (scaled_w - config.width) / 2;
                let y_offset = (scaled_h - config.height) / 2;

                DynamicImage::ImageRgba8(
                    imageops::crop_imm(&scaled, x_offset, y_offset, config.width, config.height)
                        .to_image(),
                )
            }
            DatasetResizeMode::Stretch => img.resize_exact(config.width, config.height, filter),
        };

        Ok(resized)
    }

    /// Apply data augmentation
    fn apply_data_augmentation(
        &self,
        img: &DynamicImage,
        config: &AugmentationConfig,
    ) -> PreprocessingResult<DynamicImage> {
        use scirs2_core::random::quick::random_f64;
        let mut result = img.clone();

        // Horizontal flip
        if random_f64() < config.horizontal_flip_prob as f64 {
            result = DynamicImage::ImageRgba8(imageops::flip_horizontal(&result));
        }

        // Vertical flip
        if random_f64() < config.vertical_flip_prob as f64 {
            result = DynamicImage::ImageRgba8(imageops::flip_vertical(&result));
        }

        // Rotation (if rotation_range > 0)
        if config.rotation_range > 0.0 {
            let _angle = (random_f64() * 2.0 - 1.0) * config.rotation_range as f64;
            // Note: image crate doesn't have built-in rotation, so we'll skip this for now
            // In production, you'd want to use imageproc crate for rotation
        }

        // Brightness adjustment
        if let Some((min_bright, max_bright)) = config.brightness_range {
            let factor = min_bright + random_f64() as f32 * (max_bright - min_bright);
            result = adjust_brightness(&result, factor);
        }

        // Contrast adjustment
        if let Some((min_contrast, max_contrast)) = config.contrast_range {
            let factor = min_contrast + random_f64() as f32 * (max_contrast - min_contrast);
            result = adjust_contrast(&result, factor);
        }

        Ok(result)
    }

    pub async fn apply_pipeline(
        &self,
        pipeline_id: &str,
        input_data: Bytes,
        _metadata: HashMap<String, String>,
    ) -> PreprocessingResult<Bytes> {
        let pipeline = self.get_pipeline(pipeline_id).await?;

        // Generate cache key
        let cache_key = format!("{}:{}", pipeline.id, pipeline.version);

        // Check cache first
        if let Some(cached) = self.cache.get(&cache_key).await {
            return Ok(cached);
        }

        // Try to load image from input data
        let mut current_image = load_from_memory(&input_data).ok();

        // Apply each preprocessing step
        for step in &pipeline.steps {
            if let Some(ref img) = current_image {
                current_image = match step.step_type {
                    PreprocessingStepType::ImageNormalization => {
                        let config: ImageNormalizationConfig =
                            serde_json::from_value(step.config.clone()).unwrap_or_default();
                        Some(self.apply_image_normalization(img, &config)?)
                    }
                    PreprocessingStepType::ImageResize => {
                        let config: ImageResizeConfig =
                            serde_json::from_value(step.config.clone()).unwrap_or_default();
                        Some(self.apply_image_resize(img, &config)?)
                    }
                    PreprocessingStepType::DataAugmentation => {
                        let config: AugmentationConfig =
                            serde_json::from_value(step.config.clone()).unwrap_or_default();
                        Some(self.apply_data_augmentation(img, &config)?)
                    }
                    _ => {
                        // Unsupported step type for images, skip
                        Some(img.clone())
                    }
                };
            }
        }

        // Convert image back to bytes
        let result = if let Some(img) = current_image {
            let mut buffer: Vec<u8> = Vec::new();
            let mut cursor: Cursor<&mut Vec<u8>> = Cursor::new(&mut buffer);
            img.write_to(&mut cursor, ImageFormat::Png).map_err(|e| {
                PreprocessingError::StepFailed(format!("Failed to encode image: {}", e))
            })?;
            Bytes::from(buffer)
        } else {
            // Not an image, return as-is
            input_data
        };

        // Cache result if configured
        if pipeline.steps.iter().any(|step| step.cache_results) {
            let _ = self
                .cache
                .put(cache_key, result.clone(), HashMap::new())
                .await;
        }

        Ok(result)
    }

    /// Get cache statistics
    pub async fn cache_stats(&self) -> (usize, usize, usize) {
        self.cache.stats().await
    }

    /// Clear preprocessing cache
    pub async fn clear_cache(&self) {
        self.cache.clear().await;
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Adjust image brightness
fn adjust_brightness(img: &DynamicImage, factor: f32) -> DynamicImage {
    let (width, height) = img.dimensions();
    let rgba_img = img.to_rgba8();
    let mut adjusted = ImageBuffer::new(width, height);

    for (x, y, pixel) in rgba_img.enumerate_pixels() {
        let new_pixel = Rgba([
            (pixel[0] as f32 * factor).clamp(0.0, 255.0) as u8,
            (pixel[1] as f32 * factor).clamp(0.0, 255.0) as u8,
            (pixel[2] as f32 * factor).clamp(0.0, 255.0) as u8,
            pixel[3],
        ]);
        adjusted.put_pixel(x, y, new_pixel);
    }

    DynamicImage::ImageRgba8(adjusted)
}

/// Adjust image contrast
fn adjust_contrast(img: &DynamicImage, factor: f32) -> DynamicImage {
    let (width, height) = img.dimensions();
    let rgba_img = img.to_rgba8();
    let mut adjusted = ImageBuffer::new(width, height);

    // Use 128 as the midpoint for contrast adjustment
    let midpoint = 128.0;

    for (x, y, pixel) in rgba_img.enumerate_pixels() {
        let new_pixel = Rgba([
            ((pixel[0] as f32 - midpoint) * factor + midpoint).clamp(0.0, 255.0) as u8,
            ((pixel[1] as f32 - midpoint) * factor + midpoint).clamp(0.0, 255.0) as u8,
            ((pixel[2] as f32 - midpoint) * factor + midpoint).clamp(0.0, 255.0) as u8,
            pixel[3],
        ]);
        adjusted.put_pixel(x, y, new_pixel);
    }

    DynamicImage::ImageRgba8(adjusted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preprocessing_step_creation() {
        let step = PreprocessingStep::new(
            "norm1".to_string(),
            PreprocessingStepType::ImageNormalization,
        );
        assert_eq!(step.id, "norm1");
        assert_eq!(step.step_type, PreprocessingStepType::ImageNormalization);
        assert!(!step.cache_results);
    }

    #[test]
    fn test_preprocessing_step_with_config() {
        let config = ImageNormalizationConfig::default();
        let step = PreprocessingStep::new(
            "norm1".to_string(),
            PreprocessingStepType::ImageNormalization,
        )
        .with_config(config)
        .expect("Failed to set config");

        assert!(step.config.is_object());
    }

    #[test]
    fn test_pipeline_builder() {
        let pipeline = PipelineBuilder::new("pipe1".to_string(), "Test Pipeline".to_string())
            .version("1.0.0".to_string())
            .description("A test preprocessing pipeline".to_string())
            .add_step(
                PreprocessingStep::new("step1".to_string(), PreprocessingStepType::ImageResize)
                    .with_cache(true),
            )
            .metadata("author".to_string(), "test".to_string())
            .build();

        assert_eq!(pipeline.id, "pipe1");
        assert_eq!(pipeline.name, "Test Pipeline");
        assert_eq!(pipeline.version, "1.0.0");
        assert_eq!(pipeline.steps.len(), 1);
        assert_eq!(pipeline.metadata.get("author"), Some(&"test".to_string()));
    }

    #[test]
    fn test_image_normalization_config_default() {
        let config = ImageNormalizationConfig::default();
        assert_eq!(config.mean.len(), 3);
        assert_eq!(config.std.len(), 3);
        assert!(config.normalize_range);
    }

    #[test]
    fn test_augmentation_config_default() {
        let config = AugmentationConfig::default();
        assert_eq!(config.horizontal_flip_prob, 0.5);
        assert_eq!(config.vertical_flip_prob, 0.0);
        assert_eq!(config.rotation_range, 15.0);
    }

    #[tokio::test]
    async fn test_preprocessing_manager() {
        let temp_dir = std::env::temp_dir().join("rs3gw_preprocessing_test");
        let manager = PreprocessingManager::new(temp_dir.clone());

        let pipeline = PipelineBuilder::new("test".to_string(), "Test".to_string()).build();

        manager
            .register_pipeline(pipeline.clone())
            .await
            .expect("Failed to register pipeline");

        let retrieved = manager
            .get_pipeline("test")
            .await
            .expect("Failed to get pipeline");

        assert_eq!(retrieved.id, "test");

        let pipelines = manager.list_pipelines().await;
        assert_eq!(pipelines.len(), 1);

        manager
            .delete_pipeline("test")
            .await
            .expect("Failed to delete pipeline");

        let pipelines = manager.list_pipelines().await;
        assert_eq!(pipelines.len(), 0);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_preprocessing_cache() {
        let cache = PreprocessingCache::new(1024 * 1024); // 1MB

        let data = Bytes::from(vec![1, 2, 3, 4, 5]);
        cache
            .put("test_key".to_string(), data.clone(), HashMap::new())
            .await
            .expect("Failed to cache data");

        let retrieved = cache.get("test_key").await;
        assert_eq!(retrieved, Some(data));

        cache.invalidate("test_key").await;
        assert_eq!(cache.get("test_key").await, None);

        let (count, size, max_size) = cache.stats().await;
        assert_eq!(count, 0);
        assert_eq!(size, 0);
        assert_eq!(max_size, 1024 * 1024);
    }

    #[tokio::test]
    async fn test_pipeline_serialization() {
        let temp_dir = std::env::temp_dir().join("rs3gw_pipeline_serialization_test");
        let manager = PreprocessingManager::new(temp_dir.clone());

        let pipeline = PipelineBuilder::new(
            "serialize_test".to_string(),
            "Serialization Test".to_string(),
        )
        .version("1.0.0".to_string())
        .description("Testing pipeline serialization".to_string())
        .build();

        // Save as JSON
        let json_path = manager
            .save_pipeline_to_file(&pipeline, "json")
            .await
            .expect("Failed to save pipeline as JSON");

        assert!(json_path.exists());

        // Load back
        let loaded = manager
            .load_pipeline_from_file(&json_path)
            .await
            .expect("Failed to load pipeline from JSON");

        assert_eq!(loaded.id, pipeline.id);
        assert_eq!(loaded.name, pipeline.name);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_image_normalization_preprocessing() {
        // Create a simple test image (10x10 red image)
        let img =
            DynamicImage::ImageRgb8(ImageBuffer::from_fn(10, 10, |_, _| Rgb([255u8, 0u8, 0u8])));

        // Save to bytes
        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        img.write_to(&mut cursor, ImageFormat::Png)
            .expect("Failed to write image");
        let input_data = Bytes::from(buffer);

        // Create pipeline with normalization
        let temp_dir = std::env::temp_dir().join("rs3gw_norm_test");
        let manager = PreprocessingManager::new(temp_dir.clone());

        let config = ImageNormalizationConfig::default();
        let step = PreprocessingStep::new(
            "norm".to_string(),
            PreprocessingStepType::ImageNormalization,
        )
        .with_config(config)
        .expect("Failed to set config");

        let pipeline =
            PipelineBuilder::new("norm_pipeline".to_string(), "Normalization".to_string())
                .add_step(step)
                .build();

        manager
            .register_pipeline(pipeline)
            .await
            .expect("Failed to register");

        // Apply pipeline
        let result = manager
            .apply_pipeline("norm_pipeline", input_data, HashMap::new())
            .await
            .expect("Failed to apply pipeline");

        // Verify result is valid image data
        assert!(!result.is_empty());
        assert!(load_from_memory(&result).is_ok());

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_image_resize_preprocessing() {
        // Create a test image (100x100)
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(100, 100, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128u8])
        }));

        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        img.write_to(&mut cursor, ImageFormat::Png)
            .expect("Failed to write image");
        let input_data = Bytes::from(buffer);

        let temp_dir = std::env::temp_dir().join("rs3gw_resize_test");
        let manager = PreprocessingManager::new(temp_dir.clone());

        // Create resize config (224x224)
        let config = ImageResizeConfig {
            width: 224,
            height: 224,
            mode: DatasetResizeMode::Fit,
            filter: "lanczos3".to_string(),
        };

        let step = PreprocessingStep::new("resize".to_string(), PreprocessingStepType::ImageResize)
            .with_config(config)
            .expect("Failed to set config");

        let pipeline = PipelineBuilder::new("resize_pipeline".to_string(), "Resize".to_string())
            .add_step(step)
            .build();

        manager
            .register_pipeline(pipeline)
            .await
            .expect("Failed to register");

        let result = manager
            .apply_pipeline("resize_pipeline", input_data, HashMap::new())
            .await
            .expect("Failed to apply pipeline");

        // Verify result
        assert!(!result.is_empty());
        let result_img = load_from_memory(&result).expect("Failed to load result image");
        let (width, height) = result_img.dimensions();

        // Should be resized to fit within 224x224 while preserving aspect ratio
        assert!(width <= 224 && height <= 224);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_data_augmentation_preprocessing() {
        // Create a test image
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(50, 50, |x, y| {
            Rgb([(x * 5) as u8, (y * 5) as u8, 128u8])
        }));

        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        img.write_to(&mut cursor, ImageFormat::Png)
            .expect("Failed to write image");
        let input_data = Bytes::from(buffer);

        let temp_dir = std::env::temp_dir().join("rs3gw_augment_test");
        let manager = PreprocessingManager::new(temp_dir.clone());

        // Create augmentation config with deterministic settings
        let config = AugmentationConfig {
            horizontal_flip_prob: 0.5,
            vertical_flip_prob: 0.5,
            rotation_range: 15.0,
            brightness_range: Some((0.9, 1.1)),
            contrast_range: Some((0.9, 1.1)),
            saturation_range: None,
            random_crop_size: None,
        };

        let step = PreprocessingStep::new(
            "augment".to_string(),
            PreprocessingStepType::DataAugmentation,
        )
        .with_config(config)
        .expect("Failed to set config");

        let pipeline = PipelineBuilder::new("augment_pipeline".to_string(), "Augment".to_string())
            .add_step(step)
            .build();

        manager
            .register_pipeline(pipeline)
            .await
            .expect("Failed to register");

        let result = manager
            .apply_pipeline("augment_pipeline", input_data, HashMap::new())
            .await
            .expect("Failed to apply pipeline");

        // Verify result is valid
        assert!(!result.is_empty());
        assert!(load_from_memory(&result).is_ok());

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_full_preprocessing_pipeline() {
        // Create a test image
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(200, 200, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 128u8])
        }));

        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        img.write_to(&mut cursor, ImageFormat::Png)
            .expect("Failed to write image");
        let input_data = Bytes::from(buffer);

        let temp_dir = std::env::temp_dir().join("rs3gw_full_pipeline_test");
        let manager = PreprocessingManager::new(temp_dir.clone());

        // Create a multi-step pipeline: resize -> normalize -> augment
        let resize_config = ImageResizeConfig {
            width: 128,
            height: 128,
            mode: DatasetResizeMode::Fit,
            filter: "bilinear".to_string(),
        };

        let norm_config = ImageNormalizationConfig::default();

        let augment_config = AugmentationConfig {
            horizontal_flip_prob: 0.5,
            vertical_flip_prob: 0.0,
            rotation_range: 0.0,
            brightness_range: Some((0.95, 1.05)),
            contrast_range: None,
            saturation_range: None,
            random_crop_size: None,
        };

        let pipeline =
            PipelineBuilder::new("full_pipeline".to_string(), "Full Pipeline".to_string())
                .version("1.0.0".to_string())
                .add_step(
                    PreprocessingStep::new(
                        "resize".to_string(),
                        PreprocessingStepType::ImageResize,
                    )
                    .with_config(resize_config)
                    .expect("Failed to set resize config"),
                )
                .add_step(
                    PreprocessingStep::new(
                        "normalize".to_string(),
                        PreprocessingStepType::ImageNormalization,
                    )
                    .with_config(norm_config)
                    .expect("Failed to set norm config")
                    .with_cache(true),
                )
                .add_step(
                    PreprocessingStep::new(
                        "augment".to_string(),
                        PreprocessingStepType::DataAugmentation,
                    )
                    .with_config(augment_config)
                    .expect("Failed to set augment config"),
                )
                .build();

        manager
            .register_pipeline(pipeline)
            .await
            .expect("Failed to register");

        let result = manager
            .apply_pipeline("full_pipeline", input_data, HashMap::new())
            .await
            .expect("Failed to apply pipeline");

        // Verify result
        assert!(!result.is_empty());
        let result_img = load_from_memory(&result).expect("Failed to load result");
        let (width, height) = result_img.dimensions();

        // Should be resized to fit within 128x128
        assert!(width <= 128 && height <= 128);

        // Test cache statistics
        let (count, _size, _max_size) = manager.cache_stats().await;
        // Normalization step has caching enabled, so there should be 1 cached item
        assert!(count >= 1);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[test]
    fn test_normalization_presets() {
        // Test ImageNet preset
        let imagenet = ImageNormalizationConfig::imagenet();
        assert_eq!(imagenet.mean, vec![0.485, 0.456, 0.406]);
        assert_eq!(imagenet.std, vec![0.229, 0.224, 0.225]);
        assert!(imagenet.normalize_range);

        // Test CLIP preset
        let clip = ImageNormalizationConfig::clip();
        assert_eq!(clip.mean, vec![0.481_454_7, 0.457_827_5, 0.408_210_7]);
        assert_eq!(clip.std, vec![0.268_629_5, 0.261_302_6, 0.275_777_1]);
        assert!(clip.normalize_range);

        // Test DINOv2 preset
        let dinov2 = ImageNormalizationConfig::dinov2();
        assert_eq!(dinov2.mean, vec![0.485, 0.456, 0.406]);
        assert_eq!(dinov2.std, vec![0.229, 0.224, 0.225]);
        assert!(dinov2.normalize_range);

        // Test ViT preset
        let vit = ImageNormalizationConfig::vit();
        assert_eq!(vit.mean, vec![0.5, 0.5, 0.5]);
        assert_eq!(vit.std, vec![0.5, 0.5, 0.5]);
        assert!(vit.normalize_range);

        // Test Inception preset
        let inception = ImageNormalizationConfig::inception();
        assert_eq!(inception.mean, vec![0.5, 0.5, 0.5]);
        assert_eq!(inception.std, vec![0.5, 0.5, 0.5]);
        assert!(inception.normalize_range);

        // Test MobileNet preset
        let mobilenet = ImageNormalizationConfig::mobilenet();
        assert_eq!(mobilenet.mean, vec![0.485, 0.456, 0.406]);
        assert_eq!(mobilenet.std, vec![0.229, 0.224, 0.225]);
        assert!(mobilenet.normalize_range);

        // Test EfficientNet preset
        let efficientnet = ImageNormalizationConfig::efficientnet();
        assert_eq!(efficientnet.mean, vec![0.485, 0.456, 0.406]);
        assert_eq!(efficientnet.std, vec![0.229, 0.224, 0.225]);
        assert!(efficientnet.normalize_range);

        // Test custom preset
        let custom =
            ImageNormalizationConfig::custom(vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6], false);
        assert_eq!(custom.mean, vec![0.1, 0.2, 0.3]);
        assert_eq!(custom.std, vec![0.4, 0.5, 0.6]);
        assert!(!custom.normalize_range);
    }

    #[test]
    fn test_resize_presets() {
        // Test ResNet preset
        let resnet = ImageResizeConfig::resnet();
        assert_eq!(resnet.width, 224);
        assert_eq!(resnet.height, 224);
        assert_eq!(resnet.mode, DatasetResizeMode::Fit);
        assert_eq!(resnet.filter, "bilinear");

        // Test CLIP preset
        let clip = ImageResizeConfig::clip();
        assert_eq!(clip.width, 224);
        assert_eq!(clip.height, 224);
        assert_eq!(clip.mode, DatasetResizeMode::Fit);
        assert_eq!(clip.filter, "bicubic");

        // Test DINOv2 preset
        let dinov2 = ImageResizeConfig::dinov2();
        assert_eq!(dinov2.width, 518);
        assert_eq!(dinov2.height, 518);
        assert_eq!(dinov2.mode, DatasetResizeMode::Fit);
        assert_eq!(dinov2.filter, "bicubic");

        // Test ViT base preset
        let vit_base = ImageResizeConfig::vit_base();
        assert_eq!(vit_base.width, 224);
        assert_eq!(vit_base.height, 224);
        assert_eq!(vit_base.mode, DatasetResizeMode::Fit);
        assert_eq!(vit_base.filter, "bicubic");

        // Test ViT large preset
        let vit_large = ImageResizeConfig::vit_large();
        assert_eq!(vit_large.width, 384);
        assert_eq!(vit_large.height, 384);
        assert_eq!(vit_large.mode, DatasetResizeMode::Fit);
        assert_eq!(vit_large.filter, "bicubic");

        // Test Inception v3 preset
        let inception = ImageResizeConfig::inception_v3();
        assert_eq!(inception.width, 299);
        assert_eq!(inception.height, 299);
        assert_eq!(inception.mode, DatasetResizeMode::Fit);
        assert_eq!(inception.filter, "bicubic");

        // Test EfficientNet B0 preset
        let efficientnet_b0 = ImageResizeConfig::efficientnet_b0();
        assert_eq!(efficientnet_b0.width, 224);
        assert_eq!(efficientnet_b0.height, 224);
        assert_eq!(efficientnet_b0.mode, DatasetResizeMode::Fit);
        assert_eq!(efficientnet_b0.filter, "bicubic");

        // Test EfficientNet B7 preset
        let efficientnet_b7 = ImageResizeConfig::efficientnet_b7();
        assert_eq!(efficientnet_b7.width, 600);
        assert_eq!(efficientnet_b7.height, 600);
        assert_eq!(efficientnet_b7.mode, DatasetResizeMode::Fit);
        assert_eq!(efficientnet_b7.filter, "bicubic");

        // Test YOLO preset
        let yolo = ImageResizeConfig::yolo();
        assert_eq!(yolo.width, 640);
        assert_eq!(yolo.height, 640);
        assert_eq!(yolo.mode, DatasetResizeMode::Fit);
        assert_eq!(yolo.filter, "bilinear");

        // Test custom preset
        let custom = ImageResizeConfig::custom(512, 512, DatasetResizeMode::Fill, "lanczos3");
        assert_eq!(custom.width, 512);
        assert_eq!(custom.height, 512);
        assert_eq!(custom.mode, DatasetResizeMode::Fill);
        assert_eq!(custom.filter, "lanczos3");
    }

    #[test]
    fn test_preset_usage_in_pipeline() {
        // Test that presets can be used in pipeline construction
        let norm_config = ImageNormalizationConfig::clip();
        let resize_config = ImageResizeConfig::vit_large();

        let norm_step = PreprocessingStep::new(
            "norm".to_string(),
            PreprocessingStepType::ImageNormalization,
        )
        .with_config(norm_config)
        .expect("Failed to set norm config");

        let resize_step =
            PreprocessingStep::new("resize".to_string(), PreprocessingStepType::ImageResize)
                .with_config(resize_config)
                .expect("Failed to set resize config");

        let pipeline =
            PipelineBuilder::new("clip_vit".to_string(), "CLIP ViT Pipeline".to_string())
                .version("1.0.0".to_string())
                .description("Pipeline using CLIP normalization and ViT large resize".to_string())
                .add_step(resize_step)
                .add_step(norm_step)
                .build();

        assert_eq!(pipeline.id, "clip_vit");
        assert_eq!(pipeline.steps.len(), 2);
    }
}
