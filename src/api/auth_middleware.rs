//! SigV4 authentication middleware for rs3gw S3 API.
//!
//! # Operating modes
//!
//! The server operates in one of three authentication modes, determined at startup from the
//! configured `access_key` and `secret_key` values:
//!
//! ## Passthrough mode
//! Both `access_key` and `secret_key` are empty.  All requests (including those without any
//! `Authorization` header) pass through the middleware unconditionally.  This mode is suitable
//! for local development and internal-only deployments where network-level access controls
//! provide sufficient isolation.  **Never use in production.**
//!
//! ## Authenticated mode
//! Both `access_key` and `secret_key` are set.  Every request that is not in `EXEMPT_PATHS`
//! must carry a valid AWS Signature Version 4 signature — either via the `Authorization` request
//! header (`AWS4-HMAC-SHA256 …`) or via a presigned URL query string (`X-Amz-Signature=…`).
//! Requests without a recognised auth mechanism, or with an invalid signature, receive
//! `403 AccessDenied`.  Repeated failures from the same IP trigger `429 TooManyRequests`.
//!
//! ## Misconfigured mode
//! Exactly one of `access_key` / `secret_key` is set while the other is empty.  This is a
//! configuration error — a partial key pair cannot produce valid SigV4 signatures.  The server
//! should log an error and refuse to start when [`detect_auth_mode`] returns
//! [`AuthMode::Misconfigured`].
//!
//! # Rate limiting
//! After `MAX_AUTH_FAILURES` consecutive failures within `AUTH_FAILURE_WINDOW_SECS` from the
//! same IP address, subsequent requests receive `HTTP 429 TooManyRequests` until the window resets.

use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use std::net::IpAddr;
use std::sync::Arc;

use crate::api::utils::error_response;
use crate::auth::v4::PresignedParams;
use crate::AppState;

// ---------------------------------------------------------------------------
// Auth mode
// ---------------------------------------------------------------------------

/// Describes how the server is configured to handle authentication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMode {
    /// Both keys empty — all requests pass through. Dev/internal use only.
    Passthrough,
    /// Both keys set — all requests (except exempt paths) must be SigV4-signed.
    Authenticated,
    /// Only one key set — misconfiguration. Server should refuse to start.
    Misconfigured,
}

/// Detect the authentication operating mode from configured key material.
///
/// # Arguments
/// * `access_key` - The configured AWS access key ID (empty string if unset).
/// * `secret_key` - The configured AWS secret access key (empty string if unset).
///
/// # Returns
/// * [`AuthMode::Passthrough`] when both are empty (dev/no-auth mode).
/// * [`AuthMode::Authenticated`] when both are non-empty (production SigV4 mode).
/// * [`AuthMode::Misconfigured`] when exactly one is set (configuration error).
pub fn detect_auth_mode(access_key: &str, secret_key: &str) -> AuthMode {
    match (access_key.is_empty(), secret_key.is_empty()) {
        (true, true) => AuthMode::Passthrough,
        (false, false) => AuthMode::Authenticated,
        _ => AuthMode::Misconfigured,
    }
}

/// Paths exempt from SigV4 authentication
const EXEMPT_PATHS: &[&str] = &["/health", "/ready", "/metrics", "/openapi.json"];

/// Maximum number of consecutive auth failures per IP before rate-limiting kicks in
const MAX_AUTH_FAILURES: u32 = 10;

/// Sliding-window duration in seconds for the auth-failure rate limiter
const AUTH_FAILURE_WINDOW_SECS: u64 = 60;

/// Extract the best-effort client IP from the request.
///
/// Priority:
/// 1. `X-Forwarded-For` first value (set by load balancers / reverse proxies)
/// 2. Axum `ConnectInfo<SocketAddr>` extension
/// 3. Fallback: `127.0.0.1`
fn extract_client_ip(request: &axum::extract::Request) -> IpAddr {
    // 1. X-Forwarded-For header
    if let Some(forwarded_for) = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
    {
        if let Some(first_ip) = forwarded_for.split(',').next() {
            if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                return ip;
            }
        }
    }

    // 2. ConnectInfo extension (populated by axum::serve when using .into_make_service_with_connect_info)
    if let Some(addr) = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
    {
        return addr.0.ip();
    }

    // 3. Fallback
    IpAddr::from([127, 0, 0, 1])
}

/// Record an auth failure for the given IP and check whether it has exceeded the rate limit.
///
/// Returns `true` when the caller should be blocked (429).
fn check_and_record_failure(state: &AppState, ip: IpAddr) -> bool {
    let mut counts = state
        .auth_failure_counts
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let now = std::time::Instant::now();
    let entry = counts.entry(ip).or_insert_with(|| (0u32, now));

    // Reset counter if the window has elapsed
    if now.duration_since(entry.1).as_secs() >= AUTH_FAILURE_WINDOW_SECS {
        *entry = (0u32, now);
    }

    entry.0 = entry.0.saturating_add(1);

    entry.0 > MAX_AUTH_FAILURES
}

/// Clear the failure counter for an IP after a successful authentication.
fn clear_failure_counter(state: &AppState, ip: IpAddr) {
    let mut counts = state
        .auth_failure_counts
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    counts.remove(&ip);
}

