#![cfg(feature = "server")]
//! Cache integration tests for rs3gw

mod common;

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::Client;
use metrics_exporter_prometheus::PrometheusHandle;
use tempfile::TempDir;
use tokio::net::TcpListener;

// Global metrics handle - initialized once for all tests
static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

fn get_metrics_handle() -> PrometheusHandle {
    METRICS_HANDLE
        .get_or_init(|| {
            rs3gw::metrics::init_metrics().expect("Failed to initialize metrics for tests")
        })
        .clone()
}

/// Setup a test server with caching enabled
async fn setup_test_server_with_cache() -> (Client, TempDir, tokio::task::JoinHandle<()>) {
    common::init_tracing();
    let temp_dir = TempDir::new().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let storage_root = temp_dir.path().to_path_buf();
    let storage = Arc::new(rs3gw::storage::StorageEngine::new(storage_root.clone()).unwrap());
    let metrics_handle = get_metrics_handle();

    // Create cache settings
    let cache_settings = rs3gw::CacheSettings {
        enabled: true,
        max_size_mb: 16,
        max_objects: 100,
        ttl_secs: 60,
    };

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
        multipart_retention_hours: 168,
        fsync: false,
    };

    let state = rs3gw::AppState::new(
        config,
        storage,
        metrics_handle,
        Some(cache_settings),
        None, // No throttle
        None, // No quota
    );

    let app = axum::Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let credentials = Credentials::new("test", "test", None, None, "test");
    let client_config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .endpoint_url(format!("http://{}", addr))
        .credentials_provider(credentials)
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .force_path_style(true)
        .build();

    let client = Client::from_conf(client_config);

    (client, temp_dir, server_handle)
}

#[tokio::test]
async fn test_cache_enabled() {
    let (client, _temp_dir, _server) = setup_test_server_with_cache().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("cache-test")
        .send()
        .await
        .unwrap();

    // Put object
    let content = b"Hello, cache test!";
    client
        .put_object()
        .bucket("cache-test")
        .key("cached-file.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .unwrap();

    // Get object multiple times - second should be from cache
    for _ in 0..3 {
        let result = client
            .get_object()
            .bucket("cache-test")
            .key("cached-file.txt")
            .send()
            .await;
        assert!(result.is_ok());
        let body = result.unwrap().body.collect().await.unwrap();
        assert_eq!(body.into_bytes().as_ref(), content);
    }
}

#[tokio::test]
async fn test_cache_invalidation_on_delete() {
    let (client, _temp_dir, _server) = setup_test_server_with_cache().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("cache-invalidate-test")
        .send()
        .await
        .unwrap();

    // Put and get object
    let content = b"Original content";
    client
        .put_object()
        .bucket("cache-invalidate-test")
        .key("test.txt")
        .body(content.to_vec().into())
        .send()
        .await
        .unwrap();

    // First get
    let result = client
        .get_object()
        .bucket("cache-invalidate-test")
        .key("test.txt")
        .send()
        .await
        .unwrap();
    let body = result.body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), content);

    // Delete object
    client
        .delete_object()
        .bucket("cache-invalidate-test")
        .key("test.txt")
        .send()
        .await
        .unwrap();

    // Get should fail after delete
    let result = client
        .get_object()
        .bucket("cache-invalidate-test")
        .key("test.txt")
        .send()
        .await;
    assert!(result.is_err(), "Object should be deleted");
}

#[tokio::test]
async fn test_cache_with_overwrite() {
    let (client, _temp_dir, _server) = setup_test_server_with_cache().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("cache-overwrite-test")
        .send()
        .await
        .unwrap();

    // Put original content
    let content_v1 = b"Version 1";
    client
        .put_object()
        .bucket("cache-overwrite-test")
        .key("versioned.txt")
        .body(content_v1.to_vec().into())
        .send()
        .await
        .unwrap();

    // Get original
    let result = client
        .get_object()
        .bucket("cache-overwrite-test")
        .key("versioned.txt")
        .send()
        .await
        .unwrap();
    let body = result.body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), content_v1);

    // Overwrite with new content
    let content_v2 = b"Version 2 - Updated!";
    client
        .put_object()
        .bucket("cache-overwrite-test")
        .key("versioned.txt")
        .body(content_v2.to_vec().into())
        .send()
        .await
        .unwrap();

    // Get should return new content (cache should be invalidated)
    let result = client
        .get_object()
        .bucket("cache-overwrite-test")
        .key("versioned.txt")
        .send()
        .await
        .unwrap();
    let body = result.body.collect().await.unwrap();
    assert_eq!(body.into_bytes().as_ref(), content_v2);
}
