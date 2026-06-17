//! Observability API handlers for metrics, profiling, and anomaly detection
//!
//! This module provides REST API endpoints for accessing observability data:
//! - `/api/observability/profiling` - CPU, memory, and I/O profiling data
//! - `/api/observability/business-metrics` - Business KPI metrics
//! - `/api/observability/anomalies` - Detected performance anomalies
//! - `/api/observability/resources` - Resource manager statistics
//! - `/api/observability/health` - Comprehensive health check with all metrics

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::AppState;

/// Query parameters for anomaly filtering
#[derive(Debug, Deserialize)]
pub struct AnomalyQuery {
    /// Filter by anomaly type
    pub anomaly_type: Option<String>,
    /// Filter by severity
    pub severity: Option<String>,
    /// Limit number of results
    pub limit: Option<usize>,
}

/// Query parameters for profiling data
#[derive(Debug, Deserialize)]
pub struct ProfilingQuery {
    /// Include pprof format export
    pub pprof: Option<bool>,
}

/// Profiling response with CPU, memory, and I/O stats
#[derive(Debug, Serialize)]
pub struct ProfilingResponse {
    pub timestamp: u64,
    pub cpu: CpuStatsJson,
    pub memory: MemoryStatsJson,
    pub io: IoStatsJson,
    pub pprof_available: bool,
}

#[derive(Debug, Serialize)]
pub struct CpuStatsJson {
    pub utilization_percent: f64,
    pub total_time_seconds: f64,
    pub user_time_seconds: f64,
    pub system_time_seconds: f64,
}

#[derive(Debug, Serialize)]
pub struct MemoryStatsJson {
    pub rss_bytes: u64,
    pub virtual_bytes: u64,
    pub peak_rss_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct IoStatsJson {
    pub read_operations: u64,
    pub write_operations: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub avg_read_latency_ms: f64,
    pub avg_write_latency_ms: f64,
}

/// Business metrics response
#[derive(Debug, Serialize)]
pub struct BusinessMetricsResponse {
    pub timestamp: u64,
    pub storage_utilization: StorageUtilizationJson,
    pub data_transfer: DataTransferJson,
    pub request_patterns: RequestPatternsJson,
    pub user_activity: UserActivityJson,
    pub cost_estimation: CostEstimationJson,
}

#[derive(Debug, Serialize)]
pub struct StorageUtilizationJson {
    pub total_buckets: u64,
    pub total_objects: u64,
    pub total_size_bytes: u64,
    pub growth_rate_percent: f64,
}

#[derive(Debug, Serialize)]
pub struct DataTransferJson {
    pub upload_bytes_per_second: f64,
    pub download_bytes_per_second: f64,
    pub bandwidth_utilization_percent: f64,
    pub peak_upload_rate: f64,
    pub peak_download_rate: f64,
}

#[derive(Debug, Serialize)]
pub struct RequestPatternsJson {
    pub total_requests: u64,
    pub success_rate_percent: f64,
    pub error_rate_percent: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct UserActivityJson {
    pub active_users: u64,
    pub engagement_score: f64,
    pub top_users: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CostEstimationJson {
    pub storage_cost_usd: f64,
    pub bandwidth_cost_usd: f64,
    pub request_cost_usd: f64,
    pub total_cost_usd: f64,
}

/// Anomaly response
#[derive(Debug, Serialize)]
pub struct AnomalyResponse {
    pub timestamp: u64,
    pub anomalies: Vec<AnomalyJson>,
    pub total_count: usize,
}

#[derive(Debug, Serialize)]
pub struct AnomalyJson {
    pub anomaly_type: String,
    pub severity: String,
    pub value: f64,
    pub threshold: f64,
    pub message: String,
    pub timestamp: u64,
}

/// Resource manager response
#[derive(Debug, Serialize)]
pub struct ResourceStatsResponse {
    pub timestamp: u64,
    pub cpu_utilization_percent: f64,
    pub memory_utilization_percent: f64,
    pub active_threads: usize,
    pub active_requests: usize,
    pub pending_requests: usize,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub success_rate_percent: f64,
    pub avg_latency_ms: f64,
    pub load_shedding_active: bool,
}

/// Comprehensive health check response
#[derive(Debug, Serialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub timestamp: u64,
    pub profiling: ProfilingResponse,
    pub resources: ResourceStatsResponse,
    pub anomalies_count: usize,
    pub critical_anomalies_count: usize,
}

/// GET /api/observability/profiling - Get profiling data
pub async fn get_profiling_data(
    State(_state): State<AppState>,
    Query(query): Query<ProfilingQuery>,
) -> impl IntoResponse {
    // Get profiling data from state if available
    // For now, create a mock response since profiler integration is optional
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let response = ProfilingResponse {
        timestamp,
        cpu: CpuStatsJson {
            utilization_percent: 0.0,
            total_time_seconds: 0.0,
            user_time_seconds: 0.0,
            system_time_seconds: 0.0,
        },
        memory: MemoryStatsJson {
            rss_bytes: 0,
            virtual_bytes: 0,
            peak_rss_bytes: 0,
        },
        io: IoStatsJson {
            read_operations: 0,
            write_operations: 0,
            read_bytes: 0,
            write_bytes: 0,
            avg_read_latency_ms: 0.0,
            avg_write_latency_ms: 0.0,
        },
        pprof_available: query.pprof.unwrap_or(false),
    };

    (StatusCode::OK, Json(response))
}

/// GET /api/observability/business-metrics - Get business metrics
pub async fn get_business_metrics(State(_state): State<AppState>) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let response = BusinessMetricsResponse {
        timestamp,
        storage_utilization: StorageUtilizationJson {
            total_buckets: 0,
            total_objects: 0,
            total_size_bytes: 0,
            growth_rate_percent: 0.0,
        },
        data_transfer: DataTransferJson {
            upload_bytes_per_second: 0.0,
            download_bytes_per_second: 0.0,
            bandwidth_utilization_percent: 0.0,
            peak_upload_rate: 0.0,
            peak_download_rate: 0.0,
        },
        request_patterns: RequestPatternsJson {
            total_requests: 0,
            success_rate_percent: 0.0,
            error_rate_percent: 0.0,
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
        },
        user_activity: UserActivityJson {
            active_users: 0,
            engagement_score: 0.0,
            top_users: Vec::new(),
        },
        cost_estimation: CostEstimationJson {
            storage_cost_usd: 0.0,
            bandwidth_cost_usd: 0.0,
            request_cost_usd: 0.0,
            total_cost_usd: 0.0,
        },
    };

