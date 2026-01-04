//! # DataStatistics - Trait Implementations
//!
//! This module contains trait implementations for `DataStatistics`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use std::collections::HashMap;

use super::types::DataStatistics;

impl Default for DataStatistics {
    fn default() -> Self {
        Self {
            total_rows: 0,
            total_bytes: 0,
            avg_row_size: 0.0,
            format: String::from("unknown"),
            compression_ratio: None,
            column_cardinality: HashMap::new(),
            null_percentages: HashMap::new(),
            is_sorted: false,
            skew_factor: 0.0,
        }
    }
}
