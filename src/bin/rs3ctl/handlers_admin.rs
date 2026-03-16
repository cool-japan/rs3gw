//! Admin handlers: config, server-info, benchmark, diagnose
//!
//! These are new Phase-2 commands providing deep inspection and
//! diagnostic capabilities for rs3gw administrators.

use std::time::Instant;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde_json::Value;

use crate::types::{ConfigAction, LatencyStats, ServerInfoAction};
use crate::Cli;

// ── Config ────────────────────────────────────────────────────────────────────

/// Handle `rs3ctl config <subcommand>`
pub async fn handle_config(client: &Client, cli: &Cli, action: &ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Show => handle_config_show(client, cli).await,
        ConfigAction::Validate { config_file } => handle_config_validate(config_file).await,
    }
}

async fn handle_config_show(client: &Client, cli: &Cli) -> Result<()> {
    // Try the dedicated config endpoint first, fall back to health for basic info
    let config_url = format!("{}/api/v1/config", cli.endpoint);
    let response = client
        .get(&config_url)
        .send()
        .await
        .context("Failed to reach server config endpoint")?;

    if response.status().is_success() {
        let body: Value = response
            .json()
            .await
            .context("Server returned non-JSON response from config endpoint")?;
        println!("=== Server Configuration ===");
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(());
    }

    // Fall back: show health endpoint response (partial config info)
    eprintln!(
        "Note: /api/v1/config returned {} — falling back to /health",
        response.status()
    );
    let health_url = format!("{}/health", cli.endpoint);
    let health_resp = client
        .get(&health_url)
        .send()
        .await
        .context("Failed to reach health endpoint")?;

    if health_resp.status().is_success() {
        let body = health_resp.text().await?;
        println!("=== Server Health / Partial Config ===");
        println!("{}", body);
        println!("\nHint: A full /api/v1/config endpoint is not yet exposed by this server.");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Server unreachable — health endpoint returned {}",
            health_resp.status()
        ))
    }
}

async fn handle_config_validate(config_file: &Option<String>) -> Result<()> {
    let path = match config_file {
        Some(p) => p.clone(),
        None => {
            // Look for default config locations
            let candidates = [
                "rs3gw.toml",
                "config/rs3gw.toml",
                "/etc/rs3gw/config.toml",
                "rs3gw.json",
            ];
            candidates
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No config file specified and no default config found. \
                         Use --config-file <path>"
                    )
                })?
        }
    };

    println!("Validating config file: {}", path);

    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read config file: {}", path))?;

    // Detect format from extension or content
    let is_toml = path.ends_with(".toml")
        || (!path.ends_with(".json") && !raw.trim_start().starts_with('{'));

    if is_toml {
        match raw.parse::<toml::Value>() {
            Ok(parsed) => {
                println!("✓ TOML syntax is valid");
                validate_rs3gw_toml_fields(&parsed);
            }
            Err(e) => {
                println!("✗ TOML parse error: {}", e);
                return Err(anyhow::anyhow!("Config validation failed"));
            }
        }
    } else {
        match serde_json::from_str::<Value>(&raw) {
            Ok(parsed) => {
                println!("✓ JSON syntax is valid");
                validate_rs3gw_json_fields(&parsed);
            }
            Err(e) => {
                println!("✗ JSON parse error: {}", e);
                return Err(anyhow::anyhow!("Config validation failed"));
            }
        }
    }

    println!("✓ Config file '{}' passed validation", path);
    Ok(())
}

/// Warn about unknown top-level keys in a TOML rs3gw config
fn validate_rs3gw_toml_fields(parsed: &toml::Value) {
    let known_keys = [
        "bind_addr",
        "storage_root",
        "default_bucket",
        "compression",
        "request_timeout_secs",
        "max_concurrent_requests",
        "tls",
        "cache",
        "throttle",
        "quota",
    ];
    if let toml::Value::Table(table) = parsed {
        for key in table.keys() {
            if !known_keys.contains(&key.as_str()) {
                println!("  Warning: unknown config key '{}'", key);
            }
        }
    }
}

