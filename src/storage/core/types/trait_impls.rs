//! # StorageError - Trait Implementations
//!
//! This module contains trait implementations for `StorageError`.
//!
//! ## Implemented Traits
//!
//! - `From`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::base_types::StorageError;

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => StorageError::AccessDenied,
            std::io::ErrorKind::StorageFull => StorageError::InsufficientStorage,
            _ => StorageError::Io(e),
        }
    }
}
