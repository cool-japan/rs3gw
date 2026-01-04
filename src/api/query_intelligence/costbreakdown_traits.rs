//! # CostBreakdown - Trait Implementations
//!
//! This module contains trait implementations for `CostBreakdown`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::CostBreakdown;

impl Default for CostBreakdown {
    fn default() -> Self {
        Self {
            scan_cost: 0.0,
            filter_cost: 0.0,
            projection_cost: 0.0,
            aggregation_cost: 0.0,
            sort_cost: 0.0,
            join_cost: 0.0,
        }
    }
}
