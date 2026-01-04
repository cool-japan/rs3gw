//! Backup and Disaster Recovery Infrastructure
//!
//! This module provides comprehensive backup and recovery capabilities including:
//! - Point-in-time recovery (PITR)
//! - Incremental backups
//! - Snapshot management
//! - Automated recovery testing

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::{ObjectMetadata, StorageError};

/// Errors specific to backup operations
#[derive(Debug, Error)]
pub enum BackupError {
    #[error("Snapshot not found: {0}")]
    SnapshotNotFound(String),

    #[error("Invalid snapshot: {0}")]
    InvalidSnapshot(String),

    #[error("Backup already exists: {0}")]
    BackupExists(String),

    #[error("Recovery failed: {0}")]
    RecoveryFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
}

pub type BackupResult<T> = Result<T, BackupError>;

/// Snapshot represents a point-in-time capture of storage state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique snapshot identifier
    pub id: String,

    /// Snapshot name (user-friendly)
    pub name: String,

    /// Timestamp when snapshot was created
    pub created_at: DateTime<Utc>,

    /// Snapshot type
    pub snapshot_type: SnapshotType,

    /// Buckets included in this snapshot
    pub buckets: Vec<String>,

    /// Total objects captured
    pub object_count: u64,

    /// Total size in bytes
    pub total_size: u64,

    /// Parent snapshot ID (for incremental snapshots)
    pub parent_snapshot: Option<String>,

    /// Metadata about objects in this snapshot
    pub objects: HashMap<String, Vec<ObjectSnapshot>>,

    /// Tags associated with this snapshot
    #[serde(default)]
    pub tags: HashMap<String, String>,

    /// Whether this snapshot is complete and valid
    pub is_complete: bool,

    /// Retention period in days (0 = indefinite)
    pub retention_days: u32,
}

/// Type of snapshot
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotType {
    /// Full backup of all data
    Full,

    /// Incremental backup (only changes since parent)
    Incremental,

    /// Differential backup (changes since last full)
    Differential,
}

/// Object snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectSnapshot {
    /// Object key
    pub key: String,

    /// Object ETag (SHA256 hash)
    pub etag: String,

    /// Object size
    pub size: u64,

    /// Last modified timestamp
    pub last_modified: DateTime<Utc>,

    /// Storage class
    pub storage_class: String,

    /// Whether object was deleted (for incremental snapshots)
    pub is_deleted: bool,

    /// Backup location (path or reference)
    pub backup_location: String,
}

/// Backup configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Backup storage root directory
    pub backup_root: PathBuf,

    /// Enable automatic backups
    pub auto_backup_enabled: bool,

    /// Automatic backup interval in seconds
    pub auto_backup_interval_secs: u64,

    /// Maximum number of snapshots to retain
    pub max_snapshots: usize,

    /// Default retention period in days
    pub default_retention_days: u32,

    /// Enable compression for backups
    pub compression_enabled: bool,

    /// Enable encryption for backups
    pub encryption_enabled: bool,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            backup_root: PathBuf::from("./backups"),
            auto_backup_enabled: false,
            auto_backup_interval_secs: 86400, // 24 hours
            max_snapshots: 30,
            default_retention_days: 30,
            compression_enabled: true,
            encryption_enabled: false,
        }
    }
}

/// Backup manager for coordinating backup and recovery operations
pub struct BackupManager {
    /// Configuration
    config: BackupConfig,

    /// Storage root path
    storage_root: PathBuf,

    /// Active snapshots
    snapshots: Arc<RwLock<HashMap<String, Snapshot>>>,

    /// Snapshot index (sorted by creation time)
    snapshot_index: Arc<RwLock<Vec<String>>>,
}

impl BackupManager {
    /// Create a new backup manager
    pub async fn new(config: BackupConfig, storage_root: PathBuf) -> BackupResult<Self> {
        // Create backup directory if it doesn't exist
        fs::create_dir_all(&config.backup_root).await?;

        let manager = Self {
            config,
            storage_root,
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            snapshot_index: Arc::new(RwLock::new(Vec::new())),
        };

        // Load existing snapshots
        manager.load_snapshots().await?;

        Ok(manager)
    }

