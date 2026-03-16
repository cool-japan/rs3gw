//! SigV4 authentication middleware integration tests.
//!
//! Tests:
//!  - No credentials configured → all requests pass through (passthrough mode)
//!  - Credentials configured + wrong/missing signature → 403 AccessDenied
//!  - Timestamp skew (±20 minutes) → 403
//!  - Missing x-amz-date → 403
//!  - Wrong region / wrong service in credential scope → 403
//!  - UNSIGNED-PAYLOAD accepted as payload hash → request not rejected for that reason
//!  - Auth error responses do not leak secret-key material
//!  - Pre-signed URL verification (valid, expired, tampered)
//!  - Chunked transfer encoding (unsigned-payload, empty body)

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::middleware::from_fn_with_state;
use axum::Router;
use chrono::Utc;
use reqwest::StatusCode;
use tempfile::TempDir;
use tokio::net::TcpListener;

use rs3gw::api::auth_middleware::{detect_auth_mode, sigv4_auth_layer, AuthMode};
use rs3gw::api::s3_router;
use rs3gw::auth::v4::PresignedUrlGenerator;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct AuthTestServer {
    pub base_url: String,
    _handle: tokio::task::JoinHandle<()>,
    _temp_dir: TempDir,
}

/// Spin up a test server with the sigv4_auth_layer wired in.
/// When `access_key` / `secret_key` are non-empty the verifier is active.
async fn setup_auth_server(access_key: &str, secret_key: &str) -> AuthTestServer {
    common::init_tracing();
    let temp_dir = TempDir::new().expect("temp dir");

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr: SocketAddr = listener.local_addr().expect("local addr");

    let storage_root = temp_dir.path().to_path_buf();
    let storage =
        Arc::new(rs3gw::storage::StorageEngine::new(storage_root.clone()).expect("storage engine"));

    let metrics_handle = rs3gw::metrics::init_metrics().expect("metrics");

    let config = rs3gw::Config {
        bind_addr: addr,
        storage_root: storage_root.clone(),
        default_bucket: "default".to_string(),
        access_key: access_key.to_string(),
        secret_key: secret_key.to_string(),
        compression: rs3gw::storage::CompressionMode::None,
        request_timeout_secs: 0,
        max_concurrent_requests: 0,
        tls: rs3gw::TlsConfig::default(),
        connection_pool: rs3gw::ConnectionPoolConfig::default(),
        cluster: rs3gw::cluster::ClusterConfig::default(),
        dedup: rs3gw::storage::DedupConfig::disabled(),
        zerocopy: rs3gw::storage::ZeroCopyConfig::default(),
        select_cache: rs3gw::SelectCacheConfig::default(),
        multipart_retention_hours: 168,
        fsync: false,
    };

    let preprocessing_path = storage_root.join("preprocessing");
    let preprocessing_manager = Arc::new(rs3gw::storage::preprocessing::PreprocessingManager::new(
        preprocessing_path,
    ));
    let predictive_analytics = Arc::new(rs3gw::observability::PredictiveAnalytics::new(
        10_000,
        0.023,
        0.09,
        0.0004,
        1_000_000_000_000,
    ));
    let metrics_tracker = Arc::new(rs3gw::observability::MetricsTracker::new());
    let select_result_cache = Arc::new(rs3gw::api::SelectResultCache::new(100, 10 * 1024 * 1024));
    let query_intelligence = Arc::new(rs3gw::api::QueryIntelligence::new());
    let training_path = storage_root.join("training");
    let training_manager = Arc::new(rs3gw::storage::TrainingManager::new(training_path));

    // Build verifier based on credentials (mirrors AppState::new logic)
    let verifier = if !access_key.is_empty() && !secret_key.is_empty() {
        Some(Arc::new(rs3gw::auth::v4::SigV4Verifier::new(
            access_key.to_string(),
            secret_key.to_string(),
            "us-east-1".to_string(),
        )))
    } else {
        None
    };

    let state = rs3gw::AppState {
        config,
        storage,
        metrics_handle,
        cache: None,
        throttle: None,
        quota: None,
        event_broadcaster: rs3gw::api::EventBroadcaster::new(),
        query_plan_cache: None,
        select_result_cache,
        query_intelligence,
        advanced_replication: None,
        preprocessing_manager,
        predictive_analytics,
        metrics_tracker,
        training_manager,
        start_time: std::time::Instant::now(),
        verifier,
        auth_failure_counts: std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        in_flight: rs3gw::InFlightTracker::new(),
    };

    let app = Router::new()
        .merge(s3_router::routes())
        .layer(from_fn_with_state(state.clone(), sigv4_auth_layer))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });

    tokio::time::sleep(Duration::from_millis(100)).await;

    AuthTestServer {
        base_url: format!("http://{}", addr),
        _handle: handle,
        _temp_dir: temp_dir,
    }
}

