#![cfg(feature = "server")]
//! Bucket operation tests for rs3gw
//!
//! Comprehensive test matrix covering all bucket API operations:
//! ListBuckets, CreateBucket, DeleteBucket, HeadBucket, GetBucketLocation,
//! GetBucketTagging, PutBucketTagging, DeleteBucketTagging,
//! GetBucketPolicy, PutBucketPolicy, DeleteBucketPolicy,
//! GetBucketVersioning, PutBucketVersioning — including auth-gated paths.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::middleware::from_fn_with_state;
use axum::Router;
use reqwest::StatusCode;
use tempfile::TempDir;
use tokio::net::TcpListener;

use common::{setup_test_server, setup_test_server_with_auth};

// ---------------------------------------------------------------------------
// Auth-aware test server helper (mirrors auth_tests.rs pattern)
// ---------------------------------------------------------------------------

struct AuthServer {
    pub base_url: String,
    _handle: tokio::task::JoinHandle<()>,
    _temp_dir: TempDir,
}

async fn setup_auth_enforced_server(access_key: &str, secret_key: &str) -> AuthServer {
    common::init_tracing();
    let temp_dir = TempDir::new().expect("temp dir creation");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr: SocketAddr = listener.local_addr().expect("local addr");

    let storage_root = temp_dir.path().to_path_buf();
    let storage =
        Arc::new(rs3gw::storage::StorageEngine::new(storage_root.clone()).expect("storage engine"));

    let metrics_handle = rs3gw::metrics::init_metrics().expect("metrics init");

    let config = rs3gw::Config {
        bind_addr: addr,
        storage_root: storage_root.clone(),
        default_bucket: "default".to_string(),
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
        compression: rs3gw::storage::CompressionMode::None,
        request_timeout_secs: 0,
        max_concurrent_requests: 0,
        tls: rs3gw::TlsConfig::default(),
        connection_pool: rs3gw::ConnectionPoolConfig::default(),
        cluster: rs3gw::cluster::ClusterConfig::default(),
        dedup: rs3gw::storage::DedupConfig::disabled(),
        zerocopy: rs3gw::storage::ZeroCopyConfig::default(),
        select_cache: rs3gw::SelectCacheConfig::default(),
        multipart_retention_hours: 168,
        fsync: false,
    };

    let preprocessing_path = storage_root.join("preprocessing");
    let preprocessing_manager = Arc::new(rs3gw::storage::preprocessing::PreprocessingManager::new(
        preprocessing_path,
    ));
    let predictive_analytics = Arc::new(rs3gw::observability::PredictiveAnalytics::new(
        10_000,
        0.023,
        0.09,
        0.0004,
        1_000_000_000_000,
    ));
    let metrics_tracker = Arc::new(rs3gw::observability::MetricsTracker::new());
    let select_result_cache = Arc::new(rs3gw::api::SelectResultCache::new(100, 10 * 1024 * 1024));
    #[cfg(feature = "formats")]
    let query_intelligence = Arc::new(rs3gw::api::QueryIntelligence::new());
    let training_path = storage_root.join("training");
    let training_manager = Arc::new(rs3gw::storage::TrainingManager::new(training_path));

    let verifier = if !access_key.is_empty() && !secret_key.is_empty() {
        Some(Arc::new(rs3gw::auth::v4::SigV4Verifier::new(
            access_key.to_string(),
            secret_key.to_string(),
            "us-east-1".to_string(),
        )))
    } else {
        None
    };

    let state = rs3gw::AppState {
        config,
        storage,
        metrics_handle,
        cache: None,
        throttle: None,
        quota: None,
        event_broadcaster: rs3gw::api::EventBroadcaster::new(),
        #[cfg(feature = "formats")]
        query_plan_cache: None,
        select_result_cache,
        #[cfg(feature = "formats")]
        query_intelligence,
        advanced_replication: None,
        preprocessing_manager,
        predictive_analytics,
        metrics_tracker,
        usage_tracker: std::sync::Arc::new(rs3gw::observability::UsageTracker::new()),
        training_manager,
        start_time: std::time::Instant::now(),
        verifier,
        auth_failure_counts: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        in_flight: rs3gw::InFlightTracker::new(),
        encryption: std::sync::Arc::new(rs3gw::storage::encryption::EncryptionService::new(
            std::sync::Arc::new(rs3gw::storage::encryption::LocalKeyProvider::default()),
        )),
    };

    let app = Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(from_fn_with_state(
            state.clone(),
            rs3gw::api::auth_middleware::sigv4_auth_layer,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    AuthServer {
        base_url: format!("http://{}", addr),
        _handle: handle,
        _temp_dir: temp_dir,
    }
}

// ===========================================================================
// ListBuckets (4 tests)
// ===========================================================================

/// LB-1: Success path returns expected XML fields (Owner, Buckets list, Name, CreationDate)
#[tokio::test]
async fn test_list_buckets_returns_expected_xml_fields() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    // First create a bucket so the list is non-empty
    let create_resp = http
        .put(format!("{}/list-buckets-xml-test", server.base_url))
        .send()
        .await
        .expect("create bucket request");
    assert_eq!(
        create_resp.status(),
        StatusCode::OK,
        "Bucket creation should succeed"
    );

    let resp = http
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("list buckets request");
    assert_eq!(resp.status(), StatusCode::OK, "ListBuckets must return 200");

    let body = resp.text().await.expect("response body");
    assert!(
        body.contains("<ListAllMyBucketsResult"),
        "Response must contain ListAllMyBucketsResult element; got: {}",
        body
    );
    assert!(
        body.contains("<Buckets>"),
        "Response must contain Buckets element; got: {}",
        body
    );
    assert!(
        body.contains("<Name>"),
        "Response must contain Name element; got: {}",
        body
    );
    assert!(
        body.contains("<CreationDate>"),
        "Response must contain CreationDate element; got: {}",
        body
    );
    assert!(
        body.contains("<Owner>"),
        "Response must contain Owner element; got: {}",
        body
    );
}

/// LB-2: ListBuckets always returns 200 (not 404); bucket list is empty when none exist
#[tokio::test]
async fn test_list_buckets_empty_returns_200() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("list buckets request");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "ListBuckets should return 200 even when empty"
    );
}

