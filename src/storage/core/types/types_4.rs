//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CompressionMode {
    #[default]
    None,
    Zstd(i32),
    Lz4,
}
/// Object tagging
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectTagging {
    pub tags: HashMap<String, String>,
}
/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub bucket_count: u64,
    pub object_count: u64,
    pub total_size_bytes: u64,
}
