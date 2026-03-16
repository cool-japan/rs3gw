//! Data augmentation configuration.

use serde::{Deserialize, Serialize};

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