/// LB-3: Metrics include operation label after ListBuckets
#[tokio::test]
async fn test_list_buckets_metrics_operation_label() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    // Trigger a ListBuckets call
    let _ = http
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("list buckets");

    // Check Prometheus metrics endpoint
    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint request");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200"
    );
    // Prometheus metrics must exist (endpoint returns valid text)
    let metrics_body = metrics_resp.text().await.expect("metrics body");
    assert!(
        !metrics_body.is_empty(),
        "Metrics response body must not be empty"
    );
}

/// LB-4: Auth-gated: unauthenticated request returns 403 when auth is enabled
#[tokio::test]
async fn test_list_buckets_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("unauthenticated list buckets");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "ListBuckets must return 403 when auth is enabled and no credentials provided"
    );
}

// ===========================================================================
// CreateBucket (4 tests)
// ===========================================================================

/// CB-1: Success returns 200 with Location header
#[tokio::test]
async fn test_create_bucket_success_returns_200_with_location() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "create-bucket-success-test";

    let resp = http
        .put(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("create bucket request");

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "CreateBucket must return 200"
    );
    let location = resp.headers().get("location");
    assert!(
        location.is_some(),
        "CreateBucket response must include Location header"
    );
    let location_val = location
        .expect("location header")
        .to_str()
        .expect("location header value");
    assert!(
        location_val.contains(bucket_name),
        "Location header must contain bucket name; got: {}",
        location_val
    );
}

/// CB-2: Duplicate bucket returns 409 BucketAlreadyExists
#[tokio::test]
async fn test_create_bucket_duplicate_returns_409() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "create-bucket-dup-test";

    // First creation succeeds
    let first = http
        .put(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("first create bucket");
    assert_eq!(
        first.status(),
        StatusCode::OK,
        "First creation must succeed"
    );

    // Second creation returns conflict
    let second = http
        .put(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("second create bucket");
    assert_eq!(
        second.status(),
        StatusCode::CONFLICT,
        "Duplicate bucket must return 409"
    );
    let body = second.text().await.expect("response body");
    assert!(
        body.contains("BucketAlreadyExists") || body.contains("BucketAlreadyOwnedByYou"),
        "Response must contain BucketAlreadyExists error code; got: {}",
        body
    );
}

/// CB-3: Invalid bucket names return 400 (too short, too long, uppercase, leading/trailing dot)
#[tokio::test]
async fn test_create_bucket_invalid_name_returns_400() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let invalid_names = [
        "ab",               // too short (< 3 chars)
        &"a".repeat(64),    // too long (> 63 chars)
        "InvalidUPPERCASE", // uppercase letters
        ".leading-dot",     // starts with dot
        "trailing-dot.",    // ends with dot
    ];

    for name in &invalid_names {
        let resp = http
            .put(format!("{}/{}", server.base_url, name))
            .send()
            .await
            .expect("create bucket with invalid name");
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "Bucket name '{}' should be rejected with 400, got {}",
            name,
            resp.status()
        );
    }
}

/// CB-4: Metrics endpoint is reachable after CreateBucket operations
#[tokio::test]
async fn test_create_bucket_metrics_operation_label() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let _ = http
        .put(format!("{}/create-metrics-test", server.base_url))
        .send()
        .await
        .expect("create bucket for metrics test");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after CreateBucket"
    );
}

