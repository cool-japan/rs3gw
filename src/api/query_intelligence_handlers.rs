//! Query Intelligence API Handlers
//!
//! REST API endpoints for AI-powered query optimization and intelligence:
//! - Query cost prediction
//! - Execution strategy recommendations
//! - Query statistics and summaries
//! - Similar query detection
//!
//! # Endpoints
//!
//! - `GET /api/query/intelligence/statistics` - Get query execution statistics
//! - `GET /api/query/intelligence/summary` - Get query statistics summary
//! - `POST /api/query/intelligence/predict-cost` - Predict query execution cost
//! - `POST /api/query/intelligence/recommend-strategy` - Get execution strategy recommendation
//! - `POST /api/query/intelligence/find-similar` - Find similar queries
//! - `GET /api/query/intelligence/index-recommendations` - Get automatic index recommendations
//! - `GET /api/query/intelligence/complexity-distribution` - Get query complexity distribution

use crate::api::{
    DataStatistics, IndexRecommendation, QueryCost, QuerySimilarity, QueryStats, QueryStatsSummary,
};
use crate::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};

/// Query cost prediction request
#[derive(Debug, Serialize, Deserialize)]
pub struct PredictCostRequest {
    /// SQL query string (will be parsed)
    pub sql: String,

    /// Data statistics for the target object
    pub data_statistics: DataStatistics,
}

/// Query cost prediction response
#[derive(Debug, Serialize, Deserialize)]
pub struct PredictCostResponse {
    /// Predicted cost
    pub cost: QueryCost,

    /// Timestamp of prediction
    pub timestamp: String,
}

/// Execution strategy recommendation request
#[derive(Debug, Serialize, Deserialize)]
pub struct RecommendStrategyRequest {
    /// SQL query string (will be parsed)
    pub sql: String,

    /// Data statistics for the target object
    pub data_statistics: DataStatistics,
}

/// Execution strategy recommendation response
#[derive(Debug, Serialize, Deserialize)]
pub struct RecommendStrategyResponse {
    /// Recommended execution strategy
    pub strategy: crate::api::ExecutionStrategy,

    /// Predicted cost with this strategy
    pub estimated_cost: QueryCost,

    /// Timestamp of recommendation
    pub timestamp: String,
}

/// Find similar queries request
#[derive(Debug, Serialize, Deserialize)]
pub struct FindSimilarRequest {
    /// SQL query string
    pub sql: String,

    /// Minimum similarity threshold (0.0 - 1.0)
    #[serde(default = "default_similarity_threshold")]
    pub threshold: f64,
}

fn default_similarity_threshold() -> f64 {
    0.7
}

/// Find similar queries response
#[derive(Debug, Serialize, Deserialize)]
pub struct FindSimilarResponse {
    /// Similar queries found
    pub similar_queries: Vec<QuerySimilarity>,

    /// Number of results
    pub count: usize,

    /// Timestamp of search
    pub timestamp: String,
}

/// Statistics response
#[derive(Debug, Serialize, Deserialize)]
pub struct StatisticsResponse {
    /// All query statistics
    pub statistics: Vec<QueryStats>,

    /// Number of queries
    pub count: usize,

    /// Timestamp
    pub timestamp: String,
}

/// Summary response (alias for QueryStatsSummary with timestamp)
#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryResponse {
    /// Query statistics summary
    #[serde(flatten)]
    pub summary: QueryStatsSummary,

    /// Timestamp
    pub timestamp: String,
}

/// Error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub details: Option<String>,
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = StatusCode::BAD_REQUEST;
        (status, Json(self)).into_response()
    }
}

