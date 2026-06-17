//! Metrics module
//!
//! Provides Prometheus metrics for monitoring rs3gw performance and health.

use std::collections::{HashMap, VecDeque};
#[cfg(feature = "server")]
use std::sync::Once;
use std::sync::{LazyLock, Mutex};
#[cfg(feature = "server")]
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "server")]
use axum::{body::Body, extract::Request, http::Method, middleware::Next, response::Response};
#[cfg(feature = "server")]
use metrics::describe_histogram;
#[cfg(feature = "server")]
use metrics::Unit;
use metrics::{counter, gauge, histogram};
#[cfg(feature = "server")]
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use serde::Serialize;

/// Global metrics initialization state
#[cfg(feature = "server")]
static METRICS_INIT: Once = Once::new();
#[cfg(feature = "server")]
static METRICS_HANDLE: Mutex<Option<PrometheusHandle>> = Mutex::new(None);

/// Histogram bucket boundaries for request duration (ms)
///
/// Buckets: 0.1ms, 1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s, 60s
#[cfg(feature = "server")]
const REQUEST_DURATION_BUCKETS: &[f64] = &[
    0.1, 1.0, 5.0, 10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 60000.0,
];

/// Histogram bucket boundaries for object sizes (bytes)
///
/// Buckets: 1KB, 64KB, 1MB, 10MB, 100MB, 1GB
#[cfg(feature = "server")]
const OBJECT_SIZE_BUCKETS: &[f64] = &[
    1024.0,
    65536.0,
    1_048_576.0,
    10_485_760.0,
    104_857_600.0,
    1_073_741_824.0,
];

/// Histogram bucket boundaries for dedup savings ratio (0.0 – 1.0)
#[cfg(feature = "server")]
const DEDUP_SAVINGS_RATIO_BUCKETS: &[f64] = &[0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

/// Histogram bucket boundaries for compression ratio (compressed/original, 0.0 – 1.0+)
#[cfg(feature = "server")]
const COMPRESSION_RATIO_BUCKETS: &[f64] = &[0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0, 1.25];

/// Maximum number of latency exemplars retained per operation (recent ring).
const MAX_EXEMPLARS_PER_OP: usize = 16;

/// A latency exemplar: a single request's latency correlated with the active
/// OpenTelemetry trace, so operators can jump from a slow-latency sample to its
/// distributed trace.
///
/// `metrics-exporter-prometheus` 0.18 cannot emit OpenMetrics exemplars inline
/// in the `/metrics` text, so exemplars are kept in this side store and exposed
/// via `/metrics/exemplars`. `trace_id` is empty when no sampled trace was active
/// (e.g. tracing disabled), which makes the latency samples useful even without
/// OpenTelemetry while populating trace IDs automatically when it is enabled.
#[derive(Debug, Clone, Serialize)]
pub struct Exemplar {
    pub operation: String,
    pub latency_ms: f64,
    pub status: u16,
    /// OpenTelemetry trace id (hex), or empty if no sampled trace was active.
    pub trace_id: String,
    pub timestamp_unix_ms: u64,
}

/// Recent latency exemplars, keyed by operation (bounded ring per operation).
static EXEMPLAR_STORE: LazyLock<Mutex<HashMap<String, VecDeque<Exemplar>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Extract the active OpenTelemetry `trace_id`, if a valid (sampled) span is current.
///
/// Returns `None` when no OpenTelemetry layer is installed or the current span has
/// no valid trace context (so exemplar trace IDs populate only when tracing is on).
#[cfg(feature = "server")]
fn current_trace_id() -> Option<String> {
    use opentelemetry::trace::TraceContextExt;
    use tracing_opentelemetry::OpenTelemetrySpanExt;

    let context = tracing::Span::current().context();
    let span = context.span();
    let span_context = span.span_context();
    if span_context.is_valid() {
        Some(span_context.trace_id().to_string())
    } else {
        None
    }
}

/// Record a latency exemplar for `operation`, correlated with `trace_id` if present.
///
/// Keeps the most recent `MAX_EXEMPLARS_PER_OP` exemplars per operation.
pub fn record_exemplar(operation: &str, latency_ms: f64, status: u16, trace_id: Option<String>) {
    let timestamp_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if let Ok(mut store) = EXEMPLAR_STORE.lock() {
        let samples = store.entry(operation.to_string()).or_default();
        samples.push_back(Exemplar {
            operation: operation.to_string(),
            latency_ms,
            status,
            trace_id: trace_id.unwrap_or_default(),
            timestamp_unix_ms,
        });
        while samples.len() > MAX_EXEMPLARS_PER_OP {
            samples.pop_front();
        }
    }
}

/// Snapshot of all retained latency exemplars (flattened across operations).
pub fn exemplars_snapshot() -> Vec<Exemplar> {
    EXEMPLAR_STORE
        .lock()
        .map(|store| store.values().flat_map(|dq| dq.iter().cloned()).collect())
        .unwrap_or_default()
}

/// Build a `PrometheusBuilder` pre-configured with explicit histogram bucket boundaries.
///
/// Centralises bucket configuration so both `init_metrics` and
/// `configure_histogram_buckets` (used in tests) share the same settings.
#[cfg(feature = "server")]
pub fn build_prometheus_builder() -> Result<PrometheusBuilder, Box<dyn std::error::Error>> {
    let builder = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("rs3gw_request_duration_ms".to_string()),
            REQUEST_DURATION_BUCKETS,
        )
        .map_err(|e| format!("Failed to set request_duration_ms buckets: {e}"))?
        .set_buckets_for_metric(
            Matcher::Full("rs3gw_object_size_bytes".to_string()),
            OBJECT_SIZE_BUCKETS,
        )
        .map_err(|e| format!("Failed to set object_size_bytes buckets: {e}"))?
        .set_buckets_for_metric(
            Matcher::Full("rs3gw_dedup_savings_ratio".to_string()),
            DEDUP_SAVINGS_RATIO_BUCKETS,
        )
        .map_err(|e| format!("Failed to set dedup_savings_ratio buckets: {e}"))?
        .set_buckets_for_metric(
            Matcher::Full("rs3gw_compression_ratio".to_string()),
            COMPRESSION_RATIO_BUCKETS,
        )
        .map_err(|e| format!("Failed to set compression_ratio buckets: {e}"))?;
    Ok(builder)
}