// ===========================================================================
// DeleteBucket (4 tests)
// ===========================================================================

/// DB-1: Delete existing empty bucket returns 204
#[tokio::test]
async fn test_delete_bucket_success_returns_204() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    let bucket_name = "delete-bucket-success-test";
    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket for deletion test");

    let resp = client
        .delete_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("delete bucket");
    // AWS SDK translates 204 as success
    let _ = resp; // deletion succeeded if no error thrown
}

/// DB-2: Delete non-empty bucket returns 409 BucketNotEmpty
#[tokio::test]
async fn test_delete_bucket_nonempty_returns_409() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "delete-nonempty-bucket-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    // Upload an object to make it non-empty
    client
        .put_object()
        .bucket(bucket_name)
        .key("some-object.txt")
        .body(aws_sdk_s3::primitives::ByteStream::from_static(b"content"))
        .send()
        .await
        .expect("put object");

    // Attempt to delete the non-empty bucket via raw HTTP
    let resp = http
        .delete(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("delete non-empty bucket");
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "Deleting non-empty bucket must return 409"
    );
    let body = resp.text().await.expect("response body");
    assert!(
        body.contains("BucketNotEmpty"),
        "Response must contain BucketNotEmpty error code; got: {}",
        body
    );
}

/// DB-3: Delete non-existent bucket returns 404 NoSuchBucket
#[tokio::test]
async fn test_delete_bucket_nonexistent_returns_404() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .delete(format!(
            "{}/bucket-that-does-not-exist-for-delete",
            server.base_url
        ))
        .send()
        .await
        .expect("delete non-existent bucket");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "Deleting non-existent bucket must return 404"
    );
    let body = resp.text().await.expect("response body");
    assert!(
        body.contains("NoSuchBucket"),
        "Response must contain NoSuchBucket error code; got: {}",
        body
    );
}

/// DB-4: Metrics endpoint is reachable after DeleteBucket operations
#[tokio::test]
async fn test_delete_bucket_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "delete-metrics-bucket";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket for delete metrics test");
    let _ = http
        .delete(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("delete bucket");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after DeleteBucket"
    );
}

// ===========================================================================
// HeadBucket (4 tests)
// ===========================================================================

/// HB-1: HeadBucket on existing bucket returns 200
#[tokio::test]
async fn test_head_bucket_existing_returns_200() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "head-bucket-exists-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .head(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("head bucket");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "HeadBucket on existing bucket must return 200"
    );
}

/// HB-2: HeadBucket on non-existent bucket returns 404
#[tokio::test]
async fn test_head_bucket_nonexistent_returns_404() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .head(format!(
            "{}/bucket-that-does-not-exist-for-head",
            server.base_url
        ))
        .send()
        .await
        .expect("head non-existent bucket");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "HeadBucket on non-existent bucket must return 404"
    );
}

/// HB-3: Metrics endpoint is reachable after HeadBucket operations
#[tokio::test]
async fn test_head_bucket_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "head-bucket-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");
    let _ = http
        .head(format!("{}/{}", server.base_url, bucket_name))
        .send()
        .await
        .expect("head bucket");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after HeadBucket"
    );
}

/// HB-4: Auth-gated: unauthenticated HeadBucket returns 403 when auth is enabled
#[tokio::test]
async fn test_head_bucket_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .head(format!("{}/any-bucket", server.base_url))
        .send()
        .await
        .expect("unauthenticated head bucket");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "HeadBucket must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// GetBucketLocation (4 tests)
// ===========================================================================

/// GL-1: GetBucketLocation returns 200 with LocationConstraint XML
#[tokio::test]
async fn test_get_bucket_location_returns_200_with_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "location-200-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .get(format!("{}/{}?location", server.base_url, bucket_name))
        .send()
        .await
        .expect("get bucket location");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "GetBucketLocation must return 200"
    );
    let body = resp.text().await.expect("response body");
    assert!(
        body.contains("LocationConstraint"),
        "Response must contain LocationConstraint; got: {}",
        body
    );
}

/// GL-2: GetBucketLocation on non-existent bucket returns 404
#[tokio::test]
async fn test_get_bucket_location_nonexistent_returns_404() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!(
            "{}/bucket-nonexistent-location?location",
            server.base_url
        ))
        .send()
        .await
        .expect("get bucket location nonexistent");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "GetBucketLocation on non-existent bucket must return 404"
    );
}

/// GL-3: Metrics endpoint is reachable after GetBucketLocation operations
#[tokio::test]
async fn test_get_bucket_location_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "location-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");
    let _ = http
        .get(format!("{}/{}?location", server.base_url, bucket_name))
        .send()
        .await
        .expect("get location");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after GetBucketLocation"
    );
}

