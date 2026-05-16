//! Object Lock storage manager.
//!
//! Stores per-version Object Lock metadata (LegalHold status and Retention policy)
//! as sidecar JSON files under `<root>/<bucket>/object_lock/<sanitized_key>/<version_id>.json`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

use super::core::path_utils::sanitize_key_for_fs;
use super::StorageError;

/// Per-version Object Lock metadata stored as a sidecar JSON file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectLockMetadata {
    /// Legal hold status: `"ON"` or `"OFF"`.
    pub legal_hold_status: Option<String>,
    /// Retention mode: `"GOVERNANCE"` or `"COMPLIANCE"`.
    pub retention_mode: Option<String>,
    /// Absolute UTC timestamp until which the object is locked.
    pub retain_until_date: Option<DateTime<Utc>>,
}

/// Manager for per-version Object Lock metadata.
pub struct ObjectLockManager {
    root_path: PathBuf,
}

impl ObjectLockManager {
    /// Create a new manager rooted at the storage root.
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    /// Build the sidecar path for a given bucket/key/version.
    fn sidecar_path(&self, bucket: &str, key: &str, version_id: &str) -> PathBuf {
        let sanitized_key = sanitize_key_for_fs(key);
        let sanitized_version = sanitize_key_for_fs(version_id);
        self.root_path
            .join(bucket)
            .join("object_lock")
            .join(sanitized_key)
            .join(format!("{}.json", sanitized_version))
    }

    /// Read the sidecar for a given version.
    ///
    /// Returns `StorageError::NotFound` when no sidecar exists yet.
    pub async fn get(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<ObjectLockMetadata, StorageError> {
        let path = self.sidecar_path(bucket, key, version_id);
        if !path.exists() {
            return Err(StorageError::NotFound(format!(
                "No object lock metadata for {}/{} version {}",
                bucket, key, version_id
            )));
        }
        let data = fs::read(&path).await?;
        serde_json::from_slice(&data)
            .map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
    }

    /// Set the legal-hold status (`"ON"` or `"OFF"`) for a version.
    pub async fn put_legal_hold(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
        status: &str,
    ) -> Result<(), StorageError> {
        let path = self.sidecar_path(bucket, key, version_id);
        let mut meta = self.get(bucket, key, version_id).await.unwrap_or_default();
        meta.legal_hold_status = Some(status.to_string());
        self.write_metadata(&path, &meta).await
    }

    /// Set or extend the retention policy for a version.
    ///
    /// `bypass` must be `true` to override an active GOVERNANCE lock before its
    /// expiry.  COMPLIANCE retention dates can never be shortened.
    pub async fn put_retention(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
        mode: &str,
        until: DateTime<Utc>,
        bypass: bool,
    ) -> Result<(), StorageError> {
        let path = self.sidecar_path(bucket, key, version_id);
        let existing = self.get(bucket, key, version_id).await.unwrap_or_default();

        if let Some(ref existing_mode) = existing.retention_mode {
            if existing_mode == "COMPLIANCE" {
                // COMPLIANCE: cannot shorten retention date, even with bypass
                if let Some(existing_until) = existing.retain_until_date {
                    if until < existing_until {
                        return Err(StorageError::ObjectLocked(
                            "Cannot shorten COMPLIANCE retention period".to_string(),
                        ));
                    }
                }
            } else if existing_mode == "GOVERNANCE" {
                // GOVERNANCE: can override only with bypass header
                if let Some(existing_until) = existing.retain_until_date {
                    if existing_until > Utc::now() && !bypass {
                        return Err(StorageError::ObjectLocked(
                            "GOVERNANCE retention active; x-amz-bypass-governance-retention header required".to_string(),
                        ));
                    }
                }
            }
        }

        let mut meta = existing;
        meta.retention_mode = Some(mode.to_string());
        meta.retain_until_date = Some(until);
        self.write_metadata(&path, &meta).await
    }

    /// Returns `true` if the version is protected by an active LegalHold or
    /// an unexpired retention period.
    pub async fn is_protected(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<bool, StorageError> {
        let meta = match self.get(bucket, key, version_id).await {
            Ok(m) => m,
            Err(StorageError::NotFound(_)) => return Ok(false),
            Err(e) => return Err(e),
        };
        // LegalHold ON
        if meta.legal_hold_status.as_deref() == Some("ON") {
            return Ok(true);
        }
        // Unexpired retention
        if let Some(until) = meta.retain_until_date {
            if until > Utc::now() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns the active retention mode (`GOVERNANCE` or `COMPLIANCE`) if the
    /// retention period has not yet expired, or `None` otherwise.
    pub async fn retention_mode(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<Option<String>, StorageError> {
        let meta = match self.get(bucket, key, version_id).await {
            Ok(m) => m,
            Err(StorageError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        if let Some(until) = meta.retain_until_date {
            if until > Utc::now() {
                return Ok(meta.retention_mode);
            }
        }
        Ok(None)
    }

    /// Write metadata to disk, creating parent directories as needed.
    async fn write_metadata(
        &self,
        path: &PathBuf,
        meta: &ObjectLockMetadata,
    ) -> Result<(), StorageError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let data = serde_json::to_vec_pretty(meta).map_err(|e| {
            StorageError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        })?;
        fs::write(path, data).await?;
        Ok(())
    }
}
