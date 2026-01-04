// S3 Migration Tool for rs3gw
// Migrate data between S3-compatible storage systems

use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::{config::Region, Client as S3Client};
use clap::{Parser, Subcommand};
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Parser)]
#[clap(name = "s3-migrate")]
#[clap(about = "Migrate data between S3-compatible storage systems", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Copy objects from source to destination
    Copy {
        /// Source endpoint URL
        #[clap(long, value_parser)]
        source_endpoint: String,

        /// Source access key
        #[clap(long, value_parser)]
        source_access_key: String,

        /// Source secret key
        #[clap(long, value_parser)]
        source_secret_key: String,

        /// Source region
        #[clap(long, default_value = "us-east-1")]
        source_region: String,

        /// Source bucket
        #[clap(long, value_parser)]
        source_bucket: String,

        /// Destination endpoint URL
        #[clap(long, value_parser)]
        dest_endpoint: String,

        /// Destination access key
        #[clap(long, value_parser)]
        dest_access_key: String,

        /// Destination secret key
        #[clap(long, value_parser)]
        dest_secret_key: String,

        /// Destination region
        #[clap(long, default_value = "us-east-1")]
        dest_region: String,

        /// Destination bucket
        #[clap(long, value_parser)]
        dest_bucket: String,

        /// Prefix filter (only migrate objects with this prefix)
        #[clap(long)]
        prefix: Option<String>,

        /// Number of concurrent transfers
        #[clap(long, default_value = "10")]
        concurrency: usize,

        /// Dry run (list objects without copying)
        #[clap(long)]
        dry_run: bool,
    },

    /// Sync objects from source to destination (incremental)
    Sync {
        /// Source endpoint URL
        #[clap(long, value_parser)]
        source_endpoint: String,

        /// Source access key
        #[clap(long, value_parser)]
        source_access_key: String,

        /// Source secret key
        #[clap(long, value_parser)]
        source_secret_key: String,

        /// Source region
        #[clap(long, default_value = "us-east-1")]
        source_region: String,

        /// Source bucket
        #[clap(long, value_parser)]
        source_bucket: String,

        /// Destination endpoint URL
        #[clap(long, value_parser)]
        dest_endpoint: String,

        /// Destination access key
        #[clap(long, value_parser)]
        dest_access_key: String,

        /// Destination secret key
        #[clap(long, value_parser)]
        dest_secret_key: String,

        /// Destination region
        #[clap(long, default_value = "us-east-1")]
        dest_region: String,

        /// Destination bucket
        #[clap(long, value_parser)]
        dest_bucket: String,

        /// Prefix filter
        #[clap(long)]
        prefix: Option<String>,

        /// Number of concurrent transfers
        #[clap(long, default_value = "10")]
        concurrency: usize,

        /// Delete objects from destination that don't exist in source
        #[clap(long)]
        delete: bool,
    },

    /// Verify that source and destination match
    Verify {
        /// Source endpoint URL
        #[clap(long, value_parser)]
        source_endpoint: String,

        /// Source access key
        #[clap(long, value_parser)]
        source_access_key: String,

        /// Source secret key
        #[clap(long, value_parser)]
        source_secret_key: String,

        /// Source region
        #[clap(long, default_value = "us-east-1")]
        source_region: String,

        /// Source bucket
        #[clap(long, value_parser)]
        source_bucket: String,

        /// Destination endpoint URL
        #[clap(long, value_parser)]
        dest_endpoint: String,

        /// Destination access key
        #[clap(long, value_parser)]
        dest_access_key: String,

        /// Destination secret key
        #[clap(long, value_parser)]
        dest_secret_key: String,

        /// Destination region
        #[clap(long, default_value = "us-east-1")]
        dest_region: String,

        /// Destination bucket
        #[clap(long, value_parser)]
        dest_bucket: String,

        /// Prefix filter
        #[clap(long)]
        prefix: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Copy {
            source_endpoint,
            source_access_key,
            source_secret_key,
            source_region,
            source_bucket,
            dest_endpoint,
            dest_access_key,
            dest_secret_key,
            dest_region,
            dest_bucket,
            prefix,
            concurrency,
            dry_run,
        } => {
            let source_client = create_s3_client(
                &source_endpoint,
                &source_access_key,
                &source_secret_key,
                &source_region,
            )
            .await?;

            let dest_client = create_s3_client(
                &dest_endpoint,
                &dest_access_key,
                &dest_secret_key,
                &dest_region,
            )
            .await?;

            copy_objects(
                source_client,
                &source_bucket,
                dest_client,
                &dest_bucket,
                prefix.as_deref(),
                concurrency,
                dry_run,
            )
            .await?;
        }

        Commands::Sync {
            source_endpoint,
            source_access_key,
            source_secret_key,
            source_region,
            source_bucket,
            dest_endpoint,
            dest_access_key,
            dest_secret_key,
            dest_region,
            dest_bucket,
            prefix,
            concurrency,
            delete,
        } => {
            let source_client = create_s3_client(
                &source_endpoint,
                &source_access_key,
                &source_secret_key,
                &source_region,
            )
            .await?;

            let dest_client = create_s3_client(
                &dest_endpoint,
                &dest_access_key,
                &dest_secret_key,
                &dest_region,
            )
            .await?;

            sync_objects(
                source_client,
                &source_bucket,
                dest_client,
                &dest_bucket,
                prefix.as_deref(),
                concurrency,
                delete,
            )
            .await?;
        }

        Commands::Verify {
            source_endpoint,
            source_access_key,
            source_secret_key,
            source_region,
            source_bucket,
            dest_endpoint,
            dest_access_key,
            dest_secret_key,
            dest_region,
            dest_bucket,
            prefix,
        } => {
            let source_client = create_s3_client(
                &source_endpoint,
                &source_access_key,
                &source_secret_key,
                &source_region,
            )
            .await?;

            let dest_client = create_s3_client(
                &dest_endpoint,
                &dest_access_key,
                &dest_secret_key,
                &dest_region,
            )
            .await?;

            verify_objects(
                source_client,
                &source_bucket,
                dest_client,
                &dest_bucket,
                prefix.as_deref(),
            )
            .await?;
        }
    }

    Ok(())
}

