//! Handlers for versioning and observability commands

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;

use crate::types::{ObservabilityAction, VersioningAction};
use crate::Cli;

pub async fn handle_versioning(
    client: &Client,
    cli: &Cli,
    action: &VersioningAction,
) -> Result<()> {
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

pub async fn handle_observability(
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
                    let bytes = response.bytes().await?;
                    println!("Received {} bytes of pprof data", bytes.len());
                    Ok(())
                } else {
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
            let mut params: Vec<String> = Vec::new();

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
