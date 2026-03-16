//! Cache entry types and helpers.

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cached query result entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CacheEntry {
    /// Query result data
    #[serde(with = "serde_bytes_helper")]
    pub(super) result: Bytes,

    /// Object ETag when this result was cached
    pub(super) etag: String,

    /// Timestamp when this entry was created (Unix timestamp)
    pub(super) created_at: i64,

    /// Time-to-live in seconds (0 = no expiration)
    pub(super) ttl_seconds: u64,

    /// Last access timestamp (for LRU)
    pub(super) last_accessed: i64,

    /// Estimated size in bytes
    pub(super) size_bytes: usize,
}

impl CacheEntry {
    /// Check if entry is expired
    pub(super) fn is_expired(&self) -> bool {
        if self.ttl_seconds == 0 {
            return false;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        now - self.created_at > self.ttl_seconds as i64
    }

    /// Update last accessed timestamp
    pub(super) fn touch(&mut self) {
        self.last_accessed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
    }
}

/// Helper module for serializing/deserializing Bytes
pub(super) mod serde_bytes_helper {
    use bytes::Bytes;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Bytes, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(bytes.as_ref())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Bytes, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<u8> = Vec::deserialize(deserializer)?;
        Ok(Bytes::from(vec))
    }
}
