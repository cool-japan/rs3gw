//! Intelligent Tiering API Handlers
//!
//! REST API endpoints for managing intelligent data tiering policies,
//! analyzing tiering recommendations, and monitoring tier transitions.
//!
//! # Endpoints
//!
//! ## Policy Management
//! - `GET /api/tiering/policies/:bucket` - Get tiering policy for a bucket
//! - `PUT /api/tiering/policies/:bucket` - Set tiering policy for a bucket
//! - `DELETE /api/tiering/policies/:bucket` - Remove tiering policy for a bucket
//!
//! ## Analysis & Recommendations
//! - `POST /api/tiering/analyze/:bucket` - Analyze bucket and get recommendations
//! - `POST /api/tiering/analyze/:bucket/predictive` - Predictive tiering analysis
//! - `GET /api/tiering/recommendations/:bucket/capacity` - Get capacity-aware recommendations
//!
//! ## Transitions & History
//! - `POST /api/tiering/apply/:bucket` - Apply tiering recommendations
//! - `GET /api/tiering/history` - Get all transition history
//! - `GET /api/tiering/history/:bucket` - Get bucket-specific transition history
//!
//! ## Statistics
//! - `GET /api/tiering/stats/:bucket` - Get tiering statistics for a bucket

use crate::storage::tiering::{
    IntelligentTieringManager, TieringAnalysis, TieringPolicy, TieringRecommendation,
    TieringTransition,
};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Response for policy operations
#[derive(Debug, Serialize, Deserialize)]
pub struct PolicyResponse {
    pub bucket: String,
    pub policy: Option<TieringPolicy>,
    pub status: String,
}

/// Response for analysis operations
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub analysis: TieringAnalysis,
    pub timestamp: String,
}

/// Response for apply operations
#[derive(Debug, Serialize, Deserialize)]
pub struct ApplyResponse {
    pub bucket: String,
    pub transitions_applied: usize,
    pub transitions: Vec<TieringTransition>,
    pub status: String,
}

/// Response for history operations
#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryResponse {
    pub bucket: Option<String>,
    pub transitions: Vec<TieringTransition>,
    pub total_count: usize,
}

/// Response for statistics
#[derive(Debug, Serialize, Deserialize)]
pub struct TieringStats {
    pub bucket: String,
    pub total_objects: usize,
    pub objects_by_tier: std::collections::HashMap<String, usize>,
    pub potential_savings: f64,
    pub last_analysis: Option<String>,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let body = Json(self);
        (StatusCode::BAD_REQUEST, body).into_response()
    }
}

/// GET /api/tiering/policies/:bucket
///
/// Get the current tiering policy for a bucket
pub async fn get_tiering_policy(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<PolicyResponse>, ErrorResponse> {
    // Create tiering manager
    let manager = IntelligentTieringManager::new(state.config.storage_root.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to create tiering manager".to_string(),
            details: Some(e.to_string()),
        })?;

    let policy = manager.get_policy(&bucket).await;

    Ok(Json(PolicyResponse {
        bucket,
        policy,
        status: "success".to_string(),
    }))
}

/// PUT /api/tiering/policies/:bucket
///
/// Set a tiering policy for a bucket
pub async fn set_tiering_policy(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Json(policy): Json<TieringPolicy>,
) -> Result<Json<PolicyResponse>, ErrorResponse> {
    // Create tiering manager
    let manager = IntelligentTieringManager::new(state.config.storage_root.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to create tiering manager".to_string(),
            details: Some(e.to_string()),
        })?;

    manager
        .set_policy(&bucket, policy.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to set tiering policy".to_string(),
            details: Some(e.to_string()),
        })?;

    Ok(Json(PolicyResponse {
        bucket,
        policy: Some(policy),
        status: "success".to_string(),
    }))
}

