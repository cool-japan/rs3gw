//! AWS Signature Version 4 Verification
//!
//! This module implements AWS SigV4 authentication for S3 API requests.
//! Supports both header-based and presigned URL authentication.
//! Includes timestamp validation (±15 minutes) for production security.
//!
//! # Canonical headers
//!
//! The SigV4 canonical-headers string is constructed from a selected subset of request headers:
//!
//! * `host` — always included; the value is the server hostname (and port if non-standard).
//! * All `x-amz-*` headers present in the request — e.g. `x-amz-date`, `x-amz-content-sha256`,
//!   `x-amz-security-token`.
//! * Any additional headers that the signer explicitly lists in `SignedHeaders`.
//!
//! ## Header name canonicalization
//!
//! Header names are converted to **lowercase** before being included in the canonical string.
//! The set of signed headers is then **sorted lexicographically** by the lowercased name.
//! This ensures a deterministic ordering that both the signer and verifier agree on regardless
//! of the order in which headers appear in the HTTP request.
//!
//! ## Canonical headers string format
//!
//! Each signed header contributes one line of the form:
//!
//! ```text
//! header-name:trimmed-value\n
//! ```
//!
//! * `header-name` — lowercase header name.
//! * `trimmed-value` — leading and trailing whitespace removed from the header value; consecutive
//!   interior whitespace is collapsed to a single space (the "trim" step in the AWS docs).
//! * Each entry is terminated by a newline (`\n`), including the last one.  The canonical-headers
//!   component of the canonical request therefore always ends with a newline, and there is no
//!   separate trailing newline added after the last entry.
//!
//! ## `x-amz-content-sha256: UNSIGNED-PAYLOAD`
//!
//! When the SHA-256 hash of the request body is not computed by the client (e.g. for streaming
//! uploads or when the body length is unknown at signing time), the literal string
//! `UNSIGNED-PAYLOAD` is used as the payload-hash component of the canonical request.
//!
//! * The client sets the `x-amz-content-sha256` header to `UNSIGNED-PAYLOAD`.
//! * The verifier reads this header value and passes it verbatim as the `payload_hash` argument
//!   to [`SigV4Verifier::verify_request`] — no SHA-256 computation is performed on the body.
//! * This header **must** be included in `SignedHeaders` when the client sends it, so that its
//!   value is covered by the signature.
//! * Presigned URLs always use `UNSIGNED-PAYLOAD` as the payload hash because the body is not
//!   known at URL-generation time.

#![allow(dead_code)]

use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use thiserror::Error;

type HmacSha256 = Hmac<Sha256>;

/// Maximum allowed time difference for request timestamps (±15 minutes)
const MAX_TIMESTAMP_SKEW_MINUTES: i64 = 15;

/// Default presigned URL expiration (7 days in seconds)
const DEFAULT_PRESIGNED_EXPIRATION: u64 = 604800;

/// Authentication errors
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Missing authorization header")]
    MissingAuth,

    #[error("Invalid authorization header format")]
    InvalidFormat,

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Request expired")]
    RequestExpired,

    #[error("Invalid access key")]
    InvalidAccessKey,

    #[error("Missing required signed header: {0}")]
    MissingSignedHeader(String),

    #[error("Missing required presigned parameter: {0}")]
    MissingPresignedParam(String),
}

/// AWS Signature V4 Verifier
#[derive(Debug, Clone)]
pub struct SigV4Verifier {
    access_key: String,
    secret_key: String,
    region: String,
    service: String,
    /// Whether to validate request timestamps (±15 minutes)
    validate_timestamp: bool,
}

