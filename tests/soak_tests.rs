#![cfg(feature = "server")]
//! Soak / sustained-load stability test.
//!
//! Drives a concurrent mixed workload (PUT → HEAD → GET → DELETE cycles) against
//! the gateway for a configurable duration and asserts the server stays healthy:
//! zero operation errors, no catastrophic latency drift between the first and
//! second half of the run (a coarse proxy for leaks / unbounded growth), and the
//! server remains responsive afterwards.
//!
//! Defaults are deliberately short so this runs as part of the normal suite, but
//! it is the real soak harness — crank it up for multi-hour runs via env vars:
//!
//! ```bash
//! RS3GW_SOAK_DURATION_SECS=3600 RS3GW_SOAK_CONCURRENCY=32 \
//!     cargo test --test soak_tests -- --nocapture
//! ```

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use common::setup_test_server;

/// Per-worker accumulated stats over the soak run.
#[derive(Default)]
struct WorkerStats {
    ops: u64,
    errors: u64,
    first_error: Option<String>,
    /// Latency sums (ms) and counts, split at the run midpoint to detect drift.
    first_half_ms: f64,
    first_half_count: u64,
    second_half_ms: f64,
    second_half_count: u64,
}

impl WorkerStats {
    fn record(&mut self, latency: Duration, now: Instant, midpoint: Instant) {
        self.ops += 1;
        let ms = latency.as_secs_f64() * 1000.0;
        if now < midpoint {
            self.first_half_ms += ms;
            self.first_half_count += 1;
        } else {
            self.second_half_ms += ms;
            self.second_half_count += 1;
        }
    }

    fn note_error(&mut self, msg: String) {
        self.errors += 1;
        if self.first_error.is_none() {
            self.first_error = Some(msg);
        }
    }
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

/// One PUT → HEAD → GET → DELETE cycle on `key`. Returns Err(msg) on the first
/// failing step. Each step's latency is recorded individually.
async fn run_cycle(
    client: &Client,
    bucket: &str,
    key: &str,
    payload: &[u8],
    stats: &mut WorkerStats,
    deadline: Instant,
    midpoint: Instant,
) -> Result<(), String> {
    // PUT
    let t = Instant::now();
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from(payload.to_vec()))
        .send()
        .await
        .map_err(|e| format!("PUT {key}: {e}"))?;
    stats.record(t.elapsed(), Instant::now(), midpoint);
    if Instant::now() >= deadline {
        return Ok(());
    }

    // HEAD
    let t = Instant::now();
    client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("HEAD {key}: {e}"))?;
    stats.record(t.elapsed(), Instant::now(), midpoint);

    // GET (and drain the body)
    let t = Instant::now();
    let got = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("GET {key}: {e}"))?;
    let body = got
        .body
        .collect()
        .await
        .map_err(|e| format!("GET-collect {key}: {e}"))?;
    if body.into_bytes().len() != payload.len() {
        return Err(format!("GET {key}: body length mismatch"));
    }
    stats.record(t.elapsed(), Instant::now(), midpoint);

    // DELETE
    let t = Instant::now();
    client
        .delete_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .map_err(|e| format!("DELETE {key}: {e}"))?;
    stats.record(t.elapsed(), Instant::now(), midpoint);

    Ok(())
}

#[tokio::test]
async fn soak_sustained_mixed_workload() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("soak-{}", uuid::Uuid::new_v4());
    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create soak bucket");

    let duration_secs = env_u64("RS3GW_SOAK_DURATION_SECS", 2);
    let concurrency = env_u64("RS3GW_SOAK_CONCURRENCY", 4) as usize;
    let payload = Arc::new(vec![0x5Au8; 4096]);

    let start = Instant::now();
    let deadline = start + Duration::from_secs(duration_secs);
    let midpoint = start + Duration::from_secs_f64(duration_secs as f64 / 2.0);

    eprintln!(
        "soak: duration={}s concurrency={} payload={}B",
        duration_secs,
        concurrency,
        payload.len()
    );

    let mut handles = Vec::with_capacity(concurrency);
    for worker_id in 0..concurrency {
        let client = client.clone();
        let bucket = bucket.clone();
        let payload = payload.clone();
        handles.push(tokio::spawn(async move {
            let mut stats = WorkerStats::default();
            let key = format!("soak/w{worker_id}/obj");
            while Instant::now() < deadline {
                if let Err(msg) = run_cycle(
                    &client, &bucket, &key, &payload, &mut stats, deadline, midpoint,
                )
                .await
                {
                    stats.note_error(msg);
                    // Keep going so we surface the aggregate, not just the first error.
                }
            }
            stats
        }));
    }

    // Aggregate worker stats.
    let mut total = WorkerStats::default();
    for h in handles {
        let s = h.await.expect("soak worker panicked");
        total.ops += s.ops;
        total.errors += s.errors;
        total.first_half_ms += s.first_half_ms;
        total.first_half_count += s.first_half_count;
        total.second_half_ms += s.second_half_ms;
        total.second_half_count += s.second_half_count;
        if total.first_error.is_none() {
            total.first_error = s.first_error;
        }
    }

    let first_avg = if total.first_half_count > 0 {
        total.first_half_ms / total.first_half_count as f64
    } else {
        0.0
    };
    let second_avg = if total.second_half_count > 0 {
        total.second_half_ms / total.second_half_count as f64
    } else {
        0.0
    };
    eprintln!(
        "soak: ops={} errors={} first_half_avg={:.3}ms second_half_avg={:.3}ms",
        total.ops, total.errors, first_avg, second_avg
    );

    // --- Assertions ---
    assert_eq!(
        total.errors, 0,
        "soak run had {} operation errors; first: {:?}",
        total.errors, total.first_error
    );
    assert!(
        total.ops >= concurrency as u64,
        "soak run completed too few operations ({})",
        total.ops
    );
    // No catastrophic latency drift: second half must not be wildly slower than the
    // first (generous bound to tolerate scheduling noise while catching real leaks).
    if total.first_half_count > 0 && total.second_half_count > 0 {
        assert!(
            second_avg <= first_avg * 8.0 + 50.0,
            "latency degraded during soak: first_half_avg={first_avg:.3}ms second_half_avg={second_avg:.3}ms"
        );
    }

    // Server remains responsive after sustained load.
    let http = reqwest::Client::new();
    let health = http
        .get(format!("{}/health", server.base_url))
        .send()
        .await
        .expect("post-soak /health request");
    assert_eq!(
        health.status(),
        200,
        "server must stay responsive after the soak run"
    );
}
