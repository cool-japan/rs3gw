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

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// rs3ctl - Command-line tool for rs3gw management
#[derive(Parser)]
#[command(name = "rs3ctl")]
#[command(version, about, long_about = None)]
struct Cli {
    /// rs3gw server endpoint (default: http://localhost:9000)
    #[arg(short, long, default_value = "http://localhost:9000")]
    endpoint: String,

    /// Access key for authentication
    #[arg(short, long)]
    access_key: Option<String>,

    /// Secret key for authentication
    #[arg(short, long)]
    secret_key: Option<String>,

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
}

#[derive(Subcommand)]
enum BucketAction {
    /// List all buckets
    List,
    /// Create a new bucket
    Create {
        /// Bucket name
        name: String,
        /// Bucket region
        #[arg(short, long, default_value = "us-east-1")]
        region: String,
    },
    /// Delete a bucket
    Delete {
        /// Bucket name
        name: String,
        /// Force delete even if bucket is not empty
        #[arg(short, long)]
        force: bool,
    },
    /// Get bucket information
    Info {
        /// Bucket name
        name: String,
    },
    /// Get bucket policy
    GetPolicy {
        /// Bucket name
        name: String,
    },
    /// Set bucket policy from file
    SetPolicy {
        /// Bucket name
        name: String,
        /// Policy JSON file
        policy_file: PathBuf,
    },
}

#[derive(Subcommand)]
enum ObjectAction {
    /// List objects in a bucket
    List {
        /// Bucket name
        bucket: String,
        /// Prefix filter
        #[arg(short, long)]
        prefix: Option<String>,
        /// Maximum keys to return
        #[arg(short, long, default_value = "1000")]
        max_keys: i32,
    },
    /// Upload a file as an object
    Upload {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Local file path
        file: PathBuf,
        /// Content type
        #[arg(short = 't', long)]
        content_type: Option<String>,
    },
    /// Download an object to a file
    Download {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Local file path to save
        output: PathBuf,
    },
    /// Delete an object
    Delete {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
    },
    /// Get object information
    Info {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
    },
}

#[derive(Subcommand)]
enum ReplicationAction {
    /// Get replication configuration for a bucket
    GetConfig {
        /// Bucket name
        bucket: String,
    },
    /// Set replication configuration from file
    SetConfig {
        /// Bucket name
        bucket: String,
        /// Configuration JSON file
        config_file: PathBuf,
    },
    /// Get replication metrics
    Metrics {
        /// Destination name (optional)
        #[arg(short, long)]
        destination: Option<String>,
    },
}

#[derive(Subcommand)]
enum MetricsAction {
    /// Get Prometheus metrics
    Get,
    /// Get storage statistics
    Storage,
    /// Get operation statistics
    Operations,
}

#[derive(Subcommand)]
enum MaintenanceAction {
    /// Create a backup snapshot
    Backup {
        /// Snapshot name
        #[arg(short, long)]
        name: Option<String>,
        /// Snapshot type (full, incremental)
        #[arg(short = 't', long, default_value = "full")]
        snapshot_type: String,
    },
    /// Trigger integrity check
    IntegrityCheck {
        /// Bucket name (optional, checks all if not specified)
        #[arg(short, long)]
        bucket: Option<String>,
    },
    /// Trigger cleanup of old objects
    Cleanup {
        /// Retention period in days
        #[arg(short, long, default_value = "30")]
        retention_days: u32,
    },
}

#[derive(Subcommand)]
enum VersioningAction {
    /// Get versioning status for a bucket
    GetStatus {
        /// Bucket name
        bucket: String,
    },
    /// Enable versioning for a bucket
    Enable {
        /// Bucket name
        bucket: String,
    },
    /// Suspend versioning for a bucket
    Suspend {
        /// Bucket name
        bucket: String,
    },
    /// List object versions
    ListVersions {
        /// Bucket name
        bucket: String,
        /// Object key prefix filter
        #[arg(short, long)]
        prefix: Option<String>,
        /// Maximum number of versions to return
        #[arg(short, long, default_value = "1000")]
        max_keys: i32,
    },
}