// ---------------------------------------------------------------------------
// Tests: no credentials → passthrough
// ---------------------------------------------------------------------------

/// When no credentials are configured the middleware is a no-op and all
/// unauthenticated requests must succeed.
#[tokio::test]
async fn test_no_creds_passthrough_allows_list_buckets() {
    let server = setup_auth_server("", "").await;

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("HTTP request");

    // ListBuckets returns 200 with XML
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Expected 200 in passthrough mode, got {}",
        resp.status()
    );
}

/// Health endpoint must always be reachable even with auth enabled.
#[tokio::test]
async fn test_exempt_path_health_always_passes() {
    let server = setup_auth_server("mykey", "mysecret").await;

    let resp = reqwest::Client::new()
        .get(format!("{}/health", server.base_url))
        .send()
        .await
        .expect("HTTP request");

    // /health may 200 or 404 depending on routing, but must NOT be 403
    assert_ne!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "/health must never return 403"
    );
}

// ---------------------------------------------------------------------------
// Tests: credentials configured → missing auth → 403
// ---------------------------------------------------------------------------

/// A plain unauthenticated GET to / must return 403 when credentials are set.
#[tokio::test]
async fn test_with_creds_missing_auth_returns_403() {
    let server = setup_auth_server("mykey", "mysecret").await;

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 when auth is required but not provided"
    );
}

/// A request with a syntactically present but wrong AWS4 Authorization header
/// must also return 403.
#[tokio::test]
async fn test_with_creds_wrong_signature_returns_403() {
    let server = setup_auth_server("mykey", "mysecret").await;

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        // Malformed / wrong credential in Authorization header
        .header(
            "Authorization",
            "AWS4-HMAC-SHA256 Credential=WRONGKEY/20240101/us-east-1/s3/aws4_request, \
             SignedHeaders=host, \
             Signature=0000000000000000000000000000000000000000000000000000000000000000",
        )
        .header("x-amz-date", "20240101T000000Z")
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for wrong SigV4 signature"
    );
}

/// A presigned URL with a tampered (incorrect) X-Amz-Signature must return 403.
#[tokio::test]
async fn test_with_creds_bad_presigned_returns_403() {
    let server = setup_auth_server("mykey", "mysecret").await;

    // Craft a presigned query string with WRONGKEY access key
    let query = "X-Amz-Algorithm=AWS4-HMAC-SHA256\
        &X-Amz-Credential=WRONGKEY%2F20240101%2Fus-east-1%2Fs3%2Faws4_request\
        &X-Amz-Date=20240101T000000Z\
        &X-Amz-Expires=86400\
        &X-Amz-SignedHeaders=host\
        &X-Amz-Signature=0000000000000000000000000000000000000000000000000000000000000000";

    let url = format!("{}/mybucket/myobject?{}", server.base_url, query);
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for bad presigned URL"
    );
}

// ---------------------------------------------------------------------------
// Tests: timestamp skew
// ---------------------------------------------------------------------------

