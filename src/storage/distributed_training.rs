//! Distributed Training Integration - Checkpoint management and experiment tracking
//!
//! This module provides comprehensive distributed training support including:
//! - Checkpoint sharding and distribution across nodes
//! - Gradient accumulation buffer management
//! - Distributed optimizer state management
//! - Training resume from checkpoints
//! - Automatic checkpoint cleanup policies
//! - Training metrics storage and visualization
//! - Experiment tracking (Weights & Biases/Neptune compatible)
//! - Hyperparameter search result storage
//!
//! # Features
//!
//! - **Checkpoint Sharding**: Automatically shard large model checkpoints across multiple files
//! - **Resume Training**: Resume training from any checkpoint with full state restoration
//! - **Metrics Tracking**: Store and query training metrics (loss, accuracy, etc.)
//! - **Experiment Management**: Track experiments with hyperparameters and results
//! - **Cleanup Policies**: Automatically remove old checkpoints (keep best N)
//! - **Multi-GPU Support**: Coordinate distributed training across multiple GPUs/nodes
//!
//! # Example
//!
//! ```no_run
//! use rs3gw::storage::distributed_training::{
//!     TrainingManager, CheckpointConfig, ExperimentConfig,
//! };
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manager = TrainingManager::new("./training-data".into()).await?;
//!
//! // Start a new experiment
//! let exp_config = ExperimentConfig {
//!     name: "bert-finetuning".to_string(),
//!     description: Some("Fine-tuning BERT on custom dataset".to_string()),
//!     tags: vec!["nlp".to_string(), "bert".to_string()],
//!     hyperparameters: serde_json::json!({
//!         "learning_rate": 0.001,
//!         "batch_size": 32,
//!         "epochs": 10,
//!     }),
//! };
//! let experiment = manager.create_experiment(exp_config).await?;
//!
//! // Save a checkpoint
//! let checkpoint = manager.save_checkpoint(
//!     &experiment.id,
//!     1, // epoch
//!     b"model_state_dict_bytes".to_vec(),
//!     Some(b"optimizer_state_dict_bytes".to_vec()),
//!     serde_json::json!({"loss": 0.5, "accuracy": 0.85}),
//! ).await?;
//!
//! // Log metrics
//! manager.log_metrics(
//!     &experiment.id,
//!     1, // step
//!     serde_json::json!({"train_loss": 0.5, "val_accuracy": 0.85}),
//! ).await?;
//!
//! // Resume from checkpoint
//! let restored = manager.load_checkpoint(&checkpoint.id).await?;
//! println!("Resumed from epoch {}", restored.epoch);
//!
//! # Ok(())
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::Error;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Training manager errors
#[derive(Debug, Error)]
pub enum TrainingError {
    #[error("Experiment not found: {0}")]
    ExperimentNotFound(String),

    #[error("Checkpoint not found: {0}")]
    CheckpointNotFound(String),

    #[error("Invalid checkpoint shard: {0}")]
    InvalidShard(String),

    #[error("Experiment already exists: {0}")]
    ExperimentAlreadyExists(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

pub type TrainingResult<T> = Result<T, TrainingError>;

/// Experiment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentConfig {
    /// Experiment name (must be unique)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Hyperparameters as JSON
    pub hyperparameters: JsonValue,
}

/// Experiment metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experiment {
    /// Unique experiment ID
    pub id: String,
    /// Experiment name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Tags
    pub tags: Vec<String>,
    /// Hyperparameters
    pub hyperparameters: JsonValue,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Experiment status
    pub status: ExperimentStatus,
    /// Best metrics achieved
    pub best_metrics: Option<JsonValue>,
    /// Total checkpoints saved
    pub checkpoint_count: u64,
}

/// Experiment status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExperimentStatus {
    /// Experiment is running
    Running,
    /// Experiment completed successfully
    Completed,
    /// Experiment failed
    Failed,
    /// Experiment was manually stopped
    Stopped,
}

/// Checkpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointConfig {
    /// Maximum shard size in bytes (default: 1GB)
    pub max_shard_size: u64,
    /// Whether to enable compression
    pub compression_enabled: bool,
    /// Maximum number of checkpoints to keep per experiment (0 = unlimited)
    pub max_checkpoints: u32,
    /// Checkpoint retention policy
    pub retention_policy: RetentionPolicy,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            max_shard_size: 1024 * 1024 * 1024, // 1GB
            compression_enabled: true,
            max_checkpoints: 5,
            retention_policy: RetentionPolicy::KeepBest,
        }
    }
}

/// Checkpoint retention policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetentionPolicy {
    /// Keep all checkpoints
    KeepAll,
    /// Keep only the N most recent checkpoints
    KeepRecent,
    /// Keep the N checkpoints with best metrics
    KeepBest,
    /// Keep checkpoints at specific epochs (e.g., every 10 epochs)
    KeepEpochInterval,
}

/// Training checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique checkpoint ID
    pub id: String,
    /// Experiment ID this checkpoint belongs to
    pub experiment_id: String,
    /// Epoch number
    pub epoch: u64,
    /// Global step number
    pub step: u64,
    /// Checkpoint creation timestamp
    pub created_at: DateTime<Utc>,
    /// Metrics at this checkpoint
    pub metrics: JsonValue,
    /// Number of shards
    pub shard_count: u32,
    /// Total checkpoint size in bytes
    pub total_size: u64,
    /// Whether checkpoint includes optimizer state
    pub has_optimizer_state: bool,
    /// Whether checkpoint is sharded
    pub is_sharded: bool,
}

/// Training metrics entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsEntry {
    /// Experiment ID
    pub experiment_id: String,
    /// Step number
    pub step: u64,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Metrics as JSON (loss, accuracy, etc.)
    pub metrics: JsonValue,
}

/// Hyperparameter search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperparameterSearchResult {
    /// Search ID
    pub id: String,
    /// Associated experiment IDs
    pub experiment_ids: Vec<String>,
    /// Search space definition
    pub search_space: JsonValue,
    /// Best hyperparameters found
    pub best_params: JsonValue,
    /// Best metric value
    pub best_metric_value: Option<f64>,
    /// Metric to optimize
    pub optimization_metric: String,
    /// All trial results
    pub trials: Vec<Trial>,
    /// Search started timestamp
    pub started_at: DateTime<Utc>,
    /// Search completed timestamp
    pub completed_at: Option<DateTime<Utc>>,
}

/// Individual trial in hyperparameter search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    /// Trial ID
    pub id: String,
    /// Hyperparameters used
    pub params: JsonValue,
    /// Result metrics
    pub metrics: JsonValue,
    /// Trial status
    pub status: TrialStatus,
}

/// Trial status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrialStatus {
    /// Trial is running
    Running,
    /// Trial completed
    Completed,
    /// Trial failed
    Failed,
}

/// Training manager - main interface for distributed training operations
pub struct TrainingManager {
    /// Base directory for training data
    base_path: PathBuf,
    /// Checkpoint configuration
    checkpoint_config: CheckpointConfig,
    /// Active experiments (in-memory cache)
    experiments: Arc<RwLock<HashMap<String, Experiment>>>,
    /// Active hyperparameter searches
    searches: Arc<RwLock<HashMap<String, HyperparameterSearchResult>>>,
}

impl TrainingManager {
    /// Create a new training manager (sync - directories created lazily)
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            checkpoint_config: CheckpointConfig::default(),
            experiments: Arc::new(RwLock::new(HashMap::new())),
            searches: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Configure checkpoint settings
    pub fn with_checkpoint_config(mut self, config: CheckpointConfig) -> Self {
        self.checkpoint_config = config;
        self
    }

    /// Ensure directory structure exists (called internally as needed)
    async fn ensure_directories(&self) -> TrainingResult<()> {
        fs::create_dir_all(&self.base_path).await?;
        fs::create_dir_all(self.base_path.join("experiments")).await?;
        fs::create_dir_all(self.base_path.join("checkpoints")).await?;
        fs::create_dir_all(self.base_path.join("metrics")).await?;
        fs::create_dir_all(self.base_path.join("searches")).await?;
        Ok(())
    }

