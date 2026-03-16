//! Handler for preprocessing pipeline commands

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use std::fs;

use crate::types::PreprocessingAction;
use crate::Cli;

pub async fn handle_preprocessing(
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

            let pipeline_content =
                fs::read_to_string(file).context("Failed to read pipeline definition file")?;

            let pipeline_json: Value = serde_json::from_str(&pipeline_content)
                .context("Invalid JSON in pipeline definition")?;

            if pipeline_json.get("steps").is_none() {
                return Err(anyhow::anyhow!(
                    "Pipeline definition must include 'steps' array"
                ));
            }

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

            let pipeline_content =
                fs::read_to_string(file).context("Failed to read pipeline definition file")?;

            let pipeline: Value =
                serde_json::from_str(&pipeline_content).context("Invalid JSON format")?;

            let mut errors: Vec<String> = Vec::new();

            if pipeline.get("id").and_then(|v| v.as_str()).is_none() {
                errors.push("Missing or invalid 'id' field".to_string());
            }

            if pipeline.get("name").and_then(|v| v.as_str()).is_none() {
                errors.push("Missing or invalid 'name' field".to_string());
            }

            if let Some(steps) = pipeline.get("steps").and_then(|v| v.as_array()) {
                for (i, step) in steps.iter().enumerate() {
                    if step.get("id").and_then(|v| v.as_str()).is_none() {
                        errors.push(format!("Step {}: missing 'id'", i));
                    }
                    if step.get("step_type").and_then(|v| v.as_str()).is_none() {
                        errors.push(format!("Step {}: missing 'step_type'", i));
                    }

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
                            errors.push(format!(
                                "Step {}: invalid step_type '{}'",
                                i, step_type
                            ));
                        }
                    }
                }
            } else {
                errors.push("Missing or invalid 'steps' array".to_string());
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
                for error in &errors {
                    println!("  - {}", error);
                }
                Err(anyhow::anyhow!("Pipeline validation failed"))
            }
        }
    }
}