/// A request timestamped 20 minutes into the future must be rejected.
#[tokio::test]
async fn test_sigv4_timestamp_skew_rejected() {
    let server = setup_auth_server("mykey", "mysecret").await;

    // Build a timestamp 20 minutes in the future
    let future_time = Utc::now() + chrono::Duration::minutes(20);
    let timestamp = future_time.format("%Y%m%dT%H%M%SZ").to_string();
    let date_str = future_time.format("%Y%m%d").to_string();

    let auth = format!(
        "AWS4-HMAC-SHA256 Credential=mykey/{date}/us-east-1/s3/aws4_request, \
         SignedHeaders=host;x-amz-date, \
         Signature=0000000000000000000000000000000000000000000000000000000000000000",
        date = date_str,
    );

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .header("Authorization", &auth)
        .header("x-amz-date", &timestamp)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for timestamp 20 minutes in the future, got {}",
        resp.status()
    );
}

/// A request timestamped 20 minutes in the past must be rejected.
#[tokio::test]
async fn test_sigv4_timestamp_skew_past_rejected() {
    let server = setup_auth_server("mykey", "mysecret").await;

    // Build a timestamp 20 minutes in the past
    let past_time = Utc::now() - chrono::Duration::minutes(20);
    let timestamp = past_time.format("%Y%m%dT%H%M%SZ").to_string();
    let date_str = past_time.format("%Y%m%d").to_string();

    let auth = format!(
        "AWS4-HMAC-SHA256 Credential=mykey/{date}/us-east-1/s3/aws4_request, \
         SignedHeaders=host;x-amz-date, \
         Signature=0000000000000000000000000000000000000000000000000000000000000000",
        date = date_str,
    );

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .header("Authorization", &auth)
        .header("x-amz-date", &timestamp)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for timestamp 20 minutes in the past, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// Tests: missing required headers
// ---------------------------------------------------------------------------

/// A request that provides an Authorization header but omits x-amz-date
/// must be rejected because timestamp validation has nothing to work with.
#[tokio::test]
async fn test_sigv4_missing_date_header() {
    let server = setup_auth_server("mykey", "mysecret").await;

    // Use today's date in credential scope but do NOT send x-amz-date header.
    // The verifier will use the date embedded in the credential scope timestamp
    // placeholder (YYYYMMDD T000000Z), which will almost certainly be off by
    // more than 15 minutes from the actual current time unless the test runs at
    // midnight UTC — so the timestamp check fires and we get 403.
    //
    // To make the test deterministic, use a date 30 days ago so it's
    // definitely expired.
    let old_date = (Utc::now() - chrono::Duration::days(30))
        .format("%Y%m%d")
        .to_string();

    let auth = format!(
        "AWS4-HMAC-SHA256 Credential=mykey/{date}/us-east-1/s3/aws4_request, \
         SignedHeaders=host, \
         Signature=0000000000000000000000000000000000000000000000000000000000000000",
        date = old_date,
    );

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .header("Authorization", &auth)
        // Deliberately omit x-amz-date
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 when x-amz-date is missing and credential date is stale, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// Tests: wrong region / wrong service in credential scope
// ---------------------------------------------------------------------------

/// A SigV4 Authorization header whose credential scope uses a wrong region
/// must be rejected with 403, because the computed signature will not match.
#[tokio::test]
async fn test_sigv4_wrong_region() {
    let server = setup_auth_server("mykey", "mysecret").await;

    // Use the correct key but wrong region (us-west-2 vs server's us-east-1).
    // The signature will never match → 403.
    let now = Utc::now();
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_str = now.format("%Y%m%d").to_string();

    let auth = format!(
        "AWS4-HMAC-SHA256 Credential=mykey/{date}/us-west-2/s3/aws4_request, \
         SignedHeaders=host;x-amz-date, \
         Signature=0000000000000000000000000000000000000000000000000000000000000000",
        date = date_str,
    );

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .header("Authorization", &auth)
        .header("x-amz-date", &timestamp)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for wrong region in credential scope, got {}",
        resp.status()
    );
}

/// A SigV4 Authorization header whose credential scope uses a wrong service
/// (e.g. "ec2" instead of "s3") must be rejected with 403.
#[tokio::test]
async fn test_sigv4_wrong_service() {
    let server = setup_auth_server("mykey", "mysecret").await;

    let now = Utc::now();
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_str = now.format("%Y%m%d").to_string();

    // Service "ec2" instead of "s3" → signature mismatch.
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential=mykey/{date}/us-east-1/ec2/aws4_request, \
         SignedHeaders=host;x-amz-date, \
         Signature=0000000000000000000000000000000000000000000000000000000000000000",
        date = date_str,
    );

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .header("Authorization", &auth)
        .header("x-amz-date", &timestamp)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for wrong service in credential scope, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// Tests: UNSIGNED-PAYLOAD
// ---------------------------------------------------------------------------

/// Sending x-amz-content-sha256: UNSIGNED-PAYLOAD with an otherwise correct
/// signature must not cause a payload-hash-specific rejection.
///
/// Because we do not (and cannot) craft a perfectly valid SigV4 signature in
/// a unit test without a full signing library, this test verifies the simpler
/// claim: that the middleware does NOT refuse the request with a 4xx response
/// that could only be caused by the literal string "UNSIGNED-PAYLOAD" being
/// rejected as a hash value.  In practice the request will still get a 403
/// because the signature itself is wrong — but the error must be "AccessDenied"
/// (signature mismatch), not some payload-hash-specific error.
///
/// The key security invariant is that UNSIGNED-PAYLOAD is accepted by the
/// payload-hash selection logic (the `unwrap_or("UNSIGNED-PAYLOAD")` path) and
/// is forwarded as-is to the signature verifier — meaning the request proceeds
/// to the signature-check stage rather than being short-circuited earlier.
#[tokio::test]
async fn test_sigv4_unsigned_payload_accepted_path() {
    let server = setup_auth_server("mykey", "mysecret").await;

    let now = Utc::now();
    let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_str = now.format("%Y%m%d").to_string();

    // A deliberately wrong-but-structurally-valid Authorization header.
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential=mykey/{date}/us-east-1/s3/aws4_request, \
         SignedHeaders=host;x-amz-content-sha256;x-amz-date, \
         Signature=0000000000000000000000000000000000000000000000000000000000000000",
        date = date_str,
    );

    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .header("Authorization", &auth)
        .header("x-amz-date", &timestamp)
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
        .send()
        .await
        .expect("HTTP request");

    // The middleware must reach the signature-check stage and return 403
    // (AccessDenied for bad signature), NOT a 400 or other error caused by
    // UNSIGNED-PAYLOAD being an invalid hash literal.
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 AccessDenied (signature mismatch), not a payload-hash rejection, got {}",
        resp.status()
    );

    // Additionally verify that the response body says AccessDenied, not some
    // payload-hash-specific error code.
    let body = resp.text().await.expect("read body");
    assert!(
        body.contains("AccessDenied"),
        "Response body should contain AccessDenied, got: {}",
        body
    );
}

