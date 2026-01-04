//! Dataset preprocessing pipeline for AI/ML workloads
//!
//! Provides on-the-fly data preprocessing capabilities including:
//! - Image normalization and resizing
//! - Data augmentation
//! - Text tokenization
//! - Audio/video processing
//! - Caching of preprocessing results
//! - Pipeline versioning and reproducibility

use bytes::Bytes;
use chrono::{DateTime, Utc};
// Image processing imports - used in public API implementation (apply_pipeline and helpers)
#[cfg_attr(not(test), allow(unused_imports))]
use image::{DynamicImage, GenericImageView, ImageBuffer, ImageFormat, Rgba};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg_attr(not(test), allow(unused_imports))]
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Error, Debug)]
pub enum PreprocessingError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Preprocessing step failed: {0}")]
    StepFailed(String),

    #[error("Pipeline not found: {0}")]
    PipelineNotFound(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type PreprocessingResult<T> = Result<T, PreprocessingError>;

// ============================================================================
// Data Types
// ============================================================================

/// Type of preprocessing operation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreprocessingStepType {
    /// Image normalization (mean, std)
    ImageNormalization,
    /// Image resizing (width, height, mode)
    ImageResize,
    /// Data augmentation (rotation, flip, etc.)
    DataAugmentation,
    /// Text tokenization
    TextTokenization,
    /// Audio feature extraction
    AudioFeatures,
    /// Video frame extraction
    VideoFrames,
    /// Custom preprocessing
    Custom(String),
}

/// Configuration for image normalization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageNormalizationConfig {
    /// Mean values for each channel (RGB)
    pub mean: Vec<f32>,
    /// Standard deviation for each channel
    pub std: Vec<f32>,
    /// Whether to normalize to [0, 1] first
    pub normalize_range: bool,
}

impl Default for ImageNormalizationConfig {
    fn default() -> Self {
        // ImageNet normalization
        Self {
            mean: vec![0.485, 0.456, 0.406],
            std: vec![0.229, 0.224, 0.225],
            normalize_range: true,
        }
    }
}

impl ImageNormalizationConfig {
    /// Create ImageNet normalization preset (ResNet, VGG, etc.)
    /// Mean: [0.485, 0.456, 0.406], Std: [0.229, 0.224, 0.225]
    pub fn imagenet() -> Self {
        Self {
            mean: vec![0.485, 0.456, 0.406],
            std: vec![0.229, 0.224, 0.225],
            normalize_range: true,
        }
    }

    /// Create CLIP (OpenAI) normalization preset
    /// Mean: [0.48145466, 0.4578275, 0.40821073], Std: [0.26862954, 0.26130258, 0.27577711]
    pub fn clip() -> Self {
        Self {
            mean: vec![0.481_454_7, 0.457_827_5, 0.408_210_7],
            std: vec![0.268_629_5, 0.261_302_6, 0.275_777_1],
            normalize_range: true,
        }
    }

    /// Create DINOv2 (Meta) normalization preset
    /// Mean: [0.485, 0.456, 0.406], Std: [0.229, 0.224, 0.225]
    /// Same as ImageNet but often used with different input sizes
    pub fn dinov2() -> Self {
        Self {
            mean: vec![0.485, 0.456, 0.406],
            std: vec![0.229, 0.224, 0.225],
            normalize_range: true,
        }
    }

    /// Create ViT (Vision Transformer) normalization preset
    /// Mean: [0.5, 0.5, 0.5], Std: [0.5, 0.5, 0.5]
    /// Normalizes to [-1, 1] range
    pub fn vit() -> Self {
        Self {
            mean: vec![0.5, 0.5, 0.5],
            std: vec![0.5, 0.5, 0.5],
            normalize_range: true,
        }
    }

    /// Create Inception (GoogLeNet) normalization preset
    /// Mean: [0.5, 0.5, 0.5], Std: [0.5, 0.5, 0.5]
    /// Normalizes to [-1, 1] range
    pub fn inception() -> Self {
        Self {
            mean: vec![0.5, 0.5, 0.5],
            std: vec![0.5, 0.5, 0.5],
            normalize_range: true,
        }
    }