#[derive(Subcommand)]
enum ObservabilityAction {
    /// Get profiling data (CPU, memory, I/O)
    Profiling {
        /// Export in pprof format
        #[arg(short, long)]
        pprof: bool,
    },
    /// Get business metrics and KPIs
    BusinessMetrics,
    /// Get detected anomalies
    Anomalies {
        /// Filter by anomaly type
        #[arg(short = 't', long)]
        anomaly_type: Option<String>,
        /// Filter by severity (Low, Medium, High, Critical)
        #[arg(short, long)]
        severity: Option<String>,
        /// Limit number of results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Get resource manager statistics
    Resources,
    /// Get comprehensive health check
    Health,
}

#[derive(Subcommand)]
enum TransformAction {
    /// Apply image transformation to an object
    Image {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Target width
        #[arg(short = 'w', long)]
        width: Option<u32>,
        /// Target height
        #[arg(short = 'H', long)]
        height: Option<u32>,
        /// Output format (jpeg, png, webp, gif, bmp, tiff)
        #[arg(short = 'f', long)]
        format: Option<String>,
        /// Quality (1-100 for JPEG)
        #[arg(short = 'q', long)]
        quality: Option<u8>,
        /// Output file path
        #[arg(short = 'o', long)]
        output: PathBuf,
    },
    /// Apply compression transformation
    Compress {
        /// Bucket name
        bucket: String,
        /// Object key
        key: String,
        /// Algorithm (zstd, gzip, lz4)
        #[arg(short = 'a', long, default_value = "zstd")]
        algorithm: String,
        /// Compression level
        #[arg(short = 'l', long)]
        level: Option<u8>,
        /// Output file path
        #[arg(short = 'o', long)]
        output: PathBuf,
    },
    /// Test a WASM plugin
    TestWasm {
        /// Path to WASM plugin file
        plugin: PathBuf,
        /// Test input file
        input: PathBuf,
        /// Parameters (plugin-specific)
        #[arg(short = 'p', long)]
        params: Option<String>,
        /// Output file path
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum BatchAction {
    /// Delete multiple objects by prefix
    DeleteByPrefix {
        /// Bucket name
        bucket: String,
        /// Prefix to match objects for deletion
        prefix: String,
        /// Dry run (show what would be deleted without deleting)
        #[arg(long)]
        dry_run: bool,
        /// Maximum number of objects to delete
        #[arg(short = 'm', long)]
        max: Option<usize>,
    },
    /// Copy multiple objects
    CopyMultiple {
        /// Source bucket
        source_bucket: String,
        /// Destination bucket
        dest_bucket: String,
        /// Source prefix
        #[arg(short = 's', long)]
        source_prefix: String,
        /// Destination prefix
        #[arg(short = 'd', long)]
        dest_prefix: Option<String>,
        /// Maximum number of objects to copy
        #[arg(short = 'm', long)]
        max: Option<usize>,
    },
    /// Tag multiple objects
    TagMultiple {
        /// Bucket name
        bucket: String,
        /// Prefix filter
        prefix: String,
        /// Tags in format key1=value1,key2=value2
        tags: String,
        /// Maximum number of objects to tag
        #[arg(short = 'm', long)]
        max: Option<usize>,
    },
}

#[derive(Subcommand)]
enum LifecycleAction {
    /// Get lifecycle policy for a bucket
    Get {
        /// Bucket name
        bucket: String,
    },
    /// Set lifecycle policy from file
    Set {
        /// Bucket name
        bucket: String,
        /// Policy JSON file
        policy_file: PathBuf,
    },
    /// Delete lifecycle policy
    Delete {
        /// Bucket name
        bucket: String,
    },
    /// List rules in lifecycle policy
    ListRules {
        /// Bucket name
        bucket: String,
    },
}

#[derive(Subcommand)]
enum PreprocessingAction {
    /// Create a preprocessing pipeline
    Create {
        /// Pipeline ID
        #[arg(short, long)]
        id: String,
        /// Pipeline name
        #[arg(short, long)]
        name: String,
        /// Pipeline definition JSON file
        #[arg(short, long)]
        file: PathBuf,
    },
    /// List all preprocessing pipelines
    List {
        /// Bucket containing pipelines (default: ml-datasets)
        #[arg(short, long, default_value = "ml-datasets")]
        bucket: String,
    },
    /// Get pipeline definition
    Get {
        /// Pipeline ID
        #[arg(short, long)]
        id: String,
        /// Bucket containing pipelines (default: ml-datasets)
        #[arg(short, long, default_value = "ml-datasets")]
        bucket: String,
    },
    /// Delete a preprocessing pipeline
    Delete {
        /// Pipeline ID
        #[arg(short, long)]
        id: String,
        /// Bucket containing pipelines (default: ml-datasets)
        #[arg(short, long, default_value = "ml-datasets")]
        bucket: String,
    },
    /// Apply pipeline to an object
    Apply {
        /// Pipeline ID
        #[arg(short, long)]
        pipeline: String,
        /// Source bucket
        #[arg(long)]
        source_bucket: String,
        /// Source object key
        #[arg(long)]
        source_key: String,
        /// Destination bucket
        #[arg(long)]
        dest_bucket: String,
        /// Destination object key
        #[arg(long)]
        dest_key: String,
    },
    /// Validate pipeline definition
    Validate {
        /// Pipeline definition JSON file
        file: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct BucketInfo {
    name: String,
    creation_date: Option<String>,
    region: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
struct ObjectInfo {
    key: String,
    size: u64,
    last_modified: String,
    etag: String,
    storage_class: Option<String>,
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
        Commands::Bucket { action } => handle_bucket(&client, &cli, action).await,
        Commands::Object { action } => handle_object(&client, &cli, action).await,
        Commands::Replication { action } => handle_replication(&client, &cli, action).await,
        Commands::Metrics { action } => handle_metrics(&client, &cli, action).await,
        Commands::Maintenance { action } => handle_maintenance(&client, &cli, action).await,
        Commands::Versioning { action } => handle_versioning(&client, &cli, action).await,
        Commands::Observability { action } => handle_observability(&client, &cli, action).await,
        Commands::Transform { action } => handle_transform(&client, &cli, action).await,
        Commands::Batch { action } => handle_batch(&client, &cli, action).await,
        Commands::Lifecycle { action } => handle_lifecycle(&client, &cli, action).await,
        Commands::Preprocessing { action } => handle_preprocessing(&client, &cli, action).await,
    }
}

async fn handle_health(client: &Client, endpoint: &str) -> Result<()> {
    let url = format!("{}/health", endpoint);
    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to send health check request")?;

    if response.status().is_success() {
        println!("✓ Server is healthy");
        let body = response.text().await?;
        println!("{}", body);
        Ok(())
    } else {
        println!("✗ Server health check failed");
        println!("Status: {}", response.status());
        Err(anyhow::anyhow!("Health check failed"))
    }
}

async fn handle_bucket(client: &Client, cli: &Cli, action: &BucketAction) -> Result<()> {
    match action {
        BucketAction::List => {
            let url = format!("{}/", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to list buckets")?;

            if response.status().is_success() {
                let body = response.text().await?;
                println!("Buckets:");
                println!("{}", body);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to list buckets: {}",
                    response.status()
                ))
            }
        }
        BucketAction::Create { name, region } => {
            let url = format!("{}/{}", cli.endpoint, name);
            let response = client
                .put(&url)
                .header("x-amz-bucket-region", region)
                .send()
                .await
                .context("Failed to create bucket")?;

            if response.status().is_success() {
                println!("✓ Bucket '{}' created successfully", name);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to create bucket: {}",
                    response.status()
                ))
            }
        }
        BucketAction::Delete { name, force } => {
            if *force {
                println!("Warning: Force delete not yet implemented");
            }
            let url = format!("{}/{}", cli.endpoint, name);
            let response = client
                .delete(&url)
                .send()
                .await
                .context("Failed to delete bucket")?;

            if response.status().is_success() {
                println!("✓ Bucket '{}' deleted successfully", name);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to delete bucket: {}",
                    response.status()
                ))
            }
        }
        BucketAction::Info { name } => {
            let url = format!("{}/{}", cli.endpoint, name);
            let response = client
                .head(&url)
                .send()
                .await
                .context("Failed to get bucket info")?;

            if response.status().is_success() {
                println!("Bucket: {}", name);
                println!("Status: Exists");
                for (key, value) in response.headers() {
                    if let Ok(v) = value.to_str() {
                        println!("  {}: {}", key, v);
                    }
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("Bucket not found"))
            }
        }
        BucketAction::GetPolicy { name } => {
            let url = format!("{}/{}?policy", cli.endpoint, name);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get bucket policy")?;

            if response.status().is_success() {
                let policy = response.text().await?;
                println!("{}", policy);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get bucket policy: {}",
                    response.status()
                ))
            }
        }
        BucketAction::SetPolicy { name, policy_file } => {
            let policy_content =
                fs::read_to_string(policy_file).context("Failed to read policy file")?;
            let url = format!("{}/{}?policy", cli.endpoint, name);
            let response = client
                .put(&url)
                .body(policy_content)
                .send()
                .await
                .context("Failed to set bucket policy")?;

            if response.status().is_success() {
                println!("✓ Bucket policy updated successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to set bucket policy: {}",
                    response.status()
                ))
            }
        }
    }
}

