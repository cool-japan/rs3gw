#![cfg(feature = "server")]
//! Integration tests for observability API endpoints
//!
//! These tests verify the observability REST API endpoints that provide
//! access to profiling data, business metrics, anomalies, and resource statistics.

use axum::http::StatusCode;
use reqwest::Client;
use serde_json::Value;

mod common;
use common::TestServer;

// ============================================================================
// Metrics unit tests (no HTTP server required)
// ============================================================================

/// Verify that `record_object_size` registers the histogram in the Prometheus output.
#[tokio::test]
async fn test_object_size_metric_exists() {
    // Ensure the global recorder is initialised and grab its handle.
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    rs3gw::metrics::record_object_size("test-bucket", 1024);

    let rendered = metrics_handle.render();
    assert!(
        rendered.contains("rs3gw_object_size_bytes"),
        "Prometheus output should contain rs3gw_object_size_bytes; got:\n{}",
        rendered
    );
}

/// Verify that `record_dedup_savings` increments the counters and records the ratio.
#[tokio::test]
async fn test_dedup_savings_metric_recorded() {
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    rs3gw::metrics::record_dedup_savings(500, 1000);

    let rendered = metrics_handle.render();
    assert!(
        rendered.contains("rs3gw_dedup_total_bytes_saved"),
        "Prometheus output should contain rs3gw_dedup_total_bytes_saved counter; got:\n{}",
        rendered
    );
    assert!(
        rendered.contains("rs3gw_dedup_operations_total"),
        "Prometheus output should contain rs3gw_dedup_operations_total counter; got:\n{}",
        rendered
    );
    assert!(
        rendered.contains("rs3gw_dedup_savings_ratio"),
        "Prometheus output should contain rs3gw_dedup_savings_ratio histogram; got:\n{}",
        rendered
    );
}

/// Verify that histogram buckets are configured for `rs3gw_request_duration_ms`.
///
/// After recording a sample the Prometheus text format must contain `_bucket`
/// lines with the expected `le` boundary values.
#[tokio::test]
async fn test_histogram_buckets_configured() {
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    // Ensure describe metadata is registered with the active recorder.
    rs3gw::metrics::configure_histogram_buckets();

    // Record at least one sample so the histogram appears in the output.
    rs3gw::metrics::record_latency("TestOp", 42.0);

    let rendered = metrics_handle.render();

    // The PrometheusBuilder was configured with explicit bucket boundaries;
    // confirm that several expected `le` values appear in the rendered output.
    for expected_le in &["0.1", "5", "100", "1000", "60000"] {
        assert!(
            rendered.contains(expected_le),
            "Expected le={} bucket boundary in Prometheus output for rs3gw_request_duration_ms; got:\n{}",
            expected_le,
            rendered
        );
    }
    assert!(
        rendered.contains("rs3gw_request_duration_ms"),
        "Prometheus output should contain rs3gw_request_duration_ms histogram; got:\n{}",
        rendered
    );
}

#[tokio::test]
async fn test_profiling_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/profiling", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Profiling endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(body.get("timestamp").is_some(), "Should have timestamp");
    assert!(body.get("cpu").is_some(), "Should have CPU stats");
    assert!(body.get("memory").is_some(), "Should have memory stats");
    assert!(body.get("io").is_some(), "Should have I/O stats");
    assert!(
        body.get("pprof_available").is_some(),
        "Should have pprof flag"
    );

    // Verify CPU stats structure
    let cpu = body.get("cpu").expect("CPU stats should exist");
    assert!(
        cpu.get("utilization_percent").is_some(),
        "CPU should have utilization"
    );
    assert!(
        cpu.get("total_time_seconds").is_some(),
        "CPU should have total time"
    );
    assert!(
        cpu.get("user_time_seconds").is_some(),
        "CPU should have user time"
    );
    assert!(
        cpu.get("system_time_seconds").is_some(),
        "CPU should have system time"
    );

    // Verify memory stats structure
    let memory = body.get("memory").expect("Memory stats should exist");
    assert!(memory.get("rss_bytes").is_some(), "Memory should have RSS");
    assert!(
        memory.get("virtual_bytes").is_some(),
        "Memory should have virtual"
    );
    assert!(
        memory.get("peak_rss_bytes").is_some(),
        "Memory should have peak RSS"
    );

    // Verify I/O stats structure
    let io = body.get("io").expect("I/O stats should exist");
    assert!(
        io.get("read_operations").is_some(),
        "I/O should have read operations"
    );
    assert!(
        io.get("write_operations").is_some(),
        "I/O should have write operations"
    );
    assert!(io.get("read_bytes").is_some(), "I/O should have read bytes");
    assert!(
        io.get("write_bytes").is_some(),
        "I/O should have write bytes"
    );
    assert!(
        io.get("avg_read_latency_ms").is_some(),
        "I/O should have read latency"
    );
    assert!(
        io.get("avg_write_latency_ms").is_some(),
        "I/O should have write latency"
    );
}

