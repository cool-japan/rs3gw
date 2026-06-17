#![cfg(feature = "server")]
//! Tests for S3 CORS preflight (OPTIONS) and simple-request header injection.
//!
//! Preflight tests use `setup_test_server()` (OPTIONS handler is part of
//! `s3_router::routes()`).  Simple-request tests use
//! `setup_test_server_with_cors_middleware()` which additionally wires the
//! `cors_simple_request` tower middleware — mirroring the production `main.rs`
//! setup.

mod common;

use common::{setup_test_server, setup_test_server_with_cors_middleware};

// ---------------------------------------------------------------------------
// Helper: XML bodies
// ---------------------------------------------------------------------------

fn cors_xml_example_com_get_put() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>
  <CORSRule>
    <AllowedOrigin>https://example.com</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedMethod>PUT</AllowedMethod>
    <AllowedHeader>*</AllowedHeader>
    <MaxAgeSeconds>3000</MaxAgeSeconds>
    <ExposeHeader>ETag</ExposeHeader>
  </CORSRule>
</CORSConfiguration>"#
}

fn cors_xml_wildcard_origin() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>
  <CORSRule>
    <AllowedOrigin>*</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedHeader>*</AllowedHeader>
  </CORSRule>
</CORSConfiguration>"#
}

fn cors_xml_glob_origin() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>
  <CORSRule>
    <AllowedOrigin>https://*.example.com</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedHeader>*</AllowedHeader>
  </CORSRule>
</CORSConfiguration>"#
}

fn cors_xml_get_only() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration>
  <CORSRule>
    <AllowedOrigin>https://example.com</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedHeader>*</AllowedHeader>
  </CORSRule>
</CORSConfiguration>"#
}

// ---------------------------------------------------------------------------
// Test 1: No CORS config → preflight returns 403
// ---------------------------------------------------------------------------