    (StatusCode::OK, Json(response))
}

/// GET /api/observability/anomalies - Get detected anomalies
pub async fn get_anomalies(
    State(_state): State<AppState>,
    Query(_query): Query<AnomalyQuery>,
) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // For now, return empty anomaly list
    // This will be populated when anomaly detector is integrated into AppState
    let response = AnomalyResponse {
        timestamp,
        anomalies: Vec::new(),
        total_count: 0,
    };

    (StatusCode::OK, Json(response))
}

/// GET /api/observability/resources - Get resource manager stats
pub async fn get_resource_stats(State(_state): State<AppState>) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let response = ResourceStatsResponse {
        timestamp,
        cpu_utilization_percent: 0.0,
        memory_utilization_percent: 0.0,
        active_threads: 0,
        active_requests: 0,
        pending_requests: 0,
        total_requests: 0,
        successful_requests: 0,
        failed_requests: 0,
        success_rate_percent: 0.0,
        avg_latency_ms: 0.0,
        load_shedding_active: false,
    };

    (StatusCode::OK, Json(response))
}

/// GET /api/observability/health - Comprehensive health check
pub async fn get_comprehensive_health(State(_state): State<AppState>) -> impl IntoResponse {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let profiling = ProfilingResponse {
        timestamp,
        cpu: CpuStatsJson {
            utilization_percent: 0.0,
            total_time_seconds: 0.0,
            user_time_seconds: 0.0,
            system_time_seconds: 0.0,
        },
        memory: MemoryStatsJson {
            rss_bytes: 0,
            virtual_bytes: 0,
            peak_rss_bytes: 0,
        },
        io: IoStatsJson {
            read_operations: 0,
            write_operations: 0,
            read_bytes: 0,
            write_bytes: 0,
            avg_read_latency_ms: 0.0,
            avg_write_latency_ms: 0.0,
        },
        pprof_available: false,
    };

    let resources = ResourceStatsResponse {
        timestamp,
        cpu_utilization_percent: 0.0,
        memory_utilization_percent: 0.0,
        active_threads: 0,
        active_requests: 0,
        pending_requests: 0,
        total_requests: 0,
        successful_requests: 0,
        failed_requests: 0,
        success_rate_percent: 0.0,
        avg_latency_ms: 0.0,
        load_shedding_active: false,
    };

    let response = HealthCheckResponse {
        status: "healthy".to_string(),
        timestamp,
        profiling,
        resources,
        anomalies_count: 0,
        critical_anomalies_count: 0,
    };

    (StatusCode::OK, Json(response))
}

/// GET /api/observability/predictions/storage-growth - Get storage growth predictions
pub async fn get_storage_growth_prediction(State(state): State<AppState>) -> impl IntoResponse {
    match state.predictive_analytics.predict_storage_growth().await {
        Some(prediction) => (StatusCode::OK, Json(serde_json::json!(prediction))),
        None => (
            StatusCode::OK,
            Json(serde_json::json!({
                "current_size": 0,
                "predicted_7d": 0,
                "predicted_30d": 0,
                "predicted_90d": 0,
                "daily_growth_rate": 0.0,
                "confidence": 0.0,
                "trend": "Stable",
                "note": "Insufficient data for prediction (minimum 10 data points required)"
            })),
        ),
    }
}