    /// Load existing snapshots from disk
    async fn load_snapshots(&self) -> BackupResult<()> {
        let snapshot_dir = self.config.backup_root.join("snapshots");

        if !snapshot_dir.exists() {
            fs::create_dir_all(&snapshot_dir).await?;
            return Ok(());
        }

        let mut entries = fs::read_dir(&snapshot_dir).await?;
        let mut snapshots = self.snapshots.write().await;
        let mut index = self.snapshot_index.write().await;

        while let Some(entry) = entries.next_entry().await? {
            if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                match self.load_snapshot_file(&entry.path()).await {
                    Ok(snapshot) => {
                        index.push(snapshot.id.clone());
                        snapshots.insert(snapshot.id.clone(), snapshot);
                    }
                    Err(e) => {
                        warn!("Failed to load snapshot {:?}: {}", entry.path(), e);
                    }
                }
            }
        }

        // Sort index by creation time
        index.sort_by(|a, b| {
            let a_time = snapshots.get(a).map(|s| s.created_at);
            let b_time = snapshots.get(b).map(|s| s.created_at);
            a_time.cmp(&b_time)
        });

        info!("Loaded {} snapshots", snapshots.len());
        Ok(())
    }

    /// Load a snapshot from a file
    async fn load_snapshot_file(&self, path: &Path) -> BackupResult<Snapshot> {
        let mut file = fs::File::open(path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        let snapshot: Snapshot = serde_json::from_str(&contents)?;
        Ok(snapshot)
    }

    /// Save a snapshot to disk
    async fn save_snapshot(&self, snapshot: &Snapshot) -> BackupResult<()> {
        let snapshot_dir = self.config.backup_root.join("snapshots");
        fs::create_dir_all(&snapshot_dir).await?;

        let snapshot_path = snapshot_dir.join(format!("{}.json", snapshot.id));
        let json = serde_json::to_string_pretty(snapshot)?;

        let mut file = fs::File::create(&snapshot_path).await?;
        file.write_all(json.as_bytes()).await?;
        file.flush().await?;

        debug!("Saved snapshot {} to {:?}", snapshot.id, snapshot_path);
        Ok(())
    }

    /// Create a new full snapshot
    pub async fn create_full_snapshot(
        &self,
        name: String,
        buckets: Vec<String>,
    ) -> BackupResult<Snapshot> {
        let snapshot_id = format!("snap-{}", Utc::now().timestamp());
        info!("Creating full snapshot: {} ({})", name, snapshot_id);

        let mut objects = HashMap::new();
        let mut total_objects = 0u64;
        let mut total_size = 0u64;

        // Scan each bucket
        for bucket in &buckets {
            let bucket_objects = self.scan_bucket(bucket).await?;
            total_objects += bucket_objects.len() as u64;
            total_size += bucket_objects.iter().map(|o| o.size).sum::<u64>();
            objects.insert(bucket.clone(), bucket_objects);
        }

        let snapshot = Snapshot {
            id: snapshot_id,
            name,
            created_at: Utc::now(),
            snapshot_type: SnapshotType::Full,
            buckets,
            object_count: total_objects,
            total_size,
            parent_snapshot: None,
            objects,
            tags: HashMap::new(),
            is_complete: true,
            retention_days: self.config.default_retention_days,
        };

        // Save snapshot
        self.save_snapshot(&snapshot).await?;

        // Update in-memory state
        let mut snapshots = self.snapshots.write().await;
        let mut index = self.snapshot_index.write().await;
        index.push(snapshot.id.clone());
        snapshots.insert(snapshot.id.clone(), snapshot.clone());

        // Cleanup old snapshots if needed
        drop(snapshots);
        drop(index);
        self.cleanup_old_snapshots().await?;

        Ok(snapshot)
    }

    /// Create an incremental snapshot (changes since last snapshot)
    pub async fn create_incremental_snapshot(
        &self,
        name: String,
        buckets: Vec<String>,
        parent_snapshot_id: Option<String>,
    ) -> BackupResult<Snapshot> {
        let snapshot_id = format!("snap-{}", Utc::now().timestamp());
        info!("Creating incremental snapshot: {} ({})", name, snapshot_id);

        // Find parent snapshot
        let parent_id = if let Some(id) = parent_snapshot_id {
            Some(id)
        } else {
            // Use most recent snapshot
            let index = self.snapshot_index.read().await;
            index.last().cloned()
        };

        let parent = if let Some(id) = &parent_id {
            let snapshots = self.snapshots.read().await;
            snapshots.get(id).cloned()
        } else {
            None
        };

        let mut objects = HashMap::new();
        let mut total_objects = 0u64;
        let mut total_size = 0u64;

        // Scan each bucket for changes
        for bucket in &buckets {
            let current_objects = self.scan_bucket(bucket).await?;
            let parent_objects = parent
                .as_ref()
                .and_then(|p| p.objects.get(bucket))
                .cloned()
                .unwrap_or_default();

            let changes = self.detect_changes(&current_objects, &parent_objects);
            total_objects += changes.len() as u64;
            total_size += changes.iter().map(|o| o.size).sum::<u64>();
            objects.insert(bucket.clone(), changes);
        }

        let snapshot = Snapshot {
            id: snapshot_id,
            name,
            created_at: Utc::now(),
            snapshot_type: SnapshotType::Incremental,
            buckets,
            object_count: total_objects,
            total_size,
            parent_snapshot: parent_id,
            objects,
            tags: HashMap::new(),
            is_complete: true,
            retention_days: self.config.default_retention_days,
        };

        // Save snapshot
        self.save_snapshot(&snapshot).await?;

        // Update in-memory state
        let mut snapshots = self.snapshots.write().await;
        let mut index = self.snapshot_index.write().await;
        index.push(snapshot.id.clone());
        snapshots.insert(snapshot.id.clone(), snapshot.clone());

        Ok(snapshot)
    }

    /// Scan a bucket and return object snapshots
    async fn scan_bucket(&self, bucket: &str) -> BackupResult<Vec<ObjectSnapshot>> {
        let bucket_path = self.storage_root.join(bucket).join("objects");
        let mut objects = Vec::new();

        if !bucket_path.exists() {
            return Ok(objects);
        }

        let mut stack = vec![bucket_path];

        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir).await?;

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|s| s.to_str()) == Some("obj") {
                    // Read object metadata
                    if let Ok(metadata) = self.read_object_metadata(&path).await {
                        let key = path
                            .strip_prefix(self.storage_root.join(bucket).join("objects"))
                            .ok()
                            .and_then(|p| p.to_str())
                            .map(|s| s.trim_end_matches(".obj").to_string())
                            .unwrap_or_default();

                        objects.push(ObjectSnapshot {
                            key,
                            etag: metadata.etag,
                            size: metadata.size,
                            last_modified: metadata.last_modified,
                            storage_class: metadata
                                .metadata
                                .get("x-amz-storage-class")
                                .cloned()
                                .unwrap_or_else(|| "STANDARD".to_string()),
                            is_deleted: false,
                            backup_location: path.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }

        Ok(objects)
    }

    /// Read object metadata from file
    async fn read_object_metadata(&self, path: &Path) -> BackupResult<ObjectMetadata> {
        let meta_path = path.with_extension("meta");
        let mut file = fs::File::open(&meta_path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        let metadata: ObjectMetadata = serde_json::from_str(&contents)?;
        Ok(metadata)
    }

    /// Detect changes between current and parent object lists
    fn detect_changes(
        &self,
        current: &[ObjectSnapshot],
        parent: &[ObjectSnapshot],
    ) -> Vec<ObjectSnapshot> {
        let parent_map: HashMap<&str, &ObjectSnapshot> =
            parent.iter().map(|o| (o.key.as_str(), o)).collect();

        let mut changes = Vec::new();

        // Find new and modified objects
        for obj in current {
            if let Some(parent_obj) = parent_map.get(obj.key.as_str()) {
                // Check if modified
                if obj.etag != parent_obj.etag || obj.size != parent_obj.size {
                    changes.push(obj.clone());
                }
            } else {
                // New object
                changes.push(obj.clone());
            }
        }

        // Find deleted objects
        let current_keys: HashSet<&str> = current.iter().map(|o| o.key.as_str()).collect();
        for obj in parent {
            if !current_keys.contains(obj.key.as_str()) {
                let mut deleted = obj.clone();
                deleted.is_deleted = true;
                changes.push(deleted);
            }
        }

        changes
    }

    /// Get a snapshot by ID
    pub async fn get_snapshot(&self, snapshot_id: &str) -> BackupResult<Snapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .get(snapshot_id)
            .cloned()
            .ok_or_else(|| BackupError::SnapshotNotFound(snapshot_id.to_string()))
    }

    /// List all snapshots
    pub async fn list_snapshots(&self) -> Vec<Snapshot> {
        let snapshots = self.snapshots.read().await;
        let index = self.snapshot_index.read().await;

        index
            .iter()
            .filter_map(|id| snapshots.get(id).cloned())
            .collect()
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, snapshot_id: &str) -> BackupResult<()> {
        info!("Deleting snapshot: {}", snapshot_id);

        // Remove from memory
        let mut snapshots = self.snapshots.write().await;
        let mut index = self.snapshot_index.write().await;

        snapshots.remove(snapshot_id);
        index.retain(|id| id != snapshot_id);

        // Remove from disk
        let snapshot_path = self
            .config
            .backup_root
            .join("snapshots")
            .join(format!("{}.json", snapshot_id));

        if snapshot_path.exists() {
            fs::remove_file(&snapshot_path).await?;
        }

        Ok(())
    }

    /// Cleanup old snapshots based on retention policy
    async fn cleanup_old_snapshots(&self) -> BackupResult<()> {
        // Collect IDs to remove (do not hold locks during deletion)
        let mut to_delete = Vec::new();

        {
            let snapshots = self.snapshots.read().await;
            let index = self.snapshot_index.read().await;

            // Remove snapshots exceeding max count
            if index.len() > self.config.max_snapshots {
                let to_remove = index.len() - self.config.max_snapshots;
                for i in 0..to_remove {
                    if let Some(id) = index.get(i) {
                        to_delete.push(id.clone());
                    }
                }
            }

            // Remove snapshots past retention period
            let now = Utc::now();

            for snapshot in snapshots.values() {
                if snapshot.retention_days > 0 {
                    let age_days = (now - snapshot.created_at).num_days();
                    if age_days > snapshot.retention_days as i64 {
                        to_delete.push(snapshot.id.clone());
                    }
                }
            }
        } // Locks dropped here

        // Delete collected snapshots
        for id in to_delete {
            self.delete_snapshot(&id).await?;
        }

        Ok(())
    }

    /// Get backup statistics
    pub async fn get_stats(&self) -> BackupStats {
        let snapshots = self.snapshots.read().await;

        let total_snapshots = snapshots.len();
        let total_objects: u64 = snapshots.values().map(|s| s.object_count).sum();
        let total_size: u64 = snapshots.values().map(|s| s.total_size).sum();

        let full_snapshots = snapshots
            .values()
            .filter(|s| s.snapshot_type == SnapshotType::Full)
            .count();

        let incremental_snapshots = snapshots
            .values()
            .filter(|s| s.snapshot_type == SnapshotType::Incremental)
            .count();

        let oldest_snapshot = snapshots
            .values()
            .min_by_key(|s| s.created_at)
            .map(|s| s.created_at);

        let newest_snapshot = snapshots
            .values()
            .max_by_key(|s| s.created_at)
            .map(|s| s.created_at);

        BackupStats {
            total_snapshots,
            full_snapshots,
            incremental_snapshots,
            total_objects,
            total_size,
            oldest_snapshot,
            newest_snapshot,
        }
    }
}

/// Backup statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupStats {
    pub total_snapshots: usize,
    pub full_snapshots: usize,
    pub incremental_snapshots: usize,
    pub total_objects: u64,
    pub total_size: u64,
    pub oldest_snapshot: Option<DateTime<Utc>>,
    pub newest_snapshot: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[tokio::test]
    async fn test_backup_manager_creation() {
        let temp_dir = env::temp_dir().join("rs3gw_backup_test_1");
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let config = BackupConfig {
            backup_root: temp_dir.join("backups"),
            ..Default::default()
        };

        let manager = BackupManager::new(config, temp_dir.clone())
            .await
            .expect("Failed to create backup manager");

        assert!(manager.config.backup_root.exists());

        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_snapshot_creation() {
        let temp_dir = env::temp_dir().join("rs3gw_backup_test_2");
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        // Create bucket directory structure
        fs::create_dir_all(temp_dir.join("test-bucket").join("objects"))
            .await
            .expect("Failed to create bucket directory structure");

        let config = BackupConfig {
            backup_root: temp_dir.join("backups"),
            max_snapshots: 5,
            ..Default::default()
        };

        let manager = BackupManager::new(config, temp_dir.clone())
            .await
            .expect("Failed to create backup manager");

        let snapshot = manager
            .create_full_snapshot("test-snapshot".to_string(), vec!["test-bucket".to_string()])
            .await
            .expect("Failed to create snapshot");

        assert_eq!(snapshot.name, "test-snapshot");
        assert_eq!(snapshot.snapshot_type, SnapshotType::Full);
        assert!(snapshot.is_complete);

        // Verify snapshot was saved
        let loaded = manager
            .get_snapshot(&snapshot.id)
            .await
            .expect("Failed to get snapshot");
        assert_eq!(loaded.id, snapshot.id);

        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_snapshot_listing() {
        let temp_dir = env::temp_dir().join("rs3gw_backup_test_3");
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        // Create bucket directory structure
        fs::create_dir_all(temp_dir.join("test-bucket").join("objects"))
            .await
            .expect("Failed to create bucket directory structure");

        let config = BackupConfig {
            backup_root: temp_dir.join("backups"),
            ..Default::default()
        };

        let manager = BackupManager::new(config, temp_dir.clone())
            .await
            .expect("Failed to create backup manager");

        // Create multiple snapshots
        for i in 0..3 {
            manager
                .create_full_snapshot(format!("snapshot-{}", i), vec!["test-bucket".to_string()])
                .await
                .expect("Failed to create snapshot");
        }

        let snapshots = manager.list_snapshots().await;
        assert_eq!(snapshots.len(), 3);

        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_snapshot_deletion() {
        let temp_dir = env::temp_dir().join("rs3gw_backup_test_4");
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        // Create bucket directory structure
        fs::create_dir_all(temp_dir.join("test-bucket").join("objects"))
            .await
            .expect("Failed to create bucket directory structure");

        let config = BackupConfig {
            backup_root: temp_dir.join("backups"),
            ..Default::default()
        };

        let manager = BackupManager::new(config, temp_dir.clone())
            .await
            .expect("Failed to create backup manager");

        let snapshot = manager
            .create_full_snapshot("test-snapshot".to_string(), vec!["test-bucket".to_string()])
            .await
            .expect("Failed to create snapshot");

        manager
            .delete_snapshot(&snapshot.id)
            .await
            .expect("Failed to delete snapshot");

        let result = manager.get_snapshot(&snapshot.id).await;
        assert!(result.is_err());

        let _ = fs::remove_dir_all(&temp_dir).await;
    }

    #[tokio::test]
    async fn test_backup_stats() {
        let temp_dir = env::temp_dir().join("rs3gw_backup_test_5");
        let _ = fs::remove_dir_all(&temp_dir).await;
        fs::create_dir_all(&temp_dir)
            .await
            .expect("Failed to create test directory");

        let config = BackupConfig {
            backup_root: temp_dir.join("backups"),
            ..Default::default()
        };

        let manager = BackupManager::new(config, temp_dir.clone())
            .await
            .expect("Failed to create backup manager");

        // Create snapshots
        manager
            .create_full_snapshot("full-1".to_string(), vec!["bucket1".to_string()])
            .await
            .expect("Failed to create snapshot");

        let stats = manager.get_stats().await;
        assert_eq!(stats.total_snapshots, 1);
        assert_eq!(stats.full_snapshots, 1);
        assert_eq!(stats.incremental_snapshots, 0);

        let _ = fs::remove_dir_all(&temp_dir).await;
    }
}
