//! Metrics module
//!
//! Provides Prometheus metrics for monitoring rs3gw performance and health.

use std::sync::{Mutex, Once};
use std::time::Instant;

use axum::{body::Body, extract::Request, http::Method, middleware::Next, response::Response};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Global metrics initialization state
static METRICS_INIT: Once = Once::new();
static METRICS_HANDLE: Mutex<Option<PrometheusHandle>> = Mutex::new(None);

/// Initialize the Prometheus metrics recorder and return the handle
///
/// This function can be called multiple times safely. It will only initialize
/// the global metrics recorder once and return a cloned handle on subsequent calls.
pub fn init_metrics() -> Result<PrometheusHandle, Box<dyn std::error::Error>> {
    METRICS_INIT.call_once(|| {
        match PrometheusBuilder::new().install_recorder() {
            Ok(handle) => {
                if let Ok(mut guard) = METRICS_HANDLE.lock() {
                    *guard = Some(handle);
                }
            }
            Err(_) => {
                // Metrics recorder already installed by another caller
                // Try to create a new builder to get a handle to the existing recorder
                // This will fail to install but that's OK - we just need a handle
                // Note: metrics-exporter-prometheus doesn't provide a way to get the existing handle
                // so we can't populate METRICS_HANDLE in this case
            }
        }
    });

    // Return a cloned handle
    METRICS_HANDLE
        .lock()
        .map_err(|e| format!("Failed to lock metrics handle: {}", e).into())
        .and_then(|guard| {
            guard
                .as_ref()
                .map(|h| h.clone())
                .ok_or_else(|| {
                    "Metrics not initialized: PrometheusBuilder::install_recorder() must be called first.\
                     This usually means metrics were already initialized elsewhere and the handle was not saved.".into()
                })
        })
}

/// Metrics middleware layer
///
/// Records Prometheus metrics and optionally feeds the MetricsTracker if state is provided
pub async fn metrics_layer(request: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(String::from);

    // Determine operation from method, path, and query
    let operation = classify_operation(&method, &path, query.as_deref());

    // Process the request
    let response = next.run(request).await;

    // Record metrics
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
    let status = response.status().as_u16();

    record_request(&operation, status);
    record_latency(&operation, latency_ms);

    response
}

/// Metrics tracker middleware - records request/bandwidth metrics for predictive analytics
///
/// This middleware should be added after metrics_layer in the middleware stack
pub async fn metrics_tracker_layer(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let method = request.method().clone();

    // Process the request
    let response = next.run(request).await;

    // Record request to MetricsTracker
    state.metrics_tracker.record_request();

    // Estimate bandwidth based on response body size (if available in headers)
    if let Some(content_length) = response.headers().get("content-length") {
        if let Ok(len_str) = content_length.to_str() {
            if let Ok(len) = len_str.parse::<u64>() {
                // For GET requests, count as download; for PUT/POST, count as upload
                match method.as_str() {
                    "GET" | "HEAD" => state.metrics_tracker.record_bytes_downloaded(len),
                    "PUT" | "POST" => state.metrics_tracker.record_bytes_uploaded(len),
                    _ => {}
                }
            }
        }
    }

    response
}

/// Check if a query string contains a specific parameter
fn has_query_param(query: Option<&str>, param: &str) -> bool {
    query.is_some_and(|q| {
        q.split('&')
            .any(|p| p == param || p.starts_with(&format!("{}=", param)))
    })
}

/// Classify S3 operation from method, path, and query parameters
fn classify_operation(method: &Method, path: &str, query: Option<&str>) -> String {
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();

    match (method.as_str(), parts.as_slice()) {
        // Health and metrics
        (_, ["health"]) => "HealthCheck".to_string(),
        (_, ["metrics"]) => "Metrics".to_string(),
        // Service level
        ("GET", [""]) | ("GET", []) => "ListBuckets".to_string(),
        // Bucket level
        ("HEAD", [_bucket]) => "HeadBucket".to_string(),
        ("PUT", [_bucket]) => {
            if has_query_param(query, "versioning") {
                "PutBucketVersioning".to_string()
            } else if has_query_param(query, "acl") {
                "PutBucketAcl".to_string()
            } else {
                "CreateBucket".to_string()
            }
        }
        ("DELETE", [_bucket]) => "DeleteBucket".to_string(),
        ("POST", [_bucket]) if has_query_param(query, "delete") => "DeleteObjects".to_string(),
        ("GET", [_bucket]) => {
            // Distinguish bucket GET operations by query parameters
            if has_query_param(query, "location") {
                "GetBucketLocation".to_string()
            } else if has_query_param(query, "versioning") {
                "GetBucketVersioning".to_string()
            } else if has_query_param(query, "acl") {
                "GetBucketAcl".to_string()
            } else if has_query_param(query, "uploads") {
                "ListMultipartUploads".to_string()
            } else {
                "ListObjectsV2".to_string()
            }
        }
        // Object level
        ("HEAD", [_bucket, ..]) => "HeadObject".to_string(),
        ("GET", [_bucket, ..]) => {
            if has_query_param(query, "tagging") {
                "GetObjectTagging".to_string()
            } else if has_query_param(query, "acl") {
                "GetObjectAcl".to_string()
            } else if has_query_param(query, "uploadId") {
                "ListParts".to_string()
            } else {
                "GetObject".to_string()
            }
        }
        ("PUT", [_bucket, ..]) => {
            if has_query_param(query, "tagging") {
                "PutObjectTagging".to_string()
            } else if has_query_param(query, "partNumber") {
                "UploadPart".to_string()
            } else {
                "PutObject".to_string()
            }
        }
        ("POST", [_bucket, ..]) => {
            if has_query_param(query, "uploads") {
                "CreateMultipartUpload".to_string()
            } else if has_query_param(query, "uploadId") {
                "CompleteMultipartUpload".to_string()
            } else {
                "PostObject".to_string()
            }
        }
        ("DELETE", [_bucket, ..]) => {
            if has_query_param(query, "tagging") {
                "DeleteObjectTagging".to_string()
            } else if has_query_param(query, "uploadId") {
                "AbortMultipartUpload".to_string()
            } else {
                "DeleteObject".to_string()
            }
        }
        _ => "Unknown".to_string(),
    }
}

