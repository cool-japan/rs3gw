//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::storage::{ObjectMetadata, StorageEngine, StorageError};
use std::collections::HashMap;
use std::sync::Arc;

use super::functions::default_true;

/// Local filesystem backend implementation
///
/// Wraps StorageEngine to provide the StorageBackend interface
pub struct LocalBackend {
    pub(super) engine: Arc<StorageEngine>,
}
impl LocalBackend {
    /// Create a new LocalBackend from a StorageEngine
    pub fn new(engine: Arc<StorageEngine>) -> Self {
        Self { engine }
    }
}
/// Backend configuration
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct BackendConfig {
    /// Backend type
    pub backend_type: BackendType,
    /// Endpoint URL (for remote backends)
    pub endpoint: Option<String>,
    /// Access key (for remote backends)
    pub access_key: Option<String>,
    /// Secret key (for remote backends)
    pub secret_key: Option<String>,
    /// Region (for cloud backends)
    pub region: Option<String>,
    /// Use SSL/TLS (for remote backends)
    #[serde(default = "default_true")]
    pub use_ssl: bool,
    /// Additional backend-specific configuration
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
/// Backend type configuration
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    /// Local filesystem backend
    #[default]
    Local,
    /// MinIO backend (delegate to remote MinIO server)
    MinIO,
    /// Ceph/RADOS backend
    Ceph,
    /// GlusterFS backend
    GlusterFS,
    /// AWS S3 backend (proxy)
    S3,
    /// Google Cloud Storage backend (proxy)
    Gcs,
    /// Azure Blob Storage backend (proxy)
    Azure,
}
/// MinIO backend implementation
///
/// Delegates storage operations to a remote MinIO server using the AWS SDK
pub struct MinIOBackend {
    pub(super) client: aws_sdk_s3::Client,
    #[allow(dead_code)]
    config: BackendConfig,
}
impl MinIOBackend {
    /// Create a new MinIO backend
    ///
    /// # Arguments
    /// * `config` - Backend configuration with endpoint, access key, and secret key
    ///
    /// # Errors
    /// Returns an error if the configuration is invalid or the client cannot be created
    pub async fn new(config: BackendConfig) -> Result<Self, StorageError> {
        let endpoint = config
            .endpoint
            .as_ref()
            .ok_or_else(|| StorageError::Internal("MinIO endpoint not configured".into()))?;
        let access_key = config
            .access_key
            .as_ref()
            .ok_or_else(|| StorageError::Internal("MinIO access key not configured".into()))?;
        let secret_key = config
            .secret_key
            .as_ref()
            .ok_or_else(|| StorageError::Internal("MinIO secret key not configured".into()))?;
        let credentials =
            aws_sdk_s3::config::Credentials::new(access_key, secret_key, None, None, "rs3gw-minio");
        let region = config
            .region
            .as_ref()
            .map(|r| aws_sdk_s3::config::Region::new(r.clone()))
            .unwrap_or_else(|| aws_sdk_s3::config::Region::new("us-east-1"));
        let sdk_config = aws_sdk_s3::config::Builder::new()
            .endpoint_url(endpoint)
            .region(region)
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();
        let client = aws_sdk_s3::Client::from_conf(sdk_config);
        Ok(Self { client, config })
    }
}
/// Result of listing objects
#[derive(Debug, Clone)]
pub struct ObjectListResult {
    /// List of object keys and metadata
    pub objects: Vec<(String, ObjectMetadata)>,
    /// Common prefixes (for delimiter-based listing)
    pub common_prefixes: Vec<String>,
    /// Is the result truncated?
    pub is_truncated: bool,
    /// Next continuation token
    pub next_continuation_token: Option<String>,
}
/// AWS S3 backend implementation
///
/// Delegates storage operations to AWS S3 using the AWS SDK
pub struct S3Backend {
    pub(super) client: aws_sdk_s3::Client,
    #[allow(dead_code)]
    config: BackendConfig,
}
impl S3Backend {
    /// Create a new AWS S3 backend
    ///
    /// # Arguments
    /// * `config` - Backend configuration with optional endpoint, access key, secret key, and region
    ///
    /// # Errors
    /// Returns an error if the AWS SDK configuration fails
    pub async fn new(config: BackendConfig) -> Result<Self, StorageError> {
        let sdk_config = if let (Some(access_key), Some(secret_key)) =
            (&config.access_key, &config.secret_key)
        {
            let credentials = aws_sdk_s3::config::Credentials::new(
                access_key, secret_key, None, None, "rs3gw-s3",
            );
            let mut builder = aws_sdk_s3::config::Builder::new().credentials_provider(credentials);
            if let Some(region) = &config.region {
                builder = builder.region(aws_sdk_s3::config::Region::new(region.clone()));
            }
            if let Some(endpoint) = &config.endpoint {
                builder = builder.endpoint_url(endpoint);
            }
            builder.build()
        } else {
            let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .load()
                .await;
            let mut builder = aws_sdk_s3::config::Builder::from(&aws_config);
            if let Some(region) = &config.region {
                builder = builder.region(aws_sdk_s3::config::Region::new(region.clone()));
            }
            if let Some(endpoint) = &config.endpoint {
                builder = builder.endpoint_url(endpoint);
            }
            builder.build()
        };
        let client = aws_sdk_s3::Client::from_conf(sdk_config);
        Ok(Self { client, config })
    }
}