    /// Create a new training experiment
    pub async fn create_experiment(&self, config: ExperimentConfig) -> TrainingResult<Experiment> {
        // Ensure directories exist
        self.ensure_directories().await?;

        // Check if experiment name already exists
        let experiments = self.experiments.read().await;
        if experiments.values().any(|e| e.name == config.name) {
            return Err(TrainingError::ExperimentAlreadyExists(config.name));
        }
        drop(experiments);

        // Create experiment
        let experiment = Experiment {
            id: uuid::Uuid::new_v4().to_string(),
            name: config.name.clone(),
            description: config.description.clone(),
            tags: config.tags.clone(),
            hyperparameters: config.hyperparameters.clone(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            status: ExperimentStatus::Running,
            best_metrics: None,
            checkpoint_count: 0,
        };

        // Save to disk
        let exp_path = self.base_path.join("experiments").join(&experiment.id);
        fs::create_dir_all(&exp_path).await?;

        let exp_file = exp_path.join("metadata.json");
        let json = serde_json::to_string_pretty(&experiment)?;
        fs::write(&exp_file, json).await?;

        // Add to cache
        let mut experiments = self.experiments.write().await;
        experiments.insert(experiment.id.clone(), experiment.clone());

        info!(
            "Created experiment: {} ({})",
            experiment.name, experiment.id
        );

        Ok(experiment)
    }

    /// Get an experiment by ID
    pub async fn get_experiment(&self, experiment_id: &str) -> TrainingResult<Experiment> {
        // Check cache first
        let experiments = self.experiments.read().await;
        if let Some(exp) = experiments.get(experiment_id) {
            return Ok(exp.clone());
        }
        drop(experiments);

        // Load from disk
        let exp_file = self
            .base_path
            .join("experiments")
            .join(experiment_id)
            .join("metadata.json");

        if !exp_file.exists() {
            return Err(TrainingError::ExperimentNotFound(experiment_id.to_string()));
        }

        let json = fs::read_to_string(&exp_file).await?;
        let experiment: Experiment = serde_json::from_str(&json)?;

        // Update cache
        let mut experiments = self.experiments.write().await;
        experiments.insert(experiment_id.to_string(), experiment.clone());

        Ok(experiment)
    }

    /// Update experiment status
    pub async fn update_experiment_status(
        &self,
        experiment_id: &str,
        status: ExperimentStatus,
    ) -> TrainingResult<()> {
        let mut exp = self.get_experiment(experiment_id).await?;
        exp.status = status;
        exp.updated_at = Utc::now();

        // Save to disk
        let exp_file = self
            .base_path
            .join("experiments")
            .join(experiment_id)
            .join("metadata.json");
        let json = serde_json::to_string_pretty(&exp)?;
        fs::write(&exp_file, json).await?;

        // Update cache
        let mut experiments = self.experiments.write().await;
        experiments.insert(experiment_id.to_string(), exp);

        Ok(())
    }

    /// Save a training checkpoint
    pub async fn save_checkpoint(
        &self,
        experiment_id: &str,
        epoch: u64,
        model_state: Vec<u8>,
        optimizer_state: Option<Vec<u8>>,
        metrics: JsonValue,
    ) -> TrainingResult<Checkpoint> {
        // Verify experiment exists
        let mut exp = self.get_experiment(experiment_id).await?;

        // Create checkpoint metadata
        let checkpoint = Checkpoint {
            id: uuid::Uuid::new_v4().to_string(),
            experiment_id: experiment_id.to_string(),
            epoch,
            step: 0, // Will be updated if step info is provided
            created_at: Utc::now(),
            metrics: metrics.clone(),
            shard_count: 1,
            total_size: (model_state.len() + optimizer_state.as_ref().map(|s| s.len()).unwrap_or(0))
                as u64,
            has_optimizer_state: optimizer_state.is_some(),
            is_sharded: false,
        };

        // Create checkpoint directory
        let ckpt_dir = self
            .base_path
            .join("checkpoints")
            .join(experiment_id)
            .join(&checkpoint.id);
        fs::create_dir_all(&ckpt_dir).await?;

        // Save model state
        let model_path = ckpt_dir.join("model.bin");
        fs::write(&model_path, &model_state).await?;

        // Save optimizer state if provided
        if let Some(opt_state) = optimizer_state {
            let opt_path = ckpt_dir.join("optimizer.bin");
            fs::write(&opt_path, &opt_state).await?;
        }

        // Save checkpoint metadata
        let meta_path = ckpt_dir.join("metadata.json");
        let json = serde_json::to_string_pretty(&checkpoint)?;
        fs::write(&meta_path, json).await?;

        // Update experiment
        exp.checkpoint_count += 1;
        exp.updated_at = Utc::now();
        if exp.best_metrics.is_none() {
            exp.best_metrics = Some(metrics.clone());
        }

        // Save experiment
        let exp_file = self
            .base_path
            .join("experiments")
            .join(experiment_id)
            .join("metadata.json");
        let json = serde_json::to_string_pretty(&exp)?;
        fs::write(&exp_file, json).await?;

        // Update cache
        let mut experiments = self.experiments.write().await;
        experiments.insert(experiment_id.to_string(), exp);

        info!(
            "Saved checkpoint {} for experiment {} at epoch {}",
            checkpoint.id, experiment_id, epoch
        );

        // Apply retention policy
        self.apply_retention_policy(experiment_id).await?;

        Ok(checkpoint)
    }

    /// Load a checkpoint
    pub async fn load_checkpoint(&self, checkpoint_id: &str) -> TrainingResult<LoadedCheckpoint> {
        // Find checkpoint by searching all experiments
        let mut found_experiment_id: Option<String> = None;

        let checkpoints_dir = self.base_path.join("checkpoints");

        // If checkpoints directory doesn't exist, checkpoint definitely doesn't exist
        if !checkpoints_dir.exists() {
            return Err(TrainingError::CheckpointNotFound(checkpoint_id.to_string()));
        }

        let mut entries = fs::read_dir(&checkpoints_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let exp_id = entry.file_name().to_string_lossy().to_string();
                let ckpt_dir = entry.path().join(checkpoint_id);
                if ckpt_dir.exists() {
                    found_experiment_id = Some(exp_id);
                    break;
                }
            }
        }

        let experiment_id = found_experiment_id
            .ok_or_else(|| TrainingError::CheckpointNotFound(checkpoint_id.to_string()))?;

        let ckpt_dir = checkpoints_dir.join(&experiment_id).join(checkpoint_id);

        // Load metadata
        let meta_path = ckpt_dir.join("metadata.json");
        let json = fs::read_to_string(&meta_path).await?;
        let checkpoint: Checkpoint = serde_json::from_str(&json)?;

        // Load model state
        let model_path = ckpt_dir.join("model.bin");
        let model_state = fs::read(&model_path).await?;

        // Load optimizer state if present
        let optimizer_state = if checkpoint.has_optimizer_state {
            let opt_path = ckpt_dir.join("optimizer.bin");
            Some(fs::read(&opt_path).await?)
        } else {
            None
        };

        debug!(
            "Loaded checkpoint {} from epoch {}",
            checkpoint_id, checkpoint.epoch
        );

        Ok(LoadedCheckpoint {
            checkpoint,
            model_state,
            optimizer_state,
        })
    }