/// OPTIONS to a bucket with no CORS configuration must return 403.
#[tokio::test]
async fn test_preflight_no_cors_config_returns_403() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("cors-nocfg-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://example.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("OPTIONS request should complete");

    assert_eq!(
        resp.status(),
        403,
        "Preflight with no CORS config must return 403"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 2: Matching origin + method → 200 with CORS headers
// ---------------------------------------------------------------------------

/// OPTIONS with a matching Origin and Method returns 200 with the expected
/// `access-control-allow-origin`, `vary: Origin`, and
/// `access-control-allow-methods` headers.
#[tokio::test]
async fn test_preflight_matching_rule_returns_200_with_headers() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("cors-match-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT CORS config
    let put_resp = http
        .put(format!("{}/{}?cors", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(cors_xml_example_com_get_put())
        .send()
        .await
        .expect("PUT ?cors should complete");
    assert_eq!(
        put_resp.status(),
        200,
        "PutBucketCors should return 200, got: {}",
        put_resp.status()
    );

    // OPTIONS preflight
    let resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://example.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("OPTIONS preflight should complete");

    assert_eq!(
        resp.status(),
        200,
        "Preflight with matching rule must return 200"
    );
    assert_eq!(
        resp.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("https://example.com"),
        "Preflight must echo Access-Control-Allow-Origin"
    );
    assert!(
        resp.headers().get("vary").is_some(),
        "Preflight response must include Vary header"
    );
    assert_eq!(
        resp.headers().get("vary").and_then(|v| v.to_str().ok()),
        Some("Origin"),
        "Vary header must be 'Origin'"
    );
    assert!(
        resp.headers().get("access-control-allow-methods").is_some(),
        "Preflight response must include Access-Control-Allow-Methods"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 3: Origin mismatch → 403
// ---------------------------------------------------------------------------

/// OPTIONS with an Origin not listed in the CORS config must return 403.
#[tokio::test]
async fn test_preflight_origin_mismatch_returns_403() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("cors-mism-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}?cors", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(cors_xml_example_com_get_put())
        .send()
        .await
        .expect("PUT ?cors should complete");
    assert_eq!(put_resp.status(), 200, "PutBucketCors should return 200");

    let resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://evil.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("OPTIONS with wrong origin should complete");

    assert_eq!(
        resp.status(),
        403,
        "Preflight with wrong origin must return 403"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 4: Wildcard origin "*" matches any origin
// ---------------------------------------------------------------------------

/// `AllowedOrigin: *` must match any Origin.
#[tokio::test]
async fn test_preflight_wildcard_origin() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("cors-wild-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}?cors", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(cors_xml_wildcard_origin())
        .send()
        .await
        .expect("PUT ?cors should complete");
    assert_eq!(put_resp.status(), 200, "PutBucketCors should return 200");

    // Any origin should match
    let resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://arbitrary-origin.example.org")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("OPTIONS with wildcard origin should complete");

    assert_eq!(
        resp.status(),
        200,
        "Preflight with wildcard origin must return 200"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 5: Glob origin "https://*.example.com" — match vs. non-match
// ---------------------------------------------------------------------------

/// `AllowedOrigin: https://*.example.com` must match subdomains of example.com
/// but reject other domains.
#[tokio::test]
async fn test_preflight_glob_origin() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("cors-glob-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}?cors", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(cors_xml_glob_origin())
        .send()
        .await
        .expect("PUT ?cors should complete");
    assert_eq!(put_resp.status(), 200, "PutBucketCors should return 200");

    // Matching subdomain → 200
    let match_resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://app.example.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("OPTIONS with matching subdomain should complete");
    assert_eq!(
        match_resp.status(),
        200,
        "Preflight with matching glob subdomain must return 200"
    );

    // Non-matching domain → 403
    let no_match_resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://app.evil.com")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("OPTIONS with non-matching domain should complete");
    assert_eq!(
        no_match_resp.status(),
        403,
        "Preflight with non-matching glob must return 403"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 6: Simple GET with Origin attaches CORS headers (middleware)
// ---------------------------------------------------------------------------

/// A non-OPTIONS GET request carrying an Origin header must have
/// `access-control-allow-origin` and `vary: Origin` attached to the response
/// when the origin matches a stored CORS rule.
///
/// This test uses `setup_test_server_with_cors_middleware()` which wires the
/// `cors_simple_request` tower middleware — absent from the default test setup.
#[tokio::test]
async fn test_simple_get_with_origin_attaches_allow_origin() {
    let (client, _temp_dir, server) = setup_test_server_with_cors_middleware().await;
    let bucket = format!("cors-simple-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    // PUT CORS config
    let put_cors = http
        .put(format!("{}/{}?cors", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(cors_xml_example_com_get_put())
        .send()
        .await
        .expect("PUT ?cors should complete");
    assert_eq!(put_cors.status(), 200, "PutBucketCors should return 200");

    // PUT an object so there is something to GET
    let put_obj = http
        .put(format!("{}/{}/doc.txt", base_url, bucket))
        .body(b"hello cors".as_ref())
        .send()
        .await
        .expect("PUT object should complete");
    assert_eq!(put_obj.status(), 200, "PUT object should return 200");

    // GET with Origin header → simple CORS request
    let get_resp = http
        .get(format!("{}/{}/doc.txt", base_url, bucket))
        .header("Origin", "https://example.com")
        .send()
        .await
        .expect("GET with Origin should complete");

    assert_eq!(get_resp.status(), 200, "GET should return 200");
    assert_eq!(
        get_resp
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("https://example.com"),
        "Simple-request GET must include access-control-allow-origin"
    );
    assert_eq!(
        get_resp.headers().get("vary").and_then(|v| v.to_str().ok()),
        Some("Origin"),
        "Simple-request GET must include Vary: Origin"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}

// ---------------------------------------------------------------------------
// Test 7: Method mismatch → 403
// ---------------------------------------------------------------------------

/// CORS config allows only `GET`. OPTIONS with `DELETE` must return 403.
#[tokio::test]
async fn test_preflight_method_mismatch_returns_403() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket = format!("cors-methm-{}", uuid::Uuid::new_v4());
    let base_url = &server.base_url;
    let http = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket)
        .send()
        .await
        .expect("create_bucket should succeed");

    let put_resp = http
        .put(format!("{}/{}?cors", base_url, bucket))
        .header("Content-Type", "application/xml")
        .body(cors_xml_get_only())
        .send()
        .await
        .expect("PUT ?cors should complete");
    assert_eq!(put_resp.status(), 200, "PutBucketCors should return 200");

    let resp = http
        .request(reqwest::Method::OPTIONS, format!("{}/{}", base_url, bucket))
        .header("Origin", "https://example.com")
        .header("Access-Control-Request-Method", "DELETE")
        .send()
        .await
        .expect("OPTIONS with disallowed method should complete");

    assert_eq!(
        resp.status(),
        403,
        "Preflight with method not in CORS rule must return 403"
    );

    client.delete_bucket().bucket(&bucket).send().await.ok();
}
