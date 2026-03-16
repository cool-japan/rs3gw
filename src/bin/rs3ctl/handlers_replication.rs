//! Handlers for replication, metrics, and maintenance commands

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::fs;

use crate::types::{MaintenanceAction, MetricsAction, ReplicationAction};
use crate::Cli;

pub async fn handle_replication(
    client: &Client,
    cli: &Cli,
    action: &ReplicationAction,
) -> Result<()> {
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

pub async fn handle_metrics(client: &Client, cli: &Cli, action: &MetricsAction) -> Result<()> {
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

pub async fn handle_maintenance(
    client: &Client,
    cli: &Cli,
    action: &MaintenanceAction,
) -> Result<()> {
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
