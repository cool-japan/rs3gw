# Cluster Module

The cluster module provides distributed deployment capabilities for rs3gw, enabling multi-node setups with automatic replication, failover, and load distribution.

## Overview

This module implements a multi-leader cluster architecture where:
- **Any node can accept writes** - No single point of failure
- **Automatic replication** - Changes propagated to all peers
- **Configurable consistency** - Sync, async, or quorum replication
- **Conflict resolution** - Vector clocks and last-writer-wins
- **Node discovery** - Gossip protocol with seed nodes
- **Health monitoring** - Automatic failure detection

## Architecture

### Multi-Leader Topology

```
          Client Requests
                |
      ┌─────────┴─────────┐
      ↓                   ↓
   Node A              Node B              Node C
   (Leader)            (Leader)            (Leader)
      ↓                   ↓                   ↓
   Storage             Storage             Storage
      ↑                   ↑                   ↑
      └───────────────────┴───────────────────┘
           Bidirectional Replication
```

**Benefits:**
- High availability - No single point of failure
- Write scalability - Distribute write load
- Read scalability - Serve reads from any node
- Geographic distribution - Place nodes near users
- Automatic failover - Cluster self-heals

**Trade-offs:**
- Eventual consistency (async mode)
- Conflict resolution complexity
- Network bandwidth for replication

## Components

### Cluster Configuration (`config.rs`)
Cluster-wide configuration and node settings.

**Configuration Structure:**
```rust
pub struct ClusterConfig {
    pub node_id: String,                    // Unique node identifier
    pub advertise_addr: String,             // Address for cluster communication
    pub bind_addr: String,                  // Bind address for cluster port
    pub seed_nodes: Vec<String>,            // Bootstrap seed nodes
    pub replication_mode: ReplicationMode,  // Default replication mode
    pub replication_factor: usize,          // Number of replicas
    pub heartbeat_interval: Duration,       // Health check interval
    pub heartbeat_timeout: Duration,        // Node failure timeout
    pub gossip_interval: Duration,          // Gossip protocol interval
}
```

**Replication Modes:**
- `Synchronous` - Wait for all replicas (strong consistency)
- `Asynchronous` - Acknowledge immediately (eventual consistency)
- `Quorum` - Wait for majority of replicas (tunable consistency)

**Example Configuration:**
```rust
use rs3gw::cluster::config::ClusterConfig;
use std::time::Duration;

let config = ClusterConfig {
    node_id: "node-1".to_string(),
    advertise_addr: "192.168.1.10:9001".to_string(),
    bind_addr: "0.0.0.0:9001".to_string(),
    seed_nodes: vec![
        "192.168.1.11:9001".to_string(),
        "192.168.1.12:9001".to_string(),
    ],
    replication_mode: ReplicationMode::Quorum,
    replication_factor: 3,
    heartbeat_interval: Duration::from_secs(5),
    heartbeat_timeout: Duration::from_secs(15),
    gossip_interval: Duration::from_secs(1),
};
```

**Environment Variables:**
- `RS3GW_CLUSTER_ENABLED` - Enable cluster mode (default: false)
- `RS3GW_CLUSTER_NODE_ID` - Node identifier (default: auto-generated)
- `RS3GW_CLUSTER_ADVERTISE_ADDR` - Advertised address (default: 127.0.0.1:9001)
- `RS3GW_CLUSTER_PORT` - Cluster communication port (default: 9001)
- `RS3GW_CLUSTER_SEED_NODES` - Comma-separated seed nodes
- `RS3GW_REPLICATION_MODE` - Replication mode (sync/async/quorum, default: async)
- `RS3GW_REPLICATION_FACTOR` - Number of replicas (default: 2)

### Node Management (`node.rs`)
Individual node state and lifecycle management.

**Node States:**
- `Starting` - Node is initializing
- `Joining` - Node is joining the cluster
- `Active` - Node is fully operational
- `Leaving` - Node is gracefully shutting down
- `Failed` - Node has failed health checks

**Node Information:**
```rust
pub struct NodeInfo {
    pub node_id: String,
    pub address: String,
    pub state: NodeState,
    pub last_seen: DateTime<Utc>,
    pub version: String,
    pub metadata: HashMap<String, String>,
}
```