async fn create_s3_client(
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Result<S3Client> {
    let creds =
        aws_sdk_s3::config::Credentials::new(access_key, secret_key, None, None, "s3-migrate");

    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(region.to_string()))
        .credentials_provider(creds)
        .endpoint_url(endpoint)
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();

    Ok(S3Client::from_conf(s3_config))
}

async fn copy_objects(
    source_client: S3Client,
    source_bucket: &str,
    dest_client: S3Client,
    dest_bucket: &str,
    prefix: Option<&str>,
    concurrency: usize,
    dry_run: bool,
) -> Result<()> {
    println!(
        "Copying objects from {} to {}...",
        source_bucket, dest_bucket
    );
    if dry_run {
        println!("DRY RUN - No objects will be copied");
    }

    // List all objects in source bucket
    let mut continuation_token: Option<String> = None;
    let mut all_objects = Vec::new();

    loop {
        let mut request = source_client
            .list_objects_v2()
            .bucket(source_bucket)
            .max_keys(1000);

        if let Some(prefix) = prefix {
            request = request.prefix(prefix);
        }

        if let Some(token) = continuation_token {
            request = request.continuation_token(token);
        }

        let response = request
            .send()
            .await
            .context("Failed to list source objects")?;

        let is_truncated = response.is_truncated() == Some(true);
        let next_token = response.next_continuation_token.clone();

        if let Some(contents) = response.contents {
            all_objects.extend(contents);
        }

        if is_truncated {
            continuation_token = next_token;
        } else {
            break;
        }
    }

    println!("Found {} objects to copy", all_objects.len());

    if dry_run {
        for obj in &all_objects {
            if let Some(key) = &obj.key {
                println!("  {}", key);
            }
        }
        return Ok(());
    }

    // Create progress bar
    let multi_progress = MultiProgress::new();
    let overall_pb = multi_progress.add(ProgressBar::new(all_objects.len() as u64));
    overall_pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} ({eta})")
            .context("Failed to create progress style")?,
    );

    // Copy objects with concurrency control
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let source_client = Arc::new(source_client);
    let dest_client = Arc::new(dest_client);
    let source_bucket = Arc::new(source_bucket.to_string());
    let dest_bucket = Arc::new(dest_bucket.to_string());

    let tasks = futures::stream::iter(all_objects.iter().map(|obj| {
        let key = obj.key.clone().unwrap_or_default();
        let source_client = source_client.clone();
        let dest_client = dest_client.clone();
        let source_bucket = source_bucket.clone();
        let dest_bucket = dest_bucket.clone();
        let semaphore = semaphore.clone();
        let pb = overall_pb.clone();

        async move {
            let _permit = semaphore
                .acquire()
                .await
                .map_err(|e| anyhow::anyhow!("Semaphore error: {}", e))?;

            // Get object from source
            let get_response = source_client
                .get_object()
                .bucket(source_bucket.as_str())
                .key(&key)
                .send()
                .await
                .context(format!("Failed to get object: {}", key))?;

            let body = get_response
                .body
                .collect()
                .await
                .context(format!("Failed to read object body: {}", key))?;

            // Put object to destination
            dest_client
                .put_object()
                .bucket(dest_bucket.as_str())
                .key(&key)
                .body(body.into_bytes().into())
                .send()
                .await
                .context(format!("Failed to put object: {}", key))?;

            pb.inc(1);
            Result::<()>::Ok(())
        }
    }))
    .buffer_unordered(concurrency)
    .collect::<Vec<_>>();

    let results = tasks.await;
    overall_pb.finish_with_message("Done");

    // Report errors
    let errors: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();
    if !errors.is_empty() {
        println!("\nErrors occurred during migration:");
        for error in &errors {
            println!("  {}", error);
        }
        anyhow::bail!("{} errors occurred", errors.len());
    }

    println!("\nMigration complete!");
    Ok(())
}

