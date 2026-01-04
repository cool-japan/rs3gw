//! Dataset Registry - Version control and lineage tracking for ML datasets
//!
//! This module provides a dataset registry system for tracking dataset versions,
//! metadata, splits, and lineage relationships with models.
//!
//! # Features
//!
//! - Dataset registration and versioning
//! - Dataset split management (train/validation/test)
//! - Dataset metadata and statistics
//! - Lineage tracking (dataset → model relationships)
//! - Provenance information (source, transformations)
//! - Annotation version control
//! - Dataset diff capabilities
//!
//! # Example
//!
//! ```no_run
//! use rs3gw::storage::dataset_registry::{DatasetRegistry, DatasetSplit};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = DatasetRegistry::new("./datasets-registry".into()).await?;
//!
//! // Register a new dataset
//! let dataset = registry.register_dataset(
//!     "imagenet-subset",
//!     "ImageNet validation subset for testing"
//! ).await?;
//!
//! // Create a new version with splits
//! let version = registry.create_dataset_version(
//!     "imagenet-subset",
//!     "s3://bucket/datasets/imagenet-v1/",
//!     None
//! ).await?;
//!
//! // Add split information
//! registry.add_dataset_split(
//!     &dataset.name,
//!     version.version,
//!     DatasetSplit::Train,
//!     "s3://bucket/datasets/imagenet-v1/train/",
//!     10000
//! ).await?;
//!
//! // Link dataset to model for reproducibility
//! registry.link_dataset_to_model(&dataset.name, version.version, "my-classifier", 1).await?;
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::info;

/// Dataset registry errors
#[derive(Debug, Error)]
pub enum DatasetRegistryError {
    #[error("Dataset not found: {0}")]
    DatasetNotFound(String),

    #[error("Version not found: {dataset}/{version}")]
    VersionNotFound { dataset: String, version: u32 },

    #[error("Dataset already exists: {0}")]
    DatasetAlreadyExists(String),

    #[error("Split not found: {split:?}")]
    SplitNotFound { split: DatasetSplit },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub type DatasetRegistryResult<T> = Result<T, DatasetRegistryError>;

/// Dataset split types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum DatasetSplit {
    /// Training set
    Train,
    /// Validation set
    Validation,
    /// Test set
    Test,
    /// Full dataset (unsplit)
    Full,
}

/// Registered dataset information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredDataset {
    /// Dataset name (unique identifier)
    pub name: String,
    /// Dataset description
    pub description: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
    /// Latest version number
    pub latest_version: u32,
    /// Tags for categorization
    pub tags: HashMap<String, String>,
}

/// Dataset version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetVersion {
    /// Dataset name
    pub dataset_name: String,
    /// Version number (monotonically increasing)
    pub version: u32,
    /// Source URI (e.g., s3://bucket/path/dataset/)
    pub source: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
    /// Dataset metadata
    pub metadata: DatasetMetadata,
    /// Dataset splits
    pub splits: HashMap<DatasetSplit, SplitInfo>,
    /// Models trained on this dataset version
    pub trained_models: Vec<ModelReference>,
    /// Tags for this version
    pub tags: HashMap<String, String>,
}

/// Dataset metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetMetadata {
    /// Total size in bytes
    pub total_size: u64,
    /// Number of samples/records
    pub num_samples: Option<u64>,
    /// Data format (CSV, Parquet, Images, etc.)
    pub format: Option<String>,
    /// Schema information
    pub schema: Option<String>,
    /// Statistical summary
    pub statistics: HashMap<String, serde_json::Value>,
    /// Provenance information
    pub provenance: DatasetProvenance,
    /// Custom metadata
    pub custom: HashMap<String, String>,
}

/// Dataset provenance information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DatasetProvenance {
    /// Original data source
    pub source: Option<String>,
    /// Collection method/process
    pub collection_method: Option<String>,
    /// Transformations applied
    pub transformations: Vec<String>,
    /// Parent dataset versions (for derived datasets)
    pub parent_versions: Vec<String>,
    /// Creation script or process
    pub creation_script: Option<String>,
    /// Creator information
    pub created_by: Option<String>,
    /// License information
    pub license: Option<String>,
    /// Citation information
    pub citation: Option<String>,
}

