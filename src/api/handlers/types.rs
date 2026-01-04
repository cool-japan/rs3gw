//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use serde::{Deserialize, Serialize};

/// Query parameters for ListObjectsV1 (original S3 API)
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ListObjectsV1Query {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub delimiter: Option<String>,
    #[serde(default)]
    pub max_keys: Option<usize>,
    #[serde(default)]
    pub marker: Option<String>,
    #[serde(default)]
    pub encoding_type: Option<String>,
}
/// Query parameters for ListMultipartUploads
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ListMultipartUploadsQuery {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub delimiter: Option<String>,
    #[serde(default)]
    pub max_uploads: Option<u32>,
    #[serde(default, rename = "key-marker")]
    pub key_marker: Option<String>,
    #[serde(default, rename = "upload-id-marker")]
    pub upload_id_marker: Option<String>,
}
/// Presigned URL response
#[derive(Debug, Serialize)]
pub struct PresignedUrlResponse {
    pub url: String,
    pub expires_in: u64,
    pub method: String,
}
/// Health check response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// Service status
    #[schema(example = "healthy")]
    pub status: String,
    /// Service version
    #[schema(example = "0.1.0")]
    pub version: String,
    /// Compression configuration
    #[schema(example = "zstd:3")]
    pub compression: String,
}
/// Query parameters for ListObjectsV2
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ListObjectsV2Query {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub delimiter: Option<String>,
    #[serde(default)]
    pub max_keys: Option<usize>,
    #[serde(default)]
    pub continuation_token: Option<String>,
    #[serde(default)]
    pub start_after: Option<String>,
    #[serde(default)]
    pub encoding_type: Option<String>,
    #[serde(rename = "list-type", default)]
    pub list_type: Option<String>,
}
/// Query parameters for ListObjectVersions
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ListObjectVersionsQuery {
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub delimiter: Option<String>,
    #[serde(default)]
    pub max_keys: Option<usize>,
    #[serde(default, rename = "key-marker")]
    pub key_marker: Option<String>,
}
/// Query parameters for presigned URL generation
#[derive(Debug, Deserialize, Default)]
pub struct PresignQuery {
    /// HTTP method (GET or PUT)
    pub method: Option<String>,
    /// Expiration time in seconds (default: 3600, max: 604800)
    pub expires: Option<u64>,
}
/// Result of checking conditional headers (If-Match, If-None-Match, etc.)
pub enum ConditionalResult {
    /// Proceed with the request
    Proceed,
    /// Return 304 Not Modified
    NotModified(String),
    /// Return 412 Precondition Failed
    PreconditionFailed(String),
}