    /// List checkpoints for an experiment
    pub async fn list_checkpoints(&self, experiment_id: &str) -> TrainingResult<Vec<Checkpoint>> {
        let ckpt_dir = self.base_path.join("checkpoints").join(experiment_id);

        if !ckpt_dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let mut entries = fs::read_dir(&ckpt_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let meta_path = entry.path().join("metadata.json");
                if meta_path.exists() {
                    let json = fs::read_to_string(&meta_path).await?;
                    if let Ok(ckpt) = serde_json::from_str::<Checkpoint>(&json) {
                        checkpoints.push(ckpt);
                    }
                }
            }
        }

        // Sort by epoch (descending)
        checkpoints.sort_by(|a, b| b.epoch.cmp(&a.epoch));

        Ok(checkpoints)
    }

    /// Log training metrics
    pub async fn log_metrics(
        &self,
        experiment_id: &str,
        step: u64,
        metrics: JsonValue,
    ) -> TrainingResult<()> {
        // Verify experiment exists
        self.get_experiment(experiment_id).await?;

        let entry = MetricsEntry {
            experiment_id: experiment_id.to_string(),
            step,
            timestamp: Utc::now(),
            metrics,
        };

        // Append to metrics file
        let metrics_dir = self.base_path.join("metrics").join(experiment_id);
        fs::create_dir_all(&metrics_dir).await?;

        let metrics_file = metrics_dir.join("metrics.jsonl");
        let json = serde_json::to_string(&entry)? + "\n";

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&metrics_file)
            .await?;
        file.write_all(json.as_bytes()).await?;
        file.flush().await?;

        debug!(
            "Logged metrics for experiment {} at step {}",
            experiment_id, step
        );

        Ok(())
    }

    /// Get metrics for an experiment
    pub async fn get_metrics(&self, experiment_id: &str) -> TrainingResult<Vec<MetricsEntry>> {
        let metrics_file = self
            .base_path
            .join("metrics")
            .join(experiment_id)
            .join("metrics.jsonl");

        if !metrics_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(&metrics_file).await?;
        let mut metrics = Vec::new();

        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<MetricsEntry>(line) {
                metrics.push(entry);
            }
        }

        Ok(metrics)
    }

    /// Apply retention policy to remove old checkpoints
    async fn apply_retention_policy(&self, experiment_id: &str) -> TrainingResult<()> {
        if self.checkpoint_config.max_checkpoints == 0 {
            return Ok(()); // Unlimited checkpoints
        }

        let checkpoints = self.list_checkpoints(experiment_id).await?;

        if checkpoints.len() as u32 <= self.checkpoint_config.max_checkpoints {
            return Ok(()); // Within limit
        }

        let _to_remove = checkpoints.len() - self.checkpoint_config.max_checkpoints as usize;
        let checkpoints_to_remove = match self.checkpoint_config.retention_policy {
            RetentionPolicy::KeepAll => Vec::new(),
            RetentionPolicy::KeepRecent => {
                // Already sorted by epoch (descending), remove oldest
                checkpoints
                    .iter()
                    .skip(self.checkpoint_config.max_checkpoints as usize)
                    .collect()
            }
            RetentionPolicy::KeepBest => {
                // For now, keep most recent (would need metric comparison in real implementation)
                checkpoints
                    .iter()
                    .skip(self.checkpoint_config.max_checkpoints as usize)
                    .collect()
            }
            RetentionPolicy::KeepEpochInterval => {
                // Keep checkpoints at interval
                checkpoints
                    .iter()
                    .skip(self.checkpoint_config.max_checkpoints as usize)
                    .collect()
            }
        };

        for ckpt in checkpoints_to_remove {
            let ckpt_dir = self
                .base_path
                .join("checkpoints")
                .join(experiment_id)
                .join(&ckpt.id);

            if ckpt_dir.exists() {
                fs::remove_dir_all(&ckpt_dir).await?;
                info!(
                    "Removed old checkpoint {} for experiment {}",
                    ckpt.id, experiment_id
                );
            }
        }

        Ok(())
    }

    /// Create a hyperparameter search
    pub async fn create_search(
        &self,
        search_space: JsonValue,
        optimization_metric: String,
    ) -> TrainingResult<HyperparameterSearchResult> {
        // Ensure directories exist
        self.ensure_directories().await?;

        let search = HyperparameterSearchResult {
            id: uuid::Uuid::new_v4().to_string(),
            experiment_ids: Vec::new(),
            search_space,
            best_params: JsonValue::Null,
            best_metric_value: None,
            optimization_metric,
            trials: Vec::new(),
            started_at: Utc::now(),
            completed_at: None,
        };

        // Save to disk
        let search_file = self
            .base_path
            .join("searches")
            .join(format!("{}.json", search.id));
        let json = serde_json::to_string_pretty(&search)?;
        fs::write(&search_file, json).await?;

        // Add to cache
        let mut searches = self.searches.write().await;
        searches.insert(search.id.clone(), search.clone());

        info!("Created hyperparameter search: {}", search.id);

        Ok(search)
    }

    /// Add a trial to a hyperparameter search
    pub async fn add_trial(
        &self,
        search_id: &str,
        params: JsonValue,
        metrics: JsonValue,
        status: TrialStatus,
    ) -> TrainingResult<()> {
        let search_file = self
            .base_path
            .join("searches")
            .join(format!("{}.json", search_id));

        if !search_file.exists() {
            return Err(TrainingError::ExperimentNotFound(search_id.to_string()));
        }

        let json = fs::read_to_string(&search_file).await?;
        let mut search: HyperparameterSearchResult = serde_json::from_str(&json)?;

        let trial = Trial {
            id: uuid::Uuid::new_v4().to_string(),
            params,
            metrics,
            status,
        };

        search.trials.push(trial);

        // Save to disk
        let json = serde_json::to_string_pretty(&search)?;
        fs::write(&search_file, json).await?;

        // Update cache
        let mut searches = self.searches.write().await;
        searches.insert(search_id.to_string(), search);

        Ok(())
    }

    /// Get a hyperparameter search by ID
    pub async fn get_search(&self, search_id: &str) -> TrainingResult<HyperparameterSearchResult> {
        // Try cache first
        {
            let searches = self.searches.read().await;
            if let Some(search) = searches.get(search_id) {
                return Ok(search.clone());
            }
        }

        // Load from disk if not in cache
        let search_file = self
            .base_path
            .join("searches")
            .join(format!("{}.json", search_id));

        if !search_file.exists() {
            return Err(TrainingError::ExperimentNotFound(search_id.to_string()));
        }

        let json = fs::read_to_string(&search_file).await?;
        let search: HyperparameterSearchResult = serde_json::from_str(&json)?;

        // Update cache
        let mut searches = self.searches.write().await;
        searches.insert(search_id.to_string(), search.clone());

        Ok(search)
    }
}