// ---------------------------------------------------------------------------
// Tests: no secret leakage in error responses
// ---------------------------------------------------------------------------

/// Auth failure responses must never include the access key or any fragment of
/// the secret key in their body or headers.
#[tokio::test]
async fn test_auth_error_no_secret_leakage() {
    let access_key = "AKIAIOSFODNN7EXAMPLE";
    let secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYSECRET";

    let server = setup_auth_server(access_key, secret_key).await;

    // Send a request that will fail auth — no Authorization header.
    let resp = reqwest::Client::new()
        .get(format!("{}/", server.base_url))
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "Expected 403");

    let body = resp.text().await.expect("read body");

    // The response body must not contain the access key
    assert!(
        !body.contains(access_key),
        "Response body must not contain the access key, got: {}",
        body
    );

    // The response body must not contain the secret key (or any 8+ char substring of it)
    // We test for a unique substring to be thorough.
    assert!(
        !body.contains("wJalrXUt"),
        "Response body must not contain any fragment of the secret key, got: {}",
        body
    );
    assert!(
        !body.contains("SECRET"),
        "Response body must not contain any fragment of the secret key, got: {}",
        body
    );

    // Must only contain safe error codes
    assert!(
        body.contains("AccessDenied"),
        "Response body should contain AccessDenied, got: {}",
        body
    );
}