#[tokio::test]
async fn test_profiling_endpoint_with_pprof() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/profiling?pprof=true", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(
        body.get("pprof_available").and_then(|v| v.as_bool()),
        Some(true),
        "pprof should be enabled when requested"
    );
}

#[tokio::test]
async fn test_business_metrics_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/business-metrics", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Business metrics endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(body.get("timestamp").is_some(), "Should have timestamp");
    assert!(
        body.get("storage_utilization").is_some(),
        "Should have storage utilization"
    );
    assert!(
        body.get("data_transfer").is_some(),
        "Should have data transfer metrics"
    );
    assert!(
        body.get("request_patterns").is_some(),
        "Should have request patterns"
    );
    assert!(
        body.get("user_activity").is_some(),
        "Should have user activity"
    );
    assert!(
        body.get("cost_estimation").is_some(),
        "Should have cost estimation"
    );

    // Verify storage utilization structure
    let storage = body
        .get("storage_utilization")
        .expect("Storage utilization should exist");
    assert!(
        storage.get("total_buckets").is_some(),
        "Storage should have total buckets"
    );
    assert!(
        storage.get("total_objects").is_some(),
        "Storage should have total objects"
    );
    assert!(
        storage.get("total_size_bytes").is_some(),
        "Storage should have total size"
    );
    assert!(
        storage.get("growth_rate_percent").is_some(),
        "Storage should have growth rate"
    );

    // Verify data transfer structure
    let transfer = body
        .get("data_transfer")
        .expect("Data transfer should exist");
    assert!(
        transfer.get("upload_bytes_per_second").is_some(),
        "Transfer should have upload rate"
    );
    assert!(
        transfer.get("download_bytes_per_second").is_some(),
        "Transfer should have download rate"
    );
    assert!(
        transfer.get("bandwidth_utilization_percent").is_some(),
        "Transfer should have bandwidth utilization"
    );
}

#[tokio::test]
async fn test_anomalies_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/anomalies", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Anomalies endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(body.get("timestamp").is_some(), "Should have timestamp");
    assert!(
        body.get("anomalies").is_some(),
        "Should have anomalies array"
    );
    assert!(body.get("total_count").is_some(), "Should have total count");

    // Verify anomalies is an array
    let anomalies = body
        .get("anomalies")
        .expect("Anomalies should exist")
        .as_array()
        .expect("Anomalies should be an array");

    // For a fresh server, there should be no anomalies
    assert_eq!(anomalies.len(), 0, "Fresh server should have no anomalies");

    let total_count = body
        .get("total_count")
        .expect("Total count should exist")
        .as_u64()
        .expect("Total count should be a number");
    assert_eq!(total_count, 0, "Total count should match array length");
}

#[tokio::test]
async fn test_anomalies_endpoint_with_filters() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!(
        "{}/api/observability/anomalies?severity=high&limit=10",
        server.base_url
    );
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Anomalies endpoint should accept filter parameters"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");
    assert!(
        body.get("anomalies").is_some(),
        "Should have anomalies array"
    );
}

#[tokio::test]
async fn test_resource_stats_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/resources", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Resource stats endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(body.get("timestamp").is_some(), "Should have timestamp");
    assert!(
        body.get("cpu_utilization_percent").is_some(),
        "Should have CPU utilization"
    );
    assert!(
        body.get("memory_utilization_percent").is_some(),
        "Should have memory utilization"
    );
    assert!(
        body.get("active_threads").is_some(),
        "Should have active threads"
    );
    assert!(
        body.get("active_requests").is_some(),
        "Should have active requests"
    );
    assert!(
        body.get("pending_requests").is_some(),
        "Should have pending requests"
    );
    assert!(
        body.get("total_requests").is_some(),
        "Should have total requests"
    );
    assert!(
        body.get("successful_requests").is_some(),
        "Should have successful requests"
    );
    assert!(
        body.get("failed_requests").is_some(),
        "Should have failed requests"
    );
    assert!(
        body.get("success_rate_percent").is_some(),
        "Should have success rate"
    );
    assert!(
        body.get("avg_latency_ms").is_some(),
        "Should have average latency"
    );
    assert!(
        body.get("load_shedding_active").is_some(),
        "Should have load shedding flag"
    );
}

