//! Auto-generated module structure

pub mod azurebackend_traits;
pub mod backendconfig_traits;
pub mod cephbackend_traits;
pub mod functions;
pub mod gcsbackend_traits;
pub mod glusterbackend_traits;
pub mod localbackend_traits;
pub mod miniobackend_traits;
pub mod s3backend_traits;
pub mod types;

// Re-export all types
pub use functions::{
    create_backend_from_config, default_true, ByteStream, DynBackend, StorageBackend,
};
pub use types::*;