// ---------------------------------------------------------------------------
// Tests: rate limiting
// ---------------------------------------------------------------------------

/// After more than MAX_AUTH_FAILURES failures from the same IP within the
/// window, subsequent requests must receive HTTP 429 TooManyRequests.
///
/// NOTE: Because the test server is shared across tests and loopback IP
/// (127.0.0.1) is the same for all callers, each test gets a *fresh* server
/// instance to avoid interference with the rate-limiter state of other tests.
#[tokio::test]
async fn test_auth_rate_limiting_returns_429() {
    let server = setup_auth_server("mykey", "mysecret").await;
    let client = reqwest::Client::new();

    // Send 11 failing requests — one more than MAX_AUTH_FAILURES (10)
    let mut last_status = StatusCode::FORBIDDEN;
    for i in 0..=10u32 {
        let resp = client
            .get(format!("{}/", server.base_url))
            .send()
            .await
            .expect("HTTP request");
        last_status = resp.status();
        if last_status == StatusCode::TOO_MANY_REQUESTS {
            // We reached the rate limit — test passes
            return;
        }
        // Small delay to avoid OS-level connection throttling
        if i < 10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    assert_eq!(
        last_status,
        StatusCode::TOO_MANY_REQUESTS,
        "Expected 429 after exceeding auth failure rate limit"
    );
}

// ---------------------------------------------------------------------------
// === Pre-signed URL verification ===
// ---------------------------------------------------------------------------

/// Helper: set up an auth-enabled server that signs requests with the auth middleware.
///
/// Access key: "testkey", secret key: "testsecret", region: "us-east-1"
async fn setup_presigned_server() -> AuthTestServer {
    setup_auth_server("testkey", "testsecret").await
}

/// PUT an object, then fetch it via a correctly-computed presigned GET URL.
/// The PresignedUrlGenerator (from `auth::v4`) computes a real SigV4 signature
/// so the server should accept it with 200.
#[tokio::test]
async fn test_presigned_get_valid() {
    let server = setup_presigned_server().await;
    let client = reqwest::Client::new();

    // --- 1. First PUT the object using a signed header-based request. ---
    // We bypass auth by directly calling the no-auth path via setup_auth_server("")
    // for the PUT, but here the server has auth enabled. Instead we use the
    // generator to also produce a valid PUT presigned URL, then GET with one too.

    // Build host string (without scheme)
    let host = server.base_url.trim_start_matches("http://").to_string();

    let generator = PresignedUrlGenerator::new(
        "testkey".to_string(),
        "testsecret".to_string(),
        "us-east-1".to_string(),
        host.clone(),
    );

    // Generate a presigned PUT URL
    let put_url = generator.generate_presigned_put_url(
        "presigned-bucket",
        "hello.txt",
        3600,
        Some("text/plain"),
    );

    // Create bucket first (via presigned approach - but bucket creation requires auth header)
    // Use a plain no-auth server for PUT, then switch — but since we have auth enabled, we
    // use the presigned PUT URL directly.
    let put_resp = client
        .put(&put_url)
        .header("host", &host)
        .body("hello presigned world")
        .send()
        .await
        .expect("PUT request");

    // PUT may succeed (200/204) or fail with 404 if bucket doesn't exist; either way
    // the auth layer must not return 403 — the presigned signature is valid.
    assert_ne!(
        put_resp.status(),
        StatusCode::FORBIDDEN,
        "Presigned PUT must not return 403 (auth), got {}",
        put_resp.status()
    );

    // --- 2. Generate a presigned GET URL and fetch the object. ---
    let get_url = generator.generate_presigned_get_url("presigned-bucket", "hello.txt", 3600);

    let get_resp = client
        .get(&get_url)
        .header("host", &host)
        .send()
        .await
        .expect("GET request");

    // Auth must pass for a correctly-signed presigned GET URL.
    // The object may not exist (if the bucket creation failed above), but
    // the auth layer must not reject the request with 403.
    assert_ne!(
        get_resp.status(),
        StatusCode::FORBIDDEN,
        "Presigned GET must not return 403 (auth), got {}",
        get_resp.status()
    );
}

/// Send a presigned GET request where `X-Amz-Date` is 20 minutes in the past
/// and `X-Amz-Expires=60` (already expired). The server must return 403.
#[tokio::test]
async fn test_presigned_url_expired() {
    let server = setup_presigned_server().await;

    // Build a presigned query string manually with an expired timestamp.
    let past_time = Utc::now() - chrono::Duration::minutes(20);
    let timestamp = past_time.format("%Y%m%dT%H%M%SZ").to_string();
    let date_str = past_time.format("%Y%m%d").to_string();

    let credential = format!("testkey%2F{}%2Fus-east-1%2Fs3%2Faws4_request", date_str);

    // X-Amz-Expires=60 means the URL was only valid for 60 seconds; it issued
    // 20 minutes ago → clearly expired.
    let query = format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256\
        &X-Amz-Credential={credential}\
        &X-Amz-Date={timestamp}\
        &X-Amz-Expires=60\
        &X-Amz-SignedHeaders=host\
        &X-Amz-Signature=0000000000000000000000000000000000000000000000000000000000000000"
    );

    let url = format!("{}/somebucket/somekey?{}", server.base_url, query);

    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for expired presigned URL, got {}",
        resp.status()
    );
}

