//! Cluster configuration
//!
//! Defines configuration for cluster membership and replication behavior.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Replication mode for data synchronization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReplicationMode {
    /// Wait for all replicas before acknowledging (strong consistency)
    Synchronous,
    /// Acknowledge immediately, replicate in background (eventual consistency)
    #[default]
    Asynchronous,
    /// Write to quorum of nodes before acknowledging
    Quorum,
}

/// Replication configuration for a bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// Replication mode
    pub mode: ReplicationMode,
    /// Number of replicas (including the primary)
    pub replication_factor: u8,
    /// Timeout for synchronous replication
    #[serde(with = "humantime_serde", default = "default_sync_timeout")]
    pub sync_timeout: Duration,
    /// Whether to replicate deletes
    pub replicate_deletes: bool,
    /// Prefix filter (empty = replicate all)
    pub prefix_filter: Option<String>,
}

fn default_sync_timeout() -> Duration {
    Duration::from_secs(5)
}

impl Default for ReplicationConfig {
    fn default() -> Self {
        Self {
            mode: ReplicationMode::Asynchronous,
            replication_factor: 2,
            sync_timeout: default_sync_timeout(),
            replicate_deletes: true,
            prefix_filter: None,
        }
    }
}

/// Cluster configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    /// This node's unique identifier (auto-generated if not set)
    pub node_id: Option<String>,
    /// This node's advertised address for other nodes to connect
    pub advertise_addr: String,
    /// Addresses of seed nodes for initial cluster discovery
    pub seed_nodes: Vec<String>,
    /// Port for cluster communication (gossip + replication)
    pub cluster_port: u16,
    /// Interval between gossip rounds
    #[serde(with = "humantime_serde", default = "default_gossip_interval")]
    pub gossip_interval: Duration,
    /// Timeout for considering a node dead
    #[serde(with = "humantime_serde", default = "default_node_timeout")]
    pub node_timeout: Duration,
    /// Default replication configuration
    pub default_replication: ReplicationConfig,
    /// Whether cluster mode is enabled
    pub enabled: bool,
}

fn default_gossip_interval() -> Duration {
    Duration::from_secs(1)
}

fn default_node_timeout() -> Duration {
    Duration::from_secs(10)
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            node_id: None,
            advertise_addr: "127.0.0.1:9001".to_string(),
            seed_nodes: Vec::new(),
            cluster_port: 9001,
            gossip_interval: default_gossip_interval(),
            node_timeout: default_node_timeout(),
            default_replication: ReplicationConfig::default(),
            enabled: false,
        }
    }
}

impl ClusterConfig {
    /// Create a new cluster configuration
    pub fn new(advertise_addr: String, cluster_port: u16) -> Self {
        Self {
            advertise_addr,
            cluster_port,
            ..Default::default()
        }
    }

    /// Add a seed node
    pub fn with_seed_node(mut self, addr: String) -> Self {
        self.seed_nodes.push(addr);
        self
    }

    /// Set replication factor
    pub fn with_replication_factor(mut self, factor: u8) -> Self {
        self.default_replication.replication_factor = factor;
        self
    }

    /// Set replication mode
    pub fn with_replication_mode(mut self, mode: ReplicationMode) -> Self {
        self.default_replication.mode = mode;
        self
    }

    /// Enable cluster mode
    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }
}

/// Serde helper for Duration using humantime format
mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let s = humantime::format_duration(*duration).to_string();
        serializer.serialize_str(&s)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        humantime::parse_duration(&s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ClusterConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.default_replication.replication_factor, 2);
        assert_eq!(
            config.default_replication.mode,
            ReplicationMode::Asynchronous
        );
    }

    #[test]
    fn test_config_builder() {
        let config = ClusterConfig::new("10.0.0.1:9001".to_string(), 9001)
            .with_seed_node("10.0.0.2:9001".to_string())
            .with_replication_factor(3)
            .with_replication_mode(ReplicationMode::Quorum)
            .enabled();

        assert!(config.enabled);
        assert_eq!(config.seed_nodes.len(), 1);
        assert_eq!(config.default_replication.replication_factor, 3);
        assert_eq!(config.default_replication.mode, ReplicationMode::Quorum);
    }

    #[test]
    fn test_replication_mode_default() {
        let mode = ReplicationMode::default();
        assert_eq!(mode, ReplicationMode::Asynchronous);
    }
}
