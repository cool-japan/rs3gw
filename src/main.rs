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
use rs3gw::grpc::{GrpcConfig, GrpcServer};
use rs3gw::metrics::{init_metrics, metrics_layer, metrics_tracker_layer};
use rs3gw::storage::StorageEngine;
use rs3gw::{AppState, Config, InFlightGuard};

/// Middleware that tracks in-flight requests for graceful shutdown drain
async fn in_flight_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let _guard = InFlightGuard::new(&state.in_flight);
    next.run(request).await
}

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

    let checksum_validation = std::env::var("RS3GW_CHECKSUM_VALIDATION")
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    if checksum_validation {
        info!("Checksum validation on read: enabled");
    }

    let storage = Arc::new(
        StorageEngine::new(config.storage_root.clone())?
            .with_compression(config.compression)
            .with_checksum_validation(checksum_validation),
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

    // Start background multipart GC (runs every hour)
    let _gc_handle = rs3gw::storage::gc::spawn_multipart_gc(
        state.storage.clone(),
        state.config.multipart_retention_hours,
        3600, // run every hour
    );
    info!(
        "Background multipart GC started (retention={}h, interval=3600s)",
        config.multipart_retention_hours
    );

    // Start background metrics collection for predictive analytics
    // Collect metrics every 60 seconds
    info!("Starting background metrics collection for predictive analytics");
    state.start_metrics_collection(60);

    // Conditionally start gRPC server from environment configuration
    let grpc_config = GrpcConfig::from_env();
    if grpc_config.enabled {
        let grpc_storage = state.storage.clone();
        tokio::spawn(async move {
            let grpc_server = GrpcServer::new(grpc_storage, grpc_config.bind_addr);
            if let Err(e) = grpc_server.serve_with_config(grpc_config).await {
                tracing::error!("gRPC server error: {}", e);
            }
        });
        info!("gRPC server spawned");
    }

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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            in_flight_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state.clone());

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

    // Wait for in-flight requests to drain
    info!("Waiting for in-flight requests to drain...");
    state.in_flight.wait_drain(Duration::from_secs(30)).await;
    info!("All in-flight requests drained, shutting down");
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