    /// Create MobileNet normalization preset
    /// Mean: [0.485, 0.456, 0.406], Std: [0.229, 0.224, 0.225]
    /// Same as ImageNet (commonly used for MobileNet)
    pub fn mobilenet() -> Self {
        Self {
            mean: vec![0.485, 0.456, 0.406],
            std: vec![0.229, 0.224, 0.225],
            normalize_range: true,
        }
    }

    /// Create EfficientNet normalization preset
    /// Mean: [0.485, 0.456, 0.406], Std: [0.229, 0.224, 0.225]
    /// Same as ImageNet (commonly used for EfficientNet)
    pub fn efficientnet() -> Self {
        Self {
            mean: vec![0.485, 0.456, 0.406],
            std: vec![0.229, 0.224, 0.225],
            normalize_range: true,
        }
    }

    /// Create custom normalization preset
    /// Allows specifying custom mean and std values
    pub fn custom(mean: Vec<f32>, std: Vec<f32>, normalize_range: bool) -> Self {
        Self {
            mean,
            std,
            normalize_range,
        }
    }
}

/// Image resize mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DatasetResizeMode {
    /// Resize to exact dimensions (may distort aspect ratio)
    Exact,
    /// Fit within dimensions (preserve aspect ratio)
    Fit,
    /// Fill dimensions (crop if needed)
    Fill,
    /// Stretch to dimensions
    Stretch,
}

/// Configuration for image resizing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageResizeConfig {
    /// Target width
    pub width: u32,
    /// Target height
    pub height: u32,
    /// Resize mode
    pub mode: DatasetResizeMode,
    /// Interpolation filter
    pub filter: String, // "nearest", "bilinear", "bicubic", "lanczos3"
}

impl Default for ImageResizeConfig {
    fn default() -> Self {
        Self {
            width: 224,
            height: 224,
            mode: DatasetResizeMode::Fit,
            filter: "bilinear".to_string(),
        }
    }
}

impl ImageResizeConfig {
    /// ResNet/ImageNet standard size: 224x224
    pub fn resnet() -> Self {
        Self {
            width: 224,
            height: 224,
            mode: DatasetResizeMode::Fit,
            filter: "bilinear".to_string(),
        }
    }

