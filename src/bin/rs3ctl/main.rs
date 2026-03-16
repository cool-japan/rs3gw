#!/usr/bin/env rust
//! rs3ctl - Command-line tool for managing rs3gw S3-compatible object storage
//!
//! This tool provides comprehensive management capabilities for rs3gw servers:
//! - Bucket operations (create, delete, list, info)
//! - Object operations (upload, download, list, delete, info)
//! - Replication management
//! - Metrics and monitoring
//! - Maintenance operations
//! - Policy management
//! - Config inspection and validation
//! - Server status dashboard
//! - Performance benchmarking
//! - Diagnostics

mod handlers_admin;
mod handlers_bucket;
mod handlers_preprocessing;
mod handlers_replication;
mod handlers_transform;
mod handlers_versioning;
mod types;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use std::time::Duration;

use types::{
    BatchAction, BucketAction, ConfigAction, LifecycleAction, MaintenanceAction, MetricsAction,
    ObjectAction, ObservabilityAction, PreprocessingAction, ReplicationAction, ServerInfoAction,
    TransformAction, VersioningAction,
};

mod handlers_multipart_gc;

/// rs3ctl - Command-line tool for rs3gw management
#[derive(Parser)]
#[command(name = "rs3ctl")]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// rs3gw server endpoint (default: http://localhost:9000)
    #[arg(short, long, default_value = "http://localhost:9000")]
    pub endpoint: String,

    /// Access key for authentication
    #[arg(short, long)]
    pub access_key: Option<String>,

    /// Secret key for authentication
    #[arg(short, long)]
    pub secret_key: Option<String>,

    /// Command to execute
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Bucket management commands
    Bucket {
        #[command(subcommand)]
        action: BucketAction,
    },
    /// Object management commands
    Object {
        #[command(subcommand)]
        action: ObjectAction,
    },
    /// Replication management commands
    Replication {
        #[command(subcommand)]
        action: ReplicationAction,
    },
    /// Metrics and monitoring commands
    Metrics {
        #[command(subcommand)]
        action: MetricsAction,
    },
    /// Maintenance operations
    Maintenance {
        #[command(subcommand)]
        action: MaintenanceAction,
    },
    /// Versioning management commands
    Versioning {
        #[command(subcommand)]
        action: VersioningAction,
    },
    /// Observability commands (profiling, anomalies, business metrics)
    Observability {
        #[command(subcommand)]
        action: ObservabilityAction,
    },
    /// Transformation operations
    Transform {
        #[command(subcommand)]
        action: TransformAction,
    },
    /// Batch operations
    Batch {
        #[command(subcommand)]
        action: BatchAction,
    },
    /// Lifecycle policy management
    Lifecycle {
        #[command(subcommand)]
        action: LifecycleAction,
    },
    /// Dataset preprocessing pipeline management
    Preprocessing {
        #[command(subcommand)]
        action: PreprocessingAction,
    },
    /// Health check
    Health,
    /// Configuration management (show, validate)
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Server information and status
    ServerInfo {
        #[command(subcommand)]
        action: ServerInfoAction,
    },
    /// Quick S3 performance benchmark (PUT/GET latency)
    Benchmark {
        /// Number of PUT/GET iterations
        #[arg(long, default_value = "100")]
        iterations: u32,
        /// Size of each test object in bytes
        #[arg(long, default_value = "1024")]
        size: usize,
        /// Bucket to use for benchmark objects
        #[arg(long)]
        bucket: String,
    },
    /// Run connectivity and storage diagnostics
    Diagnose {
        /// Only check server connectivity (skip auth/storage checks)
        #[arg(long)]
        connectivity_only: bool,
    },
    /// Garbage-collect abandoned multipart uploads
    GcMultipart {
        /// Restrict GC to this bucket (optional; all buckets if absent)
        #[arg(short, long)]
        bucket: Option<String>,
        /// Age threshold in hours (default: 168 = 7 days)
        #[arg(long, default_value = "168")]
        retention_hours: u64,
    },
    /// Display Prometheus metrics from the server (pretty-printed by family)
    ShowMetrics {
        /// Only show metrics whose name contains this prefix/substring
        #[arg(long)]
        filter: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .context("Failed to create HTTP client")?;

    match &cli.command {
        Commands::Health => handle_health(&client, &cli.endpoint).await,
        Commands::Bucket { action } => {
            handlers_bucket::handle_bucket(&client, &cli, action).await
        }
        Commands::Object { action } => {
            handlers_bucket::handle_object(&client, &cli, action).await
        }
        Commands::Replication { action } => {
            handlers_replication::handle_replication(&client, &cli, action).await
        }
        Commands::Metrics { action } => {
            handlers_replication::handle_metrics(&client, &cli, action).await
        }
        Commands::Maintenance { action } => {
            handlers_replication::handle_maintenance(&client, &cli, action).await
        }
        Commands::Versioning { action } => {
            handlers_versioning::handle_versioning(&client, &cli, action).await
        }
        Commands::Observability { action } => {
            handlers_versioning::handle_observability(&client, &cli, action).await
        }
        Commands::Transform { action } => {
            handlers_transform::handle_transform(&client, &cli, action).await
        }
        Commands::Batch { action } => {
            handlers_transform::handle_batch(&client, &cli, action).await
        }
        Commands::Lifecycle { action } => {
            handlers_transform::handle_lifecycle(&client, &cli, action).await
        }
        Commands::Preprocessing { action } => {
            handlers_preprocessing::handle_preprocessing(&client, &cli, action).await
        }
        Commands::Config { action } => {
            handlers_admin::handle_config(&client, &cli, action).await
        }
        Commands::ServerInfo { action } => {
            handlers_admin::handle_server_info(&client, &cli, action).await
        }
        Commands::Benchmark {
            iterations,
            size,
            bucket,
        } => {
            handlers_admin::handle_benchmark(&client, &cli, *iterations, *size, bucket).await
        }
        Commands::Diagnose { connectivity_only } => {
            handlers_admin::handle_diagnose(&client, &cli, *connectivity_only).await
        }
        Commands::GcMultipart {
            bucket,
            retention_hours,
        } => {
            handlers_multipart_gc::handle_gc_multipart(
                &client,
                &cli,
                bucket.as_deref(),
                *retention_hours,
            )
            .await
        }
        Commands::ShowMetrics { filter } => {
            handlers_multipart_gc::handle_show_metrics(&client, &cli, filter.as_deref()).await
        }
    }
}

