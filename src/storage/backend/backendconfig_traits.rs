//! # BackendConfig - Trait Implementations
//!
//! This module contains trait implementations for `BackendConfig`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use std::collections::HashMap;

use super::types::{BackendConfig, BackendType};

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            backend_type: BackendType::Local,
            endpoint: None,
            access_key: None,
            secret_key: None,
            region: None,
            use_ssl: true,
            extra: HashMap::new(),
        }
    }
}
