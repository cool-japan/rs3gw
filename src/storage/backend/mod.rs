//! Auto-generated module structure

#[cfg(feature = "azure")]
pub mod azurebackend_traits;
pub mod backendconfig_traits;
pub mod cephbackend_traits;
pub mod functions;
#[cfg(feature = "gcs")]
pub mod gcsbackend_traits;
pub mod glusterbackend_traits;
pub mod localbackend_traits;
#[cfg(feature = "s3")]
pub mod miniobackend_traits;
#[cfg(feature = "s3")]
pub mod s3backend_traits;
pub mod types;

// Re-export all types
pub use functions::{
    create_backend_from_config, default_true, ByteStream, DynBackend, StorageBackend,
};
pub use types::*;
