//! Presigned POST Policy Generation
//!
//! Implements S3-compatible presigned POST for browser-based uploads.
//! This allows creating HTML forms that can upload directly to S3.

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const BASE64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

/// Presigned POST policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostPolicy {
    /// Expiration time (ISO 8601)
    pub expiration: String,
    /// Policy conditions
    pub conditions: Vec<PostCondition>,
}

/// A condition in the POST policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PostCondition {
    /// Exact match condition: {"bucket": "mybucket"}
    ExactMatch(serde_json::Map<String, serde_json::Value>),
    /// Starts-with condition: ["starts-with", "$key", "user/"]
    StartsWith(String, String, String),
    /// Content-length-range condition: ["content-length-range", min, max]
    ContentLengthRange(String, u64, u64),
    /// Equality condition as array: ["eq", "$key", "value"]
    Equality(String, String, String),
}

impl PostCondition {
    /// Create an exact match condition
    pub fn exact(field: &str, value: &str) -> Self {
        let mut map = serde_json::Map::new();
        map.insert(
            field.to_string(),
            serde_json::Value::String(value.to_string()),
        );
        PostCondition::ExactMatch(map)
    }

    /// Create a starts-with condition
    pub fn starts_with(field: &str, prefix: &str) -> Self {
        PostCondition::StartsWith(
            "starts-with".to_string(),
            format!("${}", field),
            prefix.to_string(),
        )
    }

    /// Create a content-length-range condition
    pub fn content_length_range(min: u64, max: u64) -> Self {
        PostCondition::ContentLengthRange("content-length-range".to_string(), min, max)
    }
}

/// Result of generating a presigned POST
#[derive(Debug, Clone, Serialize)]
pub struct PresignedPost {
    /// URL to POST to
    pub url: String,
    /// Form fields to include
    pub fields: PresignedPostFields,
}

/// Form fields for presigned POST
#[derive(Debug, Clone, Serialize)]
pub struct PresignedPostFields {
    /// The bucket name
    pub bucket: String,
    /// Object key (may include ${filename} placeholder)
    pub key: String,
    /// Base64-encoded policy
    pub policy: String,
    /// AWS access key ID
    #[serde(rename = "X-Amz-Algorithm")]
    pub algorithm: String,
    /// Credential string
    #[serde(rename = "X-Amz-Credential")]
    pub credential: String,
    /// Date in ISO 8601 format
    #[serde(rename = "X-Amz-Date")]
    pub date: String,
    /// Signature
    #[serde(rename = "X-Amz-Signature")]
    pub signature: String,
    /// Optional ACL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acl: Option<String>,
    /// Optional content type
    #[serde(rename = "Content-Type", skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// Optional success redirect
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_action_redirect: Option<String>,
    /// Optional success status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success_action_status: Option<String>,
}

/// Builder for presigned POST
pub struct PresignedPostBuilder {
    access_key: String,
    secret_key: String,
    region: String,
    host: String,
    bucket: String,
    key: String,
    expiration: DateTime<Utc>,
    conditions: Vec<PostCondition>,
    acl: Option<String>,
    content_type: Option<String>,
    content_length_range: Option<(u64, u64)>,
    success_redirect: Option<String>,
    success_status: Option<String>,
}

impl PresignedPostBuilder {
    /// Create a new builder
    pub fn new(
        access_key: String,
        secret_key: String,
        region: String,
        host: String,
        bucket: String,
        key: String,
    ) -> Self {
        Self {
            access_key,
            secret_key,
            region,
            host,
            bucket,
            key,
            expiration: Utc::now() + Duration::hours(1),
            conditions: Vec::new(),
            acl: None,
            content_type: None,
            content_length_range: None,
            success_redirect: None,
            success_status: None,
        }
    }