/// Initialize the Prometheus metrics recorder and return the handle
///
/// This function can be called multiple times safely. It will only initialize
/// the global metrics recorder once and return a cloned handle on subsequent calls.
#[cfg(feature = "server")]
pub fn init_metrics() -> Result<PrometheusHandle, Box<dyn std::error::Error>> {
    let mut init_error: Option<String> = None;

    METRICS_INIT.call_once(|| {
        match build_prometheus_builder() {
            Ok(builder) => match builder.install_recorder() {
                Ok(handle) => {
                    if let Ok(mut guard) = METRICS_HANDLE.lock() {
                        *guard = Some(handle);
                    }
                    configure_histogram_buckets();
                }
                Err(_) => {
                    // Metrics recorder already installed by another caller.
                    // The existing global recorder will be used; we cannot
                    // retrieve its handle here.
                }
            },
            Err(e) => {
                init_error = Some(e.to_string());
            }
        }
    });

    if let Some(e) = init_error {
        return Err(e.into());
    }

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

/// Configure histogram bucket documentation via `describe_histogram!`.
///
/// This is automatically called at the end of `init_metrics()`.  It can also
/// be called standalone in tests that build their own recorder so that the
/// description metadata is registered with whatever recorder is currently
/// installed.
///
/// Intended histogram buckets per metric:
/// - `rs3gw_request_duration_ms`:  [0.1, 1, 5, 10, 50, 100, 500, 1 000, 5 000, 60 000] ms
/// - `rs3gw_object_size_bytes`:    [1 024, 65 536, 1 048 576, 10 485 760, 104 857 600, 1 073 741 824] bytes
/// - `rs3gw_dedup_savings_ratio`:  [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]
/// - `rs3gw_compression_ratio`:    [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0, 1.25]
#[cfg(feature = "server")]
pub fn configure_histogram_buckets() {
    describe_histogram!(
        "rs3gw_request_duration_ms",
        Unit::Milliseconds,
        "Request latency histogram. Buckets: [0.1, 1, 5, 10, 50, 100, 500, 1000, 5000, 60000] ms"
    );
    describe_histogram!(
        "rs3gw_object_size_bytes",
        Unit::Bytes,
        "Object size distribution. Buckets: [1024, 65536, 1048576, 10485760, 104857600, 1073741824] bytes"
    );
    describe_histogram!(
        "rs3gw_dedup_savings_ratio",
        Unit::Count,
        "Dedup savings ratio (bytes_saved/original_bytes). Buckets: [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]"
    );
    describe_histogram!(
        "rs3gw_compression_ratio",
        Unit::Count,
        "Compression ratio (compressed/original). Buckets: [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0, 1.25]"
    );
}

/// Metrics middleware layer
///
/// Records Prometheus metrics and optionally feeds the MetricsTracker if state is provided
#[cfg(feature = "server")]
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
    // `metrics_layer` runs inside `TraceLayer`'s instrumented future, so the request
    // span is current here and its trace_id (if sampled) can be correlated with this
    // latency sample.
    record_exemplar(&operation, latency_ms, status, current_trace_id());

    response
}

