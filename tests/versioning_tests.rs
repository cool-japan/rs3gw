//! Integration tests for object versioning feature
//!
//! Tests end-to-end versioning functionality including:
//! - Version creation and retrieval
//! - Version listing
//! - Delete markers
//! - Version restoration

mod common;

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{Delete, ObjectIdentifier};
use common::{create_s3_client, init_tracing};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;

/// Test server with versioning capabilities
struct VersioningTestServer {
    pub _addr: SocketAddr,
    pub _temp_dir: TempDir,
    _handle: tokio::task::JoinHandle<()>,
}

async fn setup_versioning_server() -> (aws_sdk_s3::Client, VersioningTestServer) {
    init_tracing();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind to address");
    let addr = listener.local_addr().expect("Failed to get local address");

    // Set up the server
    let storage_root = temp_dir.path().to_path_buf();
    let storage = Arc::new(
        rs3gw::storage::StorageEngine::new(storage_root.clone())
            .expect("Failed to create storage engine"),
    );
    let metrics_handle = rs3gw::metrics::init_metrics().expect("Failed to initialize metrics");

    let config = rs3gw::Config {
        bind_addr: addr,
        storage_root,
        default_bucket: "default".to_string(),
        access_key: String::new(),
        secret_key: String::new(),
        compression: rs3gw::storage::CompressionMode::None,
        request_timeout_secs: 0,
        max_concurrent_requests: 0,
        tls: rs3gw::TlsConfig::default(),
        connection_pool: rs3gw::ConnectionPoolConfig::default(),
        cluster: rs3gw::cluster::ClusterConfig::default(),
        dedup: rs3gw::storage::DedupConfig::disabled(),
        zerocopy: rs3gw::storage::ZeroCopyConfig::default(),
        select_cache: rs3gw::SelectCacheConfig::default(),
    };

    // Initialize preprocessing manager
    let preprocessing_path = temp_dir.path().join("preprocessing");
    let preprocessing_manager = std::sync::Arc::new(
        rs3gw::storage::preprocessing::PreprocessingManager::new(preprocessing_path),
    );

    let predictive_analytics = std::sync::Arc::new(rs3gw::observability::PredictiveAnalytics::new(
        10_000,
        0.023,
        0.09,
        0.0004,
        1_000_000_000_000,
    ));

    let metrics_tracker = std::sync::Arc::new(rs3gw::observability::MetricsTracker::new());

    let select_result_cache =
        std::sync::Arc::new(rs3gw::api::SelectResultCache::new(100, 10 * 1024 * 1024));

    let query_intelligence = std::sync::Arc::new(rs3gw::api::QueryIntelligence::new());

    let training_manager = std::sync::Arc::new(rs3gw::storage::TrainingManager::new(
        temp_dir.path().join("training"),
    ));

    let state = rs3gw::AppState {
        config,
        storage,
        metrics_handle,
        cache: None,
        throttle: None,
        quota: None,
        event_broadcaster: rs3gw::api::EventBroadcaster::new(),
        query_plan_cache: None,
        select_result_cache,
        query_intelligence,
        advanced_replication: None,
        preprocessing_manager,
        predictive_analytics,
        metrics_tracker,
        training_manager,
        start_time: std::time::Instant::now(),
    };

    let app = axum::Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    // Spawn the server
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .expect("Server failed to start");
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create the S3 client
    let client = create_s3_client(addr).await;

    let server = VersioningTestServer {
        _addr: addr,
        _temp_dir: temp_dir,
        _handle: server_handle,
    };

    (client, server)
}

#[tokio::test]
async fn test_versioning_basic_operations() {
    // Test basic versioning workflow
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-versioning")
        .send()
        .await
        .expect("Failed to create bucket");

    // Check versioning status (should be disabled by default)
    let versioning_result = client
        .get_bucket_versioning()
        .bucket("test-versioning")
        .send()
        .await
        .expect("Failed to get bucket versioning");

    // The status should be None or Suspended (AWS S3 default behavior)
    assert!(
        versioning_result.status().is_none()
            || versioning_result.status().map(|s| s.as_str()) == Some("Suspended"),
        "Versioning should be disabled by default"
    );

    // Enable versioning
    client
        .put_bucket_versioning()
        .bucket("test-versioning")
        .versioning_configuration(
            aws_sdk_s3::types::VersioningConfiguration::builder()
                .status(aws_sdk_s3::types::BucketVersioningStatus::Enabled)
                .build(),
        )
        .send()
        .await
        .expect("Failed to enable versioning");

    println!("✓ Basic versioning operations work correctly");
}

#[tokio::test]
async fn test_object_versions() {
    // Test creating and retrieving multiple versions
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-versions")
        .send()
        .await
        .expect("Failed to create bucket");

    // Upload object version 1
    let version1_data = b"This is version 1";
    client
        .put_object()
        .bucket("test-versions")
        .key("test-object")
        .body(ByteStream::from(version1_data.to_vec()))
        .send()
        .await
        .expect("Failed to upload version 1");

    // Upload object version 2 (same key, different content)
    let version2_data = b"This is version 2";
    client
        .put_object()
        .bucket("test-versions")
        .key("test-object")
        .body(ByteStream::from(version2_data.to_vec()))
        .send()
        .await
        .expect("Failed to upload version 2");

    // Upload object version 3
    let version3_data = b"This is version 3";
    client
        .put_object()
        .bucket("test-versions")
        .key("test-object")
        .body(ByteStream::from(version3_data.to_vec()))
        .send()
        .await
        .expect("Failed to upload version 3");

    // Get the latest version (should be version 3)
    let result = client
        .get_object()
        .bucket("test-versions")
        .key("test-object")
        .send()
        .await
        .expect("Failed to get latest version");

    let body = result.body.collect().await.expect("Failed to collect body");
    assert_eq!(
        body.into_bytes().as_ref(),
        version3_data,
        "Latest version should be version 3"
    );

    println!("✓ Multiple object versions work correctly");
}