async fn sync_objects(
    source_client: S3Client,
    source_bucket: &str,
    dest_client: S3Client,
    dest_bucket: &str,
    prefix: Option<&str>,
    concurrency: usize,
    delete: bool,
) -> Result<()> {
    println!(
        "Syncing objects from {} to {}...",
        source_bucket, dest_bucket
    );

    // List source objects
    let source_objects = list_all_objects(&source_client, source_bucket, prefix).await?;
    let dest_objects = list_all_objects(&dest_client, dest_bucket, prefix).await?;

    // Build destination object map for quick lookup
    let mut dest_map = std::collections::HashMap::new();
    for obj in &dest_objects {
        if let Some(key) = &obj.key {
            dest_map.insert(key.clone(), obj);
        }
    }

    // Find objects to copy (new or modified)
    let mut objects_to_copy = Vec::new();
    for obj in &source_objects {
        if let Some(key) = &obj.key {
            let should_copy = match dest_map.get(key) {
                None => true, // Object doesn't exist in destination
                Some(dest_obj) => {
                    // Compare ETags
                    obj.e_tag != dest_obj.e_tag
                }
            };

            if should_copy {
                objects_to_copy.push(obj.clone());
            }
        }
    }

    println!("Found {} objects to copy", objects_to_copy.len());

    // Copy objects
    if !objects_to_copy.is_empty() {
        // Reuse copy logic
        let semaphore = Arc::new(Semaphore::new(concurrency));
        let source_client = Arc::new(source_client);
        let dest_client_arc = Arc::new(dest_client.clone());
        let source_bucket = Arc::new(source_bucket.to_string());
        let dest_bucket_arc = Arc::new(dest_bucket.to_string());

        let tasks = futures::stream::iter(objects_to_copy.iter().map(|obj| {
            let key = obj.key.clone().unwrap_or_default();
            let source_client = source_client.clone();
            let dest_client = dest_client_arc.clone();
            let source_bucket = source_bucket.clone();
            let dest_bucket = dest_bucket_arc.clone();
            let semaphore = semaphore.clone();

            async move {
                let _permit = semaphore
                    .acquire()
                    .await
                    .map_err(|e| anyhow::anyhow!("Semaphore error: {}", e))?;

                let get_response = source_client
                    .get_object()
                    .bucket(source_bucket.as_str())
                    .key(&key)
                    .send()
                    .await
                    .context(format!("Failed to get object: {}", key))?;

                let body = get_response
                    .body
                    .collect()
                    .await
                    .context(format!("Failed to read object body: {}", key))?;

                dest_client
                    .put_object()
                    .bucket(dest_bucket.as_str())
                    .key(&key)
                    .body(body.into_bytes().into())
                    .send()
                    .await
                    .context(format!("Failed to put object: {}", key))?;

                println!("  Copied: {}", key);
                Result::<()>::Ok(())
            }
        }))
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>();

        let results = tasks.await;
        let errors: Vec<_> = results.into_iter().filter_map(|r| r.err()).collect();
        if !errors.is_empty() {
            println!("\nErrors occurred during sync:");
            for error in &errors {
                println!("  {}", error);
            }
            anyhow::bail!("{} errors occurred", errors.len());
        }
    }

    // Handle deletions
    if delete {
        let source_keys: std::collections::HashSet<_> = source_objects
            .iter()
            .filter_map(|obj| obj.key.clone())
            .collect();

        let mut objects_to_delete = Vec::new();
        for obj in &dest_objects {
            if let Some(key) = &obj.key {
                if !source_keys.contains(key) {
                    objects_to_delete.push(key.clone());
                }
            }
        }

        if !objects_to_delete.is_empty() {
            println!(
                "Deleting {} objects from destination",
                objects_to_delete.len()
            );
            for key in objects_to_delete {
                dest_client
                    .delete_object()
                    .bucket(dest_bucket)
                    .key(&key)
                    .send()
                    .await
                    .context(format!("Failed to delete object: {}", key))?;
                println!("  Deleted: {}", key);
            }
        }
    }

    println!("\nSync complete!");
    Ok(())
}