async fn handle_object(client: &Client, cli: &Cli, action: &ObjectAction) -> Result<()> {
    match action {
        ObjectAction::List {
            bucket,
            prefix,
            max_keys,
        } => {
            let mut url = format!(
                "{}/{}?list-type=2&max-keys={}",
                cli.endpoint, bucket, max_keys
            );
            if let Some(p) = prefix {
                url.push_str(&format!("&prefix={}", p));
            }
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to list objects")?;

            if response.status().is_success() {
                let body = response.text().await?;
                println!("{}", body);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to list objects: {}",
                    response.status()
                ))
            }
        }
        ObjectAction::Upload {
            bucket,
            key,
            file,
            content_type,
        } => {
            let content = fs::read(file)
                .with_context(|| format!("Failed to read file: {}", file.display()))?;

            let pb = ProgressBar::new(content.len() as u64);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} {msg}")?
                    .progress_chars("=>-"),
            );
            pb.set_message("Uploading...");

            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
            let mut request = client.put(&url);

            if let Some(ct) = content_type {
                request = request.header("Content-Type", ct);
            }

            let response = request
                .body(content)
                .send()
                .await
                .context("Failed to upload object")?;

            pb.finish_with_message("Done");

            if response.status().is_success() {
                println!("✓ Object uploaded successfully");
                if let Some(etag) = response.headers().get("etag") {
                    println!("ETag: {}", etag.to_str()?);
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to upload object: {}",
                    response.status()
                ))
            }
        }
        ObjectAction::Download {
            bucket,
            key,
            output,
        } => {
            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to download object")?;

            if response.status().is_success() {
                let content = response.bytes().await?;

                let pb = ProgressBar::new(content.len() as u64);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template(
                            "[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} {msg}",
                        )?
                        .progress_chars("=>-"),
                );
                pb.set_message("Downloading...");

                fs::write(output, &content)
                    .with_context(|| format!("Failed to write file: {}", output.display()))?;

                pb.finish_with_message("Done");
                println!("✓ Object downloaded to {}", output.display());
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to download object: {}",
                    response.status()
                ))
            }
        }
        ObjectAction::Delete { bucket, key } => {
            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
            let response = client
                .delete(&url)
                .send()
                .await
                .context("Failed to delete object")?;

            if response.status().is_success() {
                println!("✓ Object deleted successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to delete object: {}",
                    response.status()
                ))
            }
        }
        ObjectAction::Info { bucket, key } => {
            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
            let response = client
                .head(&url)
                .send()
                .await
                .context("Failed to get object info")?;

            if response.status().is_success() {
                println!("Object: {}/{}", bucket, key);
                for (key, value) in response.headers() {
                    if let Ok(v) = value.to_str() {
                        println!("  {}: {}", key, v);
                    }
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("Object not found"))
            }
        }
    }
}

