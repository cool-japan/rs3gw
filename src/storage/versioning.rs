//! Object versioning support for S3-compatible storage
//!
//! This module implements S3-compatible object versioning, allowing multiple
//! versions of the same object to coexist. When versioning is enabled on a bucket,
//! S3 preserves all versions of an object (including all writes and even if you
//! delete an object).
//!
//! # Features
//!
//! - Enable/disable/suspend versioning per bucket
//! - Automatic version ID generation (UUID v4)
//! - Version history tracking
//! - Delete markers for soft deletes
//! - List object versions
//! - Retrieve specific versions
//! - Delete specific versions
//!
//! # Versioning States
//!
//! - **Unversioned**: Default state, no versioning
//! - **Enabled**: All PUT/DELETE operations create new versions
//! - **Suspended**: New objects get version ID "null", existing versions preserved

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::StorageError;

/// Versioning configuration state for a bucket
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum VersioningStatus {
    /// Versioning is not enabled (default)
    #[default]
    Unversioned,
    /// Versioning is enabled - all operations create versions
    Enabled,
    /// Versioning is suspended - new objects get "null" version ID
    Suspended,
}

impl VersioningStatus {
    /// Check if versioning is enabled
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled)
    }

    /// Check if versioning is suspended
    pub fn is_suspended(&self) -> bool {
        matches!(self, Self::Suspended)
    }
}

/// Configuration for bucket versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketVersioningConfig {
    /// Current versioning status
    pub status: VersioningStatus,
    /// When versioning was enabled (if ever)
    pub enabled_at: Option<DateTime<Utc>>,
    /// When versioning was last modified
    pub modified_at: DateTime<Utc>,
}

impl Default for BucketVersioningConfig {
    fn default() -> Self {
        Self {
            status: VersioningStatus::Unversioned,
            enabled_at: None,
            modified_at: Utc::now(),
        }
    }
}

impl BucketVersioningConfig {
    /// Create a new versioning config with status
    pub fn new(status: VersioningStatus) -> Self {
        Self {
            status,
            enabled_at: if status == VersioningStatus::Enabled {
                Some(Utc::now())
            } else {
                None
            },
            modified_at: Utc::now(),
        }
    }

    /// Enable versioning
    pub fn enable(&mut self) {
        if self.enabled_at.is_none() {
            self.enabled_at = Some(Utc::now());
        }
        self.status = VersioningStatus::Enabled;
        self.modified_at = Utc::now();
    }

    /// Suspend versioning
    pub fn suspend(&mut self) {
        self.status = VersioningStatus::Suspended;
        self.modified_at = Utc::now();
    }
}

/// Metadata for a specific object version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectVersionMetadata {
    /// Version ID (UUID or "null" for suspended versioning)
    pub version_id: String,
    /// Object key
    pub key: String,
    /// Is this the latest version?
    pub is_latest: bool,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Size in bytes
    pub size: u64,
    /// ETag
    pub etag: String,
    /// Storage class
    pub storage_class: String,
    /// Is this a delete marker?
    pub is_delete_marker: bool,
    /// Owner (optional)
    pub owner: Option<String>,
}

impl ObjectVersionMetadata {
    /// Create new version metadata
    pub fn new(key: String, size: u64, etag: String) -> Self {
        Self {
            version_id: Uuid::new_v4().to_string(),
            key,
            is_latest: true,
            created_at: Utc::now(),
            size,
            etag,
            storage_class: "STANDARD".to_string(),
            is_delete_marker: false,
            owner: None,
        }
    }

    /// Create a delete marker
    pub fn delete_marker(key: String) -> Self {
        Self {
            version_id: Uuid::new_v4().to_string(),
            key,
            is_latest: true,
            created_at: Utc::now(),
            size: 0,
            etag: String::new(),
            storage_class: "STANDARD".to_string(),
            is_delete_marker: true,
            owner: None,
        }
    }

    /// Create version with "null" ID (for suspended versioning)
    pub fn null_version(key: String, size: u64, etag: String) -> Self {
        Self {
            version_id: "null".to_string(),
            key,
            is_latest: true,
            created_at: Utc::now(),
            size,
            etag,
            storage_class: "STANDARD".to_string(),
            is_delete_marker: false,
            owner: None,
        }
    }
}

/// Version index for a single object (all versions of one key)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectVersionIndex {
    /// Object key
    pub key: String,
    /// All versions, sorted by creation time (newest first)
    pub versions: Vec<ObjectVersionMetadata>,
}