async fn verify_objects(
    source_client: S3Client,
    source_bucket: &str,
    dest_client: S3Client,
    dest_bucket: &str,
    prefix: Option<&str>,
) -> Result<()> {
    println!(
        "Verifying objects between {} and {}...",
        source_bucket, dest_bucket
    );

    let source_objects = list_all_objects(&source_client, source_bucket, prefix).await?;
    let dest_objects = list_all_objects(&dest_client, dest_bucket, prefix).await?;

    let mut dest_map = std::collections::HashMap::new();
    for obj in &dest_objects {
        if let Some(key) = &obj.key {
            dest_map.insert(key.clone(), obj);
        }
    }

    let mut missing = Vec::new();
    let mut mismatch = Vec::new();
    let mut matched = 0;

    for obj in &source_objects {
        if let Some(key) = &obj.key {
            match dest_map.get(key) {
                None => missing.push(key.clone()),
                Some(dest_obj) => {
                    if obj.e_tag != dest_obj.e_tag {
                        mismatch.push(key.clone());
                    } else {
                        matched += 1;
                    }
                }
            }
        }
    }

    println!("\nVerification Results:");
    println!("  Matched: {}", matched);
    println!("  Missing in destination: {}", missing.len());
    println!("  ETag mismatch: {}", mismatch.len());

    if !missing.is_empty() {
        println!("\nMissing objects:");
        for key in &missing {
            println!("  {}", key);
        }
    }

    if !mismatch.is_empty() {
        println!("\nMismatched objects:");
        for key in &mismatch {
            println!("  {}", key);
        }
    }

    if missing.is_empty() && mismatch.is_empty() {
        println!("\n✓ All objects match!");
        Ok(())
    } else {
        anyhow::bail!(
            "Verification failed: {} missing, {} mismatched",
            missing.len(),
            mismatch.len()
        )
    }
}

async fn list_all_objects(
    client: &S3Client,
    bucket: &str,
    prefix: Option<&str>,
) -> Result<Vec<aws_sdk_s3::types::Object>> {
    let mut continuation_token: Option<String> = None;
    let mut all_objects = Vec::new();

    loop {
        let mut request = client.list_objects_v2().bucket(bucket).max_keys(1000);

        if let Some(prefix) = prefix {
            request = request.prefix(prefix);
        }

        if let Some(token) = continuation_token {
            request = request.continuation_token(token);
        }

        let response = request
            .send()
            .await
            .context(format!("Failed to list objects in bucket: {}", bucket))?;

        let is_truncated = response.is_truncated() == Some(true);
        let next_token = response.next_continuation_token.clone();

        if let Some(contents) = response.contents {
            all_objects.extend(contents);
        }

        if is_truncated {
            continuation_token = next_token;
        } else {
            break;
        }
    }

    Ok(all_objects)
}