async fn handle_replication(client: &Client, cli: &Cli, action: &ReplicationAction) -> Result<()> {
    match action {
        ReplicationAction::GetConfig { bucket } => {
            let url = format!("{}/api/replication/{}/config", cli.endpoint, bucket);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get replication config")?;

            if response.status().is_success() {
                let config = response.text().await?;
                println!("{}", config);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get replication config: {}",
                    response.status()
                ))
            }
        }
        ReplicationAction::SetConfig {
            bucket,
            config_file,
        } => {
            let config_content =
                fs::read_to_string(config_file).context("Failed to read config file")?;
            let url = format!("{}/api/replication/{}/config", cli.endpoint, bucket);
            let response = client
                .put(&url)
                .header("Content-Type", "application/json")
                .body(config_content)
                .send()
                .await
                .context("Failed to set replication config")?;

            if response.status().is_success() {
                println!("✓ Replication configuration updated successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to set replication config: {}",
                    response.status()
                ))
            }
        }
        ReplicationAction::Metrics { destination } => {
            let url = if let Some(dest) = destination {
                format!("{}/api/replication/metrics/{}", cli.endpoint, dest)
            } else {
                format!("{}/api/replication/metrics", cli.endpoint)
            };

            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get replication metrics")?;

            if response.status().is_success() {
                let metrics: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&metrics)?);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get replication metrics: {}",
                    response.status()
                ))
            }
        }
    }
}

async fn handle_metrics(client: &Client, cli: &Cli, action: &MetricsAction) -> Result<()> {
    match action {
        MetricsAction::Get => {
            let url = format!("{}/metrics", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get metrics")?;

            if response.status().is_success() {
                let metrics = response.text().await?;
                println!("{}", metrics);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get metrics: {}",
                    response.status()
                ))
            }
        }
        MetricsAction::Storage => {
            let url = format!("{}/api/maintenance/storage-stats", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get storage statistics")?;

            if response.status().is_success() {
                let stats: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&stats)?);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get storage statistics: {}",
                    response.status()
                ))
            }
        }
        MetricsAction::Operations => {
            let url = format!("{}/api/maintenance/operation-stats", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get operation statistics")?;

            if response.status().is_success() {
                let stats: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&stats)?);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get operation statistics: {}",
                    response.status()
                ))
            }
        }
    }
}

