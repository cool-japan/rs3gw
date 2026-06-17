//! REST API handlers for Distributed Training Integration
//!
//! This module provides HTTP endpoints for managing training experiments,
//! checkpoints, metrics, and hyperparameter searches.
//!
//! # Endpoints
//!
//! ## Experiment Management
//! - `POST /api/training/experiments` - Create new experiment
//! - `GET /api/training/experiments/:id` - Get experiment details
//! - `GET /api/training/experiments` - List all experiments
//! - `PUT /api/training/experiments/:id/status` - Update experiment status
//!
//! ## Checkpoint Management
//! - `POST /api/training/experiments/:id/checkpoints` - Save checkpoint
//! - `GET /api/training/checkpoints/:checkpoint_id` - Load checkpoint
//! - `GET /api/training/experiments/:id/checkpoints` - List checkpoints
//!
//! ## Metrics
//! - `POST /api/training/experiments/:id/metrics` - Log metrics
//! - `GET /api/training/experiments/:id/metrics` - Get metrics history
//!
//! ## Hyperparameter Search
//! - `POST /api/training/searches` - Create hyperparameter search
//! - `POST /api/training/searches/:id/trials` - Add trial result
//! - `GET /api/training/searches/:id` - Get search results

use crate::storage::distributed_training::{
    Checkpoint, Experiment, ExperimentConfig, ExperimentStatus, HyperparameterSearchResult,
    MetricsEntry, TrialStatus,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Request to create a new experiment
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateExperimentRequest {
    /// Experiment name (must be unique)
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
    /// Hyperparameters as JSON
    pub hyperparameters: JsonValue,
}

/// Response with experiment details
#[derive(Debug, Serialize)]
pub struct ExperimentResponse {
    pub experiment: Experiment,
}

/// Request to update experiment status
#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: ExperimentStatus,
}

/// Request to save a checkpoint
#[derive(Debug, Deserialize)]
pub struct SaveCheckpointRequest {
    /// Epoch number
    pub epoch: u64,
    /// Model state (base64 encoded)
    pub model_state: String,
    /// Optimizer state (base64 encoded, optional)
    pub optimizer_state: Option<String>,
    /// Metrics at this checkpoint
    pub metrics: JsonValue,
}

/// Response with checkpoint details
#[derive(Debug, Serialize)]
pub struct CheckpointResponse {
    pub checkpoint: Checkpoint,
}

/// Response with loaded checkpoint (includes data)
#[derive(Debug, Serialize)]
pub struct LoadedCheckpointResponse {
    pub checkpoint: Checkpoint,
    /// Model state (base64 encoded)
    pub model_state: String,
    /// Optimizer state (base64 encoded, optional)
    pub optimizer_state: Option<String>,
}

/// Request to log metrics
#[derive(Debug, Deserialize)]
pub struct LogMetricsRequest {
    /// Step number
    pub step: u64,
    /// Metrics as JSON
    pub metrics: JsonValue,
}

/// Response with metrics history
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub metrics: Vec<MetricsEntry>,
    pub count: usize,
}

/// Request to create hyperparameter search
#[derive(Debug, Deserialize)]
pub struct CreateSearchRequest {
    /// Search space definition
    pub search_space: JsonValue,
    /// Metric to optimize
    pub optimization_metric: String,
}

/// Request to add a trial
#[derive(Debug, Deserialize)]
pub struct AddTrialRequest {
    /// Hyperparameters used
    pub params: JsonValue,
    /// Result metrics
    pub metrics: JsonValue,
    /// Trial status
    pub status: TrialStatus,
}

/// Response with search results
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub search: HyperparameterSearchResult,
}

/// List experiments response
#[derive(Debug, Serialize)]
pub struct ListExperimentsResponse {
    pub experiments: Vec<Experiment>,
    pub count: usize,
}

/// List checkpoints response
#[derive(Debug, Serialize)]
pub struct ListCheckpointsResponse {
    pub checkpoints: Vec<Checkpoint>,
    pub count: usize,
}

/// Error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, Json(self)).into_response()
    }
}

