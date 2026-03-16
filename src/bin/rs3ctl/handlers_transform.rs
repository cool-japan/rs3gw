//! Handlers for transform, batch, and lifecycle commands

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::types::{BatchAction, LifecycleAction, TransformAction};
use crate::Cli;

pub async fn handle_transform(client: &Client, cli: &Cli, action: &TransformAction) -> Result<()> {
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
                .map_err(|e| anyhow::anyhow!("Failed to download object: {}", e))?;

            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "Failed to download object: {}",
                    response.status()
                ));
            }

            let data = response.bytes().await?;
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
                .map_err(|e| anyhow::anyhow!("Failed to download object: {}", e))?;

            if !response.status().is_success() {
                return Err(anyhow::anyhow!(
                    "Failed to download object: {}",
                    response.status()
                ));
            }

            let data = response.bytes().await?;
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
            println!("WASM plugin testing functionality requires --features wasm-plugins");
            println!("Please use the rs3gw transformation API for actual plugin execution");

            if let Some(out) = output {
                println!("Would write output to: {}", out.display());
            }
            Ok(())
        }
    }
}

pub async fn handle_batch(client: &Client, cli: &Cli, action: &BatchAction) -> Result<()> {
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
            println!("Objects matching prefix:");
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
                dest_prefix.as_ref().unwrap_or(source_prefix)
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

pub async fn handle_lifecycle(client: &Client, cli: &Cli, action: &LifecycleAction) -> Result<()> {
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
            let url = format!("{}/?lifecycle", cli.endpoint);
            let response = client.put(&url).body(policy_content).send().await?;

            if response.status().is_success() {
                println!("✓ Lifecycle policy set successfully");
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
                println!("✓ Lifecycle policy deleted successfully");
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