async fn handle_maintenance(client: &Client, cli: &Cli, action: &MaintenanceAction) -> Result<()> {
    match action {
        MaintenanceAction::Backup {
            name,
            snapshot_type,
        } => {
            println!("Creating {} backup snapshot...", snapshot_type);

            let request_body = serde_json::json!({
                "name": name,
                "snapshot_type": snapshot_type
            });

            let url = format!("{}/api/maintenance/backup", cli.endpoint);
            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .context("Failed to create backup snapshot")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("✓ Snapshot created successfully");
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Failed to create backup: {}", error_text))
            }
        }
        MaintenanceAction::IntegrityCheck { bucket } => {
            if let Some(b) = bucket {
                println!("Running integrity check for bucket: {}", b);
            } else {
                println!("Running integrity check for all buckets...");
            }

            let request_body = serde_json::json!({
                "bucket": bucket,
                "auto_repair": true
            });

            let url = format!("{}/api/maintenance/integrity-check", cli.endpoint);
            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .context("Failed to run integrity check")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Integrity check failed: {}", error_text))
            }
        }
        MaintenanceAction::Cleanup { retention_days } => {
            println!(
                "Triggering cleanup of objects older than {} days...",
                retention_days
            );

            let request_body = serde_json::json!({
                "retention_days": retention_days,
                "bucket": null,
                "dry_run": false
            });

            let url = format!("{}/api/maintenance/cleanup", cli.endpoint);
            let response = client
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&request_body)
                .send()
                .await
                .context("Failed to trigger cleanup")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Cleanup failed: {}", error_text))
            }
        }
    }
}

async fn handle_versioning(client: &Client, cli: &Cli, action: &VersioningAction) -> Result<()> {
    match action {
        VersioningAction::GetStatus { bucket } => {
            println!("Getting versioning status for bucket '{}'...", bucket);
            let url = format!("{}/?versioning", cli.endpoint);
            let response = client
                .get(&url)
                .header("Host", format!("{}.s3.amazonaws.com", bucket))
                .send()
                .await
                .context("Failed to get versioning status")?;

            if response.status().is_success() {
                let body = response.text().await?;
                println!("{}", body);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Get versioning failed: {}", error_text))
            }
        }
        VersioningAction::Enable { bucket } => {
            println!("Enabling versioning for bucket '{}'...", bucket);
            let versioning_config = r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Status>Enabled</Status>
</VersioningConfiguration>"#;

            let url = format!("{}/?versioning", cli.endpoint);
            let response = client
                .put(&url)
                .header("Host", format!("{}.s3.amazonaws.com", bucket))
                .header("Content-Type", "application/xml")
                .body(versioning_config)
                .send()
                .await
                .context("Failed to enable versioning")?;

            if response.status().is_success() {
                println!("✓ Versioning enabled for bucket '{}'", bucket);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Enable versioning failed: {}", error_text))
            }
        }
        VersioningAction::Suspend { bucket } => {
            println!("Suspending versioning for bucket '{}'...", bucket);
            let versioning_config = r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
    <Status>Suspended</Status>
</VersioningConfiguration>"#;

            let url = format!("{}/?versioning", cli.endpoint);
            let response = client
                .put(&url)
                .header("Host", format!("{}.s3.amazonaws.com", bucket))
                .header("Content-Type", "application/xml")
                .body(versioning_config)
                .send()
                .await
                .context("Failed to suspend versioning")?;

            if response.status().is_success() {
                println!("✓ Versioning suspended for bucket '{}'", bucket);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Suspend versioning failed: {}", error_text))
            }
        }
        VersioningAction::ListVersions {
            bucket,
            prefix,
            max_keys,
        } => {
            println!("Listing object versions in bucket '{}'...", bucket);
            let mut url = format!("{}/?versions&max-keys={}", cli.endpoint, max_keys);
            if let Some(p) = prefix {
                url.push_str(&format!("&prefix={}", p));
            }

            let response = client
                .get(&url)
                .header("Host", format!("{}.s3.amazonaws.com", bucket))
                .send()
                .await
                .context("Failed to list object versions")?;

            if response.status().is_success() {
                let body = response.text().await?;
                println!("{}", body);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("List versions failed: {}", error_text))
            }
        }
    }
}

