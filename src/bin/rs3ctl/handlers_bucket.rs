//! Handlers for bucket and object commands

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use std::fs;

use crate::types::{BucketAction, ObjectAction};
use crate::Cli;

pub async fn handle_bucket(client: &Client, cli: &Cli, action: &BucketAction) -> Result<()> {
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

pub async fn handle_object(client: &Client, cli: &Cli, action: &ObjectAction) -> Result<()> {
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
                    if let Ok(v) = etag.to_str() {
                        println!("ETag: {}", v);
                    }
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
                for (hdr_key, value) in response.headers() {
                    if let Ok(v) = value.to_str() {
                        println!("  {}: {}", hdr_key, v);
                    }
                }
                Ok(())
            } else {
                Err(anyhow::anyhow!("Object not found"))
            }
        }
    }
}