impl ObjectVersionIndex {
    /// Create new version index for a key
    pub fn new(key: String) -> Self {
        Self {
            key,
            versions: Vec::new(),
        }
    }

    /// Add a new version
    pub fn add_version(&mut self, mut version: ObjectVersionMetadata) {
        // Mark all existing versions as not latest
        for v in &mut self.versions {
            v.is_latest = false;
        }

        // Add new version as latest
        version.is_latest = true;
        self.versions.insert(0, version);
    }

    /// Get the latest version
    pub fn latest(&self) -> Option<&ObjectVersionMetadata> {
        self.versions.first()
    }

    /// Get a specific version by ID
    pub fn get_version(&self, version_id: &str) -> Option<&ObjectVersionMetadata> {
        self.versions.iter().find(|v| v.version_id == version_id)
    }

    /// Remove a specific version
    pub fn remove_version(&mut self, version_id: &str) -> bool {
        if let Some(pos) = self
            .versions
            .iter()
            .position(|v| v.version_id == version_id)
        {
            self.versions.remove(pos);
            // If we removed the latest, mark the new first as latest
            if let Some(first) = self.versions.first_mut() {
                first.is_latest = true;
            }
            true
        } else {
            false
        }
    }

    /// Check if the latest version is a delete marker
    pub fn is_deleted(&self) -> bool {
        self.latest().map(|v| v.is_delete_marker).unwrap_or(false)
    }
}

/// Bucket version index - manages all versioned objects in a bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketVersionIndex {
    /// Bucket name
    pub bucket: String,
    /// Map of object key -> version index
    pub objects: HashMap<String, ObjectVersionIndex>,
}

impl BucketVersionIndex {
    /// Create new bucket version index
    pub fn new(bucket: String) -> Self {
        Self {
            bucket,
            objects: HashMap::new(),
        }
    }

    /// Add a version for an object
    pub fn add_version(&mut self, key: String, version: ObjectVersionMetadata) {
        self.objects
            .entry(key.clone())
            .or_insert_with(|| ObjectVersionIndex::new(key))
            .add_version(version);
    }

    /// Get all versions for an object
    pub fn get_object_versions(&self, key: &str) -> Option<&ObjectVersionIndex> {
        self.objects.get(key)
    }

    /// Get a specific version
    pub fn get_version(&self, key: &str, version_id: &str) -> Option<&ObjectVersionMetadata> {
        self.objects
            .get(key)
            .and_then(|idx| idx.get_version(version_id))
    }

    /// Get the latest version of an object
    pub fn get_latest(&self, key: &str) -> Option<&ObjectVersionMetadata> {
        self.objects.get(key).and_then(|idx| idx.latest())
    }

    /// Remove a specific version
    pub fn remove_version(&mut self, key: &str, version_id: &str) -> bool {
        if let Some(idx) = self.objects.get_mut(key) {
            let removed = idx.remove_version(version_id);
            // If no versions left, remove the object entry
            if idx.versions.is_empty() {
                self.objects.remove(key);
            }
            removed
        } else {
            false
        }
    }

    /// List all versions in the bucket
    pub fn list_versions(&self) -> Vec<&ObjectVersionMetadata> {
        let mut versions: Vec<&ObjectVersionMetadata> = self
            .objects
            .values()
            .flat_map(|idx| &idx.versions)
            .collect();

        // Sort by creation time, newest first
        versions.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        versions
    }

    /// Count total versions
    pub fn total_versions(&self) -> usize {
        self.objects.values().map(|idx| idx.versions.len()).sum()
    }
}

/// Manager for bucket versioning configurations and indices
pub struct VersioningManager {
    /// Base storage path
    storage_root: PathBuf,
    /// In-memory cache of versioning configs (bucket -> config)
    configs: Arc<RwLock<HashMap<String, BucketVersioningConfig>>>,
    /// In-memory cache of version indices (bucket -> index)
    indices: Arc<RwLock<HashMap<String, BucketVersionIndex>>>,
}

