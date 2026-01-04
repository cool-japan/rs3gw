//! # OutputFormat - Trait Implementations
//!
//! This module contains trait implementations for `OutputFormat`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::{JsonOutput, OutputFormat};

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Json(JsonOutput::default())
    }
}
