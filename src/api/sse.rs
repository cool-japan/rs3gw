//! SSE decision resolution for PutObject and related operations.

use axum::{
    http::{HeaderMap, StatusCode},
    response::Response,
};
use base64::Engine as _;

use crate::api::utils::error_response;
use crate::AppState;

/// Decision tree for per-object SSE on PutObject.
pub enum SseDecision {
    /// No encryption — store plaintext.
    None,
    /// Encrypt with SSE-S3 (AES-256-GCM, server-managed key).
    Aes256,
    /// Encrypt with customer-provided key (SSE-C).
    SseC {
        /// 32-byte customer-provided key (AES-256).  Boxed to avoid accidental
        /// stack copies of key material.
        key: Box<[u8; 32]>,
        /// Base64-encoded MD5 of the raw key bytes (echoed in PUT/GET responses).
        key_md5: String,
    },
    /// Encrypt with a server-managed KMS key (SSE-KMS local shim).
    ///
    /// Functionally identical to SSE-S3 at the crypto level (AES-256-GCM envelope
    /// encryption), but uses a named KEK and different response headers.
    SseKms {
        /// Resolved KMS key ID (stored in sidecar as `kms_master_key_id`).
        key_id: String,
    },
}

/// Format a key ID as a fake AWS KMS ARN for response headers.
pub fn format_kms_arn(key_id: &str) -> String {
    format!("arn:aws:kms:us-east-1:000000000000:key/{}", key_id)
}

/// Resolve the SSE algorithm for a PutObject request.
///
/// Precedence (highest first):
///   1. SSE-C headers (`x-amz-server-side-encryption-customer-algorithm`)
///   2. Per-request `x-amz-server-side-encryption` header
///   3. Bucket-default encryption configuration
///   4. None (plaintext)
///
/// Returns 501 NotImplemented for aws:kms / aws:kms:dsse.
/// Returns 400 InvalidArgument for unknown algorithms or bad SSE-C parameters.
pub async fn resolve_sse(
    state: &AppState,
    bucket: &str,
    headers: &HeaderMap,
) -> Result<SseDecision, Response> {
    // SSE-C check takes priority — if the customer-algorithm header is present,
    // we are in SSE-C mode regardless of the generic SSE header.
    if headers
        .get("x-amz-server-side-encryption-customer-algorithm")
        .is_some()
    {
        let resource = format!("/{}", bucket);

        // Validate algorithm — only "AES256" is supported.
        let algo = headers
            .get("x-amz-server-side-encryption-customer-algorithm")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if algo != "AES256" {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                &format!(
                    "Unsupported SSE-C algorithm: '{}'. Only AES256 is supported.",
                    algo
                ),
                &resource,
            ));
        }

        // Decode the customer key (must be exactly 32 bytes for AES-256).
        let key_b64 = headers
            .get("x-amz-server-side-encryption-customer-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(key_b64)
            .map_err(|_| {
                error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    "x-amz-server-side-encryption-customer-key is not valid base64",
                    &resource,
                )
            })?;
        if key_bytes.len() != 32 {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                &format!(
                    "SSE-C customer key must be 32 bytes for AES256, got {}",
                    key_bytes.len()
                ),
                &resource,
            ));
        }

        // Compute MD5 of the raw key and compare with the provided MD5 header.
        let computed_md5_bytes = md5::compute(key_bytes.as_slice()).0;
        let computed_md5_b64 = base64::engine::general_purpose::STANDARD.encode(computed_md5_bytes);

        let provided_md5 = headers
            .get("x-amz-server-side-encryption-customer-key-MD5")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();

        if provided_md5 != computed_md5_b64 {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "The MD5 you specified did not match the calculated MD5 for the customer-provided key",
                &resource,
            ));
        }

        // Build the fixed-size key array.
        let mut key_array = [0u8; 32];
        key_array.copy_from_slice(&key_bytes);

        return Ok(SseDecision::SseC {
            key: Box::new(key_array),
            key_md5: computed_md5_b64,
        });
    }

    if let Some(h) = headers.get("x-amz-server-side-encryption") {
        let algo_str = h.to_str().unwrap_or_default();
        match algo_str {
            "AES256" => return Ok(SseDecision::Aes256),
            "aws:kms" => {
                let resource = format!("/{}", bucket);
                let requested_key_id = headers
                    .get("x-amz-server-side-encryption-aws-kms-key-id")
                    .and_then(|v| v.to_str().ok());
                let key_id = state
                    .encryption
                    .resolve_kms_key_id(requested_key_id)
                    .await
                    .map_err(|_| {
                        error_response(
                            StatusCode::BAD_REQUEST,
                            "InvalidArgument",
                            "KMS key not found",
                            &resource,
                        )
                    })?;
                return Ok(SseDecision::SseKms { key_id });
            }
            "aws:kms:dsse" => {
                return Err(error_response(
                    StatusCode::NOT_IMPLEMENTED,
                    "NotImplemented",
                    "SSE-KMS double-layer (aws:kms:dsse) is not supported.",
                    &format!("/{}", bucket),
                ));
            }
            algo => {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "InvalidArgument",
                    &format!("Unknown server-side encryption algorithm: {}", algo),
                    &format!("/{}", bucket),
                ));
            }
        }
    }

    // Fall back to bucket-default encryption
    if let Ok(cfg) = state.storage.get_bucket_encryption(bucket).await {
        if let Some(rule) = cfg.rules.first() {
            match rule.sse_algorithm.as_str() {
                "AES256" => return Ok(SseDecision::Aes256),
                "aws:kms" => {
                    let resource = format!("/{}", bucket);
                    let key_id = state
                        .encryption
                        .resolve_kms_key_id(None)
                        .await
                        .map_err(|_| {
                            error_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "InternalError",
                                "Failed to resolve default KMS key",
                                &resource,
                            )
                        })?;
                    return Ok(SseDecision::SseKms { key_id });
                }
                _ => return Ok(SseDecision::None),
            }
        }
    }

    Ok(SseDecision::None)
}

