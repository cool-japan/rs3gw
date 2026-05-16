//! CORS preflight OPTIONS handler
//!
//! Consults the bucket's stored CorsConfig and evaluates preflight requests
//! per the S3 CORS spec: origin glob, method match, case-insensitive header match.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

use crate::api::utils::error_response;
use crate::storage::{CorsConfig, CorsRule};
use crate::AppState;

/// Handle OPTIONS preflight for a bucket endpoint.
pub async fn options_bucket_handler(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    headers: HeaderMap,
) -> Response {
    options_inner(&state, &bucket, &headers).await
}

/// Handle OPTIONS preflight for an object endpoint.
pub async fn options_object_handler(
    State(state): State<AppState>,
    Path((bucket, _key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    options_inner(&state, &bucket, &headers).await
}

async fn options_inner(state: &AppState, bucket: &str, headers: &HeaderMap) -> Response {
    let cfg = match state.storage.get_bucket_cors(bucket).await {
        Ok(c) => c,
        Err(_) => return cors_access_denied(),
    };

    let origin = match headers.get("origin").and_then(|v| v.to_str().ok()) {
        Some(o) => o.to_string(),
        None => return cors_access_denied(),
    };

    let req_method = match headers
        .get("access-control-request-method")
        .and_then(|v| v.to_str().ok())
    {
        Some(m) => m.to_string(),
        None => return cors_access_denied(),
    };

    let req_headers_raw = headers
        .get("access-control-request-headers")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let req_headers: Vec<&str> = req_headers_raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let rule = match match_cors_rule(&cfg, &origin, &req_method, &req_headers) {
        Some(r) => r,
        None => return cors_access_denied(),
    };

    build_preflight_response(&origin, &req_method, &req_headers_raw, rule)
}

/// Build a 200 preflight response from a matched CORS rule.
fn build_preflight_response(
    origin: &str,
    req_method: &str,
    req_headers_raw: &str,
    rule: &CorsRule,
) -> Response {
    let mut resp = StatusCode::OK.into_response();
    let h = resp.headers_mut();

    if let Ok(v) = HeaderValue::from_str(origin) {
        h.insert("access-control-allow-origin", v);
    }
    if let Ok(v) = HeaderValue::from_str(req_method) {
        h.insert("access-control-allow-methods", v);
    }
    if !req_headers_raw.is_empty() {
        if let Ok(v) = HeaderValue::from_str(req_headers_raw) {
            h.insert("access-control-allow-headers", v);
        }
    }
    if let Some(age) = rule.max_age_seconds {
        if let Ok(v) = HeaderValue::from_str(&age.to_string()) {
            h.insert("access-control-max-age", v);
        }
    }
    let expose = rule.expose_headers.join(",");
    if !expose.is_empty() {
        if let Ok(v) = HeaderValue::from_str(&expose) {
            h.insert("access-control-expose-headers", v);
        }
    }
    // D6: Vary: Origin MUST be on every CORS response to prevent cache poisoning.
    h.insert("vary", HeaderValue::from_static("Origin"));

    resp
}

/// Find the first matching CORS rule for a preflight request.
pub fn match_cors_rule<'a>(
    cfg: &'a CorsConfig,
    origin: &str,
    method: &str,
    req_headers: &[&str],
) -> Option<&'a CorsRule> {
    cfg.rules.iter().find(|r| {
        r.allowed_origins.iter().any(|o| origin_matches(o, origin))
            && r.allowed_methods
                .iter()
                .any(|m| m.eq_ignore_ascii_case(method))
            && req_headers
                .iter()
                .all(|h| r.allowed_headers.iter().any(|a| header_matches(a, h)))
    })
}

/// Origin matching: `"*"` matches all; `"https://*.example.com"` glob (single `*`); exact otherwise.
fn origin_matches(rule_pat: &str, request: &str) -> bool {
    if rule_pat == "*" {
        return true;
    }
    if let Some(star_pos) = rule_pat.find('*') {
        let prefix = &rule_pat[..star_pos];
        let suffix = &rule_pat[star_pos + 1..];
        return request.starts_with(prefix)
            && request.ends_with(suffix)
            && request.len() >= prefix.len() + suffix.len();
    }
    rule_pat == request
}

/// Header matching: `"*"` matches any header; case-insensitive exact match otherwise. (D6)
fn header_matches(allowed: &str, requested: &str) -> bool {
    allowed == "*" || allowed.eq_ignore_ascii_case(requested)
}

/// S3-style 403 response for CORS rejection.
///
/// Per D6, this MUST NOT be an empty body — returns XML `<Error>` content.
fn cors_access_denied() -> Response {
    error_response(
        StatusCode::FORBIDDEN,
        "AccessDenied",
        "CORSResponse: This CORS request is not allowed. Check the bucket CORS configuration.",
        "",
    )
}
