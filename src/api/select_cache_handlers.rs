//! S3 Select Query Result Cache API Handlers
//!
//! REST API endpoints for managing and monitoring the S3 Select query result cache.
//!
//! # Endpoints
//!
//! - `GET /api/select/cache/stats` - Get cache statistics
//! - `POST /api/select/cache/clear` - Clear all cache entries
//! - `DELETE /api/select/cache/invalidate/:etag` - Invalidate cache entries for an object
//! - `GET /api/select/cache/patterns` - Get query pattern statistics
//! - `GET /api/select/cache/patterns/top` - Get top N frequently executed queries
//! - `GET /api/select/cache/patterns/recent` - Get recently executed queries
//! - `POST /api/select/cache/patterns/clear` - Clear query pattern history
//! - `POST /api/select/cache/save` - Manually save cache to disk
//! - `POST /api/select/cache/load` - Manually load cache from disk

use crate::api::select_cache::QueryPattern;
use crate::api::SelectCacheStats;
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};

/// Cache statistics response
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStatsResponse {
    pub stats: SelectCacheStats,
    pub timestamp: String,
}

/// Cache clear response
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheClearResponse {
    pub status: String,
    pub message: String,
}

/// Cache invalidation response
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheInvalidateResponse {
    pub status: String,
    pub etag: String,
    pub message: String,
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

/// GET /api/select/cache/stats
///
/// Get S3 Select query result cache statistics
pub async fn get_cache_stats(
    State(state): State<AppState>,
) -> Result<Json<CacheStatsResponse>, ErrorResponse> {
    let stats = state.select_result_cache.stats().await;

    Ok(Json(CacheStatsResponse {
        stats,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// POST /api/select/cache/clear
///
/// Clear all cached query results
pub async fn clear_cache(
    State(state): State<AppState>,
) -> Result<Json<CacheClearResponse>, ErrorResponse> {
    state.select_result_cache.clear().await;

    Ok(Json(CacheClearResponse {
        status: "success".to_string(),
        message: "Cache cleared successfully".to_string(),
    }))
}

/// DELETE /api/select/cache/invalidate/:etag
///
/// Invalidate all cached query results for a specific object (by ETag)
pub async fn invalidate_object_cache(
    State(state): State<AppState>,
    Path(etag): Path<String>,
) -> Result<Json<CacheInvalidateResponse>, ErrorResponse> {
    state.select_result_cache.invalidate_object(&etag).await;

    Ok(Json(CacheInvalidateResponse {
        status: "success".to_string(),
        etag,
        message: "Cache invalidated for object".to_string(),
    }))
}

// ===== Cache Warming Endpoints =====

/// Query parameters for top queries endpoint
#[derive(Debug, Deserialize)]
pub struct TopQueriesParams {
    /// Number of top queries to return (default: 10)
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    10
}

/// Query parameters for recent queries endpoint
#[derive(Debug, Deserialize)]
pub struct RecentQueriesParams {
    /// Time window in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_time_window")]
    pub seconds: i64,
}

fn default_time_window() -> i64 {
    3600
}

/// Pattern statistics response
#[derive(Debug, Serialize)]
pub struct PatternStatsResponse {
    pub stats: std::collections::HashMap<String, serde_json::Value>,
    pub timestamp: String,
}

/// Top queries response
#[derive(Debug, Serialize)]
pub struct TopQueriesResponse {
    pub queries: Vec<QueryPattern>,
    pub timestamp: String,
}

/// Recent queries response
#[derive(Debug, Serialize)]
pub struct RecentQueriesResponse {
    pub queries: Vec<QueryPattern>,
    pub window_seconds: i64,
    pub timestamp: String,
}

/// Pattern clear response
#[derive(Debug, Serialize)]
pub struct PatternClearResponse {
    pub status: String,
    pub message: String,
}

/// GET /api/select/cache/patterns
///
/// Get query pattern statistics for cache warming analysis
pub async fn get_pattern_stats(
    State(state): State<AppState>,
) -> Result<Json<PatternStatsResponse>, ErrorResponse> {
    let stats = state.select_result_cache.pattern_stats().await;

    Ok(Json(PatternStatsResponse {
        stats,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// GET /api/select/cache/patterns/top?limit=N
///
/// Get top N most frequently executed queries (default: 10)
pub async fn get_top_queries(
    State(state): State<AppState>,
    Query(params): Query<TopQueriesParams>,
) -> Result<Json<TopQueriesResponse>, ErrorResponse> {
    let queries = state
        .select_result_cache
        .get_top_queries(params.limit)
        .await;

    Ok(Json(TopQueriesResponse {
        queries,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// GET /api/select/cache/patterns/recent?seconds=N
///
/// Get queries executed within the last N seconds (default: 3600 = 1 hour)
pub async fn get_recent_queries(
    State(state): State<AppState>,
    Query(params): Query<RecentQueriesParams>,
) -> Result<Json<RecentQueriesResponse>, ErrorResponse> {
    let queries = state
        .select_result_cache
        .get_recent_queries(params.seconds)
        .await;

    Ok(Json(RecentQueriesResponse {
        queries,
        window_seconds: params.seconds,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// POST /api/select/cache/patterns/clear
///
/// Clear query pattern history
pub async fn clear_patterns(
    State(state): State<AppState>,
) -> Result<Json<PatternClearResponse>, ErrorResponse> {
    state.select_result_cache.clear_patterns().await;

    Ok(Json(PatternClearResponse {
        status: "success".to_string(),
        message: "Query patterns cleared successfully".to_string(),
    }))
}

// ===== Cache Persistence Endpoints =====

/// Cache save response
#[derive(Debug, Serialize)]
pub struct CacheSaveResponse {
    pub status: String,
    pub message: String,
    pub entries_saved: usize,
    pub patterns_saved: usize,
}

/// Cache load response
#[derive(Debug, Serialize)]
pub struct CacheLoadResponse {
    pub status: String,
    pub message: String,
    pub entries_loaded: usize,
    pub entries_expired: usize,
    pub patterns_loaded: usize,
}

/// POST /api/select/cache/save
///
/// Manually trigger cache save to disk
pub async fn save_cache(
    State(state): State<AppState>,
) -> Result<Json<CacheSaveResponse>, ErrorResponse> {
    // Use default cache file path from storage root
    let cache_path = std::path::PathBuf::from(&state.config.storage_root)
        .join(".cache")
        .join("select_cache.json");

    // Ensure cache directory exists
    if let Some(parent) = cache_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ErrorResponse {
                error: "Failed to create cache directory".to_string(),
                details: Some(e.to_string()),
            })?;
    }

    // Get current stats before save
    let cache_stats = state.select_result_cache.stats().await;
    let pattern_stats = state.select_result_cache.pattern_stats().await;

    // Save cache
    state
        .select_result_cache
        .save_to_file(&cache_path)
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to save cache".to_string(),
            details: Some(e.to_string()),
        })?;

    Ok(Json(CacheSaveResponse {
        status: "success".to_string(),
        message: format!("Cache saved to {}", cache_path.display()),
        entries_saved: cache_stats.current_entries,
        patterns_saved: pattern_stats
            .get("total_patterns")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
    }))
}

/// POST /api/select/cache/load
///
/// Manually load cache from disk
pub async fn load_cache(
    State(state): State<AppState>,
) -> Result<Json<CacheLoadResponse>, ErrorResponse> {
    // Use default cache file path from storage root
    let cache_path = std::path::PathBuf::from(&state.config.storage_root)
        .join(".cache")
        .join("select_cache.json");

    // Check if file exists
    if !cache_path.exists() {
        return Err(ErrorResponse {
            error: "Cache file not found".to_string(),
            details: Some(format!("No cache file at {}", cache_path.display())),
        });
    }

    // Get stats before load
    let stats_before = state.select_result_cache.stats().await;

    // Load cache
    state
        .select_result_cache
        .load_from_file(&cache_path)
        .await
        .map_err(|e| ErrorResponse {
            error: "Failed to load cache".to_string(),
            details: Some(e.to_string()),
        })?;

    // Get stats after load
    let stats_after = state.select_result_cache.stats().await;
    let pattern_stats = state.select_result_cache.pattern_stats().await;

    let entries_expired = (stats_after.expirations - stats_before.expirations) as usize;

    Ok(Json(CacheLoadResponse {
        status: "success".to_string(),
        message: format!("Cache loaded from {}", cache_path.display()),
        entries_loaded: stats_after.current_entries,
        entries_expired,
        patterns_loaded: pattern_stats
            .get("total_patterns")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize,
    }))
}

#[cfg(test)]
mod tests {
    // Note: Integration tests for these handlers are in tests/select_cache_tests.rs
    // Unit tests here would require creating a full AppState which is complex.
    // We verify the handler logic through integration tests instead.
}