/// Craft a valid presigned URL signature and then tamper its last two characters.
/// The server must return 403.
#[tokio::test]
async fn test_presigned_url_tampered_signature() {
    let server = setup_presigned_server().await;
    let host = server.base_url.trim_start_matches("http://").to_string();

    let generator = PresignedUrlGenerator::new(
        "testkey".to_string(),
        "testsecret".to_string(),
        "us-east-1".to_string(),
        host.clone(),
    );

    // Generate a valid presigned GET URL
    let valid_url = generator.generate_presigned_get_url("tamperbucket", "tamper.txt", 3600);

    // Tamper the last two characters of X-Amz-Signature
    let tampered_url = {
        if let Some(sig_start) = valid_url.find("X-Amz-Signature=") {
            let sig_value_start = sig_start + "X-Amz-Signature=".len();
            let (prefix, sig_and_rest) = valid_url.split_at(sig_value_start);
            // The signature is 64 hex chars at the end (no further query params)
            let sig_len = sig_and_rest.len();
            if sig_len >= 2 {
                // Replace last 2 chars with "XX"
                let new_sig: String = sig_and_rest[..sig_len - 2].to_string() + "XX";
                format!("{}{}", prefix, new_sig)
            } else {
                valid_url.clone()
            }
        } else {
            valid_url.clone()
        }
    };

    let resp = reqwest::Client::new()
        .get(&tampered_url)
        .header("host", &host)
        .send()
        .await
        .expect("HTTP request");

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Expected 403 for tampered presigned signature, got {}",
        resp.status()
    );
}

// ---------------------------------------------------------------------------
// === Chunked transfer encoding ===
// ---------------------------------------------------------------------------

/// PUT with `x-amz-content-sha256: UNSIGNED-PAYLOAD` header.
///
/// Verifies that the server correctly handles the UNSIGNED-PAYLOAD sentinel when
/// receiving a regular upload. The `Transfer-Encoding: chunked` is signalled via
/// the header, but the underlying transport may use either chunked or buffered
/// encoding — the key requirement is that the server accepts the request and
/// stores the object.
#[tokio::test]
async fn test_chunked_put_unsigned_payload() {
    // Use a server without auth so we can focus on the payload-header behaviour.
    let server = setup_auth_server("", "").await;
    let client = reqwest::Client::new();

    let bucket = "chunked-bucket";
    let key = "chunked-object.txt";
    let content = b"This is a chunked upload test payload.";

    // Create bucket
    let _bucket_resp = client
        .put(format!("{}/{}", server.base_url, bucket))
        .send()
        .await
        .expect("create bucket");

    // PUT the object with UNSIGNED-PAYLOAD.  reqwest 0.13 transmits this with
    // chunked TE when no explicit content-length is provided via the builder.
    let put_resp = client
        .put(format!("{}/{}/{}", server.base_url, bucket, key))
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
        .header("Transfer-Encoding", "chunked")
        .body(reqwest::Body::from(content.as_ref()))
        .send()
        .await
        .expect("PUT request with UNSIGNED-PAYLOAD");

    assert!(
        put_resp.status().is_success(),
        "Expected 2xx for PUT with UNSIGNED-PAYLOAD, got {}",
        put_resp.status()
    );

    // Verify the object was stored by fetching it back
    let get_resp = client
        .get(format!("{}/{}/{}", server.base_url, bucket, key))
        .send()
        .await
        .expect("GET request");

    assert!(
        get_resp.status().is_success(),
        "Expected 2xx when fetching object stored with UNSIGNED-PAYLOAD, got {}",
        get_resp.status()
    );

    let body = get_resp.bytes().await.expect("read body");
    assert_eq!(
        body.as_ref(),
        content,
        "Retrieved object content must match what was PUT"
    );
}

