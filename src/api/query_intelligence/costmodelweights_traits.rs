//! # CostModelWeights - Trait Implementations
//!
//! This module contains trait implementations for `CostModelWeights`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::CostModelWeights;

impl Default for CostModelWeights {
    fn default() -> Self {
        Self {
            scan_per_row: 0.001,
            filter_per_row: 0.002,
            projection_per_column: 0.0005,
            aggregation_per_group: 0.01,
            sort_per_comparison: 0.003,
            join_per_row: 0.005,
            memory_per_byte: 0.000001,
            sample_count: 0,
        }
    }
}