/// Build the 429 TooManyRequests response for auth rate limiting.
fn too_many_requests_response(_uri_path: &str) -> Response {
    use axum::body::Body;
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
               <Error>\
               <Code>TooManyRequests</Code>\
               <Message>Too many authentication failures</Message>\
               </Error>";

    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("Content-Type", "application/xml")
        .header("Retry-After", "60")
        .header("x-amz-request-id", uuid::Uuid::new_v4().to_string())
        .body(Body::from(xml))
        .unwrap_or_else(|_| {
            // Last-resort fallback: plain 429 with no body
            let mut resp = axum::response::Response::new(Body::empty());
            *resp.status_mut() = StatusCode::TOO_MANY_REQUESTS;
            resp
        })
}

/// SigV4 authentication middleware.
///
/// Behaviour:
/// - Paths in `EXEMPT_PATHS` are always passed through.
/// - If `state.verifier` is `None` (no credentials configured), all requests pass through.
/// - If the query string contains `X-Amz-Signature`, presigned-URL verification is used.
/// - If the `Authorization` header starts with `AWS4-HMAC-SHA256`, header auth is used.
/// - Any other request without auth → 403 AccessDenied.
/// - After `MAX_AUTH_FAILURES` failures within `AUTH_FAILURE_WINDOW_SECS` per IP → 429.
pub async fn sigv4_auth_layer(
    State(state): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, Response> {
    let path = request.uri().path().to_owned();

    // Always exempt health/metrics/openapi endpoints
    if EXEMPT_PATHS.iter().any(|&p| path == p) {
        return Ok(next.run(request).await);
    }

    // Emit diagnostic logs based on the configured auth mode.
    // These are emitted once per request that reaches this point; callers should
    // use server startup checks (detect_auth_mode) to log once at init time.
    {
        let mode = detect_auth_mode(&state.config.access_key, &state.config.secret_key);
        match mode {
            AuthMode::Passthrough => {
                tracing::warn!(
                    "Auth middleware is in PASSTHROUGH mode (no credentials configured). \
                     All requests pass without authentication. Do not use in production."
                );
            }
            AuthMode::Misconfigured => {
                tracing::error!(
                    "Auth misconfiguration: only one of access_key/secret_key is set. \
                     Server should refuse to start."
                );
            }
            AuthMode::Authenticated => {}
        }
    }

    // If no verifier is configured, pass through unconditionally (MVP passthrough mode)
    let verifier = match &state.verifier {
        Some(v) => Arc::clone(v),
        None => return Ok(next.run(request).await),
    };

    let client_ip = extract_client_ip(&request);
    let method = request.method().as_str().to_owned();
    let uri_path = path;
    let query_string = request.uri().query().unwrap_or("").to_owned();

    // Build headers vec from request headers
    let headers_vec: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_lowercase(), v.to_owned()))
        })
        .collect();

    // Determine payload hash: prefer x-amz-content-sha256, else UNSIGNED-PAYLOAD
    let payload_hash = headers_vec
        .iter()
        .find(|(name, _)| name == "x-amz-content-sha256")
        .map(|(_, v)| v.as_str())
        .unwrap_or("UNSIGNED-PAYLOAD")
        .to_owned();

    // Presigned URL path: query string carries X-Amz-Signature
    if PresignedParams::is_presigned_request(&query_string) {
        match verifier.verify_presigned_request(&method, &uri_path, &query_string, &headers_vec) {
            Ok(()) => {
                clear_failure_counter(&state, client_ip);
                return Ok(next.run(request).await);
            }
            Err(e) => {
                tracing::warn!(
                    "Presigned URL auth failed for {} {}: {}",
                    method,
                    uri_path,
                    e
                );
                if check_and_record_failure(&state, client_ip) {
                    return Err(too_many_requests_response(&uri_path));
                }
                return Err(error_response(
                    StatusCode::FORBIDDEN,
                    "AccessDenied",
                    "Access Denied",
                    &uri_path,
                ));
            }
        }
    }

    // Header-based auth: look for AWS4-HMAC-SHA256 Authorization header
    let auth_header_value = headers_vec
        .iter()
        .find(|(name, _)| name == "authorization")
        .map(|(_, v)| v.clone());

    if let Some(ref auth_header) = auth_header_value {
        if auth_header.starts_with("AWS4-HMAC-SHA256") {
            match verifier.verify_request(
                &method,
                &uri_path,
                &query_string,
                &headers_vec,
                &payload_hash,
                Some(auth_header.as_str()),
            ) {
                Ok(()) => {
                    clear_failure_counter(&state, client_ip);
                    return Ok(next.run(request).await);
                }
                Err(e) => {
                    tracing::warn!(
                        "SigV4 header auth failed for {} {}: {}",
                        method,
                        uri_path,
                        e
                    );
                    if check_and_record_failure(&state, client_ip) {
                        return Err(too_many_requests_response(&uri_path));
                    }
                    return Err(error_response(
                        StatusCode::FORBIDDEN,
                        "AccessDenied",
                        "Access Denied",
                        &uri_path,
                    ));
                }
            }
        }
    }

    // No recognised auth mechanism present and verifier is active → deny
    tracing::warn!(
        "Request {} {} missing SigV4 auth (verifier enabled)",
        method,
        uri_path
    );
    if check_and_record_failure(&state, client_ip) {
        return Err(too_many_requests_response(&uri_path));
    }
    Err(error_response(
        StatusCode::FORBIDDEN,
        "AccessDenied",
        "Access Denied",
        &uri_path,
    ))
}
