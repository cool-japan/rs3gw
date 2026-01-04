//! # CacheConfig - Trait Implementations
//!
//! This module contains trait implementations for `CacheConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::CacheConfig;

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_size_bytes: 256 * 1024 * 1024,
            max_objects: 10000,
            ttl_secs: 300,
        }
    }
}