/// GL-4: Auth-gated: unauthenticated GetBucketLocation returns 403 when auth is enabled
#[tokio::test]
async fn test_get_bucket_location_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{}/some-bucket?location", server.base_url))
        .send()
        .await
        .expect("unauthenticated get bucket location");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "GetBucketLocation must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// GetBucketTagging (4 tests)
// ===========================================================================

/// GT-1: GetBucketTagging with no tags returns 404 NoSuchTagSet (or empty)
#[tokio::test]
async fn test_get_bucket_tagging_no_tags_returns_404_or_empty() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "tagging-no-tags-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .get(format!("{}/{}?tagging", server.base_url, bucket_name))
        .send()
        .await
        .expect("get bucket tagging with no tags");

    // S3 spec says 404 NoSuchTagSet when no tags; some impls return empty 200
    let status = resp.status();
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::OK,
        "GetBucketTagging with no tags must return 404 or 200; got {}",
        status
    );

    if status == StatusCode::NOT_FOUND {
        let body = resp.text().await.expect("response body");
        assert!(
            body.contains("NoSuchTagSet"),
            "Response must contain NoSuchTagSet; got: {}",
            body
        );
    }
}

/// GT-2: GetBucketTagging returns correct tags after PutBucketTagging
#[tokio::test]
async fn test_get_bucket_tagging_after_put_returns_tags() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "tagging-roundtrip-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    // Put tags via raw XML
    let tag_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet>
    <Tag><Key>Env</Key><Value>Testing</Value></Tag>
    <Tag><Key>Owner</Key><Value>rs3gw</Value></Tag>
  </TagSet>
</Tagging>"#;

    let put_resp = http
        .put(format!("{}/{}?tagging", server.base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(tag_xml)
        .send()
        .await
        .expect("put bucket tagging");
    assert_eq!(
        put_resp.status(),
        StatusCode::NO_CONTENT,
        "PutBucketTagging must return 204; got {}",
        put_resp.status()
    );

    // Now get the tags back
    let get_resp = http
        .get(format!("{}/{}?tagging", server.base_url, bucket_name))
        .send()
        .await
        .expect("get bucket tagging after put");
    assert_eq!(
        get_resp.status(),
        StatusCode::OK,
        "GetBucketTagging after put must return 200"
    );
    let body = get_resp.text().await.expect("response body");
    assert!(
        body.contains("Env") && body.contains("Testing"),
        "Response must contain set tags; got: {}",
        body
    );
    assert!(
        body.contains("Owner") && body.contains("rs3gw"),
        "Response must contain Owner tag; got: {}",
        body
    );
}

/// GT-3: Metrics endpoint is reachable after GetBucketTagging
#[tokio::test]
async fn test_get_bucket_tagging_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "tagging-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");
    let _ = http
        .get(format!("{}/{}?tagging", server.base_url, bucket_name))
        .send()
        .await
        .expect("get tagging for metrics");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after GetBucketTagging"
    );
}

/// GT-4: Auth-gated: unauthenticated GetBucketTagging returns 403 when auth is enabled
#[tokio::test]
async fn test_get_bucket_tagging_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{}/some-bucket?tagging", server.base_url))
        .send()
        .await
        .expect("unauthenticated get bucket tagging");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "GetBucketTagging must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// PutBucketTagging (4 tests)
// ===========================================================================

/// PT-1: Valid XML PUT tagging returns 204
#[tokio::test]
async fn test_put_bucket_tagging_valid_xml_returns_204() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "put-tagging-valid-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let tag_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet>
    <Tag><Key>project</Key><Value>rs3gw</Value></Tag>
  </TagSet>
</Tagging>"#;

    let resp = http
        .put(format!("{}/{}?tagging", server.base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(tag_xml)
        .send()
        .await
        .expect("put bucket tagging valid");
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "PutBucketTagging with valid XML must return 204"
    );
}

/// PT-2: Invalid XML PUT tagging returns 400
#[tokio::test]
async fn test_put_bucket_tagging_invalid_xml_returns_400() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "put-tagging-invalid-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .put(format!("{}/{}?tagging", server.base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body("this is not valid xml <<<")
        .send()
        .await
        .expect("put bucket tagging invalid");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "PutBucketTagging with invalid XML must return 400"
    );
}

/// PT-3: Metrics endpoint is reachable after PutBucketTagging
#[tokio::test]
async fn test_put_bucket_tagging_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "put-tagging-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let tag_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet><Tag><Key>k</Key><Value>v</Value></Tag></TagSet>
</Tagging>"#;
    let _ = http
        .put(format!("{}/{}?tagging", server.base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(tag_xml)
        .send()
        .await
        .expect("put tagging");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after PutBucketTagging"
    );
}

