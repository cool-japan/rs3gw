//! OpenAPI/Swagger schema definitions for rs3gw
//!
//! This module provides OpenAPI 3.0 schema generation for all S3-compatible API endpoints,
//! enabling automatic client SDK generation and interactive API documentation.

use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

/// OpenAPI schema for rs3gw S3-compatible API
#[derive(OpenApi)]
#[openapi(
    info(
        title = "rs3gw - High-Performance S3-Compatible Object Storage Gateway",
        version = "0.1.0",
        description = "A high-performance, S3-compatible object storage gateway with advanced features including S3 Select, GraphQL API, gRPC, WebSocket streaming, and ML-based caching.",
        contact(
            name = "rs3gw contributors",
            url = "https://github.com/cool-japan/rs3gw"
        ),
        license(
            name = "MIT OR Apache-2.0",
            url = "https://github.com/cool-japan/rs3gw/blob/main/LICENSE"
        )
    ),
    servers(
        (url = "http://localhost:9000", description = "Local development server"),
        (url = "https://s3.example.com", description = "Production server")
    ),
    paths(
        crate::api::handlers::health_check,
        crate::api::handlers::list_buckets,
        crate::api::handlers::head_bucket,
        crate::api::handlers::create_bucket,
        crate::api::handlers::delete_bucket,
        crate::api::handlers::list_objects_v2,
        crate::api::handlers::head_object,
        crate::api::handlers::get_object,
        crate::api::handlers::put_object,
        crate::api::handlers::delete_object,
        crate::api::handlers::copy_object,
        crate::api::handlers::get_object_tagging,
        crate::api::handlers::put_object_tagging,
    ),
    components(schemas(
        BucketInfo,
        ObjectInfo,
        ListBucketsResponse,
        OwnerInfo,
        ListObjectsResponse,
        ErrorResponse,
        SelectRequest,
        InputSerialization,
        CsvInput,
        JsonInput,
        ParquetInput,
        OutputSerialization,
        CsvOutput,
        JsonOutput,
        RequestProgress,
        HealthResponse,
        StorageStats,
        StorageClassStats,
    )),
    modifiers(&SecurityAddon),
    tags(
        (name = "Buckets", description = "Bucket management operations"),
        (name = "Objects", description = "Object storage operations"),
        (name = "Multipart", description = "Multipart upload operations"),
        (name = "S3 Select", description = "SQL queries on stored objects"),
        (name = "GraphQL", description = "GraphQL query interface"),
        (name = "Admin", description = "Administrative and monitoring endpoints")
    )
)]
pub struct ApiDoc;

/// Security scheme modifier for AWS Signature V4 authentication
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            // AWS Signature V4 authentication
            components.add_security_scheme(
                "aws_sigv4",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Basic)
                        .description(Some(
                            "AWS Signature Version 4 authentication. \
                            Provide Access Key ID and Secret Access Key."
                                .to_string(),
                        ))
                        .build(),
                ),
            );

            // API Key authentication (access key in header)
            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Amz-Access-Key-Id"))),
            );
        }
    }
}

/// Bucket information response
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct BucketInfo {
    /// Bucket name
    #[schema(example = "my-bucket")]
    pub name: String,

    /// Creation timestamp
    #[schema(example = "2024-01-01T00:00:00Z")]
    pub creation_date: String,
}

/// Object metadata response
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct ObjectInfo {
    /// Object key
    #[schema(example = "documents/readme.txt")]
    pub key: String,

    /// Last modified timestamp
    #[schema(example = "2024-01-01T12:00:00Z")]
    pub last_modified: String,

    /// ETag (entity tag)
    #[schema(example = "\"d41d8cd98f00b204e9800998ecf8427e\"")]
    pub etag: String,

    /// Object size in bytes
    #[schema(example = 1024)]
    pub size: i64,

    /// Storage class
    #[schema(example = "STANDARD")]
    pub storage_class: String,
}

/// List buckets response
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct ListBucketsResponse {
    /// Owner information
    pub owner: OwnerInfo,

    /// List of buckets
    pub buckets: Vec<BucketInfo>,
}

/// Owner information
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct OwnerInfo {
    /// Owner display name
    #[schema(example = "admin")]
    pub display_name: String,

    /// Owner ID
    #[schema(example = "admin")]
    pub id: String,
}

/// List objects response
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct ListObjectsResponse {
    /// Bucket name
    #[schema(example = "my-bucket")]
    pub name: String,

    /// Prefix filter applied
    #[schema(example = "documents/")]
    pub prefix: Option<String>,

    /// Maximum number of keys
    #[schema(example = 1000)]
    pub max_keys: i32,

    /// Indicates if results are truncated
    pub is_truncated: bool,

    /// List of objects
    pub contents: Vec<ObjectInfo>,

    /// Continuation token for pagination
    pub next_continuation_token: Option<String>,
}

/// Error response
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct ErrorResponse {
    /// Error code
    #[schema(example = "NoSuchBucket")]
    pub code: String,

    /// Error message
    #[schema(example = "The specified bucket does not exist")]
    pub message: String,

    /// Resource that caused the error
    #[schema(example = "/my-bucket")]
    pub resource: Option<String>,

    /// Request ID for tracking
    #[schema(example = "4442587FB7D0A2F9")]
    pub request_id: Option<String>,
}

