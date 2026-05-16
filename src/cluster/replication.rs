//! Data replication between cluster nodes
//!
//! Handles propagating writes to peer nodes based on replication configuration.

use crate::cluster::config::{ReplicationConfig, ReplicationMode};
use crate::cluster::node::{NodeId, NodeInfo, NodeRegistry};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

/// Replication errors
#[derive(Debug, Error)]
pub enum ReplicationError {
    #[error("Not enough replicas available: need {needed}, have {available}")]
    InsufficientReplicas { needed: u8, available: usize },

    #[error("Replication timeout")]
    Timeout,

    #[error("Replication failed: {0}")]
    Failed(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Quorum not reached: {successes}/{needed} succeeded")]
    QuorumNotReached { successes: usize, needed: usize },
}

/// Result of a replication operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationResult {
    /// Number of successful replications
    pub successes: usize,
    /// Number of failures
    pub failures: usize,
    /// Node IDs that failed
    pub failed_nodes: Vec<NodeId>,
    /// Whether quorum was reached
    pub quorum_reached: bool,
}

impl ReplicationResult {
    fn success(node_count: usize) -> Self {
        Self {
            successes: node_count,
            failures: 0,
            failed_nodes: Vec::new(),
            quorum_reached: true,
        }
    }

    fn partial(successes: usize, failed_nodes: Vec<NodeId>, quorum_reached: bool) -> Self {
        Self {
            successes,
            failures: failed_nodes.len(),
            failed_nodes,
            quorum_reached,
        }
    }
}

/// Type of replication event
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReplicationEvent {
    /// Object was created or updated
    PutObject {
        bucket: String,
        key: String,
        etag: String,
        size: u64,
        content_type: String,
        metadata: HashMap<String, String>,
        /// Source node that originated the write
        source_node: NodeId,
        /// Vector clock for conflict resolution
        vector_clock: HashMap<NodeId, u64>,
    },
    /// Object was deleted
    DeleteObject {
        bucket: String,
        key: String,
        source_node: NodeId,
        vector_clock: HashMap<NodeId, u64>,
    },
    /// Bucket was created
    CreateBucket { bucket: String, source_node: NodeId },
    /// Bucket was deleted
    DeleteBucket { bucket: String, source_node: NodeId },
}

impl ReplicationEvent {
    /// Get the bucket name for this event
    pub fn bucket(&self) -> &str {
        match self {
            Self::PutObject { bucket, .. } => bucket,
            Self::DeleteObject { bucket, .. } => bucket,
            Self::CreateBucket { bucket, .. } => bucket,
            Self::DeleteBucket { bucket, .. } => bucket,
        }
    }

    /// Get the source node for this event
    pub fn source_node(&self) -> &NodeId {
        match self {
            Self::PutObject { source_node, .. } => source_node,
            Self::DeleteObject { source_node, .. } => source_node,
            Self::CreateBucket { source_node, .. } => source_node,
            Self::DeleteBucket { source_node, .. } => source_node,
        }
    }

    /// Check if this event should be replicated based on config
    pub fn should_replicate(&self, config: &ReplicationConfig) -> bool {
        // Check delete replication
        if matches!(self, Self::DeleteObject { .. }) && !config.replicate_deletes {
            return false;
        }

        // Check prefix filter for object operations
        if let Some(prefix) = &config.prefix_filter {
            match self {
                Self::PutObject { key, .. } | Self::DeleteObject { key, .. }
                    if !key.starts_with(prefix) =>
                {
                    return false;
                }
                _ => {}
            }
        }

        true
    }
}

/// Manager for data replication
pub struct ReplicationManager {
    /// Node registry for peer discovery
    registry: Arc<NodeRegistry>,
    /// HTTP client for replication requests
    client: Client,
    /// Per-bucket replication configs
    bucket_configs: Arc<tokio::sync::RwLock<HashMap<String, ReplicationConfig>>>,
    /// Default replication config
    default_config: ReplicationConfig,
    /// Channel for async replication events
    async_tx: mpsc::UnboundedSender<ReplicationEvent>,
    /// Local node ID
    local_node_id: NodeId,
}

