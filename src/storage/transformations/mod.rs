//! Object transformation module
//!
//! Provides transformation capabilities for stored objects including:
//! - Image transformations (resize, format conversion)
//! - Video transcoding (feature-gated with FFmpeg)
//! - Compression (Zstd, Gzip, LZ4)
//! - WASM plugin execution (feature-gated)

pub mod compression;
pub mod image;
pub mod manager;
mod types;
pub mod video;
pub mod wasm;

#[cfg(test)]
mod tests;

// Re-export public types
pub use compression::CompressionTransformer;
pub use image::{ImageTransformer, Transformer};
pub use manager::{TransformationCache, TransformationManager};
pub use types::*;
pub use video::VideoTransformer;
pub use wasm::WasmPluginTransformer;