/// Metrics tracker middleware - records request/bandwidth metrics for predictive analytics
///
/// This middleware should be added after metrics_layer in the middleware stack
#[cfg(feature = "server")]
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
#[cfg(feature = "server")]
fn has_query_param(query: Option<&str>, param: &str) -> bool {
    query.is_some_and(|q| {
        q.split('&')
            .any(|p| p == param || p.starts_with(&format!("{}=", param)))
    })
}

/// Classify S3 operation from method, path, and query parameters
#[cfg(feature = "server")]
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

/// Record object size distribution
///
/// Intended histogram buckets (bytes):
/// `[1024, 65536, 1_048_576, 10_485_760, 104_857_600, 1_073_741_824]`
pub fn record_object_size(bucket: &str, size_bytes: u64) {
    histogram!("rs3gw_object_size_bytes", "bucket" => bucket.to_string()).record(size_bytes as f64);
}

/// Record deduplication savings
///
/// Increments total bytes saved and operation counters, and records the savings
/// ratio (`bytes_saved / original_bytes`) in a histogram when `original_bytes > 0`.
pub fn record_dedup_savings(bytes_saved: u64, original_bytes: u64) {
    counter!("rs3gw_dedup_total_bytes_saved").increment(bytes_saved);
    counter!("rs3gw_dedup_operations_total").increment(1);
    if original_bytes > 0 {
        let ratio = bytes_saved as f64 / original_bytes as f64;
        histogram!("rs3gw_dedup_savings_ratio").record(ratio);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exemplar_store_records_and_evicts_oldest() {
        // Unique op name so the process-global store isn't shared with other tests.
        let op = "UnitTestExemplarEvict";
        for i in 0..(MAX_EXEMPLARS_PER_OP + 5) {
            record_exemplar(op, i as f64, 200, None);
        }
        let snap: Vec<Exemplar> = exemplars_snapshot()
            .into_iter()
            .filter(|e| e.operation == op)
            .collect();
        assert_eq!(
            snap.len(),
            MAX_EXEMPLARS_PER_OP,
            "ring must cap exemplars per operation"
        );
        // `None` trace_id is stored as an empty string.
        assert!(snap.iter().all(|e| e.trace_id.is_empty()));
        // The oldest 5 (latencies 0..5) must have been evicted.
        let min_latency = snap.iter().map(|e| e.latency_ms as u64).min().unwrap();
        assert_eq!(min_latency, 5, "oldest exemplars evicted first");
    }

    #[test]
    fn exemplar_records_trace_id_and_status() {
        let op = "UnitTestExemplarTrace";
        record_exemplar(op, 3.0, 404, Some("deadbeef".to_string()));
        let snap: Vec<Exemplar> = exemplars_snapshot()
            .into_iter()
            .filter(|e| e.operation == op)
            .collect();
        assert!(snap
            .iter()
            .any(|e| e.trace_id == "deadbeef" && e.status == 404 && e.latency_ms == 3.0));
    }
}