    /// CLIP standard size: 224x224
    pub fn clip() -> Self {
        Self {
            width: 224,
            height: 224,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// DINOv2 standard size: 518x518
    pub fn dinov2() -> Self {
        Self {
            width: 518,
            height: 518,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// ViT (Vision Transformer) base size: 224x224
    pub fn vit_base() -> Self {
        Self {
            width: 224,
            height: 224,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// ViT large size: 384x384
    pub fn vit_large() -> Self {
        Self {
            width: 384,
            height: 384,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// Inception v3 size: 299x299
    pub fn inception_v3() -> Self {
        Self {
            width: 299,
            height: 299,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// EfficientNet B0 size: 224x224
    pub fn efficientnet_b0() -> Self {
        Self {
            width: 224,
            height: 224,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// EfficientNet B7 size: 600x600
    pub fn efficientnet_b7() -> Self {
        Self {
            width: 600,
            height: 600,
            mode: DatasetResizeMode::Fit,
            filter: "bicubic".to_string(),
        }
    }

    /// YOLO standard size: 640x640
    pub fn yolo() -> Self {
        Self {
            width: 640,
            height: 640,
            mode: DatasetResizeMode::Fit,
            filter: "bilinear".to_string(),
        }
    }

    /// Custom size with specified dimensions and parameters
    pub fn custom(width: u32, height: u32, mode: DatasetResizeMode, filter: &str) -> Self {
        Self {
            width,
            height,
            mode,
            filter: filter.to_string(),
        }
    }
}

/// Configuration for data augmentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AugmentationConfig {
    /// Random horizontal flip probability
    pub horizontal_flip_prob: f32,
    /// Random vertical flip probability
    pub vertical_flip_prob: f32,
    /// Random rotation range in degrees
    pub rotation_range: f32,
    /// Random brightness adjustment range
    pub brightness_range: Option<(f32, f32)>,
    /// Random contrast adjustment range
    pub contrast_range: Option<(f32, f32)>,
    /// Random saturation adjustment range
    pub saturation_range: Option<(f32, f32)>,
    /// Random crop size
    pub random_crop_size: Option<(u32, u32)>,
}

impl Default for AugmentationConfig {
    fn default() -> Self {
        Self {
            horizontal_flip_prob: 0.5,
            vertical_flip_prob: 0.0,
            rotation_range: 15.0,
            brightness_range: Some((0.8, 1.2)),
            contrast_range: Some((0.8, 1.2)),
            saturation_range: Some((0.8, 1.2)),
            random_crop_size: None,
        }
    }
}

/// Configuration for text tokenization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenizationConfig {
    /// Tokenizer type
    pub tokenizer_type: String, // "whitespace", "wordpiece", "bpe", "sentencepiece"
    /// Vocabulary file path
    pub vocab_path: Option<PathBuf>,
    /// Maximum sequence length
    pub max_length: Option<usize>,
    /// Padding strategy
    pub padding: bool,
    /// Truncation strategy
    pub truncation: bool,
    /// Whether to lowercase
    pub lowercase: bool,
}

impl Default for TokenizationConfig {
    fn default() -> Self {
        Self {
            tokenizer_type: "whitespace".to_string(),
            vocab_path: None,
            max_length: Some(512),
            padding: true,
            truncation: true,
            lowercase: true,
        }
    }
}

/// A single preprocessing step in a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessingStep {
    /// Unique identifier for this step
    pub id: String,
    /// Step type
    pub step_type: PreprocessingStepType,
    /// Configuration (JSON-serialized)
    pub config: serde_json::Value,
    /// Whether to cache results of this step
    pub cache_results: bool,
    /// Step description
    pub description: Option<String>,
}

impl PreprocessingStep {
    pub fn new(id: String, step_type: PreprocessingStepType) -> Self {
        Self {
            id,
            step_type,
            config: serde_json::Value::Null,
            cache_results: false,
            description: None,
        }
    }

    pub fn with_config<T: Serialize>(mut self, config: T) -> PreprocessingResult<Self> {
        self.config = serde_json::to_value(config)
            .map_err(|e| PreprocessingError::SerializationError(e.to_string()))?;
        Ok(self)
    }

    pub fn with_cache(mut self, cache: bool) -> Self {
        self.cache_results = cache;
        self
    }

    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }
}

/// Preprocessing pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    /// Unique pipeline identifier
    pub id: String,
    /// Pipeline name
    pub name: String,
    /// Pipeline version
    pub version: String,
    /// Pipeline description
    pub description: Option<String>,
    /// List of preprocessing steps (executed in order)
    pub steps: Vec<PreprocessingStep>,
    /// Pipeline metadata
    pub metadata: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    pub modified_at: DateTime<Utc>,
}

impl PipelineDefinition {
    pub fn new(id: String, name: String, version: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            version,
            description: None,
            steps: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            modified_at: now,
        }
    }

    pub fn add_step(&mut self, step: PreprocessingStep) {
        self.steps.push(step);
        self.modified_at = Utc::now();
    }

    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.modified_at = Utc::now();
    }
}

/// Builder for preprocessing pipelines
pub struct PipelineBuilder {
    definition: PipelineDefinition,
}

impl PipelineBuilder {
    pub fn new(id: String, name: String) -> Self {
        Self {
            definition: PipelineDefinition::new(id, name, "1.0.0".to_string()),
        }
    }

    pub fn version(mut self, version: String) -> Self {
        self.definition.version = version;
        self
    }

    pub fn description(mut self, desc: String) -> Self {
        self.definition.description = Some(desc);
        self
    }

    pub fn add_step(mut self, step: PreprocessingStep) -> Self {
        self.definition.add_step(step);
        self
    }

    pub fn metadata(mut self, key: String, value: String) -> Self {
        self.definition.add_metadata(key, value);
        self
    }

    pub fn build(self) -> PipelineDefinition {
        self.definition
    }
}

// ============================================================================
// Preprocessing Result Cache
// ============================================================================

/// Cached preprocessing result
#[derive(Debug, Clone)]
struct CachedResult {
    data: Bytes,
    #[allow(dead_code)]
    metadata: HashMap<String, String>,
    #[allow(dead_code)]
    cached_at: DateTime<Utc>,
}

/// Cache for preprocessing results
pub struct PreprocessingCache {
    cache: Arc<RwLock<HashMap<String, CachedResult>>>,
    max_size_bytes: usize,
    current_size: Arc<RwLock<usize>>,
}

impl PreprocessingCache {
    pub fn new(max_size_bytes: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_size_bytes,
            current_size: Arc::new(RwLock::new(0)),
        }
    }

    pub async fn get(&self, key: &str) -> Option<Bytes> {
        let cache = self.cache.read().await;
        cache.get(key).map(|r| r.data.clone())
    }

    pub async fn put(
        &self,
        key: String,
        data: Bytes,
        metadata: HashMap<String, String>,
    ) -> PreprocessingResult<()> {
        let data_size = data.len();

        // Check if adding this would exceed max size
        let current_size = *self.current_size.read().await;
        if current_size + data_size > self.max_size_bytes {
            // Simple eviction: clear cache if too large
            let mut cache = self.cache.write().await;
            cache.clear();
            *self.current_size.write().await = 0;
        }

        let mut cache = self.cache.write().await;
        cache.insert(
            key,
            CachedResult {
                data,
                metadata,
                cached_at: Utc::now(),
            },
        );

        *self.current_size.write().await += data_size;
        Ok(())
    }

    pub async fn invalidate(&self, key: &str) {
        let mut cache = self.cache.write().await;
        if let Some(entry) = cache.remove(key) {
            let mut size = self.current_size.write().await;
            *size = size.saturating_sub(entry.data.len());
        }
    }

    pub async fn clear(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
        *self.current_size.write().await = 0;
    }

    pub async fn stats(&self) -> (usize, usize, usize) {
        let cache = self.cache.read().await;
        let current_size = *self.current_size.read().await;
        (cache.len(), current_size, self.max_size_bytes)
    }
}

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

    /// Apply preprocessing pipeline to data
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

            normalized.put_pixel(x, y, image::Rgb(new_pixel));
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
            "nearest" => image::imageops::FilterType::Nearest,
            "bilinear" => image::imageops::FilterType::Triangle,
            "bicubic" => image::imageops::FilterType::CatmullRom,
            "lanczos3" => image::imageops::FilterType::Lanczos3,
            _ => image::imageops::FilterType::Triangle,
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
                    image::imageops::crop_imm(
                        &scaled,
                        x_offset,
                        y_offset,
                        config.width,
                        config.height,
                    )
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
            result = DynamicImage::ImageRgba8(image::imageops::flip_horizontal(&result));
        }

        // Vertical flip
        if random_f64() < config.vertical_flip_prob as f64 {
            result = DynamicImage::ImageRgba8(image::imageops::flip_vertical(&result));
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
        let mut current_image = image::load_from_memory(&input_data).ok();

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
            let mut buffer = Vec::new();
            let mut cursor = Cursor::new(&mut buffer);
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
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(10, 10, |_, _| {
            image::Rgb([255u8, 0u8, 0u8])
        }));

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
        assert!(image::load_from_memory(&result).is_ok());

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_image_resize_preprocessing() {
        // Create a test image (100x100)
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(100, 100, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128u8])
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
        let result_img = image::load_from_memory(&result).expect("Failed to load result image");
        let (width, height) = result_img.dimensions();

        // Should be resized to fit within 224x224 while preserving aspect ratio
        assert!(width <= 224 && height <= 224);

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_data_augmentation_preprocessing() {
        // Create a test image
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(50, 50, |x, y| {
            image::Rgb([(x * 5) as u8, (y * 5) as u8, 128u8])
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
        assert!(image::load_from_memory(&result).is_ok());

        let _ = tokio::fs::remove_dir_all(temp_dir).await;
    }

    #[tokio::test]
    async fn test_full_preprocessing_pipeline() {
        // Create a test image
        let img = DynamicImage::ImageRgb8(ImageBuffer::from_fn(200, 200, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128u8])
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
        let result_img = image::load_from_memory(&result).expect("Failed to load result");
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