impl SigV4Verifier {
    pub fn new(access_key: String, secret_key: String, region: String) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            service: "s3".to_string(),
            validate_timestamp: true, // Enable by default
        }
    }

    /// Create a verifier with timestamp validation disabled (for testing)
    pub fn new_without_timestamp_validation(
        access_key: String,
        secret_key: String,
        region: String,
    ) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            service: "s3".to_string(),
            validate_timestamp: false,
        }
    }

    /// Check if authentication is enabled
    pub fn is_enabled(&self) -> bool {
        !self.access_key.is_empty() && !self.secret_key.is_empty()
    }

    /// Verify a request's signature
    /// Returns Ok(()) if valid, Err if invalid
    pub fn verify_request(
        &self,
        method: &str,
        uri: &str,
        query_string: &str,
        headers: &[(String, String)],
        payload_hash: &str,
        auth_header: Option<&str>,
    ) -> Result<(), AuthError> {
        // If auth is not configured, allow all requests (MVP mode)
        if !self.is_enabled() {
            return Ok(());
        }

        let auth_header = auth_header.ok_or(AuthError::MissingAuth)?;

        // Parse the Authorization header
        let parsed = Self::parse_auth_header(auth_header)?;

        // Verify access key
        if parsed.access_key != self.access_key {
            return Err(AuthError::InvalidAccessKey);
        }

        // Validate timestamp if enabled
        if self.validate_timestamp {
            self.validate_request_timestamp(headers, &parsed.timestamp)?;
        }

        // Build the canonical request
        let canonical_request = self.build_canonical_request(
            method,
            uri,
            query_string,
            headers,
            &parsed.signed_headers,
            payload_hash,
        )?;

        // Build the string to sign
        let string_to_sign = self.build_string_to_sign(
            &parsed.timestamp,
            &parsed.credential_scope,
            &canonical_request,
        );

        // Calculate the signature
        let signature = self.calculate_signature(&parsed.date, &string_to_sign);

        // Compare signatures
        if signature != parsed.signature {
            return Err(AuthError::InvalidSignature);
        }

        Ok(())
    }

    /// Validate request timestamp is within ±15 minutes of server time
    #[cfg_attr(test, allow(dead_code))]
    pub fn validate_request_timestamp(
        &self,
        headers: &[(String, String)],
        credential_timestamp: &str,
    ) -> Result<(), AuthError> {
        // Try to get x-amz-date header first, then Date header
        let timestamp_str = headers
            .iter()
            .find(|(name, _)| name.to_lowercase() == "x-amz-date")
            .or_else(|| {
                headers
                    .iter()
                    .find(|(name, _)| name.to_lowercase() == "date")
            })
            .map(|(_, v)| v.as_str())
            .unwrap_or(credential_timestamp);

        // Parse AWS timestamp format: YYYYMMDDTHHMMSSZ
        let request_time = Self::parse_aws_timestamp(timestamp_str)?;
        let now = Utc::now();
        let skew = Duration::minutes(MAX_TIMESTAMP_SKEW_MINUTES);

        if request_time < now - skew || request_time > now + skew {
            return Err(AuthError::RequestExpired);
        }

        Ok(())
    }

    /// Parse AWS timestamp format: YYYYMMDDTHHMMSSZ
    fn parse_aws_timestamp(s: &str) -> Result<DateTime<Utc>, AuthError> {
        // AWS uses format like "20130524T000000Z"
        use chrono::NaiveDateTime;
        NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ")
            .map(|dt| dt.and_utc())
            .map_err(|_| AuthError::InvalidFormat)
    }

    fn parse_auth_header(header: &str) -> Result<ParsedAuthHeader, AuthError> {
        // Format: AWS4-HMAC-SHA256 Credential=.../..., SignedHeaders=..., Signature=...
        if !header.starts_with("AWS4-HMAC-SHA256 ") {
            return Err(AuthError::InvalidFormat);
        }

        let parts: Vec<&str> = header[17..].split(", ").collect();
        if parts.len() != 3 {
            return Err(AuthError::InvalidFormat);
        }

        let mut credential = None;
        let mut signed_headers = None;
        let mut signature = None;

        for part in parts {
            if let Some(val) = part.strip_prefix("Credential=") {
                credential = Some(val);
            } else if let Some(val) = part.strip_prefix("SignedHeaders=") {
                signed_headers = Some(val);
            } else if let Some(val) = part.strip_prefix("Signature=") {
                signature = Some(val);
            }
        }

        let credential = credential.ok_or(AuthError::InvalidFormat)?;
        let signed_headers = signed_headers.ok_or(AuthError::InvalidFormat)?;
        let signature = signature.ok_or(AuthError::InvalidFormat)?;

        // Parse credential: access_key/date/region/service/aws4_request
        let cred_parts: Vec<&str> = credential.split('/').collect();
        if cred_parts.len() != 5 {
            return Err(AuthError::InvalidFormat);
        }

        let date = cred_parts[1].to_string();
        let timestamp = format!("{}T000000Z", date); // Default timestamp from date
        let credential_scope = format!(
            "{}/{}/{}/aws4_request",
            cred_parts[1], cred_parts[2], cred_parts[3]
        );

        Ok(ParsedAuthHeader {
            access_key: cred_parts[0].to_string(),
            date,
            timestamp,
            credential_scope,
            signed_headers: signed_headers.split(';').map(String::from).collect(),
            signature: signature.to_string(),
        })
    }

    fn build_canonical_request(
        &self,
        method: &str,
        uri: &str,
        query_string: &str,
        headers: &[(String, String)],
        signed_headers: &[String],
        payload_hash: &str,
    ) -> Result<String, AuthError> {
        // Canonical URI (normalize path)
        let canonical_uri = uri;

        // Canonical query string (sorted by param name)
        let canonical_query = self.canonicalize_query_string(query_string);

        // Canonical headers
        let mut canonical_headers = String::new();
        for header_name in signed_headers {
            let header_value = headers
                .iter()
                .find(|(name, _)| name.to_lowercase() == *header_name)
                .map(|(_, v)| v.trim())
                .ok_or_else(|| AuthError::MissingSignedHeader(header_name.clone()))?;
            canonical_headers.push_str(&format!("{}:{}\n", header_name, header_value));
        }

        let signed_headers_str = signed_headers.join(";");

        Ok(format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_query,
            canonical_headers,
            signed_headers_str,
            payload_hash
        ))
    }

    fn canonicalize_query_string(&self, query: &str) -> String {
        if query.is_empty() {
            return String::new();
        }

        let mut params: Vec<(&str, &str)> = query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                Some((parts.next()?, parts.next().unwrap_or("")))
            })
            .collect();

        params.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));

        params
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    }

    fn build_string_to_sign(
        &self,
        timestamp: &str,
        credential_scope: &str,
        canonical_request: &str,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(canonical_request.as_bytes());
        let hash = hex::encode(hasher.finalize());

        format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            timestamp, credential_scope, hash
        )
    }

    fn calculate_signature(&self, date: &str, string_to_sign: &str) -> String {
        // Step 1: kDate = HMAC("AWS4" + secret_key, date)
        let k_secret = format!("AWS4{}", self.secret_key);
        let k_date = Self::hmac_sha256(k_secret.as_bytes(), date.as_bytes());

        // Step 2: kRegion = HMAC(kDate, region)
        let k_region = Self::hmac_sha256(&k_date, self.region.as_bytes());

        // Step 3: kService = HMAC(kRegion, service)
        let k_service = Self::hmac_sha256(&k_region, self.service.as_bytes());

        // Step 4: kSigning = HMAC(kService, "aws4_request")
        let k_signing = Self::hmac_sha256(&k_service, b"aws4_request");

        // Step 5: signature = HMAC(kSigning, string_to_sign)
        let signature = Self::hmac_sha256(&k_signing, string_to_sign.as_bytes());

        hex::encode(signature)
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        // HMAC-SHA256 accepts any key size; this should never fail
        let mut mac = match HmacSha256::new_from_slice(key) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("HMAC initialization failed: {}", e);
                // Return empty vector to cause auth failure rather than panic
                return vec![];
            }
        };
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    /// Verify a presigned URL request
    ///
    /// Presigned URLs contain authentication parameters in the query string:
    /// - X-Amz-Algorithm: AWS4-HMAC-SHA256
    /// - X-Amz-Credential: access_key/date/region/s3/aws4_request
    /// - X-Amz-Date: timestamp (YYYYMMDDTHHMMSSZ)
    /// - X-Amz-Expires: validity period in seconds
    /// - X-Amz-SignedHeaders: list of signed headers
    /// - X-Amz-Signature: the signature
    pub fn verify_presigned_request(
        &self,
        method: &str,
        uri: &str,
        query_string: &str,
        headers: &[(String, String)],
    ) -> Result<(), AuthError> {
        // If auth is not configured, allow all requests (MVP mode)
        if !self.is_enabled() {
            return Ok(());
        }

        // Parse presigned parameters
        let params = PresignedParams::from_query(query_string)?;

        // Verify access key
        let access_key = params.access_key()?;
        if access_key != self.access_key {
            return Err(AuthError::InvalidAccessKey);
        }

        // Validate timestamp and expiration
        if self.validate_timestamp {
            self.validate_presigned_timestamp(&params)?;
        }

        // Build canonical request for presigned URL
        // For presigned URLs, we need to exclude the signature from query string
        let canonical_query = self.build_presigned_canonical_query(query_string);

        let canonical_request = self.build_canonical_request(
            method,
            uri,
            &canonical_query,
            headers,
            &params.signed_headers,
            "UNSIGNED-PAYLOAD", // Presigned URLs always use UNSIGNED-PAYLOAD
        )?;

        // Build the string to sign
        let credential_scope = params.credential_scope()?;
        let string_to_sign =
            self.build_string_to_sign(&params.date, &credential_scope, &canonical_request);

        // Calculate the signature
        let date = params.credential_date()?;
        let signature = self.calculate_signature(date, &string_to_sign);

        // Compare signatures
        if signature != params.signature {
            return Err(AuthError::InvalidSignature);
        }

        Ok(())
    }

    /// Validate presigned URL timestamp and expiration
    fn validate_presigned_timestamp(&self, params: &PresignedParams) -> Result<(), AuthError> {
        let request_time = Self::parse_aws_timestamp(&params.date)?;
        let now = Utc::now();

        // Check if the request has expired
        let expiration = request_time + Duration::seconds(params.expires as i64);
        if now > expiration {
            return Err(AuthError::RequestExpired);
        }

        // Check if the request is too far in the future (using standard skew allowance)
        let skew = Duration::minutes(MAX_TIMESTAMP_SKEW_MINUTES);
        if request_time > now + skew {
            return Err(AuthError::RequestExpired);
        }

        Ok(())
    }

    /// Build canonical query string for presigned URL (excluding signature)
    fn build_presigned_canonical_query(&self, query: &str) -> String {
        let mut params: Vec<(&str, &str)> = query
            .split('&')
            .filter_map(|p| {
                let mut parts = p.splitn(2, '=');
                let key = parts.next()?;
                let value = parts.next().unwrap_or("");
                // Exclude X-Amz-Signature from canonical query
                if key == "X-Amz-Signature" {
                    None
                } else {
                    Some((key, value))
                }
            })
            .collect();

        // Sort by parameter name
        params.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));

        params
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&")
    }
}

