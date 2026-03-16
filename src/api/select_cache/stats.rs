//! Cache statistics types.

use serde::{Deserialize, Serialize};

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStats {
    /// Total number of cache get requests
    pub gets: u64,

    /// Number of cache hits
    pub hits: u64,

    /// Number of cache misses
    pub misses: u64,

    /// Number of entries evicted due to LRU
    pub evictions: u64,

    /// Number of entries expired due to TTL
    pub expirations: u64,

    /// Current number of cached entries
    pub current_entries: usize,

    /// Current estimated memory usage in bytes
    pub memory_bytes: u64,

    /// Maximum allowed entries
    pub max_entries: usize,

    /// Maximum allowed memory in bytes
    pub max_memory_bytes: u64,
}

impl CacheStats {
    /// Calculate hit rate as a percentage
    pub fn hit_rate(&self) -> f64 {
        if self.gets == 0 {
            0.0
        } else {
            (self.hits as f64 / self.gets as f64) * 100.0
        }
    }

    /// Calculate memory utilization percentage
    pub fn memory_utilization(&self) -> f64 {
        if self.max_memory_bytes == 0 {
            0.0
        } else {
            (self.memory_bytes as f64 / self.max_memory_bytes as f64) * 100.0
        }
    }
}
