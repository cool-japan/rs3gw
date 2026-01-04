//! Node management and registry
//!
//! Handles cluster membership, node discovery, and health tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Unique identifier for a node in the cluster
pub type NodeId = String;

/// State of a node in the cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NodeState {
    /// Node is healthy and accepting requests
    #[default]
    Alive,
    /// Node is suspected to be down (missed heartbeats)
    Suspect,
    /// Node is confirmed dead
    Dead,
    /// Node is leaving the cluster gracefully
    Leaving,
}

/// Information about a node in the cluster
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique node identifier
    pub id: NodeId,
    /// Address for S3 API requests
    pub api_addr: String,
    /// Address for cluster communication
    pub cluster_addr: String,
    /// Current state
    pub state: NodeState,
    /// Monotonically increasing incarnation number (for crdt consistency)
    pub incarnation: u64,
    /// Last time we heard from this node
    #[serde(skip)]
    pub last_seen: Option<Instant>,
    /// Node metadata (region, zone, etc.)
    pub metadata: HashMap<String, String>,
}

impl NodeInfo {
    /// Create a new node info
    pub fn new(id: NodeId, api_addr: String, cluster_addr: String) -> Self {
        Self {
            id,
            api_addr,
            cluster_addr,
            state: NodeState::Alive,
            incarnation: 0,
            last_seen: Some(Instant::now()),
            metadata: HashMap::new(),
        }
    }

    /// Update the last seen timestamp
    pub fn touch(&mut self) {
        self.last_seen = Some(Instant::now());
    }

    /// Check if the node is considered alive
    pub fn is_alive(&self) -> bool {
        self.state == NodeState::Alive
    }

    /// Check if the node is available for requests
    pub fn is_available(&self) -> bool {
        matches!(self.state, NodeState::Alive | NodeState::Suspect)
    }

    /// Duration since last contact
    pub fn time_since_last_seen(&self) -> Option<Duration> {
        self.last_seen.map(|t| t.elapsed())
    }
}

/// A node in the cluster
#[derive(Debug)]
pub struct ClusterNode {
    /// This node's information
    pub info: NodeInfo,
    /// Configuration
    #[allow(dead_code)]
    node_timeout: Duration,
}

impl ClusterNode {
    /// Create a new cluster node
    pub fn new(api_addr: String, cluster_addr: String, node_timeout: Duration) -> Self {
        let id = Uuid::new_v4().to_string();
        info!(node_id = %id, api_addr = %api_addr, cluster_addr = %cluster_addr, "Creating cluster node");

        Self {
            info: NodeInfo::new(id, api_addr, cluster_addr),
            node_timeout,
        }
    }

    /// Create with a specific node ID
    pub fn with_id(
        id: NodeId,
        api_addr: String,
        cluster_addr: String,
        node_timeout: Duration,
    ) -> Self {
        info!(node_id = %id, api_addr = %api_addr, cluster_addr = %cluster_addr, "Creating cluster node with ID");

        Self {
            info: NodeInfo::new(id, api_addr, cluster_addr),
            node_timeout,
        }
    }

    /// Get this node's ID
    pub fn id(&self) -> &NodeId {
        &self.info.id
    }

    /// Increment incarnation (used when rejoining after failure)
    pub fn increment_incarnation(&mut self) {
        self.info.incarnation += 1;
    }
}

/// Registry of all known nodes in the cluster
#[derive(Debug)]
pub struct NodeRegistry {
    /// Local node
    local: ClusterNode,
    /// Known peer nodes
    peers: Arc<RwLock<HashMap<NodeId, NodeInfo>>>,
    /// Node timeout duration
    node_timeout: Duration,
}

impl NodeRegistry {
    /// Create a new node registry
    pub fn new(local: ClusterNode, node_timeout: Duration) -> Self {
        Self {
            local,
            peers: Arc::new(RwLock::new(HashMap::new())),
            node_timeout,
        }
    }

    /// Get the local node's ID
    pub fn local_id(&self) -> &NodeId {
        self.local.id()
    }

    /// Get the local node's info
    pub fn local_info(&self) -> &NodeInfo {
        &self.local.info
    }

    /// Get a mutable reference to the local node
    pub fn local_mut(&mut self) -> &mut ClusterNode {
        &mut self.local
    }

    /// Register or update a peer node
    pub async fn upsert_peer(&self, info: NodeInfo) {
        let mut peers = self.peers.write().await;

        if info.id == self.local.info.id {
            // Don't add ourselves as a peer
            return;
        }

        if let Some(existing) = peers.get(&info.id) {
            // Only update if incarnation is higher or equal with better state
            if info.incarnation > existing.incarnation
                || (info.incarnation == existing.incarnation && info.state == NodeState::Alive)
            {
                debug!(node_id = %info.id, incarnation = info.incarnation, "Updating peer node");
                peers.insert(info.id.clone(), info);
            }
        } else {
            info!(node_id = %info.id, cluster_addr = %info.cluster_addr, "Discovered new peer node");
            peers.insert(info.id.clone(), info);
        }
    }

    /// Remove a peer node
    pub async fn remove_peer(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if peers.remove(node_id).is_some() {
            info!(node_id = %node_id, "Removed peer node");
        }
    }

    /// Mark a peer as dead
    pub async fn mark_dead(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(node_id) {
            if peer.state != NodeState::Dead {
                warn!(node_id = %node_id, "Marking peer as dead");
                peer.state = NodeState::Dead;
            }
        }
    }

