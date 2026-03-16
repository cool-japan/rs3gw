//! Query pattern tracking for cache warming analysis.

use serde::{Deserialize, Serialize};

/// Query pattern tracking for cache warming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPattern {
    /// SQL expression
    pub sql: String,

    /// Object bucket and key
    pub bucket: String,
    pub key: String,

    /// Number of times this query has been executed
    pub execution_count: u64,

    /// Last execution timestamp
    pub last_executed: i64,

    /// Average execution time in milliseconds
    pub avg_execution_ms: u64,
}