    /// Set expiration time
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.expiration = Utc::now() + duration;
        self
    }

    /// Set expiration as DateTime
    pub fn expires_at(mut self, expiration: DateTime<Utc>) -> Self {
        self.expiration = expiration;
        self
    }

    /// Add a custom condition
    pub fn condition(mut self, condition: PostCondition) -> Self {
        self.conditions.push(condition);
        self
    }

    /// Set ACL
    pub fn acl(mut self, acl: &str) -> Self {
        self.acl = Some(acl.to_string());
        self
    }

    /// Set content type
    pub fn content_type(mut self, content_type: &str) -> Self {
        self.content_type = Some(content_type.to_string());
        self
    }

    /// Set content length range
    pub fn content_length_range(mut self, min: u64, max: u64) -> Self {
        self.content_length_range = Some((min, max));
        self
    }

    /// Set success redirect URL
    pub fn success_redirect(mut self, url: &str) -> Self {
        self.success_redirect = Some(url.to_string());
        self
    }

    /// Set success status code
    pub fn success_status(mut self, status: u16) -> Self {
        self.success_status = Some(status.to_string());
        self
    }

    /// Build the presigned POST
    pub fn build(self) -> PresignedPost {
        let now = Utc::now();
        let date_stamp = now.format("%Y%m%d").to_string();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let credential = format!(
            "{}/{}/{}/s3/aws4_request",
            self.access_key, date_stamp, self.region
        );

        // Build conditions
        let mut conditions: Vec<serde_json::Value> = Vec::new();

        // Required bucket condition
        conditions.push(serde_json::json!({"bucket": self.bucket}));

        // Key condition - use starts-with if key contains ${filename}
        if self.key.contains("${filename}") {
            let prefix = self.key.replace("${filename}", "");
            conditions.push(serde_json::json!(["starts-with", "$key", prefix]));
        } else {
            conditions.push(serde_json::json!({"key": self.key}));
        }

        // Required AWS fields
        conditions.push(serde_json::json!({"X-Amz-Algorithm": "AWS4-HMAC-SHA256"}));
        conditions.push(serde_json::json!({"X-Amz-Credential": credential}));
        conditions.push(serde_json::json!({"X-Amz-Date": amz_date}));

        // Optional ACL
        if let Some(ref acl) = self.acl {
            conditions.push(serde_json::json!({"acl": acl}));
        }

        // Optional content type
        if let Some(ref ct) = self.content_type {
            conditions.push(serde_json::json!({"Content-Type": ct}));
        }

        // Optional content length range
        if let Some((min, max)) = self.content_length_range {
            conditions.push(serde_json::json!(["content-length-range", min, max]));
        }

        // Optional success redirect
        if let Some(ref redirect) = self.success_redirect {
            conditions.push(serde_json::json!({"success_action_redirect": redirect}));
        }

        // Optional success status
        if let Some(ref status) = self.success_status {
            conditions.push(serde_json::json!({"success_action_status": status}));
        }

        // Add custom conditions
        for condition in &self.conditions {
            match condition {
                PostCondition::ExactMatch(map) => {
                    conditions.push(serde_json::Value::Object(map.clone()));
                }
                PostCondition::StartsWith(op, field, prefix) => {
                    conditions.push(serde_json::json!([op, field, prefix]));
                }
                PostCondition::ContentLengthRange(op, min, max) => {
                    conditions.push(serde_json::json!([op, min, max]));
                }
                PostCondition::Equality(op, field, value) => {
                    conditions.push(serde_json::json!([op, field, value]));
                }
            }
        }

        // Create policy
        let policy = serde_json::json!({
            "expiration": self.expiration.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            "conditions": conditions
        });

        let policy_json = serde_json::to_string(&policy).unwrap_or_default();
        let policy_base64 = BASE64.encode(policy_json.as_bytes());

        // Calculate signature
        let signature = self.calculate_signature(&policy_base64, &date_stamp);

        // Build URL
        let url = format!("http://{}/{}", self.host, self.bucket);

        PresignedPost {
            url,
            fields: PresignedPostFields {
                bucket: self.bucket,
                key: self.key,
                policy: policy_base64,
                algorithm: "AWS4-HMAC-SHA256".to_string(),
                credential,
                date: amz_date,
                signature,
                acl: self.acl,
                content_type: self.content_type,
                success_action_redirect: self.success_redirect,
                success_action_status: self.success_status,
            },
        }
    }

    /// Calculate the signature for the policy
    fn calculate_signature(&self, policy_base64: &str, date_stamp: &str) -> String {
        // Step 1: Create signing key
        let k_secret = format!("AWS4{}", self.secret_key);
        let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");

        // Step 2: Sign the policy
        let signature = hmac_sha256(&k_signing, policy_base64.as_bytes());

        // Step 3: Encode as hex
        hex::encode(signature)
    }
}

/// Calculate HMAC-SHA256
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_presigned_post_builder() {
        let post = PresignedPostBuilder::new(
            "AKIAIOSFODNN7EXAMPLE".to_string(),
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            "us-east-1".to_string(),
            "localhost:9000".to_string(),
            "mybucket".to_string(),
            "uploads/${filename}".to_string(),
        )
        .expires_in(Duration::hours(1))
        .acl("public-read")
        .content_length_range(0, 10 * 1024 * 1024)
        .build();

        assert_eq!(post.fields.bucket, "mybucket");
        assert_eq!(post.fields.algorithm, "AWS4-HMAC-SHA256");
        assert!(!post.fields.policy.is_empty());
        assert!(!post.fields.signature.is_empty());
        assert!(post.url.contains("localhost:9000"));
    }

    #[test]
    fn test_presigned_post_with_fixed_key() {
        let post = PresignedPostBuilder::new(
            "testkey".to_string(),
            "testsecret".to_string(),
            "us-east-1".to_string(),
            "localhost:9000".to_string(),
            "testbucket".to_string(),
            "fixed-key.txt".to_string(),
        )
        .build();

        assert_eq!(post.fields.key, "fixed-key.txt");
        assert!(!post.fields.signature.is_empty());
    }

    #[test]
    fn test_post_condition_exact() {
        let condition = PostCondition::exact("bucket", "mybucket");
        match condition {
            PostCondition::ExactMatch(map) => {
                assert_eq!(
                    map.get("bucket"),
                    Some(&serde_json::Value::String("mybucket".to_string()))
                );
            }
            _ => panic!("Expected ExactMatch"),
        }
    }

    #[test]
    fn test_post_condition_starts_with() {
        let condition = PostCondition::starts_with("key", "user/");
        match condition {
            PostCondition::StartsWith(op, field, prefix) => {
                assert_eq!(op, "starts-with");
                assert_eq!(field, "$key");
                assert_eq!(prefix, "user/");
            }
            _ => panic!("Expected StartsWith"),
        }
    }

    #[test]
    fn test_presigned_post_policy_json() {
        let post = PresignedPostBuilder::new(
            "testkey".to_string(),
            "testsecret".to_string(),
            "us-east-1".to_string(),
            "localhost:9000".to_string(),
            "testbucket".to_string(),
            "test.txt".to_string(),
        )
        .content_type("text/plain")
        .success_status(201)
        .build();

        // Decode policy and verify it's valid JSON
        let policy_bytes = BASE64
            .decode(&post.fields.policy)
            .expect("Failed to decode base64 policy");
        let policy: serde_json::Value =
            serde_json::from_slice(&policy_bytes).expect("Failed to parse policy JSON");

        assert!(policy.get("expiration").is_some());
        assert!(policy.get("conditions").is_some());
    }
}