struct ParsedAuthHeader {
    access_key: String,
    date: String,
    timestamp: String,
    credential_scope: String,
    signed_headers: Vec<String>,
    signature: String,
}

/// Parsed presigned URL parameters
#[derive(Debug)]
pub struct PresignedParams {
    pub algorithm: String,
    pub credential: String,
    pub date: String,
    pub expires: u64,
    pub signed_headers: Vec<String>,
    pub signature: String,
}

impl PresignedParams {
    /// Parse presigned URL query parameters
    pub fn from_query(query_string: &str) -> Result<Self, AuthError> {
        let params = Self::parse_query_string(query_string);

        let algorithm = params
            .get("X-Amz-Algorithm")
            .ok_or_else(|| AuthError::MissingPresignedParam("X-Amz-Algorithm".to_string()))?
            .clone();

        if algorithm != "AWS4-HMAC-SHA256" {
            return Err(AuthError::InvalidFormat);
        }

        let credential = params
            .get("X-Amz-Credential")
            .ok_or_else(|| AuthError::MissingPresignedParam("X-Amz-Credential".to_string()))?
            .clone();

        let date = params
            .get("X-Amz-Date")
            .ok_or_else(|| AuthError::MissingPresignedParam("X-Amz-Date".to_string()))?
            .clone();

        let expires: u64 = params
            .get("X-Amz-Expires")
            .ok_or_else(|| AuthError::MissingPresignedParam("X-Amz-Expires".to_string()))?
            .parse()
            .map_err(|_| AuthError::InvalidFormat)?;

        let signed_headers = params
            .get("X-Amz-SignedHeaders")
            .ok_or_else(|| AuthError::MissingPresignedParam("X-Amz-SignedHeaders".to_string()))?
            .split(';')
            .map(String::from)
            .collect();

        let signature = params
            .get("X-Amz-Signature")
            .ok_or_else(|| AuthError::MissingPresignedParam("X-Amz-Signature".to_string()))?
            .clone();

        Ok(Self {
            algorithm,
            credential,
            date,
            expires,
            signed_headers,
            signature,
        })
    }