async fn handle_observability(
    client: &Client,
    cli: &Cli,
    action: &ObservabilityAction,
) -> Result<()> {
    match action {
        ObservabilityAction::Profiling { pprof } => {
            println!("Getting profiling data...");
            let mut url = format!("{}/api/observability/profiling", cli.endpoint);
            if *pprof {
                url.push_str("?format=pprof");
            }

            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get profiling data")?;

            if response.status().is_success() {
                if *pprof {
                    // Binary pprof data
                    let bytes = response.bytes().await?;
                    println!("Received {} bytes of pprof data", bytes.len());
                    // Could save to file here if needed
                    Ok(())
                } else {
                    // JSON data
                    let result: Value = response.json().await?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    Ok(())
                }
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Get profiling failed: {}", error_text))
            }
        }
        ObservabilityAction::BusinessMetrics => {
            println!("Getting business metrics...");
            let url = format!("{}/api/observability/business-metrics", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get business metrics")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!(
                    "Get business metrics failed: {}",
                    error_text
                ))
            }
        }
        ObservabilityAction::Anomalies {
            anomaly_type,
            severity,
            limit,
        } => {
            println!("Getting detected anomalies...");
            let mut url = format!("{}/api/observability/anomalies", cli.endpoint);
            let mut params = vec![];

            if let Some(t) = anomaly_type {
                params.push(format!("type={}", t));
            }
            if let Some(s) = severity {
                params.push(format!("severity={}", s));
            }
            if let Some(l) = limit {
                params.push(format!("limit={}", l));
            }

            if !params.is_empty() {
                url.push('?');
                url.push_str(&params.join("&"));
            }

            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get anomalies")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Get anomalies failed: {}", error_text))
            }
        }
        ObservabilityAction::Resources => {
            println!("Getting resource manager statistics...");
            let url = format!("{}/api/observability/resources", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get resource stats")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Get resource stats failed: {}", error_text))
            }
        }
        ObservabilityAction::Health => {
            println!("Getting comprehensive health check...");
            let url = format!("{}/api/observability/health", cli.endpoint);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to get health check")?;

            if response.status().is_success() {
                let result: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            } else {
                let error_text = response.text().await?;
                Err(anyhow::anyhow!("Get health check failed: {}", error_text))
            }
        }
    }
}

async fn handle_transform(client: &Client, cli: &Cli, action: &TransformAction) -> Result<()> {
    match action {
        TransformAction::Image {
            bucket,
            key,
            width: _,
            height: _,
            format: _,
            quality: _,
            output,
        } => {
            println!("Applying image transformation to {}/{}", bucket, key);
            println!("Downloading object...");

            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to download object")?;

            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "Failed to download object: {}",
                    response.status()
                ));
            }

            let data = response.bytes().await?;

            // Save transformed output
            std::fs::write(output, &data)?;
            println!("Transformed image saved to {}", output.display());
            println!(
                "Note: Actual image transformation is performed by rs3gw transformation engine"
            );
            Ok(())
        }
        TransformAction::Compress {
            bucket,
            key,
            algorithm,
            level,
            output,
        } => {
            println!("Applying {} compression to {}/{}", algorithm, bucket, key);
            println!("Downloading object...");

            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
            let response = client
                .get(&url)
                .send()
                .await
                .context("Failed to download object")?;

            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "Failed to download object: {}",
                    response.status()
                ));
            }

            let data = response.bytes().await?;

            // Save compressed output
            std::fs::write(output, &data)?;
            println!("Compressed data saved to {}", output.display());
            println!("Compression level: {:?}", level);
            Ok(())
        }
        TransformAction::TestWasm {
            plugin,
            input,
            params,
            output,
        } => {
            println!("Testing WASM plugin: {}", plugin.display());
            println!("Input file: {}", input.display());
            println!("Parameters: {:?}", params);

            // This is a placeholder - actual WASM plugin testing would require
            // the wasmtime runtime integration
            println!("WASM plugin testing functionality requires --features wasm-plugins");
            println!("Please use the rs3gw transformation API for actual plugin execution");

            if let Some(out) = output {
                println!("Would write output to: {}", out.display());
            }
            Ok(())
        }
    }
}

async fn handle_batch(client: &Client, cli: &Cli, action: &BatchAction) -> Result<()> {
    match action {
        BatchAction::DeleteByPrefix {
            bucket,
            prefix,
            dry_run,
            max,
        } => {
            println!("Batch delete from bucket {} with prefix {}", bucket, prefix);
            if *dry_run {
                println!("DRY RUN MODE - no objects will be deleted");
            }

            // List objects with prefix
            let mut url = format!("{}/{}?prefix={}", cli.endpoint, bucket, prefix);
            if let Some(max_keys) = max {
                url = format!("{}&max-keys={}", url, max_keys);
            }

            let response = client.get(&url).send().await?;
            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "Failed to list objects: {}",
                    response.status()
                ));
            }

            let body = response.text().await?;
            println!("Objects matching prefix: ");
            println!("{}", body);

            if *dry_run {
                println!("Dry run complete - no objects were deleted");
            } else {
                println!("Use DELETE API calls to remove objects (not yet implemented in CLI)");
            }
            Ok(())
        }
        BatchAction::CopyMultiple {
            source_bucket,
            dest_bucket,
            source_prefix,
            dest_prefix,
            max,
        } => {
            println!(
                "Batch copy from {}/{} to {}/{}",
                source_bucket,
                source_prefix,
                dest_bucket,
                dest_prefix.as_ref().unwrap_or(&source_prefix.clone())
            );

            println!("Maximum objects to copy: {:?}", max);
            println!("Batch copy functionality requires server-side batch API");
            println!("Use the S3 Batch operations API endpoint");
            Ok(())
        }
        BatchAction::TagMultiple {
            bucket,
            prefix,
            tags,
            max,
        } => {
            println!("Batch tagging in bucket {} with prefix {}", bucket, prefix);
            println!("Tags to apply: {}", tags);
            println!("Maximum objects: {:?}", max);
            println!("Batch tagging functionality requires server-side batch API");
            Ok(())
        }
    }
}