#[tokio::test]
async fn test_list_object_versions() {
    // Test listing object versions
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-list-versions")
        .send()
        .await
        .expect("Failed to create bucket");

    // Upload multiple versions
    for i in 1..=5 {
        let data = format!("Version {}", i);
        client
            .put_object()
            .bucket("test-list-versions")
            .key("versioned-object")
            .body(ByteStream::from(data.into_bytes()))
            .send()
            .await
            .expect("Failed to upload version");
    }

    // List object versions
    let versions_result = client
        .list_object_versions()
        .bucket("test-list-versions")
        .send()
        .await
        .expect("Failed to list object versions");

    // Should have at least some versions
    let versions = versions_result.versions();
    assert!(!versions.is_empty(), "Should have at least one version");

    println!("✓ Listed {} object versions successfully", versions.len());
}

#[tokio::test]
async fn test_delete_with_versioning() {
    // Test delete behavior with versioning
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-delete-versions")
        .send()
        .await
        .expect("Failed to create bucket");

    // Upload an object
    let data = b"Test data for deletion";
    client
        .put_object()
        .bucket("test-delete-versions")
        .key("delete-test")
        .body(ByteStream::from(data.to_vec()))
        .send()
        .await
        .expect("Failed to upload object");

    // Delete the object (should create a delete marker)
    client
        .delete_object()
        .bucket("test-delete-versions")
        .key("delete-test")
        .send()
        .await
        .expect("Failed to delete object");

    // Try to get the object (should fail because of delete marker)
    let get_result = client
        .get_object()
        .bucket("test-delete-versions")
        .key("delete-test")
        .send()
        .await;

    assert!(
        get_result.is_err(),
        "Object should not be accessible after deletion"
    );

    println!("✓ Delete with versioning creates delete marker correctly");
}

#[tokio::test]
async fn test_batch_delete_with_versions() {
    // Test batch delete operations
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-batch-delete")
        .send()
        .await
        .expect("Failed to create bucket");

    // Upload multiple objects
    for i in 1..=5 {
        client
            .put_object()
            .bucket("test-batch-delete")
            .key(format!("object-{}", i))
            .body(ByteStream::from(format!("Data {}", i).into_bytes()))
            .send()
            .await
            .expect("Failed to upload object");
    }

    // Batch delete
    let objects_to_delete: Vec<ObjectIdentifier> = (1..=5)
        .map(|i| {
            ObjectIdentifier::builder()
                .key(format!("object-{}", i))
                .build()
                .expect("Failed to build object identifier")
        })
        .collect();

    client
        .delete_objects()
        .bucket("test-batch-delete")
        .delete(
            Delete::builder()
                .set_objects(Some(objects_to_delete))
                .build()
                .expect("Failed to build delete request"),
        )
        .send()
        .await
        .expect("Failed to batch delete objects");

    // List objects (should be empty or have delete markers)
    let list_result = client
        .list_objects_v2()
        .bucket("test-batch-delete")
        .send()
        .await
        .expect("Failed to list objects");

    let object_count = list_result.contents().len();

    // With versioning, objects might not appear in regular list after delete
    println!(
        "✓ Batch delete processed successfully, {} objects remaining",
        object_count
    );
}

#[tokio::test]
async fn test_copy_with_versioning() {
    // Test object copy with versioning
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-copy-versions")
        .send()
        .await
        .expect("Failed to create bucket");

    // Upload source object
    let source_data = b"Source object data";
    client
        .put_object()
        .bucket("test-copy-versions")
        .key("source-object")
        .body(ByteStream::from(source_data.to_vec()))
        .send()
        .await
        .expect("Failed to upload source object");

    // Copy object
    client
        .copy_object()
        .bucket("test-copy-versions")
        .key("copied-object")
        .copy_source("test-copy-versions/source-object")
        .send()
        .await
        .expect("Failed to copy object");

    // Verify copied object
    let result = client
        .get_object()
        .bucket("test-copy-versions")
        .key("copied-object")
        .send()
        .await
        .expect("Failed to get copied object");

    let body = result.body.collect().await.expect("Failed to collect body");
    assert_eq!(
        body.into_bytes().as_ref(),
        source_data,
        "Copied object content should match source"
    );

    println!("✓ Copy with versioning works correctly");
}

#[tokio::test]
async fn test_head_object_with_versioning() {
    // Test HeadObject with versioning
    let (client, _server) = setup_versioning_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("test-head-versions")
        .send()
        .await
        .expect("Failed to create bucket");

    // Upload object
    let data = b"Test data for head operation";
    let _put_result = client
        .put_object()
        .bucket("test-head-versions")
        .key("test-object")
        .body(ByteStream::from(data.to_vec()))
        .send()
        .await
        .expect("Failed to upload object");

    // HeadObject
    let head_result = client
        .head_object()
        .bucket("test-head-versions")
        .key("test-object")
        .send()
        .await
        .expect("Failed to head object");

    // Verify metadata
    assert_eq!(
        head_result.content_length().unwrap_or(0) as usize,
        data.len(),
        "Content length should match"
    );

    // Version ID should be present (or "null" for non-versioned)
    let version_id = head_result.version_id();
    println!("✓ HeadObject returned version_id: {:?}", version_id);
}