/// GET /api/observability/predictions/access-patterns - Get access pattern predictions
pub async fn get_access_pattern_prediction(State(state): State<AppState>) -> impl IntoResponse {
    match state.predictive_analytics.predict_access_patterns().await {
        Some(prediction) => (StatusCode::OK, Json(serde_json::json!(prediction))),
        None => (
            StatusCode::OK,
            Json(serde_json::json!({
                "current_rps": 0.0,
                "predicted_1h": 0.0,
                "predicted_24h": 0.0,
                "expected_peak": 0.0,
                "peak_hour": 0,
                "pattern_type": "Stable",
                "confidence": 0.0,
                "note": "Insufficient data for prediction (minimum 24 data points required)"
            })),
        ),
    }
}

/// GET /api/observability/predictions/costs - Get cost forecasts
pub async fn get_cost_forecast(State(state): State<AppState>) -> impl IntoResponse {
    match state.predictive_analytics.forecast_costs().await {
        Some(forecast) => (StatusCode::OK, Json(serde_json::json!(forecast))),
        None => (
            StatusCode::OK,
            Json(serde_json::json!({
                "current_monthly_cost": 0.0,
                "predicted_next_month": 0.0,
                "predicted_3_months": 0.0,
                "predicted_6_months": 0.0,
                "storage_cost": 0.0,
                "bandwidth_cost": 0.0,
                "request_cost": 0.0,
                "growth_rate_percent": 0.0,
                "note": "Insufficient data for forecast"
            })),
        ),
    }
}

/// GET /api/observability/predictions/capacity - Get capacity planning recommendations
pub async fn get_capacity_recommendations(State(state): State<AppState>) -> impl IntoResponse {
    match state.predictive_analytics.capacity_recommendations().await {
        Some(recommendation) => (StatusCode::OK, Json(serde_json::json!(recommendation))),
        None => (
            StatusCode::OK,
            Json(serde_json::json!({
                "current_utilization": 0.0,
                "predicted_30d_utilization": 0.0,
                "predicted_90d_utilization": 0.0,
                "days_until_threshold": null,
                "recommendation": "NoActionNeeded",
                "additional_capacity_needed": null,
                "urgency": "Low",
                "note": "Insufficient data for capacity planning"
            })),
        ),
    }
}

/// Query parameters for the usage report endpoint.
#[derive(Debug, Deserialize)]
pub struct UsageQuery {
    /// When `true`, fire the registered usage export hooks in addition to
    /// returning the report (e.g. emit it to logs / an external billing sink).
    #[serde(default)]
    pub flush: bool,
}

/// Collect live per-bucket storage stats as `(bucket, storage_bytes, object_count)`.
///
/// Read fresh from the storage engine so the figures are accurate regardless of
/// overwrites / multipart / out-of-band changes (the usage tracker deliberately
/// does not try to maintain a running storage total).
async fn collect_bucket_storage_stats(state: &AppState) -> Vec<(String, u64, u64)> {
    let mut stats = Vec::new();
    if let Ok(buckets) = state.storage.list_buckets().await {
        for bucket in buckets {
            if let Ok((objects, _)) = state
                .storage
                .list_objects(&bucket.name, "", None, usize::MAX)
                .await
            {
                let size: u64 = objects.iter().map(|o| o.size).sum();
                let count = objects.len() as u64;
                stats.push((bucket.name, size, count));
            }
        }
    }
    stats
}

/// `GET /api/usage` — full cost/usage report across all buckets.
///
/// Combines the usage tracker's cumulative per-bucket counters (transfer bytes,
/// request counts) with live storage stats and an estimated cost breakdown.
/// Pass `?flush=true` to also invoke registered export hooks.
pub async fn get_usage(
    State(state): State<AppState>,
    Query(query): Query<UsageQuery>,
) -> impl IntoResponse {
    let stats = collect_bucket_storage_stats(&state).await;
    let report = if query.flush {
        state.usage_tracker.flush(&stats).await
    } else {
        state.usage_tracker.build_report(&stats).await
    };
    (StatusCode::OK, Json(report))
}

/// `GET /api/usage/{bucket}` — cost/usage for a single bucket.
pub async fn get_bucket_usage(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> impl IntoResponse {
    match state.storage.bucket_exists(&bucket).await {
        Ok(true) => {}
        Ok(false) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "NoSuchBucket", "bucket": bucket })),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "InternalError" })),
            )
                .into_response();
        }
    }
    let (size, count) = match state
        .storage
        .list_objects(&bucket, "", None, usize::MAX)
        .await
    {
        Ok((objects, _)) => (objects.iter().map(|o| o.size).sum(), objects.len() as u64),
        Err(_) => (0u64, 0u64),
    };
    let usage = state.usage_tracker.bucket_usage(&bucket, size, count).await;
    (StatusCode::OK, Json(usage)).into_response()
}

/// `GET /metrics/exemplars` — recent per-operation latency exemplars.
///
/// Each exemplar carries the latency, HTTP status, and (when an OpenTelemetry
/// trace was sampled for that request) the `trace_id`, so a slow-latency sample
/// can be correlated with its distributed trace. This complements the Prometheus
/// `/metrics` text, whose exporter cannot embed OpenMetrics exemplars inline.
pub async fn get_metrics_exemplars() -> impl IntoResponse {
    (StatusCode::OK, Json(crate::metrics::exemplars_snapshot()))
}

// Integration tests for observability handlers are in tests/observability_tests.rs