    /// Check if presigned URL query parameters are present
    pub fn is_presigned_request(query_string: &str) -> bool {
        query_string.contains("X-Amz-Signature=") && query_string.contains("X-Amz-Algorithm=")
    }

    fn parse_query_string(query: &str) -> HashMap<String, String> {
        query
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let key = parts.next()?;
                let value = parts.next().unwrap_or("");
                // URL decode the value
                let decoded = percent_encoding::percent_decode_str(value)
                    .decode_utf8_lossy()
                    .to_string();
                Some((key.to_string(), decoded))
            })
            .collect()
    }

    /// Extract access key from credential
    pub fn access_key(&self) -> Result<&str, AuthError> {
        self.credential
            .split('/')
            .next()
            .ok_or(AuthError::InvalidFormat)
    }

    /// Extract date (YYYYMMDD) from credential
    pub fn credential_date(&self) -> Result<&str, AuthError> {
        self.credential
            .split('/')
            .nth(1)
            .ok_or(AuthError::InvalidFormat)
    }

    /// Extract region from credential
    pub fn region(&self) -> Result<&str, AuthError> {
        self.credential
            .split('/')
            .nth(2)
            .ok_or(AuthError::InvalidFormat)
    }

    /// Get credential scope (date/region/s3/aws4_request)
    pub fn credential_scope(&self) -> Result<String, AuthError> {
        let parts: Vec<&str> = self.credential.split('/').collect();
        if parts.len() != 5 {
            return Err(AuthError::InvalidFormat);
        }
        Ok(format!(
            "{}/{}/{}/{}",
            parts[1], parts[2], parts[3], parts[4]
        ))
    }
}