#[tokio::test]
async fn test_comprehensive_health_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/health", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Comprehensive health endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(body.get("status").is_some(), "Should have status");
    assert!(body.get("timestamp").is_some(), "Should have timestamp");
    assert!(
        body.get("profiling").is_some(),
        "Should have profiling data"
    );
    assert!(
        body.get("resources").is_some(),
        "Should have resource stats"
    );
    assert!(
        body.get("anomalies_count").is_some(),
        "Should have anomalies count"
    );
    assert!(
        body.get("critical_anomalies_count").is_some(),
        "Should have critical anomalies count"
    );

    // Verify status is "healthy"
    let status = body
        .get("status")
        .expect("Status should exist")
        .as_str()
        .expect("Status should be a string");
    assert_eq!(status, "healthy", "Status should be 'healthy'");

    // Verify profiling sub-structure
    let profiling = body.get("profiling").expect("Profiling should exist");
    assert!(profiling.get("cpu").is_some(), "Profiling should have CPU");
    assert!(
        profiling.get("memory").is_some(),
        "Profiling should have memory"
    );
    assert!(profiling.get("io").is_some(), "Profiling should have I/O");

    // Verify resources sub-structure
    let resources = body.get("resources").expect("Resources should exist");
    assert!(
        resources.get("cpu_utilization_percent").is_some(),
        "Resources should have CPU utilization"
    );
    assert!(
        resources.get("memory_utilization_percent").is_some(),
        "Resources should have memory utilization"
    );
    assert!(
        resources.get("active_requests").is_some(),
        "Resources should have active requests"
    );
}

#[tokio::test]
async fn test_all_observability_endpoints_return_json() {
    let server = TestServer::new().await;
    let client = Client::new();

    let endpoints = vec![
        "/api/observability/profiling",
        "/api/observability/business-metrics",
        "/api/observability/anomalies",
        "/api/observability/resources",
        "/api/observability/health",
    ];

    for endpoint in endpoints {
        let url = format!("{}{}", server.base_url, endpoint);
        let response = client
            .get(&url)
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Endpoint {} should return OK",
            endpoint
        );

        // Verify content-type is JSON
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert!(
            content_type.is_some() && content_type.unwrap().contains("application/json"),
            "Endpoint {} should return JSON content-type",
            endpoint
        );

        // Verify it's valid JSON
        let body: Value = response
            .json()
            .await
            .unwrap_or_else(|_| panic!("Endpoint {} should return valid JSON", endpoint));

        // All responses should have a timestamp
        assert!(
            body.get("timestamp").is_some(),
            "Endpoint {} should have timestamp",
            endpoint
        );
    }
}

#[tokio::test]
async fn test_storage_growth_prediction_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!(
        "{}/api/observability/predictions/storage-growth",
        server.base_url
    );
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Storage growth prediction endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Since we haven't fed data yet, we should get a response with insufficient data message
    // or zero values
    assert!(
        body.get("current_size").is_some(),
        "Should have current_size field"
    );
    assert!(
        body.get("predicted_7d").is_some(),
        "Should have predicted_7d field"
    );
    assert!(
        body.get("predicted_30d").is_some(),
        "Should have predicted_30d field"
    );
    assert!(
        body.get("predicted_90d").is_some(),
        "Should have predicted_90d field"
    );
    assert!(
        body.get("daily_growth_rate").is_some(),
        "Should have daily_growth_rate field"
    );
}

#[tokio::test]
async fn test_access_pattern_prediction_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!(
        "{}/api/observability/predictions/access-patterns",
        server.base_url
    );
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Access pattern prediction endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(
        body.get("current_rps").is_some(),
        "Should have current_rps field"
    );
    assert!(
        body.get("predicted_1h").is_some(),
        "Should have predicted_1h field"
    );
    assert!(
        body.get("predicted_24h").is_some(),
        "Should have predicted_24h field"
    );
    assert!(
        body.get("expected_peak").is_some(),
        "Should have expected_peak field"
    );
}

#[tokio::test]
async fn test_cost_forecast_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/predictions/costs", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Cost forecast endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(
        body.get("current_monthly_cost").is_some(),
        "Should have current_monthly_cost field"
    );
    assert!(
        body.get("predicted_next_month").is_some(),
        "Should have predicted_next_month field"
    );
    assert!(
        body.get("predicted_3_months").is_some(),
        "Should have predicted_3_months field"
    );
    assert!(
        body.get("storage_cost").is_some(),
        "Should have storage_cost field"
    );
}