/// Get query execution statistics
///
/// Returns all recorded query execution statistics.
pub async fn get_statistics(
    State(state): State<AppState>,
) -> Result<Json<StatisticsResponse>, ErrorResponse> {
    let statistics = state.query_intelligence.get_statistics().await;
    let count = statistics.len();

    Ok(Json(StatisticsResponse {
        statistics,
        count,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get query statistics summary
///
/// Returns aggregated statistics summary including averages, cache hit rates, etc.
pub async fn get_summary(
    State(state): State<AppState>,
) -> Result<Json<SummaryResponse>, ErrorResponse> {
    let summary = state.query_intelligence.get_summary().await;

    Ok(Json(SummaryResponse {
        summary,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Predict query execution cost
///
/// Predicts the cost of executing a query based on historical data and data statistics.
pub async fn predict_cost(
    State(state): State<AppState>,
    Json(request): Json<PredictCostRequest>,
) -> Result<Json<PredictCostResponse>, ErrorResponse> {
    // Parse query
    let parsed_query = crate::api::select::parse_sql(&request.sql).map_err(
        |e: crate::api::select::SelectError| ErrorResponse {
            error: "Failed to parse SQL query".to_string(),
            details: Some(e.to_string()),
        },
    )?;

    // Predict cost
    let cost = state
        .query_intelligence
        .predict_cost(&parsed_query, &request.data_statistics)
        .await;

    Ok(Json(PredictCostResponse {
        cost,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Recommend execution strategy
///
/// Recommends the optimal execution strategy for a query based on data statistics.
pub async fn recommend_strategy(
    State(state): State<AppState>,
    Json(request): Json<RecommendStrategyRequest>,
) -> Result<Json<RecommendStrategyResponse>, ErrorResponse> {
    // Parse query
    let parsed_query = crate::api::select::parse_sql(&request.sql).map_err(
        |e: crate::api::select::SelectError| ErrorResponse {
            error: "Failed to parse SQL query".to_string(),
            details: Some(e.to_string()),
        },
    )?;

    // Get recommendation
    let strategy = state
        .query_intelligence
        .get_execution_strategy(&parsed_query, &request.data_statistics)
        .await;

    // Also predict cost with this strategy
    let cost = state
        .query_intelligence
        .predict_cost(&parsed_query, &request.data_statistics)
        .await;

    Ok(Json(RecommendStrategyResponse {
        strategy,
        estimated_cost: cost,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Find similar queries
///
/// Finds previously executed queries that are similar to the given query.
pub async fn find_similar(
    State(state): State<AppState>,
    Json(request): Json<FindSimilarRequest>,
) -> Result<Json<FindSimilarResponse>, ErrorResponse> {
    // Parse query
    let parsed_query = crate::api::select::parse_sql(&request.sql).map_err(
        |e: crate::api::select::SelectError| ErrorResponse {
            error: "Failed to parse SQL query".to_string(),
            details: Some(e.to_string()),
        },
    )?;

    // Find similar queries
    let similar_queries = state
        .query_intelligence
        .find_similar_queries(&parsed_query, request.threshold)
        .await;

    let count = similar_queries.len();

    Ok(Json(FindSimilarResponse {
        similar_queries,
        count,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Index recommendations response
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexRecommendationsResponse {
    /// Index recommendations
    pub recommendations: Vec<IndexRecommendation>,

    /// Number of recommendations
    pub count: usize,

    /// Timestamp
    pub timestamp: String,
}

/// Get automatic index recommendations
///
/// Analyzes query patterns to recommend which columns should be indexed
/// for optimal performance.
pub async fn get_index_recommendations(
    State(state): State<AppState>,
) -> Result<Json<IndexRecommendationsResponse>, ErrorResponse> {
    let recommendations = state.query_intelligence.get_index_recommendations().await;
    let count = recommendations.len();

    Ok(Json(IndexRecommendationsResponse {
        recommendations,
        count,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Complexity distribution response
#[derive(Debug, Serialize, Deserialize)]
pub struct ComplexityDistributionResponse {
    /// Distribution of query complexities
    pub distribution: std::collections::HashMap<String, usize>,

    /// Timestamp
    pub timestamp: String,
}

/// Get query complexity distribution
///
/// Returns a breakdown of queries by complexity class (trivial, simple, moderate, complex, very_complex).
pub async fn get_complexity_distribution(
    State(state): State<AppState>,
) -> Result<Json<ComplexityDistributionResponse>, ErrorResponse> {
    let distribution = state.query_intelligence.get_complexity_distribution().await;

    Ok(Json(ComplexityDistributionResponse {
        distribution,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_similarity_threshold() {
        assert_eq!(default_similarity_threshold(), 0.7);
    }

    #[test]
    fn test_predict_cost_request_serialization() {
        let req = PredictCostRequest {
            sql: "SELECT * FROM s3object".to_string(),
            data_statistics: DataStatistics::default(),
        };

        if let Ok(json) = serde_json::to_string(&req) {
            assert!(json.contains("SELECT"));
        }
    }

    #[test]
    fn test_find_similar_request_default_threshold() {
        let json = r#"{"sql": "SELECT * FROM s3object"}"#;
        if let Ok(req) = serde_json::from_str::<FindSimilarRequest>(json) {
            assert_eq!(req.threshold, 0.7);
        }
    }

    #[test]
    fn test_index_recommendations_response_serialization() {
        let response = IndexRecommendationsResponse {
            recommendations: vec![IndexRecommendation {
                column_name: "user_id".to_string(),
                index_type: crate::api::IndexType::BTree,
                reason: crate::api::IndexReason::FilterColumn,
                impact_score: 0.85,
                estimated_speedup: 5.0,
                query_count: 100,
                avg_selectivity: 0.02,
            }],
            count: 1,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };

        if let Ok(json) = serde_json::to_string(&response) {
            assert!(json.contains("user_id"));
            assert!(json.contains("FilterColumn"));
            assert!(json.contains("0.85"));
        }
    }
}
