//! # ColumnUsageStats - Trait Implementations
//!
//! This module contains trait implementations for `ColumnUsageStats`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::ColumnUsageStats;

impl Default for ColumnUsageStats {
    fn default() -> Self {
        Self {
            frequency: 0,
            avg_selectivity: 1.0,
            avg_query_time: 0.0,
            total_rows_scanned: 0,
        }
    }
}
