//! Cluster module for multi-node deployment
//!
//! Implements a multi-leader architecture where any node can accept writes.
//! Replication can be configured as synchronous or asynchronous per-bucket.
//!
//! ## Architecture
//!
//! - **Node Registry**: Tracks all nodes in the cluster using gossip protocol
//! - **Replication**: Propagates writes to peer nodes based on replication factor
//! - **Conflict Resolution**: Last-writer-wins based on vector clocks
//! - **Health Checking**: Periodic heartbeats to detect node failures

#[cfg(feature = "server")]
pub mod advanced_replication;
pub mod config;
#[cfg(feature = "server")]
pub mod node;
#[cfg(feature = "server")]
pub mod replication;

#[cfg(feature = "server")]
pub use advanced_replication::{
    AdvancedReplicationError, AdvancedReplicationManager, AdvancedReplicationResult,
    ConflictResolution, ConflictResolutionResult, CrossRegionConfig, DestinationMetrics,
    EnhancedReplicationEvent, HealthStatus, Region, RegionLocation, ReplicationBatch,
    ReplicationFilter, ReplicationMetrics,
};
pub use config::{ClusterConfig, ReplicationConfig, ReplicationMode};
#[cfg(feature = "server")]
pub use node::{ClusterNode, NodeId, NodeInfo, NodeRegistry, NodeState};
#[cfg(feature = "server")]
pub use replication::{ReplicationEvent, ReplicationManager, ReplicationResult};