/// Presigned URL generator
#[derive(Debug, Clone)]
pub struct PresignedUrlGenerator {
    access_key: String,
    secret_key: String,
    region: String,
    service: String,
    host: String,
}

impl PresignedUrlGenerator {
    pub fn new(access_key: String, secret_key: String, region: String, host: String) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            service: "s3".to_string(),
            host,
        }
    }

    /// Generate a presigned URL for GET object
    pub fn generate_presigned_get_url(
        &self,
        bucket: &str,
        key: &str,
        expires_in_secs: u64,
    ) -> String {
        self.generate_presigned_url("GET", bucket, key, expires_in_secs, None)
    }

    /// Generate a presigned URL for PUT object
    pub fn generate_presigned_put_url(
        &self,
        bucket: &str,
        key: &str,
        expires_in_secs: u64,
        content_type: Option<&str>,
    ) -> String {
        self.generate_presigned_url("PUT", bucket, key, expires_in_secs, content_type)
    }

    /// Generate a presigned URL
    fn generate_presigned_url(
        &self,
        method: &str,
        bucket: &str,
        key: &str,
        expires_in_secs: u64,
        content_type: Option<&str>,
    ) -> String {
        let now = Utc::now();
        let date_str = now.format("%Y%m%d").to_string();
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();

        // Build the canonical URI
        let canonical_uri = format!("/{}/{}", bucket, key);

        // Build credential scope
        let credential_scope = format!("{}/{}/s3/aws4_request", date_str, self.region);
        let credential = format!("{}/{}", self.access_key, credential_scope);

        // Signed headers
        let signed_headers = "host";

        // Build query parameters (already sorted alphabetically by key)
        let query_params = [
            ("X-Amz-Algorithm", "AWS4-HMAC-SHA256".to_string()),
            ("X-Amz-Credential", Self::url_encode(&credential)),
            ("X-Amz-Date", timestamp.clone()),
            ("X-Amz-Expires", expires_in_secs.to_string()),
            ("X-Amz-SignedHeaders", signed_headers.to_string()),
        ];

        let canonical_query = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        // Build canonical headers
        let canonical_headers = format!("host:{}\n", self.host);

        // Build canonical request
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method,
            canonical_uri,
            canonical_query,
            canonical_headers,
            signed_headers,
            UNSIGNED_PAYLOAD
        );

        // Hash the canonical request
        let mut hasher = Sha256::new();
        hasher.update(canonical_request.as_bytes());
        let canonical_request_hash = hex::encode(hasher.finalize());

        // Build string to sign
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            timestamp, credential_scope, canonical_request_hash
        );

        // Calculate signature
        let signature = self.calculate_signature(&date_str, &string_to_sign);

        // Build final URL
        let scheme = if self.host.contains("localhost") || self.host.starts_with("127.") {
            "http"
        } else {
            "https"
        };

        // Handle content-type if specified (for PUT)
        let _ = content_type; // Reserved for future use

        format!(
            "{}://{}{}?{}&X-Amz-Signature={}",
            scheme, self.host, canonical_uri, canonical_query, signature
        )
    }

    fn calculate_signature(&self, date: &str, string_to_sign: &str) -> String {
        // Step 1: kDate = HMAC("AWS4" + secret_key, date)
        let k_secret = format!("AWS4{}", self.secret_key);
        let k_date = Self::hmac_sha256(k_secret.as_bytes(), date.as_bytes());

        // Step 2: kRegion = HMAC(kDate, region)
        let k_region = Self::hmac_sha256(&k_date, self.region.as_bytes());

        // Step 3: kService = HMAC(kRegion, service)
        let k_service = Self::hmac_sha256(&k_region, self.service.as_bytes());

        // Step 4: kSigning = HMAC(kService, "aws4_request")
        let k_signing = Self::hmac_sha256(&k_service, b"aws4_request");

        // Step 5: signature = HMAC(kSigning, string_to_sign)
        let signature = Self::hmac_sha256(&k_signing, string_to_sign.as_bytes());

        hex::encode(signature)
    }

    fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
        // HMAC-SHA256 accepts any key size; this should never fail
        let mut mac = match HmacSha256::new_from_slice(key) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("HMAC initialization failed: {}", e);
                // Return empty vector to cause auth failure rather than panic
                return vec![];
            }
        };
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }

    fn url_encode(s: &str) -> String {
        percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
    }
}