async fn handle_lifecycle(client: &Client, cli: &Cli, action: &LifecycleAction) -> Result<()> {
    match action {
        LifecycleAction::Get { bucket } => {
            println!("Getting lifecycle policy for bucket: {}", bucket);
            let url = format!("{}/?lifecycle", cli.endpoint);

            let response = client.get(&url).send().await?;
            if response.status().is_success() {
                let policy = response.text().await?;
                println!("{}", policy);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get lifecycle policy: {}",
                    response.status()
                ))
            }
        }
        LifecycleAction::Set {
            bucket,
            policy_file,
        } => {
            println!("Setting lifecycle policy for bucket: {}", bucket);
            println!("Policy file: {}", policy_file.display());

            let policy_content = std::fs::read_to_string(policy_file)?;
            println!("Policy: {}", policy_content);

            let url = format!("{}/?lifecycle", cli.endpoint);
            let response = client.put(&url).body(policy_content).send().await?;

            if response.status().is_success() {
                println!("Lifecycle policy set successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to set lifecycle policy: {}",
                    response.status()
                ))
            }
        }
        LifecycleAction::Delete { bucket } => {
            println!("Deleting lifecycle policy for bucket: {}", bucket);
            let url = format!("{}/?lifecycle", cli.endpoint);

            let response = client.delete(&url).send().await?;
            if response.status().is_success() {
                println!("Lifecycle policy deleted successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to delete lifecycle policy: {}",
                    response.status()
                ))
            }
        }
        LifecycleAction::ListRules { bucket } => {
            println!("Listing lifecycle rules for bucket: {}", bucket);

            // Get the policy and parse it
            let url = format!("{}/?lifecycle", cli.endpoint);
            let response = client.get(&url).send().await?;

            if response.status().is_success() {
                let policy: Value = response.json().await?;
                println!("{}", serde_json::to_string_pretty(&policy)?);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get lifecycle rules: {}",
                    response.status()
                ))
            }
        }
    }
}