/// Parse and validate the SSE-C copy-source headers for CopyObject.
///
/// Headers used:
///   - `x-amz-copy-source-server-side-encryption-customer-algorithm`
///   - `x-amz-copy-source-server-side-encryption-customer-key`
///   - `x-amz-copy-source-server-side-encryption-customer-key-MD5`
///
/// Returns:
///   - `Ok(None)` — none of the copy-source SSE-C headers are present (source is not SSE-C).
///   - `Ok(Some(key))` — valid 32-byte key extracted from headers.
///   - `Err(Response)` — headers present but malformed/MD5 mismatch → 400.
pub async fn resolve_sse_c_copy_source(
    headers: &HeaderMap,
) -> Result<Option<Box<[u8; 32]>>, Response> {
    // If the algorithm header is absent, the copy source is not SSE-C.
    let algo_hv = match headers.get("x-amz-copy-source-server-side-encryption-customer-algorithm") {
        Some(v) => v,
        None => return Ok(None),
    };

    let algo = algo_hv.to_str().unwrap_or_default();
    if algo != "AES256" {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            &format!(
                "Unsupported copy-source SSE-C algorithm: '{}'. Only AES256 is supported.",
                algo
            ),
            "/copy-source",
        ));
    }

    let key_b64 = headers
        .get("x-amz-copy-source-server-side-encryption-customer-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let key_bytes = base64::engine::general_purpose::STANDARD
        .decode(key_b64)
        .map_err(|_| {
            error_response(
                StatusCode::BAD_REQUEST,
                "InvalidArgument",
                "x-amz-copy-source-server-side-encryption-customer-key is not valid base64",
                "/copy-source",
            )
        })?;
    if key_bytes.len() != 32 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            &format!(
                "SSE-C copy-source customer key must be 32 bytes for AES256, got {}",
                key_bytes.len()
            ),
            "/copy-source",
        ));
    }

    let computed_md5_bytes = md5::compute(key_bytes.as_slice()).0;
    let computed_md5_b64 = base64::engine::general_purpose::STANDARD.encode(computed_md5_bytes);

    let provided_md5 = headers
        .get("x-amz-copy-source-server-side-encryption-customer-key-MD5")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if provided_md5 != computed_md5_b64 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "The MD5 you specified did not match the calculated MD5 for the copy-source customer-provided key",
            "/copy-source",
        ));
    }

    let mut key_array = [0u8; 32];
    key_array.copy_from_slice(&key_bytes);
    Ok(Some(Box::new(key_array)))
}
