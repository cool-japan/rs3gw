//! Persistent cache state for saving/loading to disk.

use super::entry::CacheEntry;
use super::pattern::QueryPattern;
use super::stats::CacheStats;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Persistent cache state for saving to disk
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct PersistentCacheState {
    /// Cache entries (query_key -> entry)
    pub(super) cache: HashMap<String, CacheEntry>,

    /// Query patterns (pattern_key -> pattern)
    pub(super) query_patterns: HashMap<String, QueryPattern>,

    /// Cache statistics
    pub(super) stats: CacheStats,

    /// Version for future compatibility
    pub(super) version: u32,

    /// Timestamp when saved
    pub(super) saved_at: i64,
}