/// Warn about unknown top-level keys in a JSON rs3gw config
fn validate_rs3gw_json_fields(parsed: &Value) {
    let known_keys = [
        "bind_addr",
        "storage_root",
        "default_bucket",
        "compression",
        "request_timeout_secs",
        "max_concurrent_requests",
        "tls",
        "cache",
        "throttle",
        "quota",
    ];
    if let Value::Object(map) = parsed {
        for key in map.keys() {
            if !known_keys.contains(&key.as_str()) {
                println!("  Warning: unknown config key '{}'", key);
            }
        }
    }
}

// ── Server Info ───────────────────────────────────────────────────────────────

/// Handle `rs3ctl server-info <subcommand>`
pub async fn handle_server_info(
    client: &Client,
    cli: &Cli,
    action: &ServerInfoAction,
) -> Result<()> {
    match action {
        ServerInfoAction::Status => handle_server_status(client, cli).await,
    }
}

async fn handle_server_status(client: &Client, cli: &Cli) -> Result<()> {
    println!("=== Comprehensive Server Status ===\n");

    // Fetch health and metrics concurrently
    let health_url = format!("{}/health", cli.endpoint);
    let metrics_url = format!("{}/metrics", cli.endpoint);
    let storage_url = format!("{}/api/maintenance/storage-stats", cli.endpoint);

    let (health_result, metrics_result, storage_result) = tokio::join!(
        client.get(&health_url).send(),
        client.get(&metrics_url).send(),
        client.get(&storage_url).send(),
    );

    // ── Health ────────────────────────────────────────────────────────────────
    println!("[ Health ]");
    match health_result {
        Ok(resp) if resp.status().is_success() => {
            println!("  Status  : HEALTHY ({})", resp.status());
            if let Ok(body) = resp.text().await {
                // Print first line only for brevity
                if let Some(first_line) = body.lines().next() {
                    println!("  Response: {}", first_line.trim());
                }
            }
        }
        Ok(resp) => {
            println!("  Status  : DEGRADED ({})", resp.status());
        }
        Err(e) => {
            println!("  Status  : UNREACHABLE ({})", e);
        }
    }

    // ── Metrics overview ──────────────────────────────────────────────────────
    println!("\n[ Prometheus Metrics ]");
    match metrics_result {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            let line_count = body.lines().count();
            println!("  Available: YES ({} metric lines)", line_count);
            // Print a curated subset of useful metrics
            for line in body.lines() {
                if line.starts_with('#') {
                    continue;
                }
                if line.contains("rs3gw_requests_total")
                    || line.contains("rs3gw_storage_bytes")
                    || line.contains("rs3gw_active_connections")
                {
                    println!("  {}", line);
                }
            }
        }
        Ok(resp) => {
            println!("  Available: NO ({})", resp.status());
        }
        Err(e) => {
            println!("  Available: NO ({})", e);
        }
    }

    // ── Storage stats ─────────────────────────────────────────────────────────
    println!("\n[ Storage Statistics ]");
    match storage_result {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<Value>().await {
                Ok(stats) => {
                    if let Some(total) = stats.get("total_bytes") {
                        println!("  Total bytes : {}", total);
                    }
                    if let Some(used) = stats.get("used_bytes") {
                        println!("  Used bytes  : {}", used);
                    }
                    if let Some(objects) = stats.get("object_count") {
                        println!("  Object count: {}", objects);
                    }
                    if let Some(buckets) = stats.get("bucket_count") {
                        println!("  Buckets     : {}", buckets);
                    }
                }
                Err(_) => println!("  (non-JSON response from storage-stats endpoint)"),
            }
        }
        Ok(resp) => {
            println!("  Unavailable ({})", resp.status());
        }
        Err(e) => {
            println!("  Unavailable ({})", e);
        }
    }

    println!("\n=== End of Status ===");
    Ok(())
}

// ── Benchmark ─────────────────────────────────────────────────────────────────