impl VersioningManager {
    /// Create new versioning manager
    pub fn new(storage_root: PathBuf) -> Self {
        Self {
            storage_root,
            configs: Arc::new(RwLock::new(HashMap::new())),
            indices: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get path to versioning config file
    fn config_path(&self, bucket: &str) -> PathBuf {
        self.storage_root
            .join(bucket)
            .join(".versioning-config.json")
    }

    /// Get path to version index file
    fn index_path(&self, bucket: &str) -> PathBuf {
        self.storage_root.join(bucket).join(".version-index.json")
    }

    /// Load versioning config from disk
    async fn load_config(&self, bucket: &str) -> Result<BucketVersioningConfig, StorageError> {
        let path = self.config_path(bucket);
        if path.exists() {
            let data = fs::read(&path).await.map_err(|e| {
                StorageError::Internal(format!("Failed to read versioning config: {}", e))
            })?;
            serde_json::from_slice(&data).map_err(|e| {
                StorageError::Internal(format!("Failed to parse versioning config: {}", e))
            })
        } else {
            Ok(BucketVersioningConfig::default())
        }
    }

    /// Save versioning config to disk
    async fn save_config(
        &self,
        bucket: &str,
        config: &BucketVersioningConfig,
    ) -> Result<(), StorageError> {
        let path = self.config_path(bucket);
        let data = serde_json::to_vec_pretty(config).map_err(|e| {
            StorageError::Internal(format!("Failed to serialize versioning config: {}", e))
        })?;
        fs::write(&path, data).await.map_err(|e| {
            StorageError::Internal(format!("Failed to write versioning config: {}", e))
        })
    }

    /// Load version index from disk
    async fn load_index(&self, bucket: &str) -> Result<BucketVersionIndex, StorageError> {
        let path = self.index_path(bucket);
        if path.exists() {
            let data = fs::read(&path).await.map_err(|e| {
                StorageError::Internal(format!("Failed to read version index: {}", e))
            })?;
            serde_json::from_slice(&data).map_err(|e| {
                StorageError::Internal(format!("Failed to parse version index: {}", e))
            })
        } else {
            Ok(BucketVersionIndex::new(bucket.to_string()))
        }
    }

    /// Save version index to disk
    async fn save_index(
        &self,
        bucket: &str,
        index: &BucketVersionIndex,
    ) -> Result<(), StorageError> {
        let path = self.index_path(bucket);
        let data = serde_json::to_vec_pretty(index).map_err(|e| {
            StorageError::Internal(format!("Failed to serialize version index: {}", e))
        })?;
        fs::write(&path, data)
            .await
            .map_err(|e| StorageError::Internal(format!("Failed to write version index: {}", e)))
    }

    /// Get versioning config for a bucket
    pub async fn get_config(&self, bucket: &str) -> Result<BucketVersioningConfig, StorageError> {
        // Check cache first
        {
            let configs = self.configs.read().await;
            if let Some(config) = configs.get(bucket) {
                return Ok(config.clone());
            }
        }

        // Load from disk
        let config = self.load_config(bucket).await?;

        // Update cache
        {
            let mut configs = self.configs.write().await;
            configs.insert(bucket.to_string(), config.clone());
        }

        Ok(config)
    }

    /// Set versioning config for a bucket
    pub async fn set_config(
        &self,
        bucket: &str,
        config: BucketVersioningConfig,
    ) -> Result<(), StorageError> {
        // Save to disk
        self.save_config(bucket, &config).await?;

        // Update cache
        {
            let mut configs = self.configs.write().await;
            configs.insert(bucket.to_string(), config);
        }

        Ok(())
    }

    /// Enable versioning for a bucket
    pub async fn enable_versioning(&self, bucket: &str) -> Result<(), StorageError> {
        let mut config = self.get_config(bucket).await?;
        config.enable();
        self.set_config(bucket, config).await
    }

    /// Suspend versioning for a bucket
    pub async fn suspend_versioning(&self, bucket: &str) -> Result<(), StorageError> {
        let mut config = self.get_config(bucket).await?;
        config.suspend();
        self.set_config(bucket, config).await
    }

    /// Get version index for a bucket
    pub async fn get_index(&self, bucket: &str) -> Result<BucketVersionIndex, StorageError> {
        // Check cache first
        {
            let indices = self.indices.read().await;
            if let Some(index) = indices.get(bucket) {
                return Ok(index.clone());
            }
        }

        // Load from disk
        let index = self.load_index(bucket).await?;

        // Update cache
        {
            let mut indices = self.indices.write().await;
            indices.insert(bucket.to_string(), index.clone());
        }

        Ok(index)
    }

    /// Add a version to the index
    pub async fn add_version(
        &self,
        bucket: &str,
        key: String,
        version: ObjectVersionMetadata,
    ) -> Result<(), StorageError> {
        let mut index = self.get_index(bucket).await?;
        index.add_version(key, version);

        // Save to disk
        self.save_index(bucket, &index).await?;

        // Update cache
        {
            let mut indices = self.indices.write().await;
            indices.insert(bucket.to_string(), index);
        }

        Ok(())
    }

    /// Get a specific version
    pub async fn get_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<Option<ObjectVersionMetadata>, StorageError> {
        let index = self.get_index(bucket).await?;
        Ok(index.get_version(key, version_id).cloned())
    }

    /// Get the latest version
    pub async fn get_latest(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Option<ObjectVersionMetadata>, StorageError> {
        let index = self.get_index(bucket).await?;
        Ok(index.get_latest(key).cloned())
    }

    /// List all versions for an object
    pub async fn list_object_versions(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<Vec<ObjectVersionMetadata>, StorageError> {
        let index = self.get_index(bucket).await?;
        Ok(index
            .get_object_versions(key)
            .map(|idx| idx.versions.clone())
            .unwrap_or_default())
    }

    /// List all versions in a bucket
    pub async fn list_all_versions(
        &self,
        bucket: &str,
    ) -> Result<Vec<ObjectVersionMetadata>, StorageError> {
        let index = self.get_index(bucket).await?;
        Ok(index.list_versions().into_iter().cloned().collect())
    }

    /// Delete a specific version
    pub async fn delete_version(
        &self,
        bucket: &str,
        key: &str,
        version_id: &str,
    ) -> Result<bool, StorageError> {
        let mut index = self.get_index(bucket).await?;
        let removed = index.remove_version(key, version_id);

        if removed {
            // Save to disk
            self.save_index(bucket, &index).await?;

            // Update cache
            {
                let mut indices = self.indices.write().await;
                indices.insert(bucket.to_string(), index);
            }
        }

        Ok(removed)
    }

    /// Create a delete marker
    pub async fn create_delete_marker(
        &self,
        bucket: &str,
        key: String,
    ) -> Result<ObjectVersionMetadata, StorageError> {
        let delete_marker = ObjectVersionMetadata::delete_marker(key.clone());
        self.add_version(bucket, key, delete_marker.clone()).await?;
        Ok(delete_marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_versioning_status() {
        let mut config = BucketVersioningConfig::default();
        assert_eq!(config.status, VersioningStatus::Unversioned);
        assert!(!config.status.is_enabled());

        config.enable();
        assert_eq!(config.status, VersioningStatus::Enabled);
        assert!(config.status.is_enabled());
        assert!(config.enabled_at.is_some());

        config.suspend();
        assert_eq!(config.status, VersioningStatus::Suspended);
        assert!(config.status.is_suspended());
    }

    #[test]
    fn test_object_version_metadata() {
        let version =
            ObjectVersionMetadata::new("test-key".to_string(), 1024, "abc123".to_string());
        assert!(version.is_latest);
        assert!(!version.is_delete_marker);
        assert_ne!(version.version_id, "null");

        let delete_marker = ObjectVersionMetadata::delete_marker("test-key".to_string());
        assert!(delete_marker.is_delete_marker);
        assert_eq!(delete_marker.size, 0);

        let null_version =
            ObjectVersionMetadata::null_version("test-key".to_string(), 1024, "def456".to_string());
        assert_eq!(null_version.version_id, "null");
    }

    #[test]
    fn test_object_version_index() {
        let mut index = ObjectVersionIndex::new("test-key".to_string());

        let v1 = ObjectVersionMetadata::new("test-key".to_string(), 100, "etag1".to_string());
        let v1_id = v1.version_id.clone();
        index.add_version(v1);

        assert_eq!(index.versions.len(), 1);
        assert!(index.latest().is_some());
        assert!(index.latest().map(|v| v.is_latest).unwrap_or(false));

        let v2 = ObjectVersionMetadata::new("test-key".to_string(), 200, "etag2".to_string());
        let v2_id = v2.version_id.clone();
        index.add_version(v2);

        assert_eq!(index.versions.len(), 2);
        // v2 should be latest, v1 should not be
        assert_eq!(
            index
                .latest()
                .map(|v| v.version_id.clone())
                .unwrap_or_default(),
            v2_id
        );
        assert!(!index
            .get_version(&v1_id)
            .map(|v| v.is_latest)
            .unwrap_or(true));

        // Test removal
        assert!(index.remove_version(&v2_id));
        assert_eq!(index.versions.len(), 1);
        assert_eq!(
            index
                .latest()
                .map(|v| v.version_id.clone())
                .unwrap_or_default(),
            v1_id
        );
    }

    #[test]
    fn test_bucket_version_index() {
        let mut bucket_index = BucketVersionIndex::new("test-bucket".to_string());

        let v1 = ObjectVersionMetadata::new("key1".to_string(), 100, "etag1".to_string());
        bucket_index.add_version("key1".to_string(), v1);

        let v2 = ObjectVersionMetadata::new("key2".to_string(), 200, "etag2".to_string());
        bucket_index.add_version("key2".to_string(), v2);

        assert_eq!(bucket_index.objects.len(), 2);
        assert_eq!(bucket_index.total_versions(), 2);

        let all_versions = bucket_index.list_versions();
        assert_eq!(all_versions.len(), 2);
    }

    #[tokio::test]
    async fn test_versioning_manager() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_root = temp_dir.path().to_path_buf();

        // Create bucket directory
        let bucket_path = storage_root.join("test-bucket");
        fs::create_dir_all(&bucket_path)
            .await
            .expect("Failed to create bucket dir");

        let manager = VersioningManager::new(storage_root);

        // Default config should be Unversioned
        let config = manager
            .get_config("test-bucket")
            .await
            .expect("Failed to get config");
        assert_eq!(config.status, VersioningStatus::Unversioned);

        // Enable versioning
        manager
            .enable_versioning("test-bucket")
            .await
            .expect("Failed to enable versioning");
        let config = manager
            .get_config("test-bucket")
            .await
            .expect("Failed to get config");
        assert_eq!(config.status, VersioningStatus::Enabled);

        // Suspend versioning
        manager
            .suspend_versioning("test-bucket")
            .await
            .expect("Failed to suspend versioning");
        let config = manager
            .get_config("test-bucket")
            .await
            .expect("Failed to get config");
        assert_eq!(config.status, VersioningStatus::Suspended);
    }

    #[tokio::test]
    async fn test_add_and_retrieve_versions() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_root = temp_dir.path().to_path_buf();

        let bucket_path = storage_root.join("test-bucket");
        fs::create_dir_all(&bucket_path)
            .await
            .expect("Failed to create bucket dir");

        let manager = VersioningManager::new(storage_root);

        let v1 = ObjectVersionMetadata::new("test-key".to_string(), 100, "etag1".to_string());
        let v1_id = v1.version_id.clone();

        manager
            .add_version("test-bucket", "test-key".to_string(), v1)
            .await
            .expect("Failed to add version");

        let retrieved = manager
            .get_version("test-bucket", "test-key", &v1_id)
            .await
            .expect("Failed to get version");

        assert!(retrieved.is_some());
        assert_eq!(retrieved.map(|v| v.version_id).unwrap_or_default(), v1_id);
    }

    #[tokio::test]
    async fn test_delete_marker() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_root = temp_dir.path().to_path_buf();

        let bucket_path = storage_root.join("test-bucket");
        fs::create_dir_all(&bucket_path)
            .await
            .expect("Failed to create bucket dir");

        let manager = VersioningManager::new(storage_root);

        let delete_marker = manager
            .create_delete_marker("test-bucket", "test-key".to_string())
            .await
            .expect("Failed to create delete marker");

        assert!(delete_marker.is_delete_marker);

        let latest = manager
            .get_latest("test-bucket", "test-key")
            .await
            .expect("Failed to get latest");

        assert!(latest.is_some());
        assert!(latest.map(|v| v.is_delete_marker).unwrap_or(false));
    }

    #[tokio::test]
    async fn test_list_versions() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage_root = temp_dir.path().to_path_buf();

        let bucket_path = storage_root.join("test-bucket");
        fs::create_dir_all(&bucket_path)
            .await
            .expect("Failed to create bucket dir");

        let manager = VersioningManager::new(storage_root);

        // Add multiple versions
        for i in 0..3 {
            let version = ObjectVersionMetadata::new(
                "test-key".to_string(),
                100 * (i + 1),
                format!("etag{}", i),
            );
            manager
                .add_version("test-bucket", "test-key".to_string(), version)
                .await
                .expect("Failed to add version");
        }

        let versions = manager
            .list_object_versions("test-bucket", "test-key")
            .await
            .expect("Failed to list versions");

        assert_eq!(versions.len(), 3);
        // First version should be latest
        assert!(versions[0].is_latest);
        assert!(!versions[1].is_latest);
        assert!(!versions[2].is_latest);
    }
}