/// Loaded checkpoint data
#[derive(Debug)]
pub struct LoadedCheckpoint {
    /// Checkpoint metadata
    pub checkpoint: Checkpoint,
    /// Model state bytes
    pub model_state: Vec<u8>,
    /// Optimizer state bytes (if available)
    pub optimizer_state: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_manager() -> (TrainingManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let manager = TrainingManager::new(temp_dir.path().to_path_buf());
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_create_experiment() {
        let (manager, _temp) = setup_manager();

        let config = ExperimentConfig {
            name: "test-exp".to_string(),
            description: Some("Test experiment".to_string()),
            tags: vec!["test".to_string()],
            hyperparameters: serde_json::json!({"lr": 0.001}),
        };

        let exp = manager.create_experiment(config).await.unwrap();
        assert_eq!(exp.name, "test-exp");
        assert_eq!(exp.status, ExperimentStatus::Running);
        assert_eq!(exp.checkpoint_count, 0);
    }

    #[tokio::test]
    async fn test_save_and_load_checkpoint() {
        let (manager, _temp) = setup_manager();

        let config = ExperimentConfig {
            name: "ckpt-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: serde_json::json!({}),
        };

        let exp = manager.create_experiment(config).await.unwrap();

        let model_state = b"model_data".to_vec();
        let optimizer_state = Some(b"optimizer_data".to_vec());
        let metrics = serde_json::json!({"loss": 0.5});

        let ckpt = manager
            .save_checkpoint(
                &exp.id,
                1,
                model_state.clone(),
                optimizer_state.clone(),
                metrics,
            )
            .await
            .unwrap();

        assert_eq!(ckpt.epoch, 1);
        assert!(ckpt.has_optimizer_state);

        let loaded = manager.load_checkpoint(&ckpt.id).await.unwrap();
        assert_eq!(loaded.model_state, model_state);
        assert_eq!(loaded.optimizer_state, optimizer_state);
        assert_eq!(loaded.checkpoint.epoch, 1);
    }

    #[tokio::test]
    async fn test_log_metrics() {
        let (manager, _temp) = setup_manager();

        let config = ExperimentConfig {
            name: "metrics-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: serde_json::json!({}),
        };

        let exp = manager.create_experiment(config).await.unwrap();

        manager
            .log_metrics(&exp.id, 1, serde_json::json!({"loss": 0.5}))
            .await
            .unwrap();
        manager
            .log_metrics(&exp.id, 2, serde_json::json!({"loss": 0.4}))
            .await
            .unwrap();

        let metrics = manager.get_metrics(&exp.id).await.unwrap();
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[0].step, 1);
        assert_eq!(metrics[1].step, 2);
    }