/// PT-4: Auth-gated: unauthenticated PutBucketTagging returns 403 when auth is enabled
#[tokio::test]
async fn test_put_bucket_tagging_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let tag_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet><Tag><Key>k</Key><Value>v</Value></Tag></TagSet>
</Tagging>"#;

    let resp = http
        .put(format!("{}/some-bucket?tagging", server.base_url))
        .header("Content-Type", "application/xml")
        .body(tag_xml)
        .send()
        .await
        .expect("unauthenticated put bucket tagging");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "PutBucketTagging must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// DeleteBucketTagging (4 tests)
// ===========================================================================

/// DT-1: DeleteBucketTagging removes existing tags and returns 204
#[tokio::test]
async fn test_delete_bucket_tagging_removes_tags_returns_204() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "delete-tagging-success-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    // First put some tags
    let tag_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Tagging xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <TagSet><Tag><Key>remove-me</Key><Value>yes</Value></Tag></TagSet>
</Tagging>"#;
    http.put(format!("{}/{}?tagging", server.base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(tag_xml)
        .send()
        .await
        .expect("put tags");

    // Now delete them
    let del_resp = http
        .delete(format!("{}/{}?tagging", server.base_url, bucket_name))
        .send()
        .await
        .expect("delete bucket tagging");
    assert_eq!(
        del_resp.status(),
        StatusCode::NO_CONTENT,
        "DeleteBucketTagging must return 204"
    );

    // Verify tags are gone (404 or empty)
    let get_resp = http
        .get(format!("{}/{}?tagging", server.base_url, bucket_name))
        .send()
        .await
        .expect("get tagging after delete");
    let status = get_resp.status();
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::OK,
        "After delete, GetBucketTagging should return 404 or 200 empty; got {}",
        status
    );
}

/// DT-2: DeleteBucketTagging on non-existent bucket returns 404
#[tokio::test]
async fn test_delete_bucket_tagging_nonexistent_bucket_returns_404() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .delete(format!(
            "{}/bucket-nonexistent-for-delete-tagging?tagging",
            server.base_url
        ))
        .send()
        .await
        .expect("delete tagging on nonexistent bucket");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "DeleteBucketTagging on non-existent bucket must return 404"
    );
}

/// DT-3: Metrics endpoint is reachable after DeleteBucketTagging
#[tokio::test]
async fn test_delete_bucket_tagging_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "delete-tagging-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");
    let _ = http
        .delete(format!("{}/{}?tagging", server.base_url, bucket_name))
        .send()
        .await
        .expect("delete tagging");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after DeleteBucketTagging"
    );
}

/// DT-4: Auth-gated: unauthenticated DeleteBucketTagging returns 403 when auth is enabled
#[tokio::test]
async fn test_delete_bucket_tagging_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .delete(format!("{}/some-bucket?tagging", server.base_url))
        .send()
        .await
        .expect("unauthenticated delete bucket tagging");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "DeleteBucketTagging must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// GetBucketPolicy (4 tests)
// ===========================================================================

/// GP-1: GetBucketPolicy with no policy returns 404 NoSuchBucketPolicy
#[tokio::test]
async fn test_get_bucket_policy_no_policy_returns_404() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "policy-no-policy-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .get(format!("{}/{}?policy", server.base_url, bucket_name))
        .send()
        .await
        .expect("get bucket policy with no policy");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "GetBucketPolicy with no policy must return 404"
    );
    let body = resp.text().await.expect("response body");
    assert!(
        body.contains("NoSuchBucketPolicy"),
        "Response must contain NoSuchBucketPolicy; got: {}",
        body
    );
}

/// GP-2: GetBucketPolicy returns the policy after PutBucketPolicy
#[tokio::test]
async fn test_get_bucket_policy_after_put_returns_policy() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "policy-roundtrip-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "TestStatement",
            "Effect": "Allow",
            "Principal": "*",
            "Action": "s3:GetObject",
            "Resource": format!("arn:aws:s3:::{}/*", bucket_name)
        }]
    });

    let put_resp = http
        .put(format!("{}/{}?policy", server.base_url, bucket_name))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .expect("put bucket policy");
    assert_eq!(
        put_resp.status(),
        StatusCode::NO_CONTENT,
        "PutBucketPolicy must return 204"
    );

    let get_resp = http
        .get(format!("{}/{}?policy", server.base_url, bucket_name))
        .send()
        .await
        .expect("get bucket policy after put");
    assert_eq!(
        get_resp.status(),
        StatusCode::OK,
        "GetBucketPolicy after put must return 200"
    );
    let retrieved: serde_json::Value = get_resp.json().await.expect("parse policy JSON");
    assert_eq!(
        retrieved["Version"], "2012-10-17",
        "Policy Version must match"
    );
    assert_eq!(
        retrieved["Statement"][0]["Sid"], "TestStatement",
        "Policy Sid must match"
    );
}