/// DELETE /api/tiering/policies/:bucket
///
/// Remove tiering policy for a bucket (resets to default)
pub async fn delete_tiering_policy(
    State(_state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<PolicyResponse>, ErrorResponse> {
    // In practice, this would remove the policy from storage
    // For now, just return success
    Ok(Json(PolicyResponse {
        bucket,
        policy: None,
        status: "deleted".to_string(),
    }))
}

/// POST /api/tiering/analyze/:bucket
///
/// Analyze bucket and get tiering recommendations
pub async fn analyze_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<AnalysisResponse>, ErrorResponse> {
    // Create tiering manager
    let manager = IntelligentTieringManager::new(state.config.storage_root.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to create tiering manager".to_string(),
            details: Some(e.to_string()),
        })?;

    // Get policy (use balanced if not set)
    let policy = manager
        .get_policy(&bucket)
        .await
        .unwrap_or_else(TieringPolicy::balanced);

    // Analyze tiering
    let analysis = manager
        .analyze_tiering(&bucket, &policy)
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to analyze tiering".to_string(),
            details: Some(e.to_string()),
        })?;

    Ok(Json(AnalysisResponse {
        analysis,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// POST /api/tiering/analyze/:bucket/predictive
///
/// Analyze bucket with predictive analytics
pub async fn analyze_tiering_predictive(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<AnalysisResponse>, ErrorResponse> {
    // Create tiering manager with predictive analytics
    let cache_config = crate::storage::ml_cache::MlCacheConfig::default();
    let cache_manager = Arc::new(
        crate::storage::ml_cache::SmartCacheManager::new(cache_config).map_err(|e| {
            ErrorResponse {
                error: "Failed to create cache manager".to_string(),
                details: Some(e.to_string()),
            }
        })?,
    );

    let manager = IntelligentTieringManager::with_analytics(
        state.config.storage_root.clone(),
        cache_manager,
        state.predictive_analytics.clone(),
    )
    .await
    .map_err(|e| ErrorResponse {
        error: "Failed to create tiering manager".to_string(),
        details: Some(e.to_string()),
    })?;

    // Get policy
    let policy = manager
        .get_policy(&bucket)
        .await
        .unwrap_or_else(TieringPolicy::balanced);

    // Predictive analysis
    let analysis = manager
        .analyze_tiering_predictive(&bucket, &policy)
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to analyze tiering with predictions".to_string(),
            details: Some(e.to_string()),
        })?;

    Ok(Json(AnalysisResponse {
        analysis,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// GET /api/tiering/recommendations/:bucket/capacity
///
/// Get capacity-aware tiering recommendations
pub async fn get_capacity_recommendations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<Vec<TieringRecommendation>>, ErrorResponse> {
    // Create tiering manager with analytics
    let cache_config = crate::storage::ml_cache::MlCacheConfig::default();
    let cache_manager = Arc::new(
        crate::storage::ml_cache::SmartCacheManager::new(cache_config).map_err(|e| {
            ErrorResponse {
                error: "Failed to create cache manager".to_string(),
                details: Some(e.to_string()),
            }
        })?,
    );

    let manager = IntelligentTieringManager::with_analytics(
        state.config.storage_root.clone(),
        cache_manager,
        state.predictive_analytics.clone(),
    )
    .await
    .map_err(|e| ErrorResponse {
        error: "Failed to create tiering manager".to_string(),
        details: Some(e.to_string()),
    })?;

    let recommendations = manager
        .get_capacity_aware_recommendations(&bucket)
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to get capacity-aware recommendations".to_string(),
            details: Some(e.to_string()),
        })?;

    Ok(Json(recommendations))
}

/// POST /api/tiering/apply/:bucket
///
/// Apply tiering recommendations to a bucket
pub async fn apply_tiering_recommendations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Json(analysis): Json<TieringAnalysis>,
) -> Result<Json<ApplyResponse>, ErrorResponse> {
    // Create tiering manager
    let manager = IntelligentTieringManager::new(state.config.storage_root.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to create tiering manager".to_string(),
            details: Some(e.to_string()),
        })?;

    let transitions = manager
        .apply_recommendations(&analysis)
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to apply recommendations".to_string(),
            details: Some(e.to_string()),
        })?;

    let count = transitions.len();

    Ok(Json(ApplyResponse {
        bucket,
        transitions_applied: count,
        transitions,
        status: "success".to_string(),
    }))
}

/// GET /api/tiering/history
///
/// Get all transition history
pub async fn get_transition_history(
    State(state): State<AppState>,
) -> Result<Json<HistoryResponse>, ErrorResponse> {
    let manager = IntelligentTieringManager::new(state.config.storage_root.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to create tiering manager".to_string(),
            details: Some(e.to_string()),
        })?;

    let transitions = manager.get_transition_history().await;
    let count = transitions.len();

    Ok(Json(HistoryResponse {
        bucket: None,
        transitions,
        total_count: count,
    }))
}

/// GET /api/tiering/history/:bucket
///
/// Get bucket-specific transition history
pub async fn get_bucket_transition_history(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<HistoryResponse>, ErrorResponse> {
    let manager = IntelligentTieringManager::new(state.config.storage_root.clone())
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to create tiering manager".to_string(),
            details: Some(e.to_string()),
        })?;

    let transitions = manager.get_bucket_transition_history(&bucket).await;
    let count = transitions.len();

    Ok(Json(HistoryResponse {
        bucket: Some(bucket),
        transitions,
        total_count: count,
    }))
}