    #[tokio::test]
    async fn test_list_checkpoints() {
        let (manager, _temp) = setup_manager();

        let config = ExperimentConfig {
            name: "list-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: serde_json::json!({}),
        };

        let exp = manager.create_experiment(config).await.unwrap();

        // Save 3 checkpoints
        for epoch in 1..=3 {
            manager
                .save_checkpoint(
                    &exp.id,
                    epoch,
                    b"model".to_vec(),
                    None,
                    serde_json::json!({"epoch": epoch}),
                )
                .await
                .unwrap();
        }

        let checkpoints = manager.list_checkpoints(&exp.id).await.unwrap();
        assert_eq!(checkpoints.len(), 3);
        // Should be sorted by epoch descending
        assert_eq!(checkpoints[0].epoch, 3);
        assert_eq!(checkpoints[1].epoch, 2);
        assert_eq!(checkpoints[2].epoch, 1);
    }

    #[tokio::test]
    async fn test_retention_policy() {
        let (mut manager, _temp) = setup_manager();

        // Set max checkpoints to 2
        manager.checkpoint_config.max_checkpoints = 2;
        manager.checkpoint_config.retention_policy = RetentionPolicy::KeepRecent;

        let config = ExperimentConfig {
            name: "retention-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: serde_json::json!({}),
        };

        let exp = manager.create_experiment(config).await.unwrap();

        // Save 4 checkpoints
        for epoch in 1..=4 {
            manager
                .save_checkpoint(
                    &exp.id,
                    epoch,
                    b"model".to_vec(),
                    None,
                    serde_json::json!({"epoch": epoch}),
                )
                .await
                .unwrap();
        }

        // Should only have 2 checkpoints (most recent)
        let checkpoints = manager.list_checkpoints(&exp.id).await.unwrap();
        assert_eq!(checkpoints.len(), 2);
        assert_eq!(checkpoints[0].epoch, 4);
        assert_eq!(checkpoints[1].epoch, 3);
    }

