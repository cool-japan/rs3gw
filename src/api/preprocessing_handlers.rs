//! Preprocessing API handlers
//!
//! Provides HTTP endpoints for managing and applying preprocessing pipelines.

use crate::storage::preprocessing::{PipelineDefinition, PreprocessingError, PreprocessingStep};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};

// ============================================================================
// Error Handling
// ============================================================================

impl IntoResponse for PreprocessingError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            PreprocessingError::InvalidConfig(msg) => (StatusCode::BAD_REQUEST, msg),
            PreprocessingError::PipelineNotFound(msg) => (StatusCode::NOT_FOUND, msg),
            PreprocessingError::UnsupportedFormat(msg) => (StatusCode::BAD_REQUEST, msg),
            PreprocessingError::SerializationError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            PreprocessingError::StepFailed(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            PreprocessingError::Io(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

        let body = Json(serde_json::json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePipelineRequest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub steps: Vec<PreprocessingStep>,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePipelineResponse {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListPipelinesResponse {
    pub pipelines: Vec<PipelineDefinition>,
    pub count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApplyPipelineRequest {
    pub pipeline_id: String,
    pub bucket: String,
    pub key: String,
    pub output_bucket: Option<String>,
    pub output_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApplyPipelineResponse {
    pub success: bool,
    pub output_bucket: String,
    pub output_key: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatePipelineRequest {
    pub pipeline: PipelineDefinition,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatePipelineResponse {
    pub valid: bool,
    pub message: String,
    pub errors: Vec<String>,
}

// ============================================================================
// Handler Functions
// ============================================================================

/// POST /api/preprocessing/pipelines - Create a new preprocessing pipeline
pub async fn create_pipeline(
    State(state): State<AppState>,
    Json(req): Json<CreatePipelineRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), PreprocessingError> {
    tracing::info!(
        "Creating preprocessing pipeline: {} (version: {})",
        req.name,
        req.version
    );

    let mut pipeline = PipelineDefinition::new(req.id.clone(), req.name, req.version);

    if let Some(desc) = req.description {
        pipeline = pipeline.with_description(desc);
    }

    for step in req.steps {
        pipeline.add_step(step);
    }

    for (key, value) in req.metadata {
        pipeline.add_metadata(key, value);
    }

    state
        .preprocessing_manager
        .register_pipeline(pipeline)
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "created",
            "pipeline_id": req.id,
        })),
    ))
}

/// GET /api/preprocessing/pipelines - List all pipelines
pub async fn list_pipelines(
    State(state): State<AppState>,
) -> Result<Json<ListPipelinesResponse>, PreprocessingError> {
    tracing::debug!("Listing all preprocessing pipelines");

    let pipelines: Vec<PipelineDefinition> = state.preprocessing_manager.list_pipelines().await;
    let count = pipelines.len();

    Ok(Json(ListPipelinesResponse { pipelines, count }))
}

/// GET /api/preprocessing/pipelines/{id} - Get a specific pipeline
pub async fn get_pipeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, PreprocessingError> {
    tracing::debug!("Getting preprocessing pipeline: {}", id);

    let pipeline: PipelineDefinition = state.preprocessing_manager.get_pipeline(&id).await?;

    Ok(Json(serde_json::json!({
        "pipeline": pipeline,
    })))
}

/// DELETE /api/preprocessing/pipelines/{id} - Delete a pipeline
pub async fn delete_pipeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, PreprocessingError> {
    tracing::info!("Deleting preprocessing pipeline: {}", id);

    state.preprocessing_manager.delete_pipeline(&id).await?;

    Ok(Json(serde_json::json!({
        "status": "deleted",
        "pipeline_id": id,
    })))
}

/// POST /api/preprocessing/apply - Apply a pipeline to an object
pub async fn apply_pipeline(
    State(state): State<AppState>,
    Json(req): Json<ApplyPipelineRequest>,
) -> Result<Json<ApplyPipelineResponse>, PreprocessingError> {
    use futures::stream::StreamExt;

    tracing::info!(
        "Applying pipeline '{}' to {}/{}",
        req.pipeline_id,
        req.bucket,
        req.key
    );

    // Get the input object
    let (metadata, mut stream) = state
        .storage
        .get_object(&req.bucket, &req.key)
        .await
        .map_err(|e| PreprocessingError::StepFailed(format!("Failed to get object: {}", e)))?;

    // Read the stream into bytes
    let mut data = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk: bytes::Bytes = chunk.map_err(|e: crate::storage::StorageError| {
            PreprocessingError::Io(std::io::Error::other(e.to_string()))
        })?;
        data.extend_from_slice(&chunk);
    }
    let input_data = bytes::Bytes::from(data);

    // Apply the pipeline
    let processed_data = state
        .preprocessing_manager
        .apply_pipeline(&req.pipeline_id, input_data, metadata.metadata.clone())
        .await?;

    // Determine output location
    let output_bucket = req.output_bucket.unwrap_or_else(|| req.bucket.clone());
    let output_key = req
        .output_key
        .unwrap_or_else(|| format!("{}.processed", req.key));

    // Store the processed object
    let output_metadata = std::collections::HashMap::new();
    let content_type = metadata.content_type.as_str();
    state
        .storage
        .put_object(
            &output_bucket,
            &output_key,
            content_type,
            output_metadata,
            processed_data,
        )
        .await
        .map_err(|e| PreprocessingError::StepFailed(format!("Failed to store result: {}", e)))?;

    Ok(Json(ApplyPipelineResponse {
        success: true,
        output_bucket: output_bucket.clone(),
        output_key: output_key.clone(),
        message: format!(
            "Pipeline applied successfully. Result stored at {}/{}",
            output_bucket, output_key
        ),
    }))
}

/// POST /api/preprocessing/validate - Validate a pipeline definition
pub async fn validate_pipeline(
    Json(req): Json<ValidatePipelineRequest>,
) -> Json<ValidatePipelineResponse> {
    let mut errors = Vec::new();

    // Validate pipeline ID
    if req.pipeline.id.is_empty() {
        errors.push("Pipeline ID cannot be empty".to_string());
    }

    // Validate pipeline name
    if req.pipeline.name.is_empty() {
        errors.push("Pipeline name cannot be empty".to_string());
    }

    // Validate version
    if req.pipeline.version.is_empty() {
        errors.push("Pipeline version cannot be empty".to_string());
    }

    // Validate steps
    if req.pipeline.steps.is_empty() {
        errors.push("Pipeline must have at least one step".to_string());
    }

    // Validate each step
    for (idx, step) in req.pipeline.steps.iter().enumerate() {
        if step.id.is_empty() {
            errors.push(format!("Step {} has empty ID", idx));
        }
    }

    let valid = errors.is_empty();
    let message = if valid {
        "Pipeline is valid".to_string()
    } else {
        format!("Pipeline has {} validation error(s)", errors.len())
    };

    Json(ValidatePipelineResponse {
        valid,
        message,
        errors,
    })
}

/// GET /api/preprocessing/cache/stats - Get cache statistics
pub async fn get_cache_stats(State(state): State<AppState>) -> Json<serde_json::Value> {
    let (size, items, hits) = state.preprocessing_manager.cache_stats().await;

    Json(serde_json::json!({
        "cache_size_bytes": size,
        "cached_items": items,
        "cache_hits": hits,
    }))
}

/// POST /api/preprocessing/cache/clear - Clear the preprocessing cache
pub async fn clear_cache(State(state): State<AppState>) -> Json<serde_json::Value> {
    state.preprocessing_manager.clear_cache().await;

    Json(serde_json::json!({
        "status": "cleared",
    }))
}