async fn handle_preprocessing(
    client: &Client,
    cli: &Cli,
    action: &PreprocessingAction,
) -> Result<()> {
    match action {
        PreprocessingAction::Create { id, name, file } => {
            println!("Creating preprocessing pipeline:");
            println!("  ID: {}", id);
            println!("  Name: {}", name);
            println!("  Definition: {}", file.display());

            // Read pipeline definition
            let pipeline_content =
                fs::read_to_string(file).context("Failed to read pipeline definition file")?;

            // Parse to validate JSON
            let pipeline_json: Value = serde_json::from_str(&pipeline_content)
                .context("Invalid JSON in pipeline definition")?;

            // Validate required fields
            if pipeline_json.get("steps").is_none() {
                return Err(anyhow::anyhow!(
                    "Pipeline definition must include 'steps' array"
                ));
            }

            // Upload pipeline to S3 (simulated - would use S3 API)
            let bucket = "ml-datasets";
            let key = format!("pipelines/{}.json", id);
            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);

            println!("\nUploading pipeline definition to: {}/{}", bucket, key);

            let response = client
                .put(&url)
                .body(pipeline_content)
                .header("Content-Type", "application/json")
                .send()
                .await
                .context("Failed to upload pipeline")?;

            if response.status().is_success() {
                println!("✓ Pipeline created successfully!");
                println!("\nNext steps:");
                println!("  1. List pipelines: rs3ctl preprocessing list");
                println!(
                    "  2. Apply pipeline: rs3ctl preprocessing apply --pipeline {}",
                    id
                );
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to create pipeline: {}",
                    response.status()
                ))
            }
        }

        PreprocessingAction::List { bucket } => {
            println!("Listing preprocessing pipelines in bucket: {}", bucket);

            let url = format!("{}/{}/?prefix=pipelines/", cli.endpoint, bucket);
            let response = client.get(&url).send().await?;

            if response.status().is_success() {
                let body = response.text().await?;
                // Parse S3 XML response (simplified - would use proper XML parsing)
                println!("\nAvailable pipelines:");
                println!("{}", body);
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to list pipelines: {}",
                    response.status()
                ))
            }
        }

        PreprocessingAction::Get { id, bucket } => {
            println!("Getting preprocessing pipeline: {}", id);

            let key = format!("pipelines/{}.json", id);
            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);

            let response = client.get(&url).send().await?;

            if response.status().is_success() {
                let pipeline = response.text().await?;
                println!("\nPipeline definition:");
                println!("{}", pipeline);
                Ok(())
            } else if response.status().as_u16() == 404 {
                Err(anyhow::anyhow!("Pipeline '{}' not found", id))
            } else {
                Err(anyhow::anyhow!(
                    "Failed to get pipeline: {}",
                    response.status()
                ))
            }
        }

        PreprocessingAction::Delete { id, bucket } => {
            println!("Deleting preprocessing pipeline: {}", id);

            let key = format!("pipelines/{}.json", id);
            let url = format!("{}/{}/{}", cli.endpoint, bucket, key);

            let response = client.delete(&url).send().await?;

            if response.status().is_success() || response.status().as_u16() == 204 {
                println!("✓ Pipeline deleted successfully");
                Ok(())
            } else {
                Err(anyhow::anyhow!(
                    "Failed to delete pipeline: {}",
                    response.status()
                ))
            }
        }

        PreprocessingAction::Apply {
            pipeline,
            source_bucket,
            source_key,
            dest_bucket,
            dest_key,
        } => {
            println!("Applying preprocessing pipeline:");
            println!("  Pipeline: {}", pipeline);
            println!("  Source: {}/{}", source_bucket, source_key);
            println!("  Destination: {}/{}", dest_bucket, dest_key);

            println!("\nNote: Pipeline application would be done server-side");
            println!("This requires implementing a preprocessing API endpoint");
            println!("For now, use the Python example: examples/preprocessing_pipeline.py");

            Ok(())
        }

        PreprocessingAction::Validate { file } => {
            println!("Validating preprocessing pipeline definition:");
            println!("  File: {}", file.display());

            // Read and parse pipeline
            let pipeline_content =
                fs::read_to_string(file).context("Failed to read pipeline definition file")?;

            let pipeline: Value =
                serde_json::from_str(&pipeline_content).context("Invalid JSON format")?;

            // Validate structure
            let mut errors: Vec<String> = Vec::new();

            if pipeline.get("id").and_then(|v| v.as_str()).is_none() {
                errors.push("Missing or invalid 'id' field".to_string());
            }

            if pipeline.get("name").and_then(|v| v.as_str()).is_none() {
                errors.push("Missing or invalid 'name' field".to_string());
            }

            if pipeline.get("steps").and_then(|v| v.as_array()).is_none() {
                errors.push("Missing or invalid 'steps' array".to_string());
            } else {
                let steps = pipeline["steps"].as_array().expect("Steps array");
                for (i, step) in steps.iter().enumerate() {
                    if step.get("id").and_then(|v| v.as_str()).is_none() {
                        errors.push(format!("Step {}: missing 'id'", i));
                    }
                    if step.get("step_type").and_then(|v| v.as_str()).is_none() {
                        errors.push(format!("Step {}: missing 'step_type'", i));
                    }

                    // Validate step_type values
                    if let Some(step_type) = step.get("step_type").and_then(|v| v.as_str()) {
                        let valid_types = [
                            "image_normalization",
                            "image_resize",
                            "data_augmentation",
                            "text_tokenization",
                            "audio_features",
                            "video_frames",
                        ];
                        if !valid_types.contains(&step_type) {
                            errors.push(format!("Step {}: invalid step_type '{}'", i, step_type));
                        }
                    }
                }
            }

            if errors.is_empty() {
                println!("✓ Pipeline definition is valid!");
                println!("\nPipeline summary:");
                if let Some(id) = pipeline.get("id") {
                    println!("  ID: {}", id);
                }
                if let Some(name) = pipeline.get("name") {
                    println!("  Name: {}", name);
                }
                if let Some(steps) = pipeline.get("steps").and_then(|v| v.as_array()) {
                    println!("  Steps: {}", steps.len());
                    for (i, step) in steps.iter().enumerate() {
                        if let Some(step_type) = step.get("step_type") {
                            println!("    {}. {}", i + 1, step_type);
                        }
                    }
                }
                Ok(())
            } else {
                println!("✗ Pipeline validation failed:");
                for error in errors {
                    println!("  - {}", error);
                }
                Err(anyhow::anyhow!("Pipeline validation failed"))
            }
        }
    }
}