/// Record an S3 request
pub fn record_request(operation: &str, status: u16) {
    let status_class = match status {
        200..=299 => "2xx",
        300..=399 => "3xx",
        400..=499 => "4xx",
        500..=599 => "5xx",
        _ => "other",
    };
    let labels = [
        ("operation", operation.to_string()),
        ("status", status_class.to_string()),
    ];
    counter!("rs3gw_requests_total", &labels).increment(1);
}

/// Record request latency in milliseconds
pub fn record_latency(operation: &str, latency_ms: f64) {
    let labels = [("operation", operation.to_string())];
    histogram!("rs3gw_request_duration_ms", &labels).record(latency_ms);
}

/// Record bytes transferred
pub fn record_bytes(direction: &str, bytes: u64) {
    let labels = [("direction", direction.to_string())];
    counter!("rs3gw_bytes_total", &labels).increment(bytes);
}

/// Update storage stats (called periodically or on changes)
pub fn update_storage_stats(bucket_count: usize, object_count: usize, total_size: u64) {
    gauge!("rs3gw_buckets_total").set(bucket_count as f64);
    gauge!("rs3gw_objects_total").set(object_count as f64);
    gauge!("rs3gw_storage_bytes").set(total_size as f64);
}

/// Record compression stats
pub fn record_compression(algorithm: &str, original_size: u64, compressed_size: u64) {
    let labels = [("algorithm", algorithm.to_string())];
    counter!("rs3gw_compression_original_bytes", &labels).increment(original_size);
    counter!("rs3gw_compression_compressed_bytes", &labels).increment(compressed_size);

    // Calculate and record compression ratio
    if original_size > 0 {
        let ratio = (compressed_size as f64) / (original_size as f64);
        histogram!("rs3gw_compression_ratio", &labels).record(ratio);
    }
}

/// Record cache operations
pub fn record_cache_operation(operation: &str, hit: bool) {
    let labels = [
        ("operation", operation.to_string()),
        ("result", if hit { "hit" } else { "miss" }.to_string()),
    ];
    counter!("rs3gw_cache_operations_total", &labels).increment(1);
}

/// Update cache stats
pub fn update_cache_stats(size_bytes: u64, object_count: usize, hit_rate: f64) {
    gauge!("rs3gw_cache_size_bytes").set(size_bytes as f64);
    gauge!("rs3gw_cache_objects_total").set(object_count as f64);
    gauge!("rs3gw_cache_hit_rate").set(hit_rate);
}

/// Record error by type
pub fn record_error(error_type: &str, operation: &str) {
    let labels = [
        ("error_type", error_type.to_string()),
        ("operation", operation.to_string()),
    ];
    counter!("rs3gw_errors_total", &labels).increment(1);
}

/// Record cluster operations
pub fn record_cluster_operation(operation: &str, status: &str) {
    let labels = [
        ("operation", operation.to_string()),
        ("status", status.to_string()),
    ];
    counter!("rs3gw_cluster_operations_total", &labels).increment(1);
}

/// Update cluster health metrics
pub fn update_cluster_health(total_nodes: usize, healthy_nodes: usize, replication_lag_ms: f64) {
    gauge!("rs3gw_cluster_nodes_total").set(total_nodes as f64);
    gauge!("rs3gw_cluster_healthy_nodes").set(healthy_nodes as f64);
    gauge!("rs3gw_cluster_replication_lag_ms").set(replication_lag_ms);
}

/// Record storage class transitions
pub fn record_storage_class_transition(from_class: &str, to_class: &str, size_bytes: u64) {
    let labels = [
        ("from_class", from_class.to_string()),
        ("to_class", to_class.to_string()),
    ];
    counter!("rs3gw_storage_class_transitions_total", &labels).increment(1);
    counter!("rs3gw_storage_class_transitioned_bytes", &labels).increment(size_bytes);
}

/// Record batch job metrics
pub fn record_batch_job(job_type: &str, status: &str, objects_processed: u64) {
    let labels = [
        ("job_type", job_type.to_string()),
        ("status", status.to_string()),
    ];
    counter!("rs3gw_batch_jobs_total", &labels).increment(1);
    counter!("rs3gw_batch_objects_processed", &labels).increment(objects_processed);
}

/// Record multipart upload metrics
pub fn record_multipart_upload(parts: usize, total_size_bytes: u64, duration_ms: f64) {
    histogram!("rs3gw_multipart_parts").record(parts as f64);
    histogram!("rs3gw_multipart_size_bytes").record(total_size_bytes as f64);
    histogram!("rs3gw_multipart_duration_ms").record(duration_ms);
}
