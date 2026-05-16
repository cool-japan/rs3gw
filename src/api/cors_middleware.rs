//! CORS middleware for simple (non-OPTIONS) requests.
//!
//! When a request carries an `Origin` header, this middleware evaluates the
//! bucket's stored CorsConfig after the inner handler runs. On a match it
//! attaches `Access-Control-Allow-Origin`, `Access-Control-Expose-Headers`
//! (if the rule specifies any), and `Vary: Origin` (cache-poisoning prevention).
//!
//! Non-bucket paths (e.g. /health, /api/...) and requests without `Origin`
//! pass through untouched.

use axum::{
    extract::{Request, State},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};

use crate::api::cors::match_cors_rule;
use crate::AppState;

/// Internal paths that do not correspond to S3 bucket names.
///
/// When the first path segment matches one of these, the middleware returns the
/// response unchanged without attempting to load a CORS configuration.
const INTERNAL_SEGMENTS: &[&str] = &[
    "health",
    "ready",
    "metrics",
    "api",
    "graphql",
    "events",
    "swagger-ui",
    "openapi.json",
    "presign",
    "WriteGetObjectResponse",
];

/// Axum middleware: attach CORS headers to simple (non-OPTIONS) requests that match a stored rule.
///
/// This must be registered via [`axum::middleware::from_fn_with_state`].
pub async fn cors_simple_request(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // Extract Origin header and path before consuming the request.
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Only proceed if an Origin header is present.
    let origin = match origin {
        Some(o) => o,
        None => return next.run(request).await,
    };

    // Extract the first non-empty path segment as a potential bucket name.
    let path = request.uri().path().to_string();
    let bucket = path
        .split('/')
        .find(|s| !s.is_empty())
        .map(|s| s.to_string());

    let method = request.method().as_str().to_string();

    // Run the inner handler first; CORS headers are appended to the real response.
    let mut response = next.run(request).await;

    // Skip internal / non-S3 paths.
    let bucket = match bucket {
        Some(b) if !INTERNAL_SEGMENTS.contains(&b.as_str()) => b,
        _ => return response,
    };

    // Attempt to load the bucket's CORS configuration.
    let cfg = match state.storage.get_bucket_cors(&bucket).await {
        Ok(c) => c,
        // No CORS config (or bucket doesn't exist) — leave the response unchanged.
        Err(_) => return response,
    };

    // For simple requests there are no request headers to forward; pass an empty slice.
    let rule = match match_cors_rule(&cfg, &origin, &method, &[]) {
        Some(r) => r,
        // No matching rule — do not add CORS headers.
        None => return response,
    };

    // Attach CORS response headers.
    let h = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&origin) {
        h.insert("access-control-allow-origin", v);
    }
    let expose = rule.expose_headers.join(",");
    if !expose.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&expose) {
            h.insert("access-control-expose-headers", v);
        }
    }
    // Vary: Origin prevents cache poisoning (required per the S3 CORS spec).
    h.insert("vary", HeaderValue::from_static("Origin"));

    response
}