async fn handle_health(client: &Client, endpoint: &str) -> Result<()> {
    let health_url = format!("{}/health", endpoint);
    let ready_url = format!("{}/ready", endpoint);

    // Check /health
    let health_resp = client
        .get(&health_url)
        .send()
        .await
        .context("Failed to send health check request")?;

    let health_ok = health_resp.status().is_success();
    if health_ok {
        println!("✓ /health  OK ({})", health_resp.status());
        let body = health_resp.text().await.unwrap_or_default();
        // Pretty-print if JSON, otherwise just print raw
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&body) {
            println!("{}", serde_json::to_string_pretty(&parsed)?);
        } else {
            println!("{}", body);
        }
    } else {
        println!("✗ /health  FAILED ({})", health_resp.status());
    }

    // Check /ready
    let ready_resp = client
        .get(&ready_url)
        .send()
        .await
        .context("Failed to send readiness check request")?;

    let ready_ok = ready_resp.status().is_success();
    if ready_ok {
        let status_text = ready_resp.text().await.unwrap_or_default();
        println!("✓ /ready   OK — {}", status_text.trim());
    } else {
        println!("✗ /ready   FAILED ({})", ready_resp.status());
    }

    if health_ok && ready_ok {
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Server health/readiness check failed — see output above"
        ))
    }
}