**Node Discovery:**
1. Node starts and reads seed nodes from configuration
2. Node connects to seed nodes via gossip protocol
3. Seed nodes share cluster membership information
4. New node receives full cluster topology
5. Node advertises itself to all cluster members
6. Cluster updates membership tables

**Health Monitoring:**
- Periodic heartbeats every 5 seconds (configurable)
- Node marked as failed after 15 seconds without heartbeat
- Failed nodes removed from routing table
- Automatic replica rebuilding for failed nodes

**Usage:**
```rust
use rs3gw::cluster::node::{Node, NodeInfo, NodeState};

// Create node
let node = Node::new(config)?;

// Start node
node.start().await?;

// Get cluster members
let members = node.get_members().await;
for member in members {
    println!("Node: {} - State: {:?}", member.node_id, member.state);
}

// Leave cluster gracefully
node.leave().await?;
```

### Replication Engine (`replication.rs`)
Data replication across cluster nodes.

**Replication Event Types:**
```rust
pub enum ReplicationEvent {
    PutObject { bucket, key, etag, size, content_type, metadata },
    DeleteObject { bucket, key },
    CreateBucket { bucket },
    DeleteBucket { bucket },
    PutBucketPolicy { bucket, policy },
    DeleteBucketPolicy { bucket },
    PutBucketTagging { bucket, tags },
    DeleteBucketTagging { bucket },
}
```

**Replication Flow:**

**Synchronous Mode:**
```
Client → Node A → [Replicate to all nodes] → [Wait for all ACKs] → Response
```
- **Consistency**: Strong (all nodes have same data)
- **Latency**: High (wait for slowest node)
- **Availability**: Low (fails if any node down)

**Asynchronous Mode:**
```
Client → Node A → Response (immediate)
                ↓
         [Background replication to all nodes]
```
- **Consistency**: Eventual (nodes converge over time)
- **Latency**: Low (immediate response)
- **Availability**: High (tolerates node failures)

**Quorum Mode:**
```
Client → Node A → [Replicate to all] → [Wait for majority ACKs] → Response
```
- **Consistency**: Tunable (majority agreement)
- **Latency**: Medium (wait for majority)
- **Availability**: Medium (tolerates minority failures)

**Usage:**
```rust
use rs3gw::cluster::replication::{ReplicationEngine, ReplicationEvent};

let engine = ReplicationEngine::new(config);

// Replicate event
let event = ReplicationEvent::PutObject {
    bucket: "test".to_string(),
    key: "file.txt".to_string(),
    etag: "abc123".to_string(),
    size: 1024,
    content_type: "text/plain".to_string(),
    metadata: HashMap::new(),
};

engine.replicate(event).await?;
```

**Replication Monitoring:**
```rust
// Get replication stats
let stats = engine.get_stats().await;
println!("Events replicated: {}", stats.events_replicated);
println!("Events pending: {}", stats.events_pending);
println!("Replication lag: {:?}", stats.avg_replication_lag);
```

### Advanced Replication (`advanced_replication.rs`)
Advanced replication features for enterprise deployments.

**Features:**
- **Conflict Resolution** - Automatic conflict detection and resolution
- **Vector Clocks** - Causality tracking for distributed writes
- **Multi-Region Support** - Cross-datacenter replication
- **Bandwidth Throttling** - Limit replication bandwidth
- **Compression** - Compress replication data
- **Encryption** - Encrypt replication streams
- **Filtering** - Selective replication by prefix/tag
- **Transformation** - Transform objects during replication

**Conflict Resolution Strategies:**
```rust
pub enum ConflictResolution {
    LastWriteWins,           // Newest timestamp wins
    FirstWriteWins,          // Oldest timestamp wins
    VectorClockMerge,        // Use vector clocks
    CustomResolver(Box<dyn Fn(&Event, &Event) -> Event>),
}
```

**Vector Clock Example:**
```
Node A: {A: 1, B: 0, C: 0}  →  Write X
Node B: {A: 1, B: 1, C: 0}  →  Write Y (concurrent with X)

Conflict detected! Vector clocks show concurrent writes.
Resolution: Apply LastWriteWins or merge strategy.
```

