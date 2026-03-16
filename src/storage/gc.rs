//! Background garbage collector for abandoned multipart uploads.
//!
//! Provides a periodic scheduler that runs [`StorageEngine::gc_abandoned_multipart`]
//! at a configurable interval, cleaning up uploads older than the retention window.

use crate::storage::StorageEngine;
use std::sync::Arc;
use tokio::time::{interval, Duration};

/// Spawns a background task that periodically cleans up abandoned multipart uploads.
///
/// # Arguments
/// * `storage` - The storage engine to scan for abandoned uploads
/// * `retention_hours` - Uploads older than this are considered abandoned
/// * `interval_secs` - How often to run the GC sweep (in seconds)
///
/// # Returns
/// A `JoinHandle` that can be used to abort the GC task on shutdown.
pub fn spawn_multipart_gc(
    storage: Arc<StorageEngine>,
    retention_hours: u64,
    interval_secs: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(interval_secs));
        loop {
            ticker.tick().await;
            match storage.gc_abandoned_multipart(None, retention_hours).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(
                            cleaned = count,
                            retention_hours,
                            "Multipart GC sweep completed"
                        );
                    } else {
                        tracing::debug!("Multipart GC sweep: no abandoned uploads found");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Multipart GC sweep failed");
                }
            }
        }
    })
}

/// Runs a single GC sweep across all buckets (or a specific bucket).
///
/// This is a convenience wrapper around [`StorageEngine::gc_abandoned_multipart`]
/// that returns the number of cleaned uploads.
pub async fn gc_sweep(
    storage: &StorageEngine,
    bucket: Option<&str>,
    retention_hours: u64,
) -> Result<u64, crate::storage::StorageError> {
    storage
        .gc_abandoned_multipart(bucket, retention_hours)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_gc_sweep_empty_storage() {
        let tmp = std::env::temp_dir().join(format!("rs3gw-gc-test-{}", uuid::Uuid::new_v4()));
        let storage = StorageEngine::new(tmp.clone()).expect("failed to create storage engine");
        let result = gc_sweep(&storage, None, 1).await;
        assert!(result.is_ok());
        assert_eq!(result.expect("gc_sweep failed"), 0);
        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }
}