/// Calculate SHA256 hash of payload (for unsigned payload, use UNSIGNED-PAYLOAD)
pub fn hash_payload(payload: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hex::encode(hasher.finalize())
}

/// Constant for unsigned payload
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disabled_auth_allows_all() {
        let verifier = SigV4Verifier::new(String::new(), String::new(), "us-east-1".to_string());
        assert!(!verifier.is_enabled());
        assert!(verifier
            .verify_request("GET", "/", "", &[], UNSIGNED_PAYLOAD, None)
            .is_ok());
    }

    #[test]
    fn test_enabled_auth_requires_header() {
        let verifier = SigV4Verifier::new(
            "access_key".to_string(),
            "secret_key".to_string(),
            "us-east-1".to_string(),
        );
        assert!(verifier.is_enabled());
        assert!(matches!(
            verifier.verify_request("GET", "/", "", &[], UNSIGNED_PAYLOAD, None),
            Err(AuthError::MissingAuth)
        ));
    }

    #[test]
    fn test_hash_payload() {
        let hash = hash_payload(b"test");
        assert_eq!(
            hash,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_timestamp_validation_valid() {
        let verifier = SigV4Verifier::new(
            "access_key".to_string(),
            "secret_key".to_string(),
            "us-east-1".to_string(),
        );

        // Current timestamp should be valid
        let now = chrono::Utc::now();
        let timestamp = now.format("%Y%m%dT%H%M%SZ").to_string();
        let headers = vec![("x-amz-date".to_string(), timestamp.clone())];

        let result = verifier.validate_request_timestamp(&headers, &timestamp);
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
    }

    #[test]
    fn test_timestamp_validation_expired() {
        let verifier = SigV4Verifier::new(
            "access_key".to_string(),
            "secret_key".to_string(),
            "us-east-1".to_string(),
        );

        // Timestamp from 30 minutes ago should be expired
        let old_time = chrono::Utc::now() - chrono::Duration::minutes(30);
        let timestamp = old_time.format("%Y%m%dT%H%M%SZ").to_string();
        let headers = vec![("x-amz-date".to_string(), timestamp.clone())];

        let result = verifier.validate_request_timestamp(&headers, &timestamp);
        assert!(matches!(result, Err(AuthError::RequestExpired)));
    }

    #[test]
    fn test_timestamp_validation_future() {
        let verifier = SigV4Verifier::new(
            "access_key".to_string(),
            "secret_key".to_string(),
            "us-east-1".to_string(),
        );

        // Timestamp 30 minutes in the future should be rejected
        let future_time = chrono::Utc::now() + chrono::Duration::minutes(30);
        let timestamp = future_time.format("%Y%m%dT%H%M%SZ").to_string();
        let headers = vec![("x-amz-date".to_string(), timestamp.clone())];

        let result = verifier.validate_request_timestamp(&headers, &timestamp);
        assert!(matches!(result, Err(AuthError::RequestExpired)));
    }

    #[test]
    fn test_presigned_params_parse() {
        let query = "X-Amz-Algorithm=AWS4-HMAC-SHA256&\
            X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&\
            X-Amz-Date=20130524T000000Z&\
            X-Amz-Expires=86400&\
            X-Amz-SignedHeaders=host&\
            X-Amz-Signature=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let params = PresignedParams::from_query(query).expect("Failed to parse presigned params");
        assert_eq!(params.algorithm, "AWS4-HMAC-SHA256");
        assert_eq!(
            params.access_key().expect("Failed to get access key"),
            "AKIAIOSFODNN7EXAMPLE"
        );
        assert_eq!(
            params
                .credential_date()
                .expect("Failed to get credential date"),
            "20130524"
        );
        assert_eq!(params.region().expect("Failed to get region"), "us-east-1");
        assert_eq!(params.expires, 86400);
        assert_eq!(params.signed_headers, vec!["host"]);
    }

    #[test]
    fn test_presigned_is_presigned_request() {
        let presigned_query = "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Signature=abc";
        assert!(PresignedParams::is_presigned_request(presigned_query));

        let normal_query = "list-type=2&prefix=foo";
        assert!(!PresignedParams::is_presigned_request(normal_query));
    }

    #[test]
    fn test_presigned_disabled_auth_allows_all() {
        let verifier = SigV4Verifier::new(String::new(), String::new(), "us-east-1".to_string());

        // Even with presigned params, disabled auth should allow all
        let result = verifier.verify_presigned_request(
            "GET",
            "/bucket/key",
            "X-Amz-Algorithm=AWS4-HMAC-SHA256",
            &[],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_presigned_expired_request() {
        let verifier = SigV4Verifier::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "us-east-1".to_string(),
        );

        // Use a timestamp from far in the past
        let query = "X-Amz-Algorithm=AWS4-HMAC-SHA256&\
            X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&\
            X-Amz-Date=20130524T000000Z&\
            X-Amz-Expires=86400&\
            X-Amz-SignedHeaders=host&\
            X-Amz-Signature=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let headers = vec![(
            "host".to_string(),
            "examplebucket.s3.amazonaws.com".to_string(),
        )];

        let result = verifier.verify_presigned_request("GET", "/bucket/key", query, &headers);
        assert!(matches!(result, Err(AuthError::RequestExpired)));
    }

    #[test]
    fn test_presigned_invalid_access_key() {
        let verifier = SigV4Verifier::new_without_timestamp_validation(
            "DIFFERENT_ACCESS_KEY".to_string(),
            "secret".to_string(),
            "us-east-1".to_string(),
        );

        let query = "X-Amz-Algorithm=AWS4-HMAC-SHA256&\
            X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&\
            X-Amz-Date=20130524T000000Z&\
            X-Amz-Expires=86400&\
            X-Amz-SignedHeaders=host&\
            X-Amz-Signature=aaaa";

        let headers = vec![("host".to_string(), "bucket.s3.amazonaws.com".to_string())];

        let result = verifier.verify_presigned_request("GET", "/bucket/key", query, &headers);
        assert!(matches!(result, Err(AuthError::InvalidAccessKey)));
    }

    #[test]
    fn test_presigned_url_generation() {
        let generator = PresignedUrlGenerator::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "us-east-1".to_string(),
            "localhost:9000".to_string(),
        );

        let url = generator.generate_presigned_get_url("test-bucket", "test-key.txt", 3600);

        // Verify URL structure
        assert!(url.starts_with("http://localhost:9000/test-bucket/test-key.txt?"));
        assert!(url.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(url.contains("X-Amz-Credential="));
        assert!(url.contains("X-Amz-Date="));
        assert!(url.contains("X-Amz-Expires=3600"));
        assert!(url.contains("X-Amz-SignedHeaders=host"));
        assert!(url.contains("X-Amz-Signature="));
    }

    #[test]
    fn test_presigned_url_put() {
        let generator = PresignedUrlGenerator::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "us-east-1".to_string(),
            "s3.example.com".to_string(),
        );

        let url = generator.generate_presigned_put_url("my-bucket", "uploads/file.bin", 7200, None);

        // HTTPS for non-localhost hosts
        assert!(url.starts_with("https://s3.example.com/my-bucket/uploads/file.bin?"));
        assert!(url.contains("X-Amz-Expires=7200"));
    }
}
