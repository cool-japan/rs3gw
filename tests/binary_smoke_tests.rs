#![cfg(feature = "server")]
//! Binary smoke tests — the ONLY integration test that boots the real compiled
//! `rs3gw` binary as a child process and drives it over a real TCP socket.
//!
//! Every other integration test in this crate constructs the Axum router
//! in-process (see `tests/common/mod.rs`) and never spawns the binary. Those
//! tests exercise handler logic but cannot catch regressions in the binary's
//! own startup path: environment parsing (`Config::from_env`), storage-root
//! creation, the TCP bind, and the serve loop. This file closes that gap by
//! launching `env!("CARGO_BIN_EXE_rs3gw")`, configuring it purely through the
//! environment, waiting for `/health` to report `healthy`, and then running a
//! handful of end-to-end S3 operations through the AWS SDK over the wire.
//!
//! Release gates covered:
//!   * "Runs locally with default configuration"
//!   * "S3 basic smoke test (mb/ls/cp/rm/rb)"
//!   * "Multipart upload smoke test"

mod common;

use std::fs::File;
use std::net::SocketAddr;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::{CompletedMultipartUpload, CompletedPart};
use tempfile::TempDir;

/// RAII handle to a spawned `rs3gw` server subprocess.
///
/// On drop the child is killed and reaped so no zombie processes leak — this is
/// important under `cargo nextest`, where each test runs in its own process and
/// must clean up its own server. The [`TempDir`] is held so the storage root
/// (and the captured child log) survive for the lifetime of the test and are
/// removed afterwards.
struct BinaryServer {
    child: std::process::Child,
    addr: SocketAddr,
    log_path: std::path::PathBuf,
    _tmp: TempDir,
}

impl BinaryServer {
    /// Boot a fresh server on a free ephemeral port with auth and TLS disabled.
    ///
    /// Blocks (asynchronously) until `/health` reports `healthy`, or panics with
    /// the captured child log if the process dies early or fails to become ready
    /// within 30 seconds.
    async fn start() -> Self {
        common::init_tracing();

        // Reserve a free port by binding then immediately releasing it, so the
        // child can bind it. Each test uses its own port for isolation.
        let probe =
            std::net::TcpListener::bind("127.0.0.1:0").expect("bind an ephemeral 127.0.0.1 port");
        let port = probe
            .local_addr()
            .expect("read the probe listener's local_addr")
            .port();
        drop(probe);

        let tmp = TempDir::new().expect("create a temp storage root");

        // Redirect child stdout+stderr to a single log file inside the temp dir.
        // Using a file (rather than piped stdio) avoids any pipe-buffer deadlock
        // while still letting us surface the log in panic messages.
        let log_path = tmp.path().join("server.log");
        let log_out = File::create(&log_path).expect("create the child log file");
        let log_err = log_out
            .try_clone()
            .expect("clone the child log file handle for stderr");

        let child = Command::new(env!("CARGO_BIN_EXE_rs3gw"))
            .env("RS3GW_BIND_ADDR", format!("127.0.0.1:{port}"))
            .env("RS3GW_STORAGE_ROOT", tmp.path())
            .env_remove("RS3GW_ACCESS_KEY")
            .env_remove("RS3GW_SECRET_KEY")
            .env_remove("RS3GW_TLS_CERT")
            .env_remove("RS3GW_TLS_KEY")
            .stdout(Stdio::from(log_out))
            .stderr(Stdio::from(log_err))
            .spawn()
            .expect("spawn the rs3gw binary");

        let addr: SocketAddr = format!("127.0.0.1:{port}")
            .parse()
            .expect("parse the chosen SocketAddr");

        // Construct first so that, if readiness fails and we panic, the Drop impl
        // still kills + reaps the child during unwinding.
        let mut server = BinaryServer {
            child,
            addr,
            log_path,
            _tmp: tmp,
        };
        server.await_healthy().await;
        server
    }

    /// Poll `/health` until it reports `healthy`; panic (with the child log) on
    /// early exit or a 30-second timeout.
    async fn await_healthy(&mut self) {
        let http = reqwest::Client::new();
        let health_url = format!("http://{}/health", self.addr);
        let deadline = Instant::now() + Duration::from_secs(30);

        loop {
            // Fail fast if the child already exited — no point waiting 30s.
            if let Some(status) = self
                .child
                .try_wait()
                .expect("poll the rs3gw child process status")
            {
                panic!(
                    "rs3gw exited before becoming healthy (exit status: {status}).\n\
                     --- server.log ---\n{}",
                    self.read_log()
                );
            }

            if let Ok(resp) = http.get(&health_url).send().await {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if json.get("status").and_then(|s| s.as_str()) == Some("healthy") {
                            return;
                        }
                    }
                }
            }