/// Google Cloud Storage backend implementation
///
/// Delegates storage operations to Google Cloud Storage using the GCS SDK
///
/// NOTE: This is currently a stub. Full implementation requires proper GCS SDK integration.
/// The google-cloud-storage SDK v0.25 has undergone significant API changes.
pub struct GcsBackend {
    #[allow(dead_code)]
    config: BackendConfig,
}

impl GcsBackend {
    /// Create a new Google Cloud Storage backend
    ///
    /// # Arguments
    /// * `config` - Backend configuration with optional project ID and credentials
    ///
    /// # Errors
    /// Returns an error if the GCS client configuration fails
    pub async fn new(config: BackendConfig) -> Result<Self, StorageError> {
        // Stub implementation - proper GCS client initialization will be added
        // when the google-cloud-storage SDK API stabilizes
        Ok(Self { config })
    }
}

/// Azure Blob Storage backend implementation
///
/// Delegates storage operations to Azure Blob Storage using the Azure SDK
#[allow(dead_code)] // Stub implementation - fields will be used when fully implemented
pub struct AzureBackend {
    pub(super) storage_client: Arc<azure_storage::StorageCredentials>,
    pub(super) account: String,
    #[allow(dead_code)]
    config: BackendConfig,
}

#[allow(dead_code)] // Stub implementation - methods will be used when fully implemented
impl AzureBackend {
    /// Create a new Azure Blob Storage backend
    ///
    /// # Arguments
    /// * `config` - Backend configuration with storage account and access key
    ///   - `access_key`: Storage account name
    ///   - `secret_key`: Storage account access key
    ///   - `endpoint`: Optional custom endpoint (e.g., for Azurite emulator)
    ///
    /// # Errors
    /// Returns an error if the Azure client configuration fails
    pub async fn new(config: BackendConfig) -> Result<Self, StorageError> {
        let account = config
            .access_key
            .as_ref()
            .ok_or_else(|| {
                StorageError::Internal("Azure storage account name not configured".into())
            })?
            .clone();

        let access_key = config.secret_key.clone().ok_or_else(|| {
            StorageError::Internal("Azure storage account key not configured".into())
        })?;

        let storage_client = if config.endpoint.is_some() {
            // Custom endpoint (e.g., Azurite emulator)
            let credentials =
                azure_storage::StorageCredentials::access_key(account.clone(), access_key.clone());
            // Note: Custom endpoint support may require additional configuration
            Arc::new(credentials)
        } else {
            // Standard Azure endpoint
            let credentials =
                azure_storage::StorageCredentials::access_key(account.clone(), access_key.clone());
            Arc::new(credentials)
        };

        Ok(Self {
            storage_client,
            account,
            config,
        })
    }

    /// Get a blob service client
    pub(crate) fn blob_service_client(&self) -> azure_storage_blobs::prelude::BlobServiceClient {
        azure_storage_blobs::prelude::BlobServiceClient::new(
            &self.account,
            (*self.storage_client).clone(),
        )
    }

    /// Get a container client
    pub(crate) fn container_client(
        &self,
        container: &str,
    ) -> azure_storage_blobs::prelude::ContainerClient {
        self.blob_service_client().container_client(container)
    }

    /// Get a blob client
    pub(crate) fn blob_client(
        &self,
        container: &str,
        blob: &str,
    ) -> azure_storage_blobs::prelude::BlobClient {
        self.container_client(container).blob_client(blob)
    }
}

/// Ceph/RADOS backend implementation (stub)
///
/// This is a placeholder implementation for Ceph/RADOS backend support.
/// Full implementation requires the rados crate and Ceph cluster configuration.
pub struct CephBackend {
    #[allow(dead_code)]
    config: BackendConfig,
}

impl CephBackend {
    /// Create a new Ceph/RADOS backend
    ///
    /// # Arguments
    /// * `config` - Backend configuration with Ceph cluster details
    ///   - `endpoint`: Ceph monitor addresses (comma-separated)
    ///   - `access_key`: Ceph user/keyring name
    ///   - `secret_key`: Ceph authentication key
    ///   - `extra.pool`: RADOS pool name (optional, defaults to "default")
    ///
    /// # Errors
    /// Returns an error - this is a stub implementation
    pub async fn new(config: BackendConfig) -> Result<Self, StorageError> {
        Ok(Self { config })
    }
}

/// GlusterFS backend implementation (stub)
///
/// This is a placeholder implementation for GlusterFS backend support.
/// Full implementation requires libgfapi bindings and GlusterFS volume configuration.
pub struct GlusterBackend {
    #[allow(dead_code)]
    config: BackendConfig,
}

impl GlusterBackend {
    /// Create a new GlusterFS backend
    ///
    /// # Arguments
    /// * `config` - Backend configuration with GlusterFS volume details
    ///   - `endpoint`: GlusterFS server address
    ///   - `extra.volume`: GlusterFS volume name (required)
    ///   - `extra.transport`: Transport type (tcp, rdma, socket)
    ///
    /// # Errors
    /// Returns an error - this is a stub implementation
    pub async fn new(config: BackendConfig) -> Result<Self, StorageError> {
        Ok(Self { config })
    }
}