/// Handle `rs3ctl benchmark`
pub async fn handle_benchmark(
    client: &Client,
    cli: &Cli,
    iterations: u32,
    size: usize,
    bucket: &str,
) -> Result<()> {
    println!(
        "=== S3 Performance Benchmark: {} iterations, {} bytes per object ===\n",
        iterations, size
    );

    // Generate deterministic pseudo-random payload (no external rand crate)
    let payload: Vec<u8> = (0..size).map(|i| (i & 0xFF) as u8).collect();

    let pb = ProgressBar::new(iterations as u64 * 2);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:50.green/white} {pos}/{len} {msg}")?
            .progress_chars("=>-"),
    );

    let mut put_latencies: Vec<f64> = Vec::with_capacity(iterations as usize);
    let mut get_latencies: Vec<f64> = Vec::with_capacity(iterations as usize);
    let mut cleanup_keys: Vec<String> = Vec::with_capacity(iterations as usize);

    // PUT phase
    pb.set_message("PUT phase...");
    for i in 0..iterations {
        let key = format!("benchmark-rs3ctl-{:06}", i);
        let url = format!("{}/{}/{}", cli.endpoint, bucket, key);

        let start = Instant::now();
        let response = client
            .put(&url)
            .header("Content-Type", "application/octet-stream")
            .body(payload.clone())
            .send()
            .await
            .with_context(|| format!("PUT failed for key {}", key))?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        if !response.status().is_success() {
            pb.abandon_with_message("PUT error");
            return Err(anyhow::anyhow!(
                "PUT {} returned HTTP {}",
                key,
                response.status()
            ));
        }

        put_latencies.push(elapsed_ms);
        cleanup_keys.push(key);
        pb.inc(1);
    }

    // GET phase
    pb.set_message("GET phase...");
    for key in &cleanup_keys {
        let url = format!("{}/{}/{}", cli.endpoint, bucket, key);

        let start = Instant::now();
        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET failed for key {}", key))?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        if !response.status().is_success() {
            pb.abandon_with_message("GET error");
            return Err(anyhow::anyhow!(
                "GET {} returned HTTP {}",
                key,
                response.status()
            ));
        }
        // Consume body to measure full transfer
        let _ = response.bytes().await?;

        get_latencies.push(elapsed_ms);
        pb.inc(1);
    }
    pb.finish_with_message("done");

    // Cleanup phase (best-effort, don't fail benchmark if cleanup fails)
    print!("\nCleaning up {} benchmark objects... ", cleanup_keys.len());
    let mut cleanup_errors = 0usize;
    for key in &cleanup_keys {
        let url = format!("{}/{}/{}", cli.endpoint, bucket, key);
        if client.delete(&url).send().await.is_err() {
            cleanup_errors += 1;
        }
    }
    if cleanup_errors > 0 {
        println!("({} cleanup errors — objects may remain)", cleanup_errors);
    } else {
        println!("done");
    }

    // Report results
    println!("\n=== Benchmark Results ===\n");

    let throughput_kbps = (size as f64 * iterations as f64) / 1024.0
        / (put_latencies.iter().sum::<f64>() / 1000.0).max(0.001);

    if let Some(put_stats) = LatencyStats::compute(put_latencies) {
        println!("PUT latency (ms):");
        println!("  min   : {:.2}", put_stats.min_ms);
        println!("  avg   : {:.2}", put_stats.avg_ms);
        println!("  p99   : {:.2}", put_stats.p99_ms);
        println!("  max   : {:.2}", put_stats.max_ms);
        println!("  samples: {}", put_stats.samples);
    }

    if let Some(get_stats) = LatencyStats::compute(get_latencies) {
        println!("\nGET latency (ms):");
        println!("  min   : {:.2}", get_stats.min_ms);
        println!("  avg   : {:.2}", get_stats.avg_ms);
        println!("  p99   : {:.2}", get_stats.p99_ms);
        println!("  max   : {:.2}", get_stats.max_ms);
        println!("  samples: {}", get_stats.samples);
    }

    println!("\nPUT throughput: {:.1} KB/s", throughput_kbps);
    println!("Object size   : {} bytes", size);
    println!("Iterations    : {}", iterations);

    Ok(())
}

// ── Diagnose ──────────────────────────────────────────────────────────────────

/// Diagnostic check result
#[derive(Debug)]
struct DiagCheck {
    name: &'static str,
    passed: bool,
    detail: String,
}

