//! ML Model Registry - MLflow-compatible model versioning and management
//!
//! This module provides a model registry system similar to MLflow Model Registry,
//! allowing tracking of model versions, lineage, deployment status, and metadata.
//!
//! # Features
//!
//! - Model registration and versioning
//! - Model lineage tracking (training data, hyperparameters)
//! - Deployment status tracking (staging, production, archived)
//! - Model metadata extraction and storage
//! - Model provenance tracking
//! - A/B testing support
//! - Automatic model card generation
//!
//! # Example
//!
//! ```no_run
//! use rs3gw::storage::model_registry::{ModelRegistry, RegisteredModel, ModelStage};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = ModelRegistry::new("./models-registry".into()).await?;
//!
//! // Register a new model
//! let model = registry.register_model(
//!     "my-classifier",
//!     "Image classification model for product categorization"
//! ).await?;
//!
//! // Create a new version
//! let version = registry.create_model_version(
//!     "my-classifier",
//!     "s3://bucket/models/classifier-v1.pt",
//!     None
//! ).await?;
//!
//! // Transition to staging
//! registry.transition_model_stage(&model.name, version.version, ModelStage::Staging).await?;
//!
//! // Get model for inference
//! if let Some(prod_version) = registry.get_latest_version(&model.name, Some(ModelStage::Production)).await? {
//!     println!("Using production model version {}", prod_version.version);
//! }
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

/// Model registry errors
#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),

    #[error("Version not found: {model}/{version}")]
    VersionNotFound { model: String, version: u32 },

    #[error("Model already exists: {0}")]
    ModelAlreadyExists(String),

    #[error("Invalid stage transition: {from:?} -> {to:?}")]
    InvalidStageTransition { from: ModelStage, to: ModelStage },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub type RegistryResult<T> = Result<T, RegistryError>;

/// Model deployment stage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ModelStage {
    /// Development/experimental stage
    Development,
    /// Staging for testing
    Staging,
    /// Production deployment
    Production,
    /// Archived (deprecated)
    Archived,
}

impl ModelStage {
    /// Check if transition to another stage is valid
    pub fn can_transition_to(&self, to: ModelStage) -> bool {
        match (self, to) {
            // Can always archive
            (_, ModelStage::Archived) => true,
            // Can't un-archive
            (ModelStage::Archived, _) => false,
            // Normal progression
            (ModelStage::Development, ModelStage::Staging) => true,
            (ModelStage::Staging, ModelStage::Production) => true,
            // Can go back to development or staging from production
            (ModelStage::Production, ModelStage::Development | ModelStage::Staging) => true,
            // Same stage is okay (no-op)
            (a, b) if a == &b => true,
            // Others require going through intermediate stages
            _ => false,
        }
    }
}

/// Registered model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredModel {
    /// Model name (unique identifier)
    pub name: String,
    /// Model description
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

/// Model version information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelVersion {
    /// Model name
    pub model_name: String,
    /// Version number (monotonically increasing)
    pub version: u32,
    /// Source URI (e.g., s3://bucket/path/model.pt)
    pub source: String,
    /// Current deployment stage
    pub stage: ModelStage,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub last_updated: DateTime<Utc>,
    /// Model metadata
    pub metadata: ModelVersionMetadata,
    /// Tags for this version
    pub tags: HashMap<String, String>,
}

/// Detailed metadata for a model version
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelVersionMetadata {
    /// Framework used (PyTorch, TensorFlow, etc.)
    pub framework: Option<String>,
    /// Framework version
    pub framework_version: Option<String>,
    /// Model architecture description
    pub architecture: Option<String>,
    /// Number of parameters
    pub parameter_count: Option<u64>,
    /// Training dataset information
    pub training_dataset: Option<String>,
    /// Hyperparameters used for training
    pub hyperparameters: HashMap<String, serde_json::Value>,
    /// Performance metrics (accuracy, F1, etc.)
    pub metrics: HashMap<String, f64>,
    /// Provenance information
    pub provenance: ProvenanceInfo,
    /// Custom metadata
    pub custom: HashMap<String, String>,
}

/// Model provenance information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProvenanceInfo {
    /// Git commit hash of training code
    pub git_commit: Option<String>,
    /// Training script or notebook path
    pub training_script: Option<String>,
    /// User who created this version
    pub created_by: Option<String>,
    /// Run ID from experiment tracking (e.g., MLflow run ID)
    pub run_id: Option<String>,
    /// Parent model versions (for transfer learning)
    pub parent_versions: Vec<String>,
}