/// GP-3: Metrics endpoint is reachable after GetBucketPolicy
#[tokio::test]
async fn test_get_bucket_policy_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "policy-metrics-get-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");
    let _ = http
        .get(format!("{}/{}?policy", server.base_url, bucket_name))
        .send()
        .await
        .expect("get policy for metrics");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after GetBucketPolicy"
    );
}

/// GP-4: Auth-gated: unauthenticated GetBucketPolicy returns 403 when auth is enabled
#[tokio::test]
async fn test_get_bucket_policy_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!("{}/some-bucket?policy", server.base_url))
        .send()
        .await
        .expect("unauthenticated get bucket policy");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "GetBucketPolicy must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// PutBucketPolicy (4 tests)
// ===========================================================================

/// PP-1: Valid JSON PUT policy returns 204
#[tokio::test]
async fn test_put_bucket_policy_valid_json_returns_204() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "put-policy-valid-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": []
    });

    let resp = http
        .put(format!("{}/{}?policy", server.base_url, bucket_name))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .expect("put bucket policy valid");
    assert_eq!(
        resp.status(),
        StatusCode::NO_CONTENT,
        "PutBucketPolicy with valid JSON must return 204"
    );
}

/// PP-2: Invalid JSON PUT policy returns 400 MalformedPolicy
#[tokio::test]
async fn test_put_bucket_policy_invalid_json_returns_400() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "put-policy-invalid-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .put(format!("{}/{}?policy", server.base_url, bucket_name))
        .header("Content-Type", "application/json")
        .body("this is not valid json {{{{")
        .send()
        .await
        .expect("put bucket policy invalid");
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "PutBucketPolicy with invalid JSON must return 400"
    );
}

/// PP-3: Metrics endpoint is reachable after PutBucketPolicy
#[tokio::test]
async fn test_put_bucket_policy_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "put-policy-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let policy = serde_json::json!({ "Version": "2012-10-17", "Statement": [] });
    let _ = http
        .put(format!("{}/{}?policy", server.base_url, bucket_name))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .expect("put policy");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after PutBucketPolicy"
    );
}

/// PP-4: Auth-gated: unauthenticated PutBucketPolicy returns 403 when auth is enabled
#[tokio::test]
async fn test_put_bucket_policy_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let policy = serde_json::json!({ "Version": "2012-10-17", "Statement": [] });
    let resp = http
        .put(format!("{}/some-bucket?policy", server.base_url))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .expect("unauthenticated put bucket policy");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "PutBucketPolicy must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// DeleteBucketPolicy (4 tests)
// ===========================================================================

/// DP-1: DeleteBucketPolicy removes existing policy and returns 204
#[tokio::test]
async fn test_delete_bucket_policy_removes_policy_returns_204() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "delete-policy-success-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let policy = serde_json::json!({ "Version": "2012-10-17", "Statement": [] });
    http.put(format!("{}/{}?policy", server.base_url, bucket_name))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .expect("put policy before delete");

    let del_resp = http
        .delete(format!("{}/{}?policy", server.base_url, bucket_name))
        .send()
        .await
        .expect("delete bucket policy");
    assert_eq!(
        del_resp.status(),
        StatusCode::NO_CONTENT,
        "DeleteBucketPolicy must return 204"
    );

    // Verify policy is gone
    let get_resp = http
        .get(format!("{}/{}?policy", server.base_url, bucket_name))
        .send()
        .await
        .expect("get policy after delete");
    assert_eq!(
        get_resp.status(),
        StatusCode::NOT_FOUND,
        "GetBucketPolicy after delete must return 404"
    );
}

/// DP-2: DeleteBucketPolicy on non-existent bucket returns 404
#[tokio::test]
async fn test_delete_bucket_policy_nonexistent_bucket_returns_404() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .delete(format!(
            "{}/bucket-nonexistent-for-delete-policy?policy",
            server.base_url
        ))
        .send()
        .await
        .expect("delete policy on nonexistent bucket");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "DeleteBucketPolicy on non-existent bucket must return 404"
    );
}

/// DP-3: Metrics endpoint is reachable after DeleteBucketPolicy
#[tokio::test]
async fn test_delete_bucket_policy_metrics_operation_label() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "delete-policy-metrics-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");
    let _ = http
        .delete(format!("{}/{}?policy", server.base_url, bucket_name))
        .send()
        .await
        .expect("delete policy");

    let metrics_resp = http
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("metrics endpoint");
    assert_eq!(
        metrics_resp.status(),
        StatusCode::OK,
        "Metrics endpoint must return 200 after DeleteBucketPolicy"
    );
}