impl DiagCheck {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            passed: true,
            detail: detail.into(),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Handle `rs3ctl diagnose`
pub async fn handle_diagnose(
    client: &Client,
    cli: &Cli,
    connectivity_only: bool,
) -> Result<()> {
    println!("=== rs3gw Diagnostic Report ===");
    println!("Endpoint: {}\n", cli.endpoint);

    let mut checks: Vec<DiagCheck> = Vec::new();

    // Check 1: HTTP connectivity to health endpoint
    let health_url = format!("{}/health", cli.endpoint);
    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            checks.push(DiagCheck::pass(
                "Connectivity (health)",
                format!("HTTP {}", resp.status()),
            ));
        }
        Ok(resp) => {
            checks.push(DiagCheck::fail(
                "Connectivity (health)",
                format!("HTTP {} — server responded but returned an error", resp.status()),
            ));
        }
        Err(e) => {
            checks.push(DiagCheck::fail(
                "Connectivity (health)",
                format!("Connection failed: {}", e),
            ));
        }
    }

    if !connectivity_only {
        // Check 2: Auth — try listing buckets
        let buckets_url = format!("{}/", cli.endpoint);
        match client.get(&buckets_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                checks.push(DiagCheck::pass(
                    "Auth (list buckets)",
                    format!("HTTP {} — bucket listing succeeded", resp.status()),
                ));
            }
            Ok(resp) if resp.status().as_u16() == 403 => {
                checks.push(DiagCheck::fail(
                    "Auth (list buckets)",
                    "HTTP 403 Forbidden — credentials may be missing or invalid",
                ));
            }
            Ok(resp) => {
                checks.push(DiagCheck::fail(
                    "Auth (list buckets)",
                    format!("HTTP {}", resp.status()),
                ));
            }
            Err(e) => {
                checks.push(DiagCheck::fail(
                    "Auth (list buckets)",
                    format!("Request failed: {}", e),
                ));
            }
        }

        // Check 3: Storage — verify the maintenance/storage-stats endpoint
        let storage_url = format!("{}/api/maintenance/storage-stats", cli.endpoint);
        match client.get(&storage_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let size_hint = resp
                    .json::<Value>()
                    .await
                    .ok()
                    .and_then(|v| v.get("total_bytes").cloned())
                    .map(|b| format!("total_bytes={}", b))
                    .unwrap_or_else(|| "ok".to_string());
                checks.push(DiagCheck::pass("Storage (stats)", size_hint));
            }
            Ok(resp) if resp.status().as_u16() == 404 => {
                checks.push(DiagCheck::pass(
                    "Storage (stats)",
                    "Endpoint not implemented — this is expected for minimal deployments",
                ));
            }
            Ok(resp) => {
                checks.push(DiagCheck::fail(
                    "Storage (stats)",
                    format!("HTTP {}", resp.status()),
                ));
            }
            Err(e) => {
                checks.push(DiagCheck::fail(
                    "Storage (stats)",
                    format!("Request failed: {}", e),
                ));
            }
        }

        // Check 4: Metrics endpoint
        let metrics_url = format!("{}/metrics", cli.endpoint);
        match client.get(&metrics_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.text().await.unwrap_or_default();
                let line_count = body.lines().filter(|l| !l.starts_with('#')).count();
                checks.push(DiagCheck::pass(
                    "Prometheus metrics",
                    format!("{} metric series exposed", line_count),
                ));
            }
            Ok(resp) => {
                checks.push(DiagCheck::fail(
                    "Prometheus metrics",
                    format!("HTTP {}", resp.status()),
                ));
            }
            Err(e) => {
                checks.push(DiagCheck::fail(
                    "Prometheus metrics",
                    format!("Request failed: {}", e),
                ));
            }
        }
    }

    // Print results table
    let pass_count = checks.iter().filter(|c| c.passed).count();
    let total = checks.len();

    println!("{:<30} {:<10} Detail", "Check", "Result");
    println!("{}", "-".repeat(80));
    for check in &checks {
        let result = if check.passed { "PASS" } else { "FAIL" };
        println!("{:<30} {:<10} {}", check.name, result, check.detail);
    }
    println!("{}", "-".repeat(80));
    println!("Summary: {}/{} checks passed", pass_count, total);

    if pass_count < total {
        Err(anyhow::anyhow!(
            "{} diagnostic check(s) failed",
            total - pass_count
        ))
    } else {
        println!("\n✓ All diagnostic checks passed");
        Ok(())
    }
}