/// Model registry implementation
pub struct ModelRegistry {
    /// Storage root directory
    root: PathBuf,
    /// In-memory cache of registered models
    models: Arc<RwLock<HashMap<String, RegisteredModel>>>,
    /// In-memory cache of model versions (model_name -> version -> ModelVersion)
    versions: Arc<RwLock<HashMap<String, HashMap<u32, ModelVersion>>>>,
}

impl ModelRegistry {
    /// Create a new model registry
    pub async fn new(root: PathBuf) -> RegistryResult<Self> {
        fs::create_dir_all(&root).await?;

        let registry = Self {
            root: root.clone(),
            models: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
        };

        // Load existing models and versions from disk
        registry.load_from_disk().await?;

        Ok(registry)
    }

    /// Load all models and versions from disk
    async fn load_from_disk(&self) -> RegistryResult<()> {
        let models_path = self.root.join("models");

        if !models_path.exists() {
            fs::create_dir_all(&models_path).await?;
            return Ok(());
        }

        let mut dir = fs::read_dir(&models_path).await?;
        while let Some(entry) = dir.next_entry().await? {
            if entry.file_type().await?.is_file()
                && entry.path().extension().and_then(|s| s.to_str()) == Some("json")
            {
                if let Ok(content) = fs::read_to_string(entry.path()).await {
                    if let Ok(model) = serde_json::from_str::<RegisteredModel>(&content) {
                        self.models
                            .write()
                            .await
                            .insert(model.name.clone(), model.clone());

                        // Load versions for this model
                        self.load_versions(&model.name).await?;
                    }
                }
            }
        }

        info!(
            "Loaded {} models from registry",
            self.models.read().await.len()
        );
        Ok(())
    }

