//! Handlers for `rs3ctl gc-multipart` and `rs3ctl show-metrics`.
//!
//! - `gc-multipart` — calls `POST /api/admin/gc/multipart` and prints the result
//! - `show-metrics` — fetches `GET /metrics` and pretty-prints metric families
//!   with optional substring filtering

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Deserialize;

use crate::Cli;

// ── GC multipart ──────────────────────────────────────────────────────────────

/// JSON response returned by the server's gc endpoint
#[derive(Debug, Deserialize)]
struct GcResponse {
    removed: u64,
    message: String,
}

/// Handle `rs3ctl gc-multipart [--bucket <b>] [--retention-hours <N>]`
///
/// Calls `POST /api/admin/gc/multipart?bucket=...&retention_hours=...`
/// and prints the number of uploads that were garbage-collected.
pub async fn handle_gc_multipart(
    client: &Client,
    cli: &Cli,
    bucket: Option<&str>,
    retention_hours: u64,
) -> Result<()> {
    let mut url = format!(
        "{}/api/admin/gc/multipart?retention_hours={}",
        cli.endpoint, retention_hours
    );
    if let Some(b) = bucket {
        url.push_str(&format!("&bucket={}", urlencoding_simple(b)));
    }

    let response = client
        .post(&url)
        .send()
        .await
        .context("Failed to reach /api/admin/gc/multipart endpoint")?;

    let status = response.status();
    if status.is_success() {
        let gc: GcResponse = response
            .json()
            .await
            .context("Server returned non-JSON response from gc endpoint")?;
        println!("{}", gc.message);
        println!("Removed: {}", gc.removed);
        Ok(())
    } else {
        let body = response.text().await.unwrap_or_default();
        Err(anyhow::anyhow!(
            "GC endpoint returned HTTP {} — {}",
            status,
            body.lines().next().unwrap_or("(empty body)")
        ))
    }
}

// ── Show metrics ──────────────────────────────────────────────────────────────

/// A parsed Prometheus metric family (one `# HELP` / `# TYPE` block + samples).
struct MetricFamily {
    help: Option<String>,
    type_hint: Option<String>,
    name: String,
    samples: Vec<String>,
}

/// Handle `rs3ctl show-metrics [--filter <prefix>]`
///
/// Fetches `GET /metrics`, groups lines into metric families, filters by the
/// optional substring, and prints them in a readable format.
pub async fn handle_show_metrics(client: &Client, cli: &Cli, filter: Option<&str>) -> Result<()> {
    let url = format!("{}/metrics", cli.endpoint);
    let response = client
        .get(&url)
        .send()
        .await
        .context("Failed to reach /metrics endpoint")?;

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!(
            "Metrics endpoint returned HTTP {}",
            status
        ));
    }

    let body = response
        .text()
        .await
        .context("Failed to read metrics response body")?;

    let families = parse_prometheus_text(&body);

    let filter_lower = filter.map(|f| f.to_lowercase());

    let mut printed = 0usize;
    for family in &families {
        // Apply filter if requested (case-insensitive substring match on name)
        if let Some(ref f) = filter_lower {
            if !family.name.to_lowercase().contains(f.as_str()) {
                continue;
            }
        }

        // Print HELP comment if present
        if let Some(ref help) = family.help {
            println!("# {}", help);
        }
        // Print TYPE comment if present
        if let Some(ref type_hint) = family.type_hint {
            println!("# TYPE {} {}", family.name, type_hint);
        }
        // Print all sample lines
        for sample in &family.samples {
            println!("{}", sample);
        }
        println!(); // blank line between families
        printed += 1;
    }

    if printed == 0 {
        if let Some(f) = filter {
            println!("No metrics matching filter '{}'", f);
        } else {
            println!("(no metrics returned)");
        }
    } else {
        println!("--- {} metric familie(s) shown ---", printed);
    }

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse a Prometheus text-format response into `MetricFamily` structs.
///
/// This is a best-effort parser: it groups `# HELP` / `# TYPE` declarations
/// with the sample lines that follow until the next `# HELP` block.
fn parse_prometheus_text(text: &str) -> Vec<MetricFamily> {
    let mut families: Vec<MetricFamily> = Vec::new();
    let mut current: Option<MetricFamily> = None;

    for line in text.lines() {
        let line = line.trim();

        if let Some(rest) = line.strip_prefix("# HELP ") {
            // Flush previous family
            if let Some(f) = current.take() {
                families.push(f);
            }
            // Parse "metric_name help text…"
            let (name, help_text) = split_first_word(rest);
            current = Some(MetricFamily {
                help: Some(format!("HELP {} {}", name, help_text)),
                type_hint: None,
                name: name.to_string(),
                samples: Vec::new(),
            });
        } else if let Some(rest) = line.strip_prefix("# TYPE ") {
            let (name, type_str) = split_first_word(rest);
            if let Some(ref mut f) = current {
                // Attach to current family if names match (or family has no name yet)
                if f.name == name || f.name.is_empty() {
                    f.type_hint = Some(type_str.to_string());
                    f.name = name.to_string();
                }
            } else {
                // TYPE without preceding HELP
                current = Some(MetricFamily {
                    help: None,
                    type_hint: Some(type_str.to_string()),
                    name: name.to_string(),
                    samples: Vec::new(),
                });
            }
        } else if line.starts_with('#') || line.is_empty() {
            // Other comments or blank lines — skip
        } else {
            // Sample line
            if let Some(ref mut f) = current {
                f.samples.push(line.to_string());
            } else {
                // Sample with no preceding metadata — create anonymous family
                let name = line
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                let mut anon = MetricFamily {
                    help: None,
                    type_hint: None,
                    name,
                    samples: Vec::new(),
                };
                anon.samples.push(line.to_string());
                current = Some(anon);
            }
        }
    }

    // Flush last family
    if let Some(f) = current {
        families.push(f);
    }

    families
}

/// Split a string at the first whitespace boundary.
/// Returns `(first_word, remainder)`.
fn split_first_word(s: &str) -> (&str, &str) {
    let s = s.trim();
    if let Some(pos) = s.find(|c: char| c.is_whitespace()) {
        (s[..pos].trim(), s[pos..].trim())
    } else {
        (s, "")
    }
}

/// Minimal URL-component encoder: percent-encodes characters that are not
/// unreserved (A-Z a-z 0-9 `-` `_` `.` `~`).
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(char::from_digit((byte >> 4) as u32, 16).unwrap_or('0'));
                out.push(char::from_digit((byte & 0xF) as u32, 16).unwrap_or('0'));
            }
        }
    }
    out
}