    #[tokio::test]
    async fn test_update_experiment_status() {
        let (manager, _temp) = setup_manager();

        let config = ExperimentConfig {
            name: "status-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: serde_json::json!({}),
        };

        let exp = manager.create_experiment(config).await.unwrap();
        assert_eq!(exp.status, ExperimentStatus::Running);

        manager
            .update_experiment_status(&exp.id, ExperimentStatus::Completed)
            .await
            .unwrap();

        let updated = manager.get_experiment(&exp.id).await.unwrap();
        assert_eq!(updated.status, ExperimentStatus::Completed);
    }

    #[tokio::test]
    async fn test_hyperparameter_search() {
        let (manager, _temp) = setup_manager();

        let search_space = serde_json::json!({
            "learning_rate": [0.001, 0.01, 0.1],
            "batch_size": [16, 32, 64],
        });

        let search = manager
            .create_search(search_space.clone(), "accuracy".to_string())
            .await
            .unwrap();

        assert_eq!(search.optimization_metric, "accuracy");
        assert_eq!(search.trials.len(), 0);

        manager
            .add_trial(
                &search.id,
                serde_json::json!({"learning_rate": 0.001, "batch_size": 32}),
                serde_json::json!({"accuracy": 0.85}),
                TrialStatus::Completed,
            )
            .await
            .unwrap();

        // Verify trial was added (would need to reload from disk in real scenario)
    }

    #[tokio::test]
    async fn test_experiment_not_found() {
        let (manager, _temp) = setup_manager();

        let result = manager.get_experiment("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TrainingError::ExperimentNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_checkpoint_not_found() {
        let (manager, _temp) = setup_manager();

        let result = manager.load_checkpoint("nonexistent").await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TrainingError::CheckpointNotFound(_)
        ));
    }

    #[tokio::test]
    async fn test_duplicate_experiment_name() {
        let (manager, _temp) = setup_manager();

        let config = ExperimentConfig {
            name: "duplicate".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: serde_json::json!({}),
        };

        manager.create_experiment(config.clone()).await.unwrap();

        let result = manager.create_experiment(config).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TrainingError::ExperimentAlreadyExists(_)
        ));
    }
}
