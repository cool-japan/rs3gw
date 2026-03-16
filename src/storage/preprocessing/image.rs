//! Image normalization and resize configuration types and presets.

use serde::{Deserialize, Serialize};

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
