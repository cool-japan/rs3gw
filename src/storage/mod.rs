//! Storage layer for rs3gw
//!
//! This module has been refactored using SplitRS. Core types and functions
//! are now in the `core` submodule.

// Core functionality (split from original mod.rs)
pub mod core;

// Feature modules
pub mod analytics;
pub mod archival;
pub mod audit;
pub mod backend;
pub mod backup;
pub mod compliance;
pub mod dataset_registry;
pub mod dedup;
pub mod distributed_training;
pub mod encryption;
pub mod gc;
pub mod ml_cache;
pub mod ml_models;
pub mod model_registry;
pub mod object_lambda;
pub mod object_lock;
pub mod preprocessing;
pub mod self_healing;
pub mod storage_class;
pub mod tiering; // Refactored from intelligent_tiering.rs
pub mod transformations;
pub mod versioning;
pub mod zerocopy;

// Re-export core types and functions
pub use core::*;

// Re-export types from feature modules
pub use analytics::*;
pub use archival::*;
pub use audit::*;
pub use backend::*;
pub use backup::*;
pub use compliance::*;
pub use dataset_registry::*;
pub use dedup::*;
pub use distributed_training::*;
pub use encryption::*;
pub use gc::*;
pub use ml_cache::*;
pub use ml_models::*;
pub use model_registry::*;
pub use object_lambda::*;
pub use preprocessing::*;
pub use self_healing::*;
pub use storage_class::*;
pub use tiering::*; // Intelligent tiering (refactored)
pub use transformations::*;
pub use versioning::*;
pub use zerocopy::*;