**Multi-Region Replication:**
```rust
use rs3gw::cluster::advanced_replication::{
    AdvancedReplicationManager,
    MultiRegionConfig,
    ReplicationRegion,
};

let manager = AdvancedReplicationManager::new();

// Define regions
let us_east = ReplicationRegion {
    id: "us-east-1".to_string(),
    endpoint: "https://rs3gw-us-east.example.com".to_string(),
    priority: 1,
};

let eu_west = ReplicationRegion {
    id: "eu-west-1".to_string(),
    endpoint: "https://rs3gw-eu-west.example.com".to_string(),
    priority: 2,
};

// Configure bucket replication
manager.set_bucket_config("my-bucket", BucketReplicationConfig {
    source_region: us_east,
    destination_regions: vec![eu_west],
    filter: Some(ReplicationFilter::prefix("important/")),
    conflict_resolution: ConflictResolution::VectorClockMerge,
    compression: true,
    encryption: true,
}).await;
```

**Bandwidth Throttling:**
```rust
// Limit replication bandwidth to 10 MB/s
manager.set_bandwidth_limit(10 * 1024 * 1024); // bytes per second

// Track bandwidth usage
let tracker = manager.get_bandwidth_tracker();
println!("Current: {} bytes/s", tracker.current_rate());
println!("Peak: {} bytes/s", tracker.peak_rate());
```

**Selective Replication:**
```rust
// Replicate only objects with specific prefix
let filter = ReplicationFilter::prefix("important/");

// Replicate only objects with specific tags
let filter = ReplicationFilter::tags(vec![
    ("tier", "premium"),
    ("replicate", "true"),
]);

// Combine filters
let filter = ReplicationFilter::and(vec![
    ReplicationFilter::prefix("data/"),
    ReplicationFilter::tags(vec![("replicate", "true")]),
]);
```

## Deployment Scenarios

### High Availability Setup (3 Nodes)

```yaml
# Node 1
RS3GW_CLUSTER_ENABLED=true
RS3GW_CLUSTER_NODE_ID=node-1
RS3GW_CLUSTER_ADVERTISE_ADDR=192.168.1.10:9001
RS3GW_CLUSTER_SEED_NODES=192.168.1.11:9001,192.168.1.12:9001
RS3GW_REPLICATION_MODE=quorum
RS3GW_REPLICATION_FACTOR=3

# Node 2
RS3GW_CLUSTER_ENABLED=true
RS3GW_CLUSTER_NODE_ID=node-2
RS3GW_CLUSTER_ADVERTISE_ADDR=192.168.1.11:9001
RS3GW_CLUSTER_SEED_NODES=192.168.1.10:9001,192.168.1.12:9001
RS3GW_REPLICATION_MODE=quorum
RS3GW_REPLICATION_FACTOR=3

# Node 3
RS3GW_CLUSTER_ENABLED=true
RS3GW_CLUSTER_NODE_ID=node-3
RS3GW_CLUSTER_ADVERTISE_ADDR=192.168.1.12:9001
RS3GW_CLUSTER_SEED_NODES=192.168.1.10:9001,192.168.1.11:9001
RS3GW_REPLICATION_MODE=quorum
RS3GW_REPLICATION_FACTOR=3
```

**Benefits:**
- Survives 1 node failure
- Quorum ensures consistency
- Load distributed across nodes

### Multi-Region Setup

```
Region: US-East           Region: EU-West
┌─────────────┐          ┌─────────────┐
│   Node 1    │  ←───→   │   Node 4    │
│   Node 2    │          │   Node 5    │
│   Node 3    │          │   Node 6    │
└─────────────┘          └─────────────┘
  (Primary)                (Replica)
```

**Configuration:**
- US-East nodes use quorum replication locally
- Cross-region replication uses async mode
- Conflict resolution with vector clocks
- Bandwidth throttling for WAN links

### Single-Site Cluster (5 Nodes)

```
              Load Balancer
                    |
    ┌───────┬───────┼───────┬───────┐
    ↓       ↓       ↓       ↓       ↓
  Node1   Node2   Node3   Node4   Node5
    |       |       |       |       |
    └───────┴───────┴───────┴───────┘
         Gossip Protocol
```

**Benefits:**
- High throughput (5x single node)
- Survives 2 node failures (quorum)
- Load balancing across all nodes
- Geographic co-location for low latency

