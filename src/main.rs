//! rs3gw binary - High-Performance AI/HPC Object Storage Gateway
//!
//! A lightweight, zero-GC S3-compatible gateway powered by scirs2-io.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use tokio::net::TcpListener;
use tokio::signal;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::{Any, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use rs3gw::api::s3_router;
use rs3gw::metrics::{init_metrics, metrics_layer, metrics_tracker_layer};
use rs3gw::storage::StorageEngine;
use rs3gw::{AppState, Config};

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let config = Config::from_env();
    info!("Starting rs3gw server");
    info!("Bind address: {}", config.bind_addr);
    info!("Storage root: {}", config.storage_root.display());
    info!("Default bucket: {}", config.default_bucket);
    info!("Compression: {:?}", config.compression);
    info!("Request timeout: {}s", config.request_timeout_secs);
    if config.max_concurrent_requests > 0 {
        info!(
            "Max concurrent requests: {}",
            config.max_concurrent_requests
        );
    }

    // Initialize Prometheus metrics
    let metrics_handle = init_metrics()?;
    info!("Prometheus metrics initialized");

    // Ensure storage directory exists
    tokio::fs::create_dir_all(&config.storage_root).await?;

    let storage = Arc::new(
        StorageEngine::new(config.storage_root.clone())?.with_compression(config.compression),
    );

    // Initialize optional managers from environment
    let cache_settings = rs3gw::CacheSettings::from_env();
    let throttle_settings = rs3gw::ThrottleSettings::from_env();

    if cache_settings.enabled {
        info!(
            "Cache enabled: {}MB max, {} objects max, {}s TTL",
            cache_settings.max_size_mb, cache_settings.max_objects, cache_settings.ttl_secs
        );
    }

    if throttle_settings.enabled {
        info!(
            "Throttle enabled: {} RPS, {}MB/s upload, {}MB/s download",
            throttle_settings.requests_per_sec,
            throttle_settings.upload_mbps,
            throttle_settings.download_mbps
        );
    }

    let state = AppState::new(
        config.clone(),
        storage,
        metrics_handle,
        Some(cache_settings),
        Some(throttle_settings),
        None, // QuotaSettings - requires async initialization
    );

    // Start background metrics collection for predictive analytics
    // Collect metrics every 60 seconds
    info!("Starting background metrics collection for predictive analytics");
    state.start_metrics_collection(60);

    // Configure CORS - permissive for S3 compatibility
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any)
        .max_age(Duration::from_secs(86400)); // 24 hours

    let mut app = Router::new()
        .merge(s3_router::routes())
        .layer(cors)
        .layer(axum::middleware::from_fn(metrics_layer))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            metrics_tracker_layer,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state);

    // Add timeout layer if configured (0 = no timeout)
    if config.request_timeout_secs > 0 {
        app = app.layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(config.request_timeout_secs),
        ));
    }

    // Add concurrency limit if configured (0 = no limit)
    if config.max_concurrent_requests > 0 {
        app = app.layer(ConcurrencyLimitLayer::new(config.max_concurrent_requests));
    }

    // Start server with or without TLS
    if config.tls.is_enabled() {
        let cert_path = config
            .tls
            .cert_path
            .as_ref()
            .ok_or("TLS enabled but cert_path not configured")?;
        let key_path = config
            .tls
            .key_path
            .as_ref()
            .ok_or("TLS enabled but key_path not configured")?;

        info!("TLS enabled with cert: {}", cert_path.display());

        let rustls_config = RustlsConfig::from_pem_file(cert_path, key_path)
            .await
            .map_err(|e| format!("Failed to load TLS certificates: {}", e))?;

        info!("rs3gw listening on {} (HTTPS)", config.bind_addr);

        axum_server::bind_rustls(config.bind_addr, rustls_config)
            .serve(app.into_make_service())
            .await?;
    } else {
        let listener = TcpListener::bind(config.bind_addr).await?;
        info!("rs3gw listening on {} (HTTP)", config.bind_addr);

        // Serve with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

    info!("rs3gw server shut down gracefully");
    Ok(())
}

/// Wait for shutdown signal (SIGINT or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {}", e);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {}", e);
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("Received SIGINT, initiating graceful shutdown...");
        }
        () = terminate => {
            info!("Received SIGTERM, initiating graceful shutdown...");
        }
    }
}