/// Create a new training experiment
pub async fn create_experiment(
    State(state): State<crate::AppState>,
    Json(req): Json<CreateExperimentRequest>,
) -> Result<Json<ExperimentResponse>, (StatusCode, Json<ErrorResponse>)> {
    let config = ExperimentConfig {
        name: req.name,
        description: req.description,
        tags: req.tags,
        hyperparameters: req.hyperparameters,
    };

    match state.training_manager.create_experiment(config).await {
        Ok(experiment) => Ok(Json(ExperimentResponse { experiment })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Get experiment by ID
pub async fn get_experiment(
    State(state): State<crate::AppState>,
    Path(experiment_id): Path<String>,
) -> Result<Json<ExperimentResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.training_manager.get_experiment(&experiment_id).await {
        Ok(experiment) => Ok(Json(ExperimentResponse { experiment })),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Update experiment status
pub async fn update_experiment_status(
    State(state): State<crate::AppState>,
    Path(experiment_id): Path<String>,
    Json(req): Json<UpdateStatusRequest>,
) -> Result<Json<ExperimentResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .training_manager
        .update_experiment_status(&experiment_id, req.status)
        .await
    {
        Ok(_) => {
            // Fetch the updated experiment to return
            match state.training_manager.get_experiment(&experiment_id).await {
                Ok(experiment) => Ok(Json(ExperimentResponse { experiment })),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )),
            }
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Save a checkpoint
pub async fn save_checkpoint(
    State(state): State<crate::AppState>,
    Path(experiment_id): Path<String>,
    Json(req): Json<SaveCheckpointRequest>,
) -> Result<Json<CheckpointResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Decode base64 model state
    let model_state = match base64::engine::general_purpose::STANDARD.decode(&req.model_state) {
        Ok(data) => data,
        Err(e) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid base64 model_state: {}", e),
                }),
            ))
        }
    };

    // Decode base64 optimizer state if present
    let optimizer_state = if let Some(opt_state_b64) = req.optimizer_state {
        match base64::engine::general_purpose::STANDARD.decode(&opt_state_b64) {
            Ok(data) => Some(data),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("Invalid base64 optimizer_state: {}", e),
                    }),
                ))
            }
        }
    } else {
        None
    };

    match state
        .training_manager
        .save_checkpoint(
            &experiment_id,
            req.epoch,
            model_state,
            optimizer_state,
            req.metrics,
        )
        .await
    {
        Ok(checkpoint) => Ok(Json(CheckpointResponse { checkpoint })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Load a checkpoint
pub async fn load_checkpoint(
    State(state): State<crate::AppState>,
    Path(checkpoint_id): Path<String>,
) -> Result<Json<LoadedCheckpointResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.training_manager.load_checkpoint(&checkpoint_id).await {
        Ok(loaded) => {
            // Encode to base64
            let model_state = base64::engine::general_purpose::STANDARD.encode(&loaded.model_state);
            let optimizer_state = loaded
                .optimizer_state
                .as_ref()
                .map(|s| base64::engine::general_purpose::STANDARD.encode(s));

            Ok(Json(LoadedCheckpointResponse {
                checkpoint: loaded.checkpoint,
                model_state,
                optimizer_state,
            }))
        }
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// List checkpoints for an experiment
pub async fn list_checkpoints(
    State(state): State<crate::AppState>,
    Path(experiment_id): Path<String>,
) -> Result<Json<ListCheckpointsResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .training_manager
        .list_checkpoints(&experiment_id)
        .await
    {
        Ok(checkpoints) => {
            let count = checkpoints.len();
            Ok(Json(ListCheckpointsResponse { checkpoints, count }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Log training metrics
pub async fn log_metrics(
    State(state): State<crate::AppState>,
    Path(experiment_id): Path<String>,
    Json(req): Json<LogMetricsRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    match state
        .training_manager
        .log_metrics(&experiment_id, req.step, req.metrics)
        .await
    {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Get metrics for an experiment
pub async fn get_metrics(
    State(state): State<crate::AppState>,
    Path(experiment_id): Path<String>,
) -> Result<Json<MetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.training_manager.get_metrics(&experiment_id).await {
        Ok(metrics) => {
            let count = metrics.len();
            Ok(Json(MetricsResponse { metrics, count }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Create hyperparameter search
pub async fn create_search(
    State(state): State<crate::AppState>,
    Json(req): Json<CreateSearchRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .training_manager
        .create_search(req.search_space, req.optimization_metric)
        .await
    {
        Ok(search) => Ok(Json(SearchResponse { search })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Add trial to hyperparameter search
pub async fn add_trial(
    State(state): State<crate::AppState>,
    Path(search_id): Path<String>,
    Json(req): Json<AddTrialRequest>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state
        .training_manager
        .add_trial(&search_id, req.params, req.metrics, req.status)
        .await
    {
        Ok(_) => {
            // Fetch the updated search to return
            match state.training_manager.get_search(&search_id).await {
                Ok(search) => Ok(Json(SearchResponse { search })),
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: e.to_string(),
                    }),
                )),
            }
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Get hyperparameter search by ID
pub async fn get_search(
    State(state): State<crate::AppState>,
    Path(search_id): Path<String>,
) -> Result<Json<SearchResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.training_manager.get_search(&search_id).await {
        Ok(search) => Ok(Json(SearchResponse { search })),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::distributed_training::TrainingManager;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup_manager() -> (Arc<TrainingManager>, TempDir) {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let manager: Arc<TrainingManager> =
            Arc::new(TrainingManager::new(temp_dir.path().to_path_buf()));
        (manager, temp_dir)
    }

    fn create_test_app_state(manager: Arc<TrainingManager>) -> crate::AppState {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let storage_root = temp_dir.path().to_path_buf();
        let storage = Arc::new(
            crate::storage::StorageEngine::new(storage_root.clone())
                .expect("failed to create storage engine"),
        );
        // Use shared test metrics handle to avoid conflicts with other test modules
        let metrics_handle = crate::test_helpers::get_test_metrics_handle();
        let preprocessing_path = temp_dir.path().join("preprocessing");
        let preprocessing_manager = Arc::new(
            crate::storage::preprocessing::PreprocessingManager::new(preprocessing_path),
        );
        let predictive_analytics = Arc::new(crate::observability::PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000_000,
        ));
        let metrics_tracker = Arc::new(crate::observability::MetricsTracker::new());
        let select_result_cache =
            Arc::new(crate::api::SelectResultCache::new(100, 10 * 1024 * 1024));
        #[cfg(feature = "formats")]
        let query_intelligence = Arc::new(crate::api::QueryIntelligence::new());

        let config = crate::Config {
            bind_addr: "127.0.0.1:9000"
                .parse()
                .expect("failed to parse bind address"),
            storage_root,
            default_bucket: "default".to_string(),
            access_key: String::new(),
            secret_key: String::new(),
            compression: crate::storage::CompressionMode::None,
            request_timeout_secs: 0,
            max_concurrent_requests: 0,
            tls: crate::TlsConfig::default(),
            connection_pool: crate::ConnectionPoolConfig::default(),
            cluster: crate::cluster::ClusterConfig::default(),
            dedup: crate::storage::DedupConfig::disabled(),
            zerocopy: crate::storage::ZeroCopyConfig::default(),
            select_cache: crate::SelectCacheConfig::default(),
            multipart_retention_hours: 168,
            fsync: false,
        };

        crate::AppState {
            config,
            storage,
            metrics_handle,
            cache: None,
            throttle: None,
            quota: None,
            event_broadcaster: crate::api::EventBroadcaster::new(),
            #[cfg(feature = "formats")]
            query_plan_cache: None,
            select_result_cache,
            #[cfg(feature = "formats")]
            query_intelligence,
            advanced_replication: None,
            preprocessing_manager,
            predictive_analytics,
            metrics_tracker,
            usage_tracker: std::sync::Arc::new(crate::observability::UsageTracker::new()),
            training_manager: manager,
            start_time: std::time::Instant::now(),
            verifier: None,
            auth_failure_counts: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            in_flight: crate::InFlightTracker::new(),
            encryption: std::sync::Arc::new(crate::storage::encryption::EncryptionService::new(
                std::sync::Arc::new(crate::storage::encryption::LocalKeyProvider::default()),
            )),
        }
    }

    #[tokio::test]
    async fn test_create_experiment_handler() {
        let (manager, _temp) = setup_manager();
        let state = create_test_app_state(manager);

        let req = CreateExperimentRequest {
            name: "test-exp".to_string(),
            description: Some("Test".to_string()),
            tags: vec!["test".to_string()],
            hyperparameters: json!({"lr": 0.001}),
        };

        let result = create_experiment(State(state), Json(req)).await;
        assert!(result.is_ok());

        let response = result.expect("create_experiment should succeed").0;
        assert_eq!(response.experiment.name, "test-exp");
    }

    #[tokio::test]
    async fn test_save_and_load_checkpoint_handler() {
        let (manager, _temp) = setup_manager();
        let state = create_test_app_state(manager.clone());

        // Create experiment first
        let exp_config = ExperimentConfig {
            name: "ckpt-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: json!({}),
        };
        let exp = manager
            .create_experiment(exp_config)
            .await
            .expect("create experiment should succeed");

        // Save checkpoint
        let model_data = b"model_data";
        let model_state_b64 = base64::engine::general_purpose::STANDARD.encode(model_data);

        let save_req = SaveCheckpointRequest {
            epoch: 1,
            model_state: model_state_b64.clone(),
            optimizer_state: None,
            metrics: json!({"loss": 0.5}),
        };

        let save_result =
            save_checkpoint(State(state.clone()), Path(exp.id.clone()), Json(save_req)).await;
        assert!(save_result.is_ok());

        let checkpoint_id = save_result
            .expect("save_checkpoint should succeed")
            .0
            .checkpoint
            .id;

        // Load checkpoint
        let load_result = load_checkpoint(State(state), Path(checkpoint_id)).await;
        assert!(load_result.is_ok());

        let loaded = load_result.expect("load_checkpoint should succeed").0;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&loaded.model_state)
            .expect("base64 decode should succeed");
        assert_eq!(decoded, model_data);
    }

    #[tokio::test]
    async fn test_log_and_get_metrics_handler() {
        let (manager, _temp) = setup_manager();
        let state = create_test_app_state(manager.clone());

        let exp_config = ExperimentConfig {
            name: "metrics-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: json!({}),
        };
        let exp = manager
            .create_experiment(exp_config)
            .await
            .expect("create experiment should succeed");

        // Log metrics
        let log_req = LogMetricsRequest {
            step: 1,
            metrics: json!({"loss": 0.5}),
        };

        let log_result =
            log_metrics(State(state.clone()), Path(exp.id.clone()), Json(log_req)).await;
        assert!(log_result.is_ok());

        // Get metrics
        let get_result = get_metrics(State(state), Path(exp.id)).await;
        assert!(get_result.is_ok());

        let response = get_result.expect("get_metrics should succeed").0;
        assert_eq!(response.count, 1);
        assert_eq!(response.metrics[0].step, 1);
    }

    #[tokio::test]
    async fn test_update_status_handler() {
        let (manager, _temp) = setup_manager();
        let state = create_test_app_state(manager.clone());

        let exp_config = ExperimentConfig {
            name: "status-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: json!({}),
        };
        let exp = manager
            .create_experiment(exp_config)
            .await
            .expect("create experiment should succeed");

        let update_req = UpdateStatusRequest {
            status: ExperimentStatus::Completed,
        };

        let result =
            update_experiment_status(State(state), Path(exp.id.clone()), Json(update_req)).await;
        assert!(result.is_ok());

        // Verify status was updated
        let updated = manager
            .get_experiment(&exp.id)
            .await
            .expect("get experiment should succeed");
        assert_eq!(updated.status, ExperimentStatus::Completed);
    }

    #[tokio::test]
    async fn test_list_checkpoints_handler() {
        let (manager, _temp) = setup_manager();
        let state = create_test_app_state(manager.clone());

        let exp_config = ExperimentConfig {
            name: "list-test".to_string(),
            description: None,
            tags: vec![],
            hyperparameters: json!({}),
        };
        let exp = manager
            .create_experiment(exp_config)
            .await
            .expect("create experiment should succeed");

        // Save 2 checkpoints
        for epoch in 1..=2 {
            manager
                .save_checkpoint(
                    &exp.id,
                    epoch,
                    b"model".to_vec(),
                    None,
                    json!({"epoch": epoch}),
                )
                .await
                .expect("save_checkpoint should succeed");
        }

        let result = list_checkpoints(State(state), Path(exp.id)).await;
        assert!(result.is_ok());

        let response = result.expect("list_checkpoints should succeed").0;
        assert_eq!(response.count, 2);
    }

    #[tokio::test]
    async fn test_create_search_handler() {
        let (manager, _temp) = setup_manager();
        let state = create_test_app_state(manager);

        let req = CreateSearchRequest {
            search_space: json!({"lr": [0.001, 0.01]}),
            optimization_metric: "accuracy".to_string(),
        };

        let result = create_search(State(state), Json(req)).await;
        assert!(result.is_ok());

        let response = result.expect("create_search should succeed").0;
        assert_eq!(response.search.optimization_metric, "accuracy");
    }
}