/// S3 Select request
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct SelectRequest {
    /// SQL query expression
    #[schema(example = "SELECT * FROM S3Object WHERE age > 25 LIMIT 10")]
    pub expression: String,

    /// Expression type (always "SQL" for S3 Select)
    #[schema(example = "SQL")]
    pub expression_type: String,

    /// Input serialization format
    pub input_serialization: InputSerialization,

    /// Output serialization format
    pub output_serialization: OutputSerialization,

    /// Request progress (optional)
    pub request_progress: Option<RequestProgress>,
}

/// Input serialization configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct InputSerialization {
    /// CSV input configuration
    pub csv: Option<CsvInput>,

    /// JSON input configuration
    pub json: Option<JsonInput>,

    /// Parquet input (no configuration needed)
    pub parquet: Option<ParquetInput>,

    /// Compression type
    #[schema(example = "NONE")]
    pub compression_type: Option<String>,
}

/// CSV input configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct CsvInput {
    /// File header info (USE, IGNORE, NONE)
    #[schema(example = "USE")]
    pub file_header_info: Option<String>,

    /// Field delimiter
    #[schema(example = ",")]
    pub field_delimiter: Option<String>,

    /// Record delimiter
    #[schema(example = "\n")]
    pub record_delimiter: Option<String>,

    /// Quote character
    #[schema(example = "\"")]
    pub quote_character: Option<String>,

    /// Quote escape character
    #[schema(example = "\"")]
    pub quote_escape_character: Option<String>,

    /// Comments character
    pub comments: Option<String>,

    /// Allow quoted record delimiter
    pub allow_quoted_record_delimiter: Option<bool>,
}

/// JSON input configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct JsonInput {
    /// Type (DOCUMENT or LINES)
    #[schema(example = "LINES")]
    pub r#type: Option<String>,
}

/// Parquet input (marker)
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct ParquetInput {}

/// Output serialization configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct OutputSerialization {
    /// CSV output configuration
    pub csv: Option<CsvOutput>,

    /// JSON output configuration
    pub json: Option<JsonOutput>,
}

/// CSV output configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct CsvOutput {
    /// Field delimiter
    #[schema(example = ",")]
    pub field_delimiter: Option<String>,

    /// Record delimiter
    #[schema(example = "\n")]
    pub record_delimiter: Option<String>,

    /// Quote character
    #[schema(example = "\"")]
    pub quote_character: Option<String>,

    /// Quote escape character
    #[schema(example = "\"")]
    pub quote_escape_character: Option<String>,

    /// Quote fields (ALWAYS, ASNEEDED)
    #[schema(example = "ASNEEDED")]
    pub quote_fields: Option<String>,
}

/// JSON output configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct JsonOutput {
    /// Record delimiter
    #[schema(example = "\n")]
    pub record_delimiter: Option<String>,
}

/// Request progress configuration
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct RequestProgress {
    /// Enable progress messages
    pub enabled: bool,
}

/// Health check response
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct HealthResponse {
    /// Service status
    #[schema(example = "healthy")]
    pub status: String,

    /// Timestamp
    #[schema(example = "2024-01-01T12:00:00Z")]
    pub timestamp: String,

    /// Uptime in seconds
    #[schema(example = 3600)]
    pub uptime_seconds: u64,
}

/// Storage statistics
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct StorageStats {
    /// Total number of buckets
    #[schema(example = 5)]
    pub bucket_count: usize,

    /// Total number of objects
    #[schema(example = 1234)]
    pub object_count: usize,

    /// Total storage used in bytes
    #[schema(example = 1073741824)]
    pub total_bytes: u64,

    /// Breakdown by storage class
    pub storage_class_breakdown: std::collections::HashMap<String, StorageClassStats>,
}

/// Storage class statistics
#[derive(utoipa::ToSchema, serde::Serialize, serde::Deserialize)]
pub struct StorageClassStats {
    /// Number of objects
    #[schema(example = 100)]
    pub object_count: usize,

    /// Total bytes
    #[schema(example = 104857600)]
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_schema_generation() {
        let doc = ApiDoc::openapi();
        assert_eq!(
            doc.info.title,
            "rs3gw - High-Performance S3-Compatible Object Storage Gateway"
        );
        assert_eq!(doc.info.version, "0.1.0");
        assert!(doc.servers.is_some());
        assert!(!doc.servers.expect("Failed to get servers").is_empty());
    }

    #[test]
    fn test_security_schemes() {
        let doc = ApiDoc::openapi();
        let components = doc.components.as_ref().expect("Failed to get components");
        assert!(components.security_schemes.contains_key("aws_sigv4"));
        assert!(components.security_schemes.contains_key("api_key"));
    }

    #[test]
    fn test_schema_definitions() {
        let doc = ApiDoc::openapi();
        let components = doc.components.as_ref().expect("Failed to get components");

        // Check that our custom schemas are registered
        let schema_names: Vec<&str> = components.schemas.keys().map(|s| s.as_str()).collect();

        // These should be present
        let expected = vec![
            "BucketInfo",
            "ObjectInfo",
            "ListBucketsResponse",
            "ErrorResponse",
            "HealthResponse",
        ];

        for name in expected {
            assert!(
                schema_names.contains(&name),
                "Schema '{}' should be present in components",
                name
            );
        }
    }

    #[test]
    fn test_paths_included() {
        let doc = ApiDoc::openapi();

        // Check that we have paths defined
        assert!(!doc.paths.paths.is_empty(), "Paths should not be empty");

        // Check for specific paths
        assert!(
            doc.paths.paths.contains_key("/health"),
            "/health path should be present"
        );
        assert!(
            doc.paths.paths.contains_key("/"),
            "/ (list buckets) path should be present"
        );
    }
}