## Monitoring & Observability

### Cluster Metrics

```rust
// Get cluster health
let health = cluster.get_health().await;
println!("Total nodes: {}", health.total_nodes);
println!("Healthy nodes: {}", health.healthy_nodes);
println!("Failed nodes: {}", health.failed_nodes);

// Get replication lag
let lag = cluster.get_replication_lag().await;
println!("Average lag: {:?}", lag.average);
println!("Max lag: {:?}", lag.max);
println!("P95 lag: {:?}", lag.p95);
```

### Prometheus Metrics

```text
# Cluster membership
rs3gw_cluster_nodes_total{state="active"} 5
rs3gw_cluster_nodes_total{state="failed"} 0

# Replication stats
rs3gw_replication_events_total 123456
rs3gw_replication_lag_seconds 0.025
rs3gw_replication_bandwidth_bytes_per_sec 1048576

# Conflict resolution
rs3gw_conflicts_total{strategy="last_write_wins"} 42
rs3gw_conflicts_resolved_total 42
```

### Health Checks

```bash
# Check cluster status
curl http://localhost:9000/api/cluster/status

# Check node health
curl http://localhost:9000/api/cluster/nodes

# Check replication status
curl http://localhost:9000/api/cluster/replication
```

## Failure Scenarios

### Single Node Failure

**Impact:**
- Read requests: Served by remaining nodes
- Write requests: Depends on replication mode
  - Sync: Fails (requires all nodes)
  - Async: Succeeds (eventual consistency)
  - Quorum: Succeeds if majority available

**Recovery:**
1. Failed node detected via heartbeat timeout
2. Node marked as failed in cluster
3. Replica rebuilding initiated (if configured)
4. Requests routed to healthy nodes
5. Failed node's data redistributed

### Network Partition (Split Brain)

**Scenario:**
```
Partition 1: Node1, Node2
Partition 2: Node3, Node4, Node5
```

**Quorum Mode Behavior:**
- Partition 1: Cannot accept writes (no quorum)
- Partition 2: Continues operation (has quorum)
- Read requests: Both partitions serve from local data

**Resolution:**
1. Network partition heals
2. Partitions exchange membership information
3. Conflict detection via vector clocks
4. Conflicts resolved per configured strategy
5. Cluster resumes normal operation

### Cascading Failures

**Prevention:**
- Circuit breakers for replication
- Rate limiting during recovery
- Backpressure signaling
- Load shedding under extreme load
- Health-based request routing

## Best Practices

### Configuration

1. **Odd Number of Nodes**: Use 3, 5, or 7 nodes for quorum
2. **Replication Factor**: Set to (N/2) + 1 for fault tolerance
3. **Heartbeat Tuning**: Balance between detection speed and network overhead
4. **Seed Nodes**: Use at least 3 seed nodes for reliability

### Deployment

1. **Rolling Updates**: Update one node at a time
2. **Health Checks**: Verify node health before routing traffic
3. **Monitoring**: Track replication lag and conflict rates
4. **Backups**: Regular backups despite replication
5. **Testing**: Test failure scenarios in staging

### Performance

1. **Network**: Use dedicated network for replication
2. **Bandwidth**: Monitor and throttle if needed
3. **Compression**: Enable for WAN replication
4. **Batching**: Batch small events for efficiency

### Security

1. **TLS**: Encrypt all cluster communication
2. **Authentication**: Mutual TLS for node authentication
3. **Authorization**: Restrict cluster operations
4. **Firewall**: Isolate cluster network

## Testing

Comprehensive test coverage for cluster functionality:

```bash
# All cluster tests
cargo test --lib cluster::

# Specific component
cargo test --lib cluster::replication::

# Integration tests
cargo test --test cluster_tests
```

## Dependencies

Key dependencies for cluster functionality:

- **tokio** - Async runtime
- **tokio-stream** - Streaming support
- **serde** - Serialization
- **uuid** - Node identifiers
- **chrono** - Timestamps

## Related Documentation

- [Storage Module](../storage/README.md) - Replicated storage operations
- [Observability Module](../observability/README.md) - Cluster monitoring
- [Main README](../../README.md) - Project overview

## License

MIT OR Apache-2.0
