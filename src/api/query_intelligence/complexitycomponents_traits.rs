//! # ComplexityComponents - Trait Implementations
//!
//! This module contains trait implementations for `ComplexityComponents`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::ComplexityComponents;

impl Default for ComplexityComponents {
    fn default() -> Self {
        Self {
            projection_score: 0.0,
            filter_score: 0.0,
            aggregation_score: 0.0,
            join_score: 0.0,
            sort_score: 0.0,
            subquery_score: 0.0,
            window_score: 0.0,
            feature_score: 0.0,
        }
    }
}