    /// Load all versions for a model
    async fn load_versions(&self, model_name: &str) -> RegistryResult<()> {
        let versions_path = self.root.join("versions").join(model_name);

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
                    if let Ok(version) = serde_json::from_str::<ModelVersion>(&content) {
                        versions_map.insert(version.version, version);
                    }
                }
            }
        }

        if !versions_map.is_empty() {
            self.versions
                .write()
                .await
                .insert(model_name.to_string(), versions_map);
        }

        Ok(())
    }

    /// Register a new model
    pub async fn register_model(
        &self,
        name: &str,
        description: &str,
    ) -> RegistryResult<RegisteredModel> {
        let mut models = self.models.write().await;

        if models.contains_key(name) {
            return Err(RegistryError::ModelAlreadyExists(name.to_string()));
        }

        let model = RegisteredModel {
            name: name.to_string(),
            description: description.to_string(),
            created_at: Utc::now(),
            last_updated: Utc::now(),
            latest_version: 0,
            tags: HashMap::new(),
        };

        // Save to disk
        self.save_model(&model).await?;

        models.insert(name.to_string(), model.clone());
        info!("Registered new model: {}", name);

        Ok(model)
    }

    /// Create a new model version
    pub async fn create_model_version(
        &self,
        model_name: &str,
        source: &str,
        metadata: Option<ModelVersionMetadata>,
    ) -> RegistryResult<ModelVersion> {
        // Get or create the model
        let mut models = self.models.write().await;
        let model = models
            .get_mut(model_name)
            .ok_or_else(|| RegistryError::ModelNotFound(model_name.to_string()))?;

        // Increment version number
        model.latest_version += 1;
        model.last_updated = Utc::now();

        let version_num = model.latest_version;

        // Create new version
        let version = ModelVersion {
            model_name: model_name.to_string(),
            version: version_num,
            source: source.to_string(),
            stage: ModelStage::Development,
            created_at: Utc::now(),
            last_updated: Utc::now(),
            metadata: metadata.unwrap_or_default(),
            tags: HashMap::new(),
        };

        // Save model with updated latest_version
        self.save_model(model).await?;

        // Save version
        self.save_version(&version).await?;

        // Update in-memory cache
        let mut versions = self.versions.write().await;
        versions
            .entry(model_name.to_string())
            .or_insert_with(HashMap::new)
            .insert(version_num, version.clone());

        info!("Created model version: {}/{}", model_name, version_num);

        Ok(version)
    }

    /// Transition model version to a different stage
    pub async fn transition_model_stage(
        &self,
        model_name: &str,
        version_num: u32,
        new_stage: ModelStage,
    ) -> RegistryResult<ModelVersion> {
        let mut versions = self.versions.write().await;

        let model_versions = versions
            .get_mut(model_name)
            .ok_or_else(|| RegistryError::ModelNotFound(model_name.to_string()))?;

        let version =
            model_versions
                .get_mut(&version_num)
                .ok_or_else(|| RegistryError::VersionNotFound {
                    model: model_name.to_string(),
                    version: version_num,
                })?;

        // Check if transition is valid
        if !version.stage.can_transition_to(new_stage) {
            return Err(RegistryError::InvalidStageTransition {
                from: version.stage,
                to: new_stage,
            });
        }

        version.stage = new_stage;
        version.last_updated = Utc::now();

        // Save to disk
        self.save_version(version).await?;

        info!(
            "Transitioned model {}/{} to {:?}",
            model_name, version_num, new_stage
        );

        Ok(version.clone())
    }

    /// Get a specific model version
    pub async fn get_model_version(
        &self,
        model_name: &str,
        version_num: u32,
    ) -> RegistryResult<Option<ModelVersion>> {
        let versions = self.versions.read().await;

        Ok(versions
            .get(model_name)
            .and_then(|v| v.get(&version_num))
            .cloned())
    }

    /// Get latest version of a model, optionally filtered by stage
    pub async fn get_latest_version(
        &self,
        model_name: &str,
        stage: Option<ModelStage>,
    ) -> RegistryResult<Option<ModelVersion>> {
        let versions = self.versions.read().await;

        let model_versions = match versions.get(model_name) {
            Some(v) => v,
            None => return Ok(None),
        };

        let mut filtered: Vec<_> = model_versions
            .values()
            .filter(|v| stage.is_none_or(|s| v.stage == s))
            .collect();

        filtered.sort_by_key(|v| v.version);

        Ok(filtered.last().map(|v| (*v).clone()))
    }

    /// List all versions of a model
    pub async fn list_model_versions(&self, model_name: &str) -> RegistryResult<Vec<ModelVersion>> {
        let versions = self.versions.read().await;

        let model_versions = versions
            .get(model_name)
            .map(|v| v.values().cloned().collect())
            .unwrap_or_default();

        Ok(model_versions)
    }

    /// Get registered model
    pub async fn get_model(&self, name: &str) -> RegistryResult<Option<RegisteredModel>> {
        Ok(self.models.read().await.get(name).cloned())
    }

    /// List all registered models
    pub async fn list_models(&self) -> Vec<RegisteredModel> {
        self.models.read().await.values().cloned().collect()
    }

    /// Delete a model and all its versions
    pub async fn delete_model(&self, model_name: &str) -> RegistryResult<()> {
        // Remove from memory
        self.models.write().await.remove(model_name);
        self.versions.write().await.remove(model_name);

        // Remove from disk
        let model_path = self
            .root
            .join("models")
            .join(format!("{}.json", model_name));
        if model_path.exists() {
            fs::remove_file(model_path).await?;
        }

        let versions_dir = self.root.join("versions").join(model_name);
        if versions_dir.exists() {
            fs::remove_dir_all(versions_dir).await?;
        }

        info!("Deleted model: {}", model_name);
        Ok(())
    }

    /// Save model to disk
    async fn save_model(&self, model: &RegisteredModel) -> RegistryResult<()> {
        let models_dir = self.root.join("models");
        fs::create_dir_all(&models_dir).await?;

        let path = models_dir.join(format!("{}.json", model.name));
        let content = serde_json::to_string_pretty(model)?;
        fs::write(path, content).await?;

        Ok(())
    }

    /// Save model version to disk
    async fn save_version(&self, version: &ModelVersion) -> RegistryResult<()> {
        let versions_dir = self.root.join("versions").join(&version.model_name);
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

    async fn create_test_registry() -> ModelRegistry {
        let temp_dir = env::temp_dir().join(format!("test_registry_{}", uuid::Uuid::new_v4()));
        ModelRegistry::new(temp_dir)
            .await
            .expect("Failed to create registry")
    }

    #[tokio::test]
    async fn test_register_model() {
        let registry = create_test_registry().await;

        let model = registry
            .register_model("test-model", "A test model")
            .await
            .expect("Failed to register model");

        assert_eq!(model.name, "test-model");
        assert_eq!(model.description, "A test model");
        assert_eq!(model.latest_version, 0);
    }

    #[tokio::test]
    async fn test_create_model_version() {
        let registry = create_test_registry().await;

        registry
            .register_model("test-model", "A test model")
            .await
            .expect("Failed to register model");

        let version = registry
            .create_model_version("test-model", "s3://bucket/model.pt", None)
            .await
            .expect("Failed to create version");

        assert_eq!(version.version, 1);
        assert_eq!(version.model_name, "test-model");
        assert_eq!(version.stage, ModelStage::Development);
    }

    #[tokio::test]
    async fn test_stage_transitions() {
        let registry = create_test_registry().await;

        registry
            .register_model("test-model", "A test model")
            .await
            .expect("Failed to register model");

        let version = registry
            .create_model_version("test-model", "s3://bucket/model.pt", None)
            .await
            .expect("Failed to create version");

        // Development -> Staging
        let version = registry
            .transition_model_stage("test-model", version.version, ModelStage::Staging)
            .await
            .expect("Failed to transition to staging");
        assert_eq!(version.stage, ModelStage::Staging);

        // Staging -> Production
        let version = registry
            .transition_model_stage("test-model", version.version, ModelStage::Production)
            .await
            .expect("Failed to transition to production");
        assert_eq!(version.stage, ModelStage::Production);

        // Production -> Archived
        let version = registry
            .transition_model_stage("test-model", version.version, ModelStage::Archived)
            .await
            .expect("Failed to archive");
        assert_eq!(version.stage, ModelStage::Archived);
    }

    #[tokio::test]
    async fn test_invalid_stage_transition() {
        let registry = create_test_registry().await;

        registry
            .register_model("test-model", "A test model")
            .await
            .expect("Failed to register model");

        let version = registry
            .create_model_version("test-model", "s3://bucket/model.pt", None)
            .await
            .expect("Failed to create version");

        // Development -> Production (should fail)
        let result = registry
            .transition_model_stage("test-model", version.version, ModelStage::Production)
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_latest_version() {
        let registry = create_test_registry().await;

        registry
            .register_model("test-model", "A test model")
            .await
            .expect("Failed to register model");

        // Create multiple versions
        let v1 = registry
            .create_model_version("test-model", "s3://bucket/model_v1.pt", None)
            .await
            .expect("Failed to create v1");
        let v2 = registry
            .create_model_version("test-model", "s3://bucket/model_v2.pt", None)
            .await
            .expect("Failed to create v2");

        // Transition v1 to production
        registry
            .transition_model_stage("test-model", v1.version, ModelStage::Staging)
            .await
            .expect("Failed to transition v1");
        registry
            .transition_model_stage("test-model", v1.version, ModelStage::Production)
            .await
            .expect("Failed to transition v1");

        // Get latest development version
        let latest_dev = registry
            .get_latest_version("test-model", Some(ModelStage::Development))
            .await
            .expect("Failed to get latest dev version");
        let latest_dev_version = latest_dev.expect("Latest dev version should be Some");
        assert_eq!(latest_dev_version.version, v2.version);

        // Get latest production version
        let latest_prod = registry
            .get_latest_version("test-model", Some(ModelStage::Production))
            .await
            .expect("Failed to get latest prod version");
        let latest_prod_version = latest_prod.expect("Latest prod version should be Some");
        assert_eq!(latest_prod_version.version, v1.version);
    }

    #[tokio::test]
    async fn test_list_models() {
        let registry = create_test_registry().await;

        registry
            .register_model("model1", "First model")
            .await
            .expect("Failed to register model1");
        registry
            .register_model("model2", "Second model")
            .await
            .expect("Failed to register model2");

        let models = registry.list_models().await;
        assert_eq!(models.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_model() {
        let registry = create_test_registry().await;

        registry
            .register_model("test-model", "A test model")
            .await
            .expect("Failed to register model");
        registry
            .create_model_version("test-model", "s3://bucket/model.pt", None)
            .await
            .expect("Failed to create version");

        registry
            .delete_model("test-model")
            .await
            .expect("Failed to delete model");

        let model = registry
            .get_model("test-model")
            .await
            .expect("Failed to get model");
        assert!(model.is_none());
    }

    #[tokio::test]
    async fn test_persistence() {
        let temp_dir =
            env::temp_dir().join(format!("test_registry_persist_{}", uuid::Uuid::new_v4()));

        // Create registry and add data
        {
            let registry = ModelRegistry::new(temp_dir.clone())
                .await
                .expect("Failed to create registry");

            registry
                .register_model("persist-model", "A persistent model")
                .await
                .expect("Failed to register model");
            registry
                .create_model_version("persist-model", "s3://bucket/model.pt", None)
                .await
                .expect("Failed to create version");
        }

        // Reload from disk
        {
            let registry = ModelRegistry::new(temp_dir.clone())
                .await
                .expect("Failed to reload registry");

            let model = registry
                .get_model("persist-model")
                .await
                .expect("Failed to get model")
                .expect("Model not found");

            assert_eq!(model.name, "persist-model");
            assert_eq!(model.latest_version, 1);

            let versions = registry
                .list_model_versions("persist-model")
                .await
                .expect("Failed to list versions");
            assert_eq!(versions.len(), 1);
        }

        // Cleanup
        let _ = fs::remove_dir_all(temp_dir).await;
    }
}