/// Split information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitInfo {
    /// Split type
    pub split: DatasetSplit,
    /// URI to split data
    pub uri: String,
    /// Number of samples in this split
    pub num_samples: u64,
    /// Split percentage (0.0-1.0)
    pub percentage: Option<f64>,
    /// Checksum for integrity
    pub checksum: Option<String>,
}

/// Reference to a model trained on this dataset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelReference {
    /// Model name
    pub model_name: String,
    /// Model version
    pub model_version: u32,
    /// Timestamp when link was created
    pub linked_at: DateTime<Utc>,
    /// Which split was used (Train, Validation, etc.)
    pub split_used: DatasetSplit,
}

/// Dataset registry implementation
pub struct DatasetRegistry {
    /// Storage root directory
    root: PathBuf,
    /// In-memory cache of registered datasets
    datasets: Arc<RwLock<HashMap<String, RegisteredDataset>>>,
    /// In-memory cache of dataset versions (dataset_name -> version -> DatasetVersion)
    versions: Arc<RwLock<HashMap<String, HashMap<u32, DatasetVersion>>>>,
}

impl DatasetRegistry {
    /// Create a new dataset registry
    pub async fn new(root: PathBuf) -> DatasetRegistryResult<Self> {
        fs::create_dir_all(&root).await?;

        let registry = Self {
            root: root.clone(),
            datasets: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load existing datasets and versions from disk
        registry.load_from_disk().await?;

        Ok(registry)
    }

    /// Load all datasets and versions from disk
    async fn load_from_disk(&self) -> DatasetRegistryResult<()> {
        let datasets_path = self.root.join("datasets");

        if !datasets_path.exists() {
            fs::create_dir_all(&datasets_path).await?;
            return Ok(());
        }

        let mut dir = fs::read_dir(&datasets_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("json")
            {
                if let Ok(content) = fs::read_to_string(entry.path()).await {
                    if let Ok(dataset) = serde_json::from_str::<RegisteredDataset>(&content) {
                        self.datasets
                            .write()
                            .await
                            .insert(dataset.name.clone(), dataset.clone());

                        // Load versions for this dataset
                        self.load_versions(&dataset.name).await?;
                    }
                }
            }
        }

        info!(
            "Loaded {} datasets from registry",
            self.datasets.read().await.len()
        );
        Ok(())
    }

    /// Load all versions for a dataset
    async fn load_versions(&self, dataset_name: &str) -> DatasetRegistryResult<()> {
        let versions_path = self.root.join("versions").join(dataset_name);

        if !versions_path.exists() {
            return Ok(());
        }

        let mut versions_map = HashMap::new();
        let mut dir = fs::read_dir(&versions_path).await?;

        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("json")
            {
                if let Ok(content) = fs::read_to_string(entry.path()).await {
                    if let Ok(version) = serde_json::from_str::<DatasetVersion>(&content) {
                        versions_map.insert(version.version, version);
                    }
                }
            }
        }

        if !versions_map.is_empty() {
            self.versions
                .write()
                .await
                .insert(dataset_name.to_string(), versions_map);
        }

        Ok(())
    }

    /// Register a new dataset
    pub async fn register_dataset(
        &self,
        name: &str,
        description: &str,
    ) -> DatasetRegistryResult<RegisteredDataset> {
        let mut datasets = self.datasets.write().await;

        if datasets.contains_key(name) {
            return Err(DatasetRegistryError::DatasetAlreadyExists(name.to_string()));
        }

        let dataset = RegisteredDataset {
            name: name.to_string(),
            description: description.to_string(),
            created_at: Utc::now(),
            last_updated: Utc::now(),
            latest_version: 0,
            tags: HashMap::new(),
        };

        // Save to disk
        self.save_dataset(&dataset).await?;

        datasets.insert(name.to_string(), dataset.clone());
        info!("Registered new dataset: {}", name);

        Ok(dataset)
    }

