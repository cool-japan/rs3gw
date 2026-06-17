//! Common test utilities for rs3gw integration tests

#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::Client;
use metrics_exporter_prometheus::PrometheusHandle;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// Global metrics handle - initialized once for all tests
static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

fn get_metrics_handle() -> PrometheusHandle {
    METRICS_HANDLE
        .get_or_init(|| {
            rs3gw::metrics::init_metrics().expect("Failed to initialize metrics for tests")
        })
        .clone()
}

pub fn init_tracing() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .try_init();
}

/// Test server info returned from setup
pub struct TestServer {
    pub addr: SocketAddr,
    pub base_url: String,
    pub _handle: tokio::task::JoinHandle<()>,
}

impl TestServer {
    /// Create a new test server with default settings
    pub async fn new() -> Self {
        let (_client, _temp_dir, server) = setup_test_server().await;
        // Note: temp_dir and client are dropped here, but that's OK for observability tests
        // The server will continue running
        server
    }
}

/// Start a test server and return the client and temp directory
pub async fn setup_test_server() -> (Client, TempDir, TestServer) {
    init_tracing();
    let temp_dir = TempDir::new().unwrap();

    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Set up the server
    let storage_root = temp_dir.path().to_path_buf();
    let storage = Arc::new(rs3gw::storage::StorageEngine::new(storage_root.clone()).unwrap());
    let metrics_handle = get_metrics_handle();
    let config = rs3gw::Config {
        bind_addr: addr,
        storage_root,
        default_bucket: "default".to_string(),
        access_key: String::new(),
        secret_key: String::new(),
        compression: rs3gw::storage::CompressionMode::None,
        request_timeout_secs: 0,          // No timeout for tests
        max_concurrent_requests: 0,       // No limit for tests
        tls: rs3gw::TlsConfig::default(), // No TLS for tests
        connection_pool: rs3gw::ConnectionPoolConfig::default(),
        cluster: rs3gw::cluster::ClusterConfig::default(), // Cluster disabled for tests
        dedup: rs3gw::storage::DedupConfig::disabled(),    // Dedup disabled for tests
        zerocopy: rs3gw::storage::ZeroCopyConfig::default(),
        select_cache: rs3gw::SelectCacheConfig::default(), // Select cache enabled for tests
        multipart_retention_hours: 168,
        fsync: false,
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

    #[cfg(feature = "formats")]
    let query_intelligence = std::sync::Arc::new(rs3gw::api::QueryIntelligence::new());

    let training_path = temp_dir.path().join("training");
    let training_manager = std::sync::Arc::new(rs3gw::storage::TrainingManager::new(training_path));

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
        verifier: None,
        auth_failure_counts: std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        in_flight: rs3gw::InFlightTracker::new(),
        encryption: std::sync::Arc::new(rs3gw::storage::encryption::EncryptionService::new(
            std::sync::Arc::new(rs3gw::storage::encryption::LocalKeyProvider::default()),
        )),
    };

    let app = axum::Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    // Spawn the server
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create the S3 client
    let client = create_s3_client(addr).await;

    let server = TestServer {
        addr,
        base_url: format!("http://{}", addr),
        _handle: server_handle,
    };

    (client, temp_dir, server)
}

pub async fn create_s3_client(addr: SocketAddr) -> Client {
    create_s3_client_with_credentials(addr, "test", "test").await
}

pub async fn create_s3_client_with_credentials(
    addr: SocketAddr,
    access_key: &str,
    secret_key: &str,
) -> Client {
    let credentials = Credentials::new(access_key, secret_key, None, None, "test");

    let config = aws_sdk_s3::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .endpoint_url(format!("http://{}", addr))
        .credentials_provider(credentials)
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .force_path_style(true)
        .build();

    Client::from_conf(config)
}

/// Start a test server with authentication keys configured (for presigned URLs)
pub async fn setup_test_server_with_auth() -> (Client, TempDir, TestServer) {
    init_tracing();
    let temp_dir = TempDir::new().unwrap();

    // Find an available port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Set up the server with auth keys (but no auth middleware for simplicity)
    let storage_root = temp_dir.path().to_path_buf();
    let storage = Arc::new(rs3gw::storage::StorageEngine::new(storage_root.clone()).unwrap());
    let metrics_handle = get_metrics_handle();
    let config = rs3gw::Config {
        bind_addr: addr,
        storage_root,
        default_bucket: "default".to_string(),
        access_key: "testkey".to_string(),
        secret_key: "testsecret".to_string(),
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

    #[cfg(feature = "formats")]
    let query_intelligence = std::sync::Arc::new(rs3gw::api::QueryIntelligence::new());

    let training_path = temp_dir.path().join("training");
    let training_manager = std::sync::Arc::new(rs3gw::storage::TrainingManager::new(training_path));

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
        verifier: None,
        auth_failure_counts: std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        in_flight: rs3gw::InFlightTracker::new(),
        encryption: std::sync::Arc::new(rs3gw::storage::encryption::EncryptionService::new(
            std::sync::Arc::new(rs3gw::storage::encryption::LocalKeyProvider::default()),
        )),
    };

    let app = axum::Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    // Spawn the server
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create the S3 client
    let client = create_s3_client(addr).await;

    let server = TestServer {
        addr,
        base_url: format!("http://{}", addr),
        _handle: server_handle,
    };

    (client, temp_dir, server)
}

/// Start a test server that includes the CORS simple-request middleware.
///
/// This mirrors the production `main.rs` setup for simple (non-OPTIONS) CORS
/// header injection via [`rs3gw::api::cors_middleware::cors_simple_request`].
pub async fn setup_test_server_with_cors_middleware() -> (Client, TempDir, TestServer) {
    init_tracing();
    let temp_dir = TempDir::new().unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let storage_root = temp_dir.path().to_path_buf();
    let storage = Arc::new(rs3gw::storage::StorageEngine::new(storage_root.clone()).unwrap());
    let metrics_handle = get_metrics_handle();
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
    #[cfg(feature = "formats")]
    let query_intelligence = std::sync::Arc::new(rs3gw::api::QueryIntelligence::new());
    let training_path = temp_dir.path().join("training");
    let training_manager = std::sync::Arc::new(rs3gw::storage::TrainingManager::new(training_path));

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
        verifier: None,
        auth_failure_counts: std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        in_flight: rs3gw::InFlightTracker::new(),
        encryption: std::sync::Arc::new(rs3gw::storage::encryption::EncryptionService::new(
            std::sync::Arc::new(rs3gw::storage::encryption::LocalKeyProvider::default()),
        )),
    };

    let app = axum::Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rs3gw::api::cors_middleware::cors_simple_request,
        ))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    let client = create_s3_client(addr).await;

    let server = TestServer {
        addr,
        base_url: format!("http://{}", addr),
        _handle: server_handle,
    };

    (client, temp_dir, server)
}