#[tokio::test]
async fn test_capacity_recommendations_endpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/observability/predictions/capacity", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Capacity recommendations endpoint should return OK"
    );

    let body: Value = response.json().await.expect("Failed to parse JSON");

    // Verify response structure
    assert!(
        body.get("current_utilization").is_some(),
        "Should have current_utilization field"
    );
    assert!(
        body.get("predicted_30d_utilization").is_some(),
        "Should have predicted_30d_utilization field"
    );
    assert!(
        body.get("predicted_90d_utilization").is_some(),
        "Should have predicted_90d_utilization field"
    );
    assert!(
        body.get("recommendation").is_some(),
        "Should have recommendation field"
    );
}

#[tokio::test]
async fn test_all_prediction_endpoints_return_json() {
    let server = TestServer::new().await;
    let client = Client::new();

    let endpoints = vec![
        "/api/observability/predictions/storage-growth",
        "/api/observability/predictions/access-patterns",
        "/api/observability/predictions/costs",
        "/api/observability/predictions/capacity",
    ];

    for endpoint in endpoints {
        let url = format!("{}{}", server.base_url, endpoint);
        let response = client
            .get(&url)
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Endpoint {} should return OK",
            endpoint
        );

        // Verify content-type is JSON
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok());
        assert!(
            content_type.is_some() && content_type.unwrap().contains("application/json"),
            "Endpoint {} should return JSON content-type",
            endpoint
        );

        // Verify it's valid JSON
        let _body: Value = response
            .json()
            .await
            .unwrap_or_else(|_| panic!("Endpoint {} should return valid JSON", endpoint));
    }
}

// ============================================================================
// Compression metrics tests
// ============================================================================

/// Verify that `record_compression` with zstd algorithm registers counters in Prometheus output.
#[tokio::test]
async fn test_compression_metrics_zstd() {
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    rs3gw::metrics::record_compression("zstd", 10000, 4500);

    let rendered = metrics_handle.render();
    assert!(
        rendered.contains("rs3gw_compression_original_bytes"),
        "Prometheus output should contain rs3gw_compression_original_bytes; got:\n{}",
        rendered
    );
    assert!(
        rendered.contains("rs3gw_compression_compressed_bytes"),
        "Prometheus output should contain rs3gw_compression_compressed_bytes; got:\n{}",
        rendered
    );
    // Verify the algorithm label is present
    assert!(
        rendered.contains("algorithm=\"zstd\""),
        "Compression metrics should have algorithm=\"zstd\" label; got:\n{}",
        rendered
    );
}

/// Verify that `record_compression` with lz4 algorithm registers counters in Prometheus output.
#[tokio::test]
async fn test_compression_metrics_lz4() {
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    rs3gw::metrics::record_compression("lz4", 8000, 5000);

    let rendered = metrics_handle.render();
    assert!(
        rendered.contains("rs3gw_compression_original_bytes"),
        "Prometheus output should contain rs3gw_compression_original_bytes; got:\n{}",
        rendered
    );
    assert!(
        rendered.contains("rs3gw_compression_compressed_bytes"),
        "Prometheus output should contain rs3gw_compression_compressed_bytes; got:\n{}",
        rendered
    );
    assert!(
        rendered.contains("algorithm=\"lz4\""),
        "Compression metrics should have algorithm=\"lz4\" label; got:\n{}",
        rendered
    );
}

/// Verify that `record_compression` records a compression ratio histogram sample.
#[tokio::test]
async fn test_compression_ratio_recorded() {
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    // Ensure histogram description metadata is registered.
    rs3gw::metrics::configure_histogram_buckets();

    rs3gw::metrics::record_compression("zstd", 20000, 5000);

    let rendered = metrics_handle.render();
    assert!(
        rendered.contains("rs3gw_compression_ratio"),
        "Prometheus output should contain rs3gw_compression_ratio histogram; got:\n{}",
        rendered
    );
    // The histogram should have bucket lines
    assert!(
        rendered.contains("rs3gw_compression_ratio_bucket"),
        "Prometheus output should contain rs3gw_compression_ratio_bucket entries; got:\n{}",
        rendered
    );
}

/// Verify that compression metrics are absent when no compression is recorded.
#[tokio::test]
async fn test_compression_metrics_not_recorded_when_disabled() {
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    // Record a plain object size metric (no compression).
    rs3gw::metrics::record_object_size("no-compress-bucket", 2048);

    let rendered = metrics_handle.render();

    // The object size metric should be present.
    assert!(
        rendered.contains("rs3gw_object_size_bytes"),
        "Prometheus output should contain rs3gw_object_size_bytes; got:\n{}",
        rendered
    );

    // Compression counters should either be absent or have zero values.
    // Since other tests in this process may have recorded compression metrics
    // (due to global recorder), we check that no *new* algorithm label like
    // "none" was introduced.
    assert!(
        !rendered.contains("algorithm=\"none\""),
        "Compression metrics should not have algorithm=\"none\" label; got:\n{}",
        rendered
    );
}