    /// Create a new dataset version
    pub async fn create_dataset_version(
        &self,
        dataset_name: &str,
        source: &str,
        metadata: Option<DatasetMetadata>,
    ) -> DatasetRegistryResult<DatasetVersion> {
        // Get or create the dataset
        let mut datasets = self.datasets.write().await;
        let dataset = datasets
            .get_mut(dataset_name)
            .ok_or_else(|| DatasetRegistryError::DatasetNotFound(dataset_name.to_string()))?;

        // Increment version number
        dataset.latest_version += 1;
        dataset.last_updated = Utc::now();

        let version_num = dataset.latest_version;

        // Create new version
        let version = DatasetVersion {
            dataset_name: dataset_name.to_string(),
            version: version_num,
            source: source.to_string(),
            created_at: Utc::now(),
            last_updated: Utc::now(),
            metadata: metadata.unwrap_or_default(),
            splits: HashMap::new(),
            trained_models: Vec::new(),
            tags: HashMap::new(),
        };

        // Save dataset with updated latest_version
        self.save_dataset(dataset).await?;

        // Save version
        self.save_version(&version).await?;

        // Update in-memory cache
        let mut versions = self.versions.write().await;
        versions
            .entry(dataset_name.to_string())
            .or_insert_with(HashMap::new)
            .insert(version_num, version.clone());

        info!("Created dataset version: {}/{}", dataset_name, version_num);

        Ok(version)
    }

    /// Add a split to a dataset version
    pub async fn add_dataset_split(
        &self,
        dataset_name: &str,
        version_num: u32,
        split: DatasetSplit,
        uri: &str,
        num_samples: u64,
    ) -> DatasetRegistryResult<()> {
        let mut versions = self.versions.write().await;

        let dataset_versions = versions
            .get_mut(dataset_name)
            .ok_or_else(|| DatasetRegistryError::DatasetNotFound(dataset_name.to_string()))?;

        let version = dataset_versions.get_mut(&version_num).ok_or_else(|| {
            DatasetRegistryError::VersionNotFound {
                dataset: dataset_name.to_string(),
                version: version_num,
            }
        })?;

        let split_info = SplitInfo {
            split,
            uri: uri.to_string(),
            num_samples,
            percentage: None,
            checksum: None,
        };

        version.splits.insert(split, split_info);
        version.last_updated = Utc::now();

        // Save to disk
        self.save_version(version).await?;

        info!(
            "Added split {:?} to dataset {}/{}",
            split, dataset_name, version_num
        );

        Ok(())
    }

    /// Link a dataset version to a model (for reproducibility)
    pub async fn link_dataset_to_model(
        &self,
        dataset_name: &str,
        dataset_version: u32,
        model_name: &str,
        model_version: u32,
    ) -> DatasetRegistryResult<()> {
        let mut versions = self.versions.write().await;

        let dataset_versions = versions
            .get_mut(dataset_name)
            .ok_or_else(|| DatasetRegistryError::DatasetNotFound(dataset_name.to_string()))?;

        let version = dataset_versions.get_mut(&dataset_version).ok_or_else(|| {
            DatasetRegistryError::VersionNotFound {
                dataset: dataset_name.to_string(),
                version: dataset_version,
            }
        })?;

        let model_ref = ModelReference {
            model_name: model_name.to_string(),
            model_version,
            linked_at: Utc::now(),
            split_used: DatasetSplit::Train, // Default to Train
        };

        version.trained_models.push(model_ref);
        version.last_updated = Utc::now();

        // Save to disk
        self.save_version(version).await?;

        info!(
            "Linked dataset {}/{} to model {}/{}",
            dataset_name, dataset_version, model_name, model_version
        );

        Ok(())
    }

    /// Get a specific dataset version
    pub async fn get_dataset_version(
        &self,
        dataset_name: &str,
        version_num: u32,
    ) -> DatasetRegistryResult<Option<DatasetVersion>> {
        let versions = self.versions.read().await;

        Ok(versions
            .get(dataset_name)
            .and_then(|v| v.get(&version_num))
            .cloned())
    }

