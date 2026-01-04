//! Integration tests for Query Intelligence API endpoints
//!
//! Tests all query intelligence endpoints:
//! - GET /api/query/intelligence/statistics
//! - GET /api/query/intelligence/summary
//! - POST /api/query/intelligence/predict-cost
//! - POST /api/query/intelligence/recommend-strategy
//! - POST /api/query/intelligence/find-similar
//! - GET /api/query/intelligence/index-recommendations
//! - GET /api/query/intelligence/complexity-distribution

mod common;

use common::TestServer;

/// Test getting query statistics (initially empty)
#[tokio::test]
async fn test_get_statistics_empty() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/query/intelligence/statistics",
            server.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["count"], 0);
    assert!(body["statistics"].is_array());
    assert!(body["timestamp"].is_string());
}

/// Test getting query summary (initially empty)
#[tokio::test]
async fn test_get_summary_empty() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/query/intelligence/summary",
            server.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["total_queries"], 0);
    assert!(body["avg_execution_time_ms"].is_number());
    assert!(body["timestamp"].is_string());
}

/// Test predicting query cost with simple query
#[tokio::test]
async fn test_predict_cost_simple_query() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "sql": "SELECT name, age FROM s3object WHERE age > 18",
        "data_statistics": {
            "total_rows": 10000,
            "total_bytes": 1024000,
            "avg_row_size": 102.4,
            "format": "CSV",
            "compression_ratio": null,
            "column_cardinality": {},
            "null_percentages": {},
            "is_sorted": false,
            "skew_factor": 0.1
        }
    });

    let response = client
        .post(format!(
            "{}/api/query/intelligence/predict-cost",
            server.base_url
        ))
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["cost"]["execution_time_ms"].is_number());
    assert!(body["cost"]["memory_bytes"].is_number());
    assert!(body["cost"]["confidence"].is_number());
    assert!(body["timestamp"].is_string());

    // Cost should be reasonable
    let exec_time = body["cost"]["execution_time_ms"].as_f64().unwrap();
    assert!(exec_time > 0.0);
}

/// Test predicting cost with complex query
#[tokio::test]
async fn test_predict_cost_complex_query() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "sql": "SELECT department, COUNT(*) as cnt, AVG(salary) as avg_sal FROM s3object WHERE age > 18 GROUP BY department ORDER BY avg_sal DESC LIMIT 10",
        "data_statistics": {
            "total_rows": 100000,
            "total_bytes": 10240000,
            "avg_row_size": 102.4,
            "format": "CSV",
            "compression_ratio": null,
            "column_cardinality": {},
            "null_percentages": {},
            "is_sorted": false,
            "skew_factor": 0.5
        }
    });

    let response = client
        .post(format!(
            "{}/api/query/intelligence/predict-cost",
            server.base_url
        ))
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");

    // Complex query should have higher cost
    let exec_time = body["cost"]["execution_time_ms"].as_f64().unwrap();
    assert!(exec_time > 0.0);

    // Should have breakdown
    assert!(body["cost"]["breakdown"].is_object());
    assert!(body["cost"]["breakdown"]["aggregation_cost"].is_number());
    assert!(body["cost"]["breakdown"]["sort_cost"].is_number());
}

/// Test invalid SQL query returns error
#[tokio::test]
async fn test_predict_cost_invalid_sql() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "sql": "INVALID SQL QUERY HERE",
        "data_statistics": {
            "total_rows": 1000,
            "total_bytes": 100000,
            "avg_row_size": 100.0,
            "format": "CSV",
            "compression_ratio": null,
            "column_cardinality": {},
            "null_percentages": {},
            "is_sorted": false,
            "skew_factor": 0.1
        }
    });

    let response = client
        .post(format!(
            "{}/api/query/intelligence/predict-cost",
            server.base_url
        ))
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 400);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["error"].is_string());
    assert_eq!(body["error"], "Failed to parse SQL query");
}

/// Test recommending execution strategy
#[tokio::test]
async fn test_recommend_strategy() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "sql": "SELECT * FROM s3object WHERE age > 25",
        "data_statistics": {
            "total_rows": 50000,
            "total_bytes": 5120000,
            "avg_row_size": 102.4,
            "format": "Parquet",
            "compression_ratio": 0.3,
            "column_cardinality": {},
            "null_percentages": {},
            "is_sorted": true,
            "skew_factor": 0.2
        }
    });

    let response = client
        .post(format!(
            "{}/api/query/intelligence/recommend-strategy",
            server.base_url
        ))
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["strategy"].is_string());
    assert!(body["estimated_cost"]["execution_time_ms"].is_number());
    assert!(body["timestamp"].is_string());
}