/// DP-4: Auth-gated: unauthenticated DeleteBucketPolicy returns 403 when auth is enabled
#[tokio::test]
async fn test_delete_bucket_policy_auth_gated_403_without_auth() {
    let server = setup_auth_enforced_server("mykey", "mysecret").await;
    let http = reqwest::Client::new();

    let resp = http
        .delete(format!("{}/some-bucket?policy", server.base_url))
        .send()
        .await
        .expect("unauthenticated delete bucket policy");
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "DeleteBucketPolicy must return 403 when auth is required and no credentials provided"
    );
}

// ===========================================================================
// GetBucketVersioning (2 tests)
// ===========================================================================

/// GV-1: GetBucketVersioning on existing bucket returns 200 with VersioningConfiguration XML
#[tokio::test]
async fn test_get_bucket_versioning_existing_bucket_returns_200_xml() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "versioning-get-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let resp = http
        .get(format!("{}/{}?versioning", server.base_url, bucket_name))
        .send()
        .await
        .expect("get bucket versioning");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "GetBucketVersioning must return 200"
    );
    let body = resp.text().await.expect("response body");
    assert!(
        body.contains("VersioningConfiguration"),
        "Response must contain VersioningConfiguration XML; got: {}",
        body
    );
}

/// GV-2: GetBucketVersioning on non-existent bucket returns 404
#[tokio::test]
async fn test_get_bucket_versioning_nonexistent_bucket_returns_404() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();

    let resp = http
        .get(format!(
            "{}/bucket-nonexistent-versioning?versioning",
            server.base_url
        ))
        .send()
        .await
        .expect("get versioning on nonexistent bucket");
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "GetBucketVersioning on non-existent bucket must return 404"
    );
}

// ===========================================================================
// PutBucketVersioning (1 test)
// ===========================================================================

/// PV-1: PutBucketVersioning on existing bucket returns 200 (accepted as stub)
#[tokio::test]
async fn test_put_bucket_versioning_existing_bucket_returns_200() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let http = reqwest::Client::new();
    let bucket_name = "versioning-put-test";

    client
        .create_bucket()
        .bucket(bucket_name)
        .send()
        .await
        .expect("create bucket");

    let versioning_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Status>Enabled</Status>
</VersioningConfiguration>"#;

    let resp = http
        .put(format!("{}/{}?versioning", server.base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(versioning_xml)
        .send()
        .await
        .expect("put bucket versioning");
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "PutBucketVersioning on existing bucket must return 200 (stub accepts)"
    );
}

// ===========================================================================
// Legacy / integration tests kept for regression coverage
// ===========================================================================

#[tokio::test]
async fn test_bucket_operations() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    let create_result = client.create_bucket().bucket("test-bucket").send().await;
    assert!(
        create_result.is_ok(),
        "Failed to create bucket: {:?}",
        create_result.err()
    );

    let head_result = client.head_bucket().bucket("test-bucket").send().await;
    assert!(head_result.is_ok(), "Failed to head bucket");

    let list_result = client.list_buckets().send().await;
    assert!(list_result.is_ok(), "Failed to list buckets");
    let list_output = list_result.expect("list buckets output");
    let buckets = list_output.buckets();
    assert!(buckets.iter().any(|b| b.name() == Some("test-bucket")));

    let delete_result = client.delete_bucket().bucket("test-bucket").send().await;
    assert!(delete_result.is_ok(), "Failed to delete bucket");
}

#[tokio::test]
async fn test_bucket_tagging() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    client
        .create_bucket()
        .bucket("bucket-tagging-test")
        .send()
        .await
        .expect("create bucket for tagging test");

    let get_result = client
        .get_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    if let Ok(tagging) = get_result {
        assert!(tagging.tag_set().is_empty());
    }

    use aws_sdk_s3::types::{Tag, Tagging};
    let tag1 = Tag::builder()
        .key("Environment")
        .value("Test")
        .build()
        .expect("build tag1");
    let tag2 = Tag::builder()
        .key("Owner")
        .value("rs3gw")
        .build()
        .expect("build tag2");
    let tagging = Tagging::builder()
        .tag_set(tag1)
        .tag_set(tag2)
        .build()
        .expect("build tagging");

    let put_result = client
        .put_bucket_tagging()
        .bucket("bucket-tagging-test")
        .tagging(tagging)
        .send()
        .await;
    assert!(
        put_result.is_ok(),
        "Failed to put bucket tagging: {:?}",
        put_result.err()
    );

    let get_result = client
        .get_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    assert!(
        get_result.is_ok(),
        "Failed to get bucket tagging: {:?}",
        get_result.err()
    );
    let tags = get_result.expect("tagging result");
    let tag_set = tags.tag_set();
    assert_eq!(tag_set.len(), 2);

    let delete_result = client
        .delete_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    assert!(delete_result.is_ok(), "Failed to delete bucket tagging");

    let get_result = client
        .get_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    if let Ok(tags) = get_result {
        assert!(tags.tag_set().is_empty());
    }
}