            if Instant::now() >= deadline {
                panic!(
                    "rs3gw did not become healthy within 30s at {}.\n--- server.log ---\n{}",
                    self.addr,
                    self.read_log()
                );
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// Best-effort read of the captured child log for panic diagnostics.
    fn read_log(&self) -> String {
        std::fs::read_to_string(&self.log_path)
            .unwrap_or_else(|e| format!("<failed to read {}: {e}>", self.log_path.display()))
    }

    /// Build an AWS S3 SDK client pointed at this server (passthrough auth).
    async fn s3_client(&self) -> aws_sdk_s3::Client {
        common::create_s3_client(self.addr).await
    }
}

impl Drop for BinaryServer {
    fn drop(&mut self) {
        // Kill + reap; ignore errors (the child may have already exited).
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The compiled binary boots with a purely default (env-only) configuration and
/// reports itself healthy and ready over real HTTP.
///
/// Closes the release gate "Runs locally with default configuration".
#[tokio::test]
async fn test_binary_boots_with_default_config_and_is_healthy() {
    let server = BinaryServer::start().await;
    let http = reqwest::Client::new();

    // /health — JSON shape and values.
    let health = http
        .get(format!("http://{}/health", server.addr))
        .send()
        .await
        .expect("GET /health should complete");
    assert_eq!(health.status().as_u16(), 200, "/health must return 200");
    let json: serde_json::Value = health.json().await.expect("/health must return JSON");
    assert_eq!(
        json.get("status").and_then(|s| s.as_str()),
        Some("healthy"),
        "/health status must be \"healthy\"; body: {json}"
    );
    let version = json
        .get("version")
        .and_then(|v| v.as_str())
        .expect("/health must include a string version field");
    assert!(!version.is_empty(), "/health version must be non-empty");
    assert_eq!(
        json.get("compression").and_then(|c| c.as_str()),
        Some("none"),
        "/health compression must be \"none\" under default config; body: {json}"
    );

    // /ready — 200 with body exactly "ok".
    let ready = http
        .get(format!("http://{}/ready", server.addr))
        .send()
        .await
        .expect("GET /ready should complete");
    assert_eq!(ready.status().as_u16(), 200, "/ready must return 200");
    let ready_body = ready.text().await.expect("read /ready body");
    assert_eq!(
        ready_body, "ok",
        "/ready body must be \"ok\"; got {ready_body:?}"
    );
}

/// Basic S3 lifecycle over the wire against the real binary: mb → ls → put →
/// cp → get → rm → rb.
///
/// Closes the release gate "S3 basic smoke test (mb/ls/cp/rm/rb)".
#[tokio::test]
async fn test_binary_s3_basic_lifecycle_over_the_wire() {
    let server = BinaryServer::start().await;
    let client = server.s3_client().await;

    let bucket = "binary-lifecycle";
    let key_a = "alpha.txt";
    let key_b = "beta.txt";
    let copy_key = "alpha-copy.txt";
    let body_a = b"alpha-over-the-wire".to_vec();
    let body_b = b"beta-over-the-wire".to_vec();

    // mb
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create_bucket (mb) should succeed");

    // ls (buckets) — the new bucket must be visible.
    let listed = client
        .list_buckets()
        .send()
        .await
        .expect("list_buckets should succeed");
    let bucket_names: Vec<&str> = listed.buckets().iter().filter_map(|b| b.name()).collect();
    assert!(
        bucket_names.contains(&bucket),
        "list_buckets must show {bucket}; got {bucket_names:?}"
    );

    // put two objects
    client
        .put_object()
        .bucket(bucket)
        .key(key_a)
        .body(ByteStream::from(body_a.clone()))
        .send()
        .await
        .expect("put_object alpha should succeed");
    client
        .put_object()
        .bucket(bucket)
        .key(key_b)
        .body(ByteStream::from(body_b.clone()))
        .send()
        .await
        .expect("put_object beta should succeed");

    // ls (objects) — both keys must be present.
    let objects = client
        .list_objects_v2()
        .bucket(bucket)
        .send()
        .await
        .expect("list_objects_v2 (ls) should succeed");
    let object_keys: Vec<&str> = objects.contents().iter().filter_map(|o| o.key()).collect();
    assert!(
        object_keys.contains(&key_a) && object_keys.contains(&key_b),
        "ls must show both {key_a} and {key_b}; got {object_keys:?}"
    );

    // cp — server-side copy of key_a → copy_key, then verify the bytes.
    client
        .copy_object()
        .bucket(bucket)
        .key(copy_key)
        .copy_source(format!("{bucket}/{key_a}"))
        .send()
        .await
        .expect("copy_object (cp) should succeed");
    let copied = client
        .get_object()
        .bucket(bucket)
        .key(copy_key)
        .send()
        .await
        .expect("get_object on the copy should succeed");
    let copied_bytes = copied
        .body
        .collect()
        .await
        .expect("collect copied body")
        .into_bytes();
    assert_eq!(
        copied_bytes.as_ref(),
        body_a.as_slice(),
        "copied object bytes must equal the source object bytes"
    );

    // rm — delete both originals and the copy.
    for key in [key_a, key_b, copy_key] {
        client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .unwrap_or_else(|e| panic!("delete_object {key} (rm) should succeed: {e:?}"));
    }

    // rb — delete the (now empty) bucket.
    client
        .delete_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("delete_bucket (rb) should succeed");

    // Final ls (buckets) — the bucket must be gone.
    let after = client
        .list_buckets()
        .send()
        .await
        .expect("final list_buckets should succeed");
    let after_names: Vec<&str> = after.buckets().iter().filter_map(|b| b.name()).collect();
    assert!(
        !after_names.contains(&bucket),
        "list_buckets must not show {bucket} after rb; got {after_names:?}"
    );
}

/// Full multipart upload (3 parts) plus an abort cycle over the wire against the
/// real binary.
///
/// Closes the release gate "Multipart upload smoke test".
#[tokio::test]
async fn test_binary_multipart_upload_over_the_wire() {
    let server = BinaryServer::start().await;
    let client = server.s3_client().await;

    let bucket = "binary-multipart";
    let key = "assembled.bin";

    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // ----- happy path: initiate → upload 3 parts → complete → verify -----
    let create = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(key)
        .content_type("application/octet-stream")
        .send()
        .await
        .expect("create_multipart_upload should succeed");
    let upload_id = create
        .upload_id()
        .expect("create_multipart_upload must return an upload id")
        .to_string();

    // Small parts are fine — rs3gw does not enforce the 5 MiB S3 minimum.
    let parts_data: [Vec<u8>; 3] = [vec![b'A'; 1024], vec![b'B'; 1024], vec![b'C'; 1024]];
    let mut completed_parts: Vec<CompletedPart> = Vec::with_capacity(parts_data.len());
    for (idx, data) in parts_data.iter().enumerate() {
        let part_number = idx as i32 + 1;
        let resp = client
            .upload_part()
            .bucket(bucket)
            .key(key)
            .upload_id(&upload_id)
            .part_number(part_number)
            .body(ByteStream::from(data.clone()))
            .send()
            .await
            .unwrap_or_else(|e| panic!("upload_part {part_number} should succeed: {e:?}"));
        let etag = resp
            .e_tag()
            .unwrap_or_else(|| panic!("upload_part {part_number} must return an ETag"))
            .to_string();
        completed_parts.push(
            CompletedPart::builder()
                .part_number(part_number)
                .e_tag(etag)
                .build(),
        );
    }

    let completed = CompletedMultipartUpload::builder()
        .set_parts(Some(completed_parts))
        .build();
    client
        .complete_multipart_upload()
        .bucket(bucket)
        .key(key)
        .upload_id(&upload_id)
        .multipart_upload(completed)
        .send()
        .await
        .expect("complete_multipart_upload should succeed");

    // The assembled object must equal part1 || part2 || part3.
    let got = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .expect("get_object should succeed after complete");
    let got_bytes = got
        .body
        .collect()
        .await
        .expect("collect assembled body")
        .into_bytes();
    let expected: Vec<u8> = parts_data.concat();
    assert_eq!(
        got_bytes.as_ref(),
        expected.as_slice(),
        "assembled object must equal part1 || part2 || part3"
    );

    // ----- abort cycle: initiate → upload 1 part → abort → list_parts errs ----
    let abort_key = "aborted.bin";
    let abort_create = client
        .create_multipart_upload()
        .bucket(bucket)
        .key(abort_key)
        .send()
        .await
        .expect("create_multipart_upload (abort cycle) should succeed");
    let abort_id = abort_create
        .upload_id()
        .expect("abort-cycle upload must return an upload id")
        .to_string();
    client
        .upload_part()
        .bucket(bucket)
        .key(abort_key)
        .upload_id(&abort_id)
        .part_number(1)
        .body(ByteStream::from(vec![b'X'; 1024]))
        .send()
        .await
        .expect("upload_part for the abort cycle should succeed");
    client
        .abort_multipart_upload()
        .bucket(bucket)
        .key(abort_key)
        .upload_id(&abort_id)
        .send()
        .await
        .expect("abort_multipart_upload should succeed");

    let list_parts = client
        .list_parts()
        .bucket(bucket)
        .key(abort_key)
        .upload_id(&abort_id)
        .send()
        .await;
    assert!(
        list_parts.is_err(),
        "list_parts after abort must error (upload no longer exists)"
    );
}