    /// Get latest version of a dataset
    pub async fn get_latest_version(
        &self,
        dataset_name: &str,
    ) -> DatasetRegistryResult<Option<DatasetVersion>> {
        let versions = self.versions.read().await;

        let dataset_versions = match versions.get(dataset_name) {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut all_versions: Vec<_> = dataset_versions.values().collect();
        all_versions.sort_by_key(|v| v.version);

        Ok(all_versions.last().map(|v| (*v).clone()))
    }

    /// List all versions of a dataset
    pub async fn list_dataset_versions(
        &self,
        dataset_name: &str,
    ) -> DatasetRegistryResult<Vec<DatasetVersion>> {
        let versions = self.versions.read().await;

        let dataset_versions = versions
            .get(dataset_name)
            .map(|v| v.values().cloned().collect())
            .unwrap_or_default();

        Ok(dataset_versions)
    }

    /// Get registered dataset
    pub async fn get_dataset(
        &self,
        name: &str,
    ) -> DatasetRegistryResult<Option<RegisteredDataset>> {
        Ok(self.datasets.read().await.get(name).cloned())
    }

    /// List all registered datasets
    pub async fn list_datasets(&self) -> Vec<RegisteredDataset> {
        self.datasets.read().await.values().cloned().collect()
    }

    /// Delete a dataset and all its versions
    pub async fn delete_dataset(&self, dataset_name: &str) -> DatasetRegistryResult<()> {
        // Remove from memory
        self.datasets.write().await.remove(dataset_name);
        self.versions.write().await.remove(dataset_name);

        // Remove from disk
        let dataset_path = self
            .root
            .join("datasets")
            .join(format!("{}.json", dataset_name));
        if dataset_path.exists() {
            fs::remove_file(dataset_path).await?;
        }

        let versions_dir = self.root.join("versions").join(dataset_name);
        if versions_dir.exists() {
            fs::remove_dir_all(versions_dir).await?;
        }

        info!("Deleted dataset: {}", dataset_name);
        Ok(())
    }

    /// Save dataset to disk
    async fn save_dataset(&self, dataset: &RegisteredDataset) -> DatasetRegistryResult<()> {
        let datasets_dir = self.root.join("datasets");
        fs::create_dir_all(&datasets_dir).await?;

        let path = datasets_dir.join(format!("{}.json", dataset.name));
        let content = serde_json::to_string_pretty(dataset)?;
        fs::write(path, content).await?;

        Ok(())
    }

    /// Save dataset version to disk
    async fn save_version(&self, version: &DatasetVersion) -> DatasetRegistryResult<()> {
        let versions_dir = self.root.join("versions").join(&version.dataset_name);
        fs::create_dir_all(&versions_dir).await?;

        let path = versions_dir.join(format!("v{}.json", version.version));
        let content = serde_json::to_string_pretty(version)?;
        fs::write(path, content).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    async fn create_test_registry() -> DatasetRegistry {
        let temp_dir =
            env::temp_dir().join(format!("test_dataset_registry_{}", uuid::Uuid::new_v4()));
        DatasetRegistry::new(temp_dir)
            .await
            .expect("Failed to create registry")
    }

    #[tokio::test]
    async fn test_register_dataset() {
        let registry = create_test_registry().await;

        let dataset = registry
            .register_dataset("test-dataset", "A test dataset")
            .await
            .expect("Failed to register dataset");

        assert_eq!(dataset.name, "test-dataset");
        assert_eq!(dataset.description, "A test dataset");
        assert_eq!(dataset.latest_version, 0);
    }

    #[tokio::test]
    async fn test_create_dataset_version() {
        let registry = create_test_registry().await;

        registry
            .register_dataset("test-dataset", "A test dataset")
            .await
            .expect("Failed to register dataset");

        let version = registry
            .create_dataset_version("test-dataset", "s3://bucket/dataset/", None)
            .await
            .expect("Failed to create version");

        assert_eq!(version.version, 1);
        assert_eq!(version.dataset_name, "test-dataset");
    }

    #[tokio::test]
    async fn test_add_dataset_split() {
        let registry = create_test_registry().await;

        registry
            .register_dataset("test-dataset", "A test dataset")
            .await
            .expect("Failed to register dataset");

        let version = registry
            .create_dataset_version("test-dataset", "s3://bucket/dataset/", None)
            .await
            .expect("Failed to create version");

        registry
            .add_dataset_split(
                "test-dataset",
                version.version,
                DatasetSplit::Train,
                "s3://bucket/dataset/train/",
                10000,
            )
            .await
            .expect("Failed to add split");

        let updated = registry
            .get_dataset_version("test-dataset", version.version)
            .await
            .expect("Failed to get version")
            .expect("Version not found");

        assert_eq!(updated.splits.len(), 1);
        assert!(updated.splits.contains_key(&DatasetSplit::Train));
    }

    #[tokio::test]
    async fn test_link_dataset_to_model() {
        let registry = create_test_registry().await;

        registry
            .register_dataset("test-dataset", "A test dataset")
            .await
            .expect("Failed to register dataset");

        let version = registry
            .create_dataset_version("test-dataset", "s3://bucket/dataset/", None)
            .await
            .expect("Failed to create version");

        registry
            .link_dataset_to_model("test-dataset", version.version, "my-model", 1)
            .await
            .expect("Failed to link dataset");

        let updated = registry
            .get_dataset_version("test-dataset", version.version)
            .await
            .expect("Failed to get version")
            .expect("Version not found");

        assert_eq!(updated.trained_models.len(), 1);
        assert_eq!(updated.trained_models[0].model_name, "my-model");
        assert_eq!(updated.trained_models[0].model_version, 1);
    }

    #[tokio::test]
    async fn test_get_latest_version() {
        let registry = create_test_registry().await;

        registry
            .register_dataset("test-dataset", "A test dataset")
            .await
            .expect("Failed to register dataset");

        let _v1 = registry
            .create_dataset_version("test-dataset", "s3://bucket/dataset_v1/", None)
            .await
            .expect("Failed to create v1");
        let v2 = registry
            .create_dataset_version("test-dataset", "s3://bucket/dataset_v2/", None)
            .await
            .expect("Failed to create v2");

        let latest = registry
            .get_latest_version("test-dataset")
            .await
            .expect("Failed to get latest version");

        let latest_version = latest.expect("Latest version should be Some");
        assert_eq!(latest_version.version, v2.version);
    }

    #[tokio::test]
    async fn test_list_datasets() {
        let registry = create_test_registry().await;

        registry
            .register_dataset("dataset1", "First dataset")
            .await
            .expect("Failed to register dataset1");
        registry
            .register_dataset("dataset2", "Second dataset")
            .await
            .expect("Failed to register dataset2");

        let datasets = registry.list_datasets().await;
        assert_eq!(datasets.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_dataset() {
        let registry = create_test_registry().await;

        registry
            .register_dataset("test-dataset", "A test dataset")
            .await
            .expect("Failed to register dataset");
        registry
            .create_dataset_version("test-dataset", "s3://bucket/dataset/", None)
            .await
            .expect("Failed to create version");

        registry
            .delete_dataset("test-dataset")
            .await
            .expect("Failed to delete dataset");

        let dataset = registry
            .get_dataset("test-dataset")
            .await
            .expect("Failed to get dataset");
        assert!(dataset.is_none());
    }

    #[tokio::test]
    async fn test_persistence() {
        let temp_dir =
            env::temp_dir().join(format!("test_dataset_persist_{}", uuid::Uuid::new_v4()));

        // Create registry and add data
        {
            let registry = DatasetRegistry::new(temp_dir.clone())
                .await
                .expect("Failed to create registry");

            registry
                .register_dataset("persist-dataset", "A persistent dataset")
                .await
                .expect("Failed to register dataset");
            registry
                .create_dataset_version("persist-dataset", "s3://bucket/dataset/", None)
                .await
                .expect("Failed to create version");
        }

        // Reload from disk
        {
            let registry = DatasetRegistry::new(temp_dir.clone())
                .await
                .expect("Failed to reload registry");

            let dataset = registry
                .get_dataset("persist-dataset")
                .await
                .expect("Failed to get dataset")
                .expect("Dataset not found");

            assert_eq!(dataset.name, "persist-dataset");
            assert_eq!(dataset.latest_version, 1);

            let versions = registry
                .list_dataset_versions("persist-dataset")
                .await
                .expect("Failed to list versions");
            assert_eq!(versions.len(), 1);
        }

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir).await;
    }
}