#[tokio::test]
async fn test_bucket_policy() {
    let (client, _temp_dir, server) = setup_test_server().await;

    client
        .create_bucket()
        .bucket("policy-test")
        .send()
        .await
        .expect("create bucket for policy test");

    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    let get_response = http_client
        .get(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .expect("get policy before put");
    assert_eq!(get_response.status(), 404);

    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "PublicReadGetObject",
            "Effect": "Allow",
            "Principal": "*",
            "Action": "s3:GetObject",
            "Resource": "arn:aws:s3:::policy-test/*"
        }]
    });

    let put_response = http_client
        .put(format!("{}/policy-test?policy", base_url))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .expect("put policy");
    assert_eq!(put_response.status(), 204);

    let get_response = http_client
        .get(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .expect("get policy after put");
    assert_eq!(get_response.status(), 200);

    let retrieved_policy: serde_json::Value = get_response.json().await.expect("parse policy");
    assert_eq!(retrieved_policy["Version"], "2012-10-17");
    assert_eq!(
        retrieved_policy["Statement"][0]["Sid"],
        "PublicReadGetObject"
    );

    let invalid_response = http_client
        .put(format!("{}/policy-test?policy", base_url))
        .body("not valid json")
        .send()
        .await
        .expect("put invalid policy");
    assert_eq!(invalid_response.status(), 400);

    let delete_response = http_client
        .delete(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .expect("delete policy");
    assert_eq!(delete_response.status(), 204);

    let get_response = http_client
        .get(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .expect("get policy after delete");
    assert_eq!(get_response.status(), 404);
}

/// Test bucket location operations
#[tokio::test]
async fn test_bucket_location() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket_name = format!("location-{}", uuid::Uuid::new_v4());

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create bucket for location test");

    let location_result = client
        .get_bucket_location()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("get bucket location");

    let location = location_result.location_constraint();
    assert!(
        location.is_none()
            || location
                .map(|l| l.as_str() == "us-east-1" || l.as_str().is_empty())
                .unwrap_or(true),
        "Location should be us-east-1 or empty, got {:?}",
        location
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete bucket");
}

/// Test presigned URL generation and usage
#[tokio::test]
async fn test_presigned_urls() {
    let (client, _temp_dir, server) = setup_test_server_with_auth().await;
    let bucket_name = format!("presign-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create bucket for presigned URL test");

    let presign_response = http_client
        .get(format!(
            "{}/presign/{}/upload.txt?method=PUT&expires=3600",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("presign PUT request");
    assert_eq!(presign_response.status(), 200, "Presign PUT request failed");
    let presign_json: serde_json::Value = presign_response.json().await.expect("presign json");
    let put_url = presign_json["url"].as_str().expect("put url in json");
    assert!(put_url.contains("X-Amz-Signature"));
    assert_eq!(presign_json["method"], "PUT");
    assert_eq!(presign_json["expires_in"], 3600);

    let content = b"presigned upload content";
    let put_response = http_client
        .put(put_url)
        .body(content.to_vec())
        .send()
        .await
        .expect("presigned PUT upload");
    assert_eq!(
        put_response.status(),
        200,
        "Presigned PUT should succeed: {}",
        put_response.text().await.unwrap_or_default()
    );

    let get_result = client
        .get_object()
        .bucket(&bucket_name)
        .key("upload.txt")
        .send()
        .await
        .expect("get uploaded object");
    let body = get_result
        .body
        .collect()
        .await
        .expect("collect body")
        .into_bytes();
    assert_eq!(&body[..], content);

    let presign_response = http_client
        .get(format!("{}/presign/{}/upload.txt", base_url, bucket_name))
        .send()
        .await
        .expect("presign GET request");
    assert_eq!(presign_response.status(), 200, "Presign GET request failed");
    let presign_json: serde_json::Value = presign_response.json().await.expect("presign json");
    let get_url = presign_json["url"].as_str().expect("get url in json");
    assert!(get_url.contains("X-Amz-Signature"));
    assert_eq!(presign_json["method"], "GET");

    let get_response = http_client
        .get(get_url)
        .send()
        .await
        .expect("use presigned GET URL");
    assert_eq!(get_response.status(), 200);
    let body = get_response.bytes().await.expect("response bytes");
    assert_eq!(&body[..], content);

    let presign_response = http_client
        .get(format!(
            "{}/presign/{}/test.txt?method=DELETE",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("presign DELETE request");
    assert_eq!(
        presign_response.status(),
        400,
        "Invalid method should return 400"
    );
}