    /// Mark a peer as suspect
    pub async fn mark_suspect(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(node_id) {
            if peer.state == NodeState::Alive {
                debug!(node_id = %node_id, "Marking peer as suspect");
                peer.state = NodeState::Suspect;
            }
        }
    }

    /// Update peer's last seen timestamp
    pub async fn touch_peer(&self, node_id: &NodeId) {
        let mut peers = self.peers.write().await;
        if let Some(peer) = peers.get_mut(node_id) {
            peer.touch();
            if peer.state == NodeState::Suspect {
                debug!(node_id = %node_id, "Peer recovered from suspect state");
                peer.state = NodeState::Alive;
            }
        }
    }

    /// Get info about a specific peer
    pub async fn get_peer(&self, node_id: &NodeId) -> Option<NodeInfo> {
        let peers = self.peers.read().await;
        peers.get(node_id).cloned()
    }

    /// Get all alive peers
    pub async fn alive_peers(&self) -> Vec<NodeInfo> {
        let peers = self.peers.read().await;
        peers.values().filter(|p| p.is_alive()).cloned().collect()
    }

    /// Get all available peers (alive or suspect)
    pub async fn available_peers(&self) -> Vec<NodeInfo> {
        let peers = self.peers.read().await;
        peers
            .values()
            .filter(|p| p.is_available())
            .cloned()
            .collect()
    }

    /// Get all peers including dead ones
    pub async fn all_peers(&self) -> Vec<NodeInfo> {
        let peers = self.peers.read().await;
        peers.values().cloned().collect()
    }

    /// Get number of alive peers
    pub async fn alive_count(&self) -> usize {
        let peers = self.peers.read().await;
        peers.values().filter(|p| p.is_alive()).count()
    }

    /// Check peer health and update states
    pub async fn check_health(&self) {
        let mut peers = self.peers.write().await;

        for peer in peers.values_mut() {
            if let Some(elapsed) = peer.time_since_last_seen() {
                if elapsed > self.node_timeout * 2 && peer.state != NodeState::Dead {
                    warn!(node_id = %peer.id, elapsed = ?elapsed, "Peer timed out, marking as dead");
                    peer.state = NodeState::Dead;
                } else if elapsed > self.node_timeout && peer.state == NodeState::Alive {
                    debug!(node_id = %peer.id, elapsed = ?elapsed, "Peer missed heartbeat, marking as suspect");
                    peer.state = NodeState::Suspect;
                }
            }
        }
    }

    /// Get cluster state summary
    pub async fn cluster_state(&self) -> ClusterState {
        let peers = self.peers.read().await;
        let alive = peers
            .values()
            .filter(|p| p.state == NodeState::Alive)
            .count();
        let suspect = peers
            .values()
            .filter(|p| p.state == NodeState::Suspect)
            .count();
        let dead = peers
            .values()
            .filter(|p| p.state == NodeState::Dead)
            .count();

        ClusterState {
            local_id: self.local.info.id.clone(),
            total_nodes: peers.len() + 1, // +1 for local
            alive_nodes: alive + 1,       // +1 for local
            suspect_nodes: suspect,
            dead_nodes: dead,
        }
    }
}

/// Summary of cluster state
#[derive(Debug, Clone, Serialize)]
pub struct ClusterState {
    pub local_id: NodeId,
    pub total_nodes: usize,
    pub alive_nodes: usize,
    pub suspect_nodes: usize,
    pub dead_nodes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_info_creation() {
        let info = NodeInfo::new(
            "node-1".to_string(),
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
        );

        assert_eq!(info.id, "node-1");
        assert!(info.is_alive());
        assert!(info.is_available());
    }

    #[test]
    fn test_node_state_transitions() {
        let mut info = NodeInfo::new(
            "node-1".to_string(),
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
        );

        assert!(info.is_alive());
        assert!(info.is_available());

        info.state = NodeState::Suspect;
        assert!(!info.is_alive());
        assert!(info.is_available());

        info.state = NodeState::Dead;
        assert!(!info.is_alive());
        assert!(!info.is_available());
    }

    #[tokio::test]
    async fn test_node_registry() {
        let local = ClusterNode::with_id(
            "local".to_string(),
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
            Duration::from_secs(10),
        );

        let registry = NodeRegistry::new(local, Duration::from_secs(10));

        // Add a peer
        let peer = NodeInfo::new(
            "peer-1".to_string(),
            "127.0.0.2:9000".to_string(),
            "127.0.0.2:9001".to_string(),
        );
        registry.upsert_peer(peer).await;

        assert_eq!(registry.alive_count().await, 1);

        // Mark as suspect
        registry.mark_suspect(&"peer-1".to_string()).await;
        let peers = registry.available_peers().await;
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].state, NodeState::Suspect);

        // Mark as dead
        registry.mark_dead(&"peer-1".to_string()).await;
        assert_eq!(registry.alive_count().await, 0);
    }

    #[tokio::test]
    async fn test_registry_ignores_local() {
        let local = ClusterNode::with_id(
            "local".to_string(),
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
            Duration::from_secs(10),
        );

        let registry = NodeRegistry::new(local, Duration::from_secs(10));

        // Try to add ourselves as a peer
        let self_peer = NodeInfo::new(
            "local".to_string(),
            "127.0.0.1:9000".to_string(),
            "127.0.0.1:9001".to_string(),
        );
        registry.upsert_peer(self_peer).await;

        // Should not be added
        assert_eq!(registry.alive_count().await, 0);
    }
}