impl ReplicationManager {
    /// Create a new replication manager
    pub fn new(
        registry: Arc<NodeRegistry>,
        default_config: ReplicationConfig,
        local_node_id: NodeId,
    ) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| {
                // Fallback to default client if build fails (should be extremely rare)
                tracing::warn!("Failed to create configured HTTP client, using default");
                Client::new()
            });

        let (async_tx, async_rx) = mpsc::unbounded_channel();

        let manager = Self {
            registry,
            client,
            bucket_configs: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            default_config,
            async_tx,
            local_node_id,
        };

        // Start background replication worker
        manager.start_async_worker(async_rx);

        manager
    }

    /// Start the background worker for async replication
    fn start_async_worker(&self, mut rx: mpsc::UnboundedReceiver<ReplicationEvent>) {
        let client = self.client.clone();
        let registry = Arc::clone(&self.registry);
        let bucket_configs = Arc::clone(&self.bucket_configs);
        let default_config = self.default_config.clone();

        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                let peers = registry.alive_peers().await;
                let config = {
                    let configs = bucket_configs.read().await;
                    configs
                        .get(event.bucket())
                        .cloned()
                        .unwrap_or_else(|| default_config.clone())
                };

                if !event.should_replicate(&config) {
                    continue;
                }

                for peer in peers.iter().take(config.replication_factor as usize - 1) {
                    if let Err(e) = Self::send_event(&client, peer, &event).await {
                        warn!(
                            node_id = %peer.id,
                            error = %e,
                            "Async replication failed"
                        );
                    }
                }
            }
        });
    }

    /// Set replication config for a bucket
    pub async fn set_bucket_config(&self, bucket: String, config: ReplicationConfig) {
        let mut configs = self.bucket_configs.write().await;
        configs.insert(bucket, config);
    }

    /// Get replication config for a bucket
    pub async fn get_bucket_config(&self, bucket: &str) -> ReplicationConfig {
        let configs = self.bucket_configs.read().await;
        configs
            .get(bucket)
            .cloned()
            .unwrap_or_else(|| self.default_config.clone())
    }

    /// Replicate an event to peer nodes
    pub async fn replicate(
        &self,
        event: ReplicationEvent,
    ) -> Result<ReplicationResult, ReplicationError> {
        let config = self.get_bucket_config(event.bucket()).await;

        if !event.should_replicate(&config) {
            return Ok(ReplicationResult::success(1));
        }

        let peers = self.registry.alive_peers().await;
        let needed_replicas = (config.replication_factor as usize).saturating_sub(1);

        if peers.len() < needed_replicas {
            // Not enough peers, but we might still proceed depending on mode
            if config.mode == ReplicationMode::Synchronous {
                return Err(ReplicationError::InsufficientReplicas {
                    needed: config.replication_factor,
                    available: peers.len() + 1,
                });
            }
        }

        match config.mode {
            ReplicationMode::Asynchronous => {
                // Queue for background replication
                let _ = self.async_tx.send(event);
                Ok(ReplicationResult::success(1))
            }
            ReplicationMode::Synchronous => {
                self.replicate_sync(&event, &peers, needed_replicas, config.sync_timeout)
                    .await
            }
            ReplicationMode::Quorum => {
                let quorum = (config.replication_factor as usize / 2) + 1;
                self.replicate_quorum(&event, &peers, quorum, config.sync_timeout)
                    .await
            }
        }
    }

    /// Synchronous replication - wait for all replicas
    async fn replicate_sync(
        &self,
        event: &ReplicationEvent,
        peers: &[NodeInfo],
        needed: usize,
        timeout_duration: Duration,
    ) -> Result<ReplicationResult, ReplicationError> {
        let targets: Vec<_> = peers.iter().take(needed).collect();

        if targets.is_empty() {
            return Ok(ReplicationResult::success(1));
        }

        let futures: Vec<_> = targets
            .iter()
            .map(|peer| Self::send_event(&self.client, peer, event))
            .collect();

        let results = match timeout(timeout_duration, futures::future::join_all(futures)).await {
            Ok(results) => results,
            Err(_) => return Err(ReplicationError::Timeout),
        };

        let mut successes = 1; // Count local write
        let mut failed_nodes = Vec::new();

        for (result, peer) in results.into_iter().zip(targets.iter()) {
            match result {
                Ok(_) => successes += 1,
                Err(e) => {
                    error!(node_id = %peer.id, error = %e, "Sync replication failed");
                    failed_nodes.push(peer.id.clone());
                }
            }
        }

        if !failed_nodes.is_empty() {
            return Err(ReplicationError::Failed(format!(
                "Replication failed to {} nodes",
                failed_nodes.len()
            )));
        }

        Ok(ReplicationResult::success(successes))
    }

    /// Quorum replication - wait for majority
    async fn replicate_quorum(
        &self,
        event: &ReplicationEvent,
        peers: &[NodeInfo],
        quorum: usize,
        timeout_duration: Duration,
    ) -> Result<ReplicationResult, ReplicationError> {
        let futures: Vec<_> = peers
            .iter()
            .map(|peer| Self::send_event(&self.client, peer, event))
            .collect();

        let results = match timeout(timeout_duration, futures::future::join_all(futures)).await {
            Ok(results) => results,
            Err(_) => return Err(ReplicationError::Timeout),
        };

        let mut successes = 1; // Count local write
        let mut failed_nodes = Vec::new();

        for (result, peer) in results.into_iter().zip(peers.iter()) {
            match result {
                Ok(_) => successes += 1,
                Err(e) => {
                    warn!(node_id = %peer.id, error = %e, "Quorum replication failed to node");
                    failed_nodes.push(peer.id.clone());
                }
            }
        }

        let quorum_reached = successes >= quorum;

        if !quorum_reached {
            return Err(ReplicationError::QuorumNotReached {
                successes,
                needed: quorum,
            });
        }

        Ok(ReplicationResult::partial(
            successes,
            failed_nodes,
            quorum_reached,
        ))
    }

    /// Send a replication event to a peer
    async fn send_event(
        client: &Client,
        peer: &NodeInfo,
        event: &ReplicationEvent,
    ) -> Result<(), ReplicationError> {
        let url = format!("http://{}/_cluster/replicate", peer.cluster_addr);

        debug!(node_id = %peer.id, url = %url, "Sending replication event");

        let response = client.post(&url).json(event).send().await?;

        if response.status().is_success() {
            debug!(node_id = %peer.id, "Replication successful");
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            Err(ReplicationError::Failed(format!(
                "HTTP {}: {}",
                status, body
            )))
        }
    }

    /// Handle an incoming replication event from a peer
    pub async fn handle_incoming(&self, event: ReplicationEvent) -> Result<(), ReplicationError> {
        // Skip if we originated this event
        if event.source_node() == &self.local_node_id {
            return Ok(());
        }

        info!(
            source = %event.source_node(),
            bucket = %event.bucket(),
            "Received replication event"
        );

        // The actual application of the event to storage is handled by the caller
        // This just validates and logs the event

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::node::ClusterNode;

    #[test]
    fn test_replication_event_should_replicate() {
        let config = ReplicationConfig {
            replicate_deletes: false,
            prefix_filter: Some("data/".to_string()),
            ..Default::default()
        };

        // Delete should not replicate
        let delete_event = ReplicationEvent::DeleteObject {
            bucket: "test".to_string(),
            key: "data/file.txt".to_string(),
            source_node: "node-1".to_string(),
            vector_clock: HashMap::new(),
        };
        assert!(!delete_event.should_replicate(&config));

        // Put with matching prefix should replicate
        let put_event = ReplicationEvent::PutObject {
            bucket: "test".to_string(),
            key: "data/file.txt".to_string(),
            etag: "abc".to_string(),
            size: 100,
            content_type: "text/plain".to_string(),
            metadata: HashMap::new(),
            source_node: "node-1".to_string(),
            vector_clock: HashMap::new(),
        };
        assert!(put_event.should_replicate(&config));

        // Put without matching prefix should not replicate
        let put_other = ReplicationEvent::PutObject {
            bucket: "test".to_string(),
            key: "other/file.txt".to_string(),
            etag: "abc".to_string(),
            size: 100,
            content_type: "text/plain".to_string(),
            metadata: HashMap::new(),
            source_node: "node-1".to_string(),
            vector_clock: HashMap::new(),
        };
        assert!(!put_other.should_replicate(&config));
    }

    #[test]
    fn test_replication_result() {
        let success = ReplicationResult::success(3);
        assert_eq!(success.successes, 3);
        assert_eq!(success.failures, 0);
        assert!(success.quorum_reached);

        let partial = ReplicationResult::partial(2, vec!["node-3".to_string()], true);
        assert_eq!(partial.successes, 2);
        assert_eq!(partial.failures, 1);
        assert!(partial.quorum_reached);
    }

    #[tokio::test]
    async fn test_replication_manager_config() {
        let local = ClusterNode::with_id(
            "local".to_string(),
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
            Duration::from_secs(10),
        );
        let registry = Arc::new(NodeRegistry::new(local, Duration::from_secs(10)));

        let manager =
            ReplicationManager::new(registry, ReplicationConfig::default(), "local".to_string());

        // Set custom config for a bucket
        let custom_config = ReplicationConfig {
            replication_factor: 3,
            mode: ReplicationMode::Synchronous,
            ..Default::default()
        };
        manager
            .set_bucket_config("my-bucket".to_string(), custom_config)
            .await;

        // Verify config is returned
        let config = manager.get_bucket_config("my-bucket").await;
        assert_eq!(config.replication_factor, 3);
        assert_eq!(config.mode, ReplicationMode::Synchronous);

        // Other buckets get default
        let default = manager.get_bucket_config("other-bucket").await;
        assert_eq!(default.replication_factor, 2);
        assert_eq!(default.mode, ReplicationMode::Asynchronous);
    }
}