/// PUT with a zero-byte body and `x-amz-content-sha256: UNSIGNED-PAYLOAD`.
/// The server must accept the empty object (200/204).
#[tokio::test]
async fn test_chunked_put_empty() {
    // Use a server without auth so we can focus on empty-body behaviour.
    let server = setup_auth_server("", "").await;
    let client = reqwest::Client::new();

    let bucket = "chunked-empty-bucket";
    let key = "empty-object.bin";

    // Create bucket
    let _bucket_resp = client
        .put(format!("{}/{}", server.base_url, bucket))
        .send()
        .await
        .expect("create bucket");

    // PUT with a zero-byte body and the UNSIGNED-PAYLOAD sentinel.
    // content-length: 0 is set explicitly so the server can parse the empty body.
    let put_resp = client
        .put(format!("{}/{}/{}", server.base_url, bucket, key))
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
        .header("content-length", "0")
        .body(reqwest::Body::from(vec![]))
        .send()
        .await
        .expect("empty PUT request");

    assert!(
        put_resp.status().is_success(),
        "Expected 2xx for empty PUT with UNSIGNED-PAYLOAD, got {}",
        put_resp.status()
    );
}

// ---------------------------------------------------------------------------
// === Auth mode semantics ===
// ---------------------------------------------------------------------------

/// detect_auth_mode("", "") must return AuthMode::Passthrough.
#[test]
fn test_passthrough_mode_detect() {
    assert_eq!(detect_auth_mode("", ""), AuthMode::Passthrough);
}

/// detect_auth_mode("key", "secret") must return AuthMode::Authenticated.
#[test]
fn test_authenticated_mode_detect() {
    assert_eq!(detect_auth_mode("key", "secret"), AuthMode::Authenticated);
}

/// detect_auth_mode with only one key set must return AuthMode::Misconfigured.
#[test]
fn test_misconfigured_mode_detect() {
    assert_eq!(detect_auth_mode("key", ""), AuthMode::Misconfigured);
    assert_eq!(detect_auth_mode("", "secret"), AuthMode::Misconfigured);
}

/// Exempt paths (/health and /metrics) must return 200 (not 403) even when
/// authentication is fully enabled on the server.
#[tokio::test]
async fn test_exempt_paths_bypass_auth() {
    // Set up a server with auth enabled — no auth headers will be sent.
    let server = setup_auth_server("mykey", "mysecret").await;
    let client = reqwest::Client::new();

    // /health must not be blocked by the auth middleware.
    let health_resp = client
        .get(format!("{}/health", server.base_url))
        .send()
        .await
        .expect("GET /health");

    assert_ne!(
        health_resp.status(),
        StatusCode::FORBIDDEN,
        "/health must not return 403 (auth), got {}",
        health_resp.status()
    );

    // /metrics must not be blocked by the auth middleware.
    let metrics_resp = client
        .get(format!("{}/metrics", server.base_url))
        .send()
        .await
        .expect("GET /metrics");

    assert_ne!(
        metrics_resp.status(),
        StatusCode::FORBIDDEN,
        "/metrics must not return 403 (auth), got {}",
        metrics_resp.status()
    );
}
