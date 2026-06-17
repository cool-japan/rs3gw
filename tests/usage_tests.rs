#![cfg(feature = "server")]
//! Integration tests for the cost/usage reporting endpoints (`/api/usage`).
//!
//! Drives real S3 operations through the gateway and asserts the per-bucket
//! usage report reflects them: cumulative transfer bytes and request counts
//! (from the usage tracker) plus live storage size/object count (read fresh)
//! and an estimated cost breakdown.

mod common;

use aws_sdk_s3::primitives::ByteStream;
use common::setup_test_server;

/// Find the usage entry for `bucket` within a `/api/usage` report body.
fn bucket_entry<'a>(report: &'a serde_json::Value, bucket: &str) -> &'a serde_json::Value {
    report["buckets"]
        .as_array()
        .expect("buckets array")
        .iter()
        .find(|b| b["bucket"] == bucket)
        .unwrap_or_else(|| panic!("bucket {} not found in usage report", bucket))
}

#[tokio::test]
async fn test_usage_report_reflects_operations() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("usage-{}", uuid::Uuid::new_v4());
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");

    // PUT obj1 (1000 bytes) and obj2 (2000 bytes).
    let obj1 = vec![1u8; 1000];
    let obj2 = vec![2u8; 2000];
    client
        .put_object()
        .bucket(&bucket)
        .key("obj1")
        .body(ByteStream::from(obj1.clone()))
        .send()
        .await
        .expect("put obj1");
    client
        .put_object()
        .bucket(&bucket)
        .key("obj2")
        .body(ByteStream::from(obj2.clone()))
        .send()
        .await
        .expect("put obj2");

    // GET obj1 (downloads 1000 bytes).
    let got = client
        .get_object()
        .bucket(&bucket)
        .key("obj1")
        .send()
        .await
        .expect("get obj1");
    let _ = got.body.collect().await.expect("collect obj1");

    // DELETE obj2.
    client
        .delete_object()
        .bucket(&bucket)
        .key("obj2")
        .send()
        .await
        .expect("delete obj2");

    // --- /api/usage (full report) ---
    let resp = http
        .get(format!("{}/api/usage", server.base_url))
        .send()
        .await
        .expect("GET /api/usage");
    assert_eq!(resp.status(), 200);
    let report: serde_json::Value = resp.json().await.expect("usage report json");

    let entry = bucket_entry(&report, &bucket);
    // Live storage: obj2 deleted, only obj1 (1000 bytes) remains.
    assert_eq!(entry["storage_bytes"], 1000, "storage_bytes (live)");
    assert_eq!(entry["object_count"], 1, "object_count (live)");
    // Cumulative transfer: uploaded 1000 + 2000, downloaded 1000.
    assert_eq!(entry["bytes_uploaded"], 3000, "bytes_uploaded");
    assert_eq!(entry["bytes_downloaded"], 1000, "bytes_downloaded");
    // Request counts by operation.
    assert_eq!(entry["requests_by_op"]["PUT"], 2, "PUT count");
    assert_eq!(entry["requests_by_op"]["GET"], 1, "GET count");
    assert_eq!(entry["requests_by_op"]["DELETE"], 1, "DELETE count");
    assert_eq!(entry["total_requests"], 4, "total_requests");
    // Cost breakdown is present and non-negative.
    let total_cost = entry["estimated_cost"]["total_usd"]
        .as_f64()
        .expect("total_usd");
    assert!(total_cost >= 0.0, "estimated cost must be non-negative");

    // Report-level totals.
    assert_eq!(report["total_objects"], 1, "report total_objects");
    assert_eq!(
        report["total_storage_bytes"], 1000,
        "report total_storage_bytes"
    );
    assert!(
        report["generated_at"].as_str().is_some(),
        "generated_at present"
    );

    // --- /api/usage/{bucket} (single bucket) ---
    let resp = http
        .get(format!("{}/api/usage/{}", server.base_url, bucket))
        .send()
        .await
        .expect("GET /api/usage/{bucket}");
    assert_eq!(resp.status(), 200);
    let single: serde_json::Value = resp.json().await.expect("single usage json");
    assert_eq!(single["bucket"], bucket);
    assert_eq!(single["storage_bytes"], 1000);
    assert_eq!(single["bytes_uploaded"], 3000);
    assert_eq!(single["requests_by_op"]["PUT"], 2);

    // --- /api/usage/{nonexistent} → 404 ---
    let resp = http
        .get(format!("{}/api/usage/no-such-bucket-xyz", server.base_url))
        .send()
        .await
        .expect("GET nonexistent bucket usage");
    assert_eq!(resp.status(), 404, "unknown bucket must 404");
}

#[tokio::test]
async fn test_usage_flush_fires_hooks() {
    // ?flush=true must still return a valid report (and, server-side, invoke the
    // registered logging hook). We assert the report shape; the hook side effect
    // is exercised by the unit test in src/observability/usage.rs.
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("usage-flush-{}", uuid::Uuid::new_v4());
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket");
    client
        .put_object()
        .bucket(&bucket)
        .key("k")
        .body(ByteStream::from(vec![7u8; 512]))
        .send()
        .await
        .expect("put");

    let resp = http
        .get(format!("{}/api/usage?flush=true", server.base_url))
        .send()
        .await
        .expect("GET /api/usage?flush=true");
    assert_eq!(resp.status(), 200);
    let report: serde_json::Value = resp.json().await.expect("json");
    let entry = bucket_entry(&report, &bucket);
    assert_eq!(entry["bytes_uploaded"], 512);
    assert_eq!(entry["requests_by_op"]["PUT"], 1);
}