/// Test finding similar queries (empty initially)
#[tokio::test]
async fn test_find_similar_empty() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "sql": "SELECT name FROM s3object",
        "threshold": 0.7
    });

    let response = client
        .post(format!(
            "{}/api/query/intelligence/find-similar",
            server.base_url
        ))
        .json(&request_body)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["count"], 0);
    assert!(body["similar_queries"].is_array());
    assert!(body["timestamp"].is_string());
}

/// Test getting index recommendations (empty initially)
#[tokio::test]
async fn test_get_index_recommendations_empty() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/query/intelligence/index-recommendations",
            server.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(body["count"], 0);
    assert!(body["recommendations"].is_array());
    assert!(body["timestamp"].is_string());
}

/// Test getting complexity distribution (empty initially)
#[tokio::test]
async fn test_get_complexity_distribution_empty() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!(
            "{}/api/query/intelligence/complexity-distribution",
            server.base_url
        ))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.expect("Failed to parse JSON");
    assert!(body["distribution"].is_object());
    assert_eq!(body["distribution"]["trivial"], 0);
    assert_eq!(body["distribution"]["simple"], 0);
    assert_eq!(body["distribution"]["moderate"], 0);
    assert_eq!(body["distribution"]["complex"], 0);
    assert_eq!(body["distribution"]["very_complex"], 0);
    assert!(body["timestamp"].is_string());
}

/// Test all query intelligence endpoints return JSON
#[tokio::test]
async fn test_all_endpoints_return_json() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();

    // Test GET endpoints
    let get_endpoints = vec![
        "/api/query/intelligence/statistics",
        "/api/query/intelligence/summary",
        "/api/query/intelligence/index-recommendations",
        "/api/query/intelligence/complexity-distribution",
    ];

    for endpoint in get_endpoints {
        let response = client
            .get(format!("{}{}", server.base_url, endpoint))
            .send()
            .await
            .expect("Failed to send request");

        assert_eq!(response.status(), 200);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
    }
}

/// Test cost prediction with different data sizes
#[tokio::test]
async fn test_predict_cost_scales_with_data_size() {
    let server = TestServer::new().await;

    let client = reqwest::Client::new();
    let sql = "SELECT * FROM s3object WHERE age > 18";

    // Small dataset
    let small_request = serde_json::json!({
        "sql": sql,
        "data_statistics": {
            "total_rows": 1000,
            "total_bytes": 100000,
            "avg_row_size": 100.0,
            "format": "CSV",
            "compression_ratio": null,
            "column_cardinality": {},
            "null_percentages": {},
            "is_sorted": false,
            "skew_factor": 0.1
        }
    });

    let small_response = client
        .post(format!(
            "{}/api/query/intelligence/predict-cost",
            server.base_url
        ))
        .json(&small_request)
        .send()
        .await
        .expect("Failed to send request");

    let small_body: serde_json::Value = small_response.json().await.expect("Failed to parse JSON");
    let small_cost = small_body["cost"]["execution_time_ms"].as_f64().unwrap();

    // Large dataset
    let large_request = serde_json::json!({
        "sql": sql,
        "data_statistics": {
            "total_rows": 1000000,
            "total_bytes": 100000000,
            "avg_row_size": 100.0,
            "format": "CSV",
            "compression_ratio": null,
            "column_cardinality": {},
            "null_percentages": {},
            "is_sorted": false,
            "skew_factor": 0.1
        }
    });

    let large_response = client
        .post(format!(
            "{}/api/query/intelligence/predict-cost",
            server.base_url
        ))
        .json(&large_request)
        .send()
        .await
        .expect("Failed to send request");

    let large_body: serde_json::Value = large_response.json().await.expect("Failed to parse JSON");
    let large_cost = large_body["cost"]["execution_time_ms"].as_f64().unwrap();

    // Large dataset should have higher cost
    assert!(
        large_cost > small_cost,
        "Large dataset should have higher cost than small dataset"
    );
}
