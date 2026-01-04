# rs3gw Configuration Examples

This directory contains example configuration files for different deployment scenarios.

## Available Configurations

### 1. Development (`development.toml`)
**Use Case**: Local development and testing

**Features**:
- No authentication (for easy testing)
- Debug logging
- Small cache
- Minimal optimization
- All features enabled for testing

**When to Use**:
- Local development
- Feature testing
- Integration development
- Quick prototyping

### 2. Production Single Node (`production-single-node.toml`)
**Use Case**: Single-server production deployment

**Features**:
- TLS encryption
- Authentication enabled
- Balanced compression (zstd:3)
- 2GB cache
- Deduplication enabled
- Audit logging
- Self-healing
- Daily backups

**When to Use**:
- Small to medium deployments
- Cost-conscious production
- Simple architecture requirements
- Up to ~1TB storage

### 3. High Availability Cluster (`production-ha-cluster.toml`)
**Use Case**: Multi-node production with replication

**Features**:
- 3-node cluster with quorum replication
- Higher concurrency limits
- Distributed tracing
- Advanced anomaly detection
- S3 backup integration
- Load balancer health checks
- Resource management

**When to Use**:
- Mission-critical deployments
- High availability requirements (99.99%+)
- Multi-datacenter deployments
- Large-scale storage (5TB+)

### 4. High Performance (`high-performance.toml`)
**Use Case**: Maximum throughput and low latency

**Features**:
- io_uring support (Linux)
- Minimal logging
- No deduplication (for speed)
- Large cache (16GB)
- Aggressive zero-copy
- jemalloc allocator
- No audit overhead

**When to Use**:
- High-throughput workloads (10k+ RPS)
- Large file transfers
- Media streaming
- Real-time processing
- Performance-critical applications

### 5. Compliance/Audit Heavy (`compliance-audit-heavy.toml`)
**Use Case**: Regulated industries (healthcare, finance, government)

**Features**:
- Comprehensive audit logging
- Encryption at rest and in transit
- 2-year log retention
- WORM compliance
- Data residency controls
- Malware scanning
- SIEM integration
- SOC2/HIPAA/GDPR compliance

**When to Use**:
- Healthcare (HIPAA)
- Finance (SOC2, PCI-DSS)
- Government (FedRAMP)
- GDPR-regulated data
- Legal/compliance requirements

## Usage

### Basic Usage

```bash
# Copy the appropriate configuration
cp examples/configurations/production-single-node.toml rs3gw.toml

# Edit with your specific values
vi rs3gw.toml

# Run rs3gw with the configuration
rs3gw --config rs3gw.toml
```

### Environment Variables

All configurations support environment variable substitution:

```bash
# Set credentials via environment
export RS3GW_ACCESS_KEY="your-access-key"
export RS3GW_SECRET_KEY="your-secret-key"
export RS3GW_NODE_ID="node-1"

# Run rs3gw
rs3gw --config rs3gw.toml
```

### Docker Usage

```bash
# Mount configuration file
docker run -v $(pwd)/rs3gw.toml:/etc/rs3gw/rs3gw.toml \
  -p 9000:9000 \
  rs3gw:latest --config /etc/rs3gw/rs3gw.toml
```

### Kubernetes Usage

```bash
# Create ConfigMap
kubectl create configmap rs3gw-config \
  --from-file=rs3gw.toml=production-ha-cluster.toml

# Reference in deployment
# See docs/production_deployment.md for full example
```

## Configuration Sections

### Core Sections

- `[server]` - HTTP server settings
- `[storage]` - Storage engine configuration
- `[auth]` - Authentication settings
- `[tls]` - TLS/HTTPS configuration

### Performance Sections

- `[cache]` - Object caching
- `[dedup]` - Deduplication
- `[zerocopy]` - Zero-copy optimizations
- `[throttle]` - Rate limiting
- `[performance]` - Advanced tuning

### High Availability

- `[cluster]` - Cluster configuration
- `[replication]` - Replication settings

### Observability

- `[logging]` - Log configuration
- `[metrics]` - Prometheus metrics
- `[observability]` - Profiling, tracing, anomaly detection
- `[audit]` - Audit logging

### Data Management

- `[quota]` - Storage quotas
- `[versioning]` - Object versioning
- `[lifecycle]` - Lifecycle policies
- `[backup]` - Backup configuration

### Compliance

- `[encryption]` - Encryption at rest
- `[object_lock]` - WORM compliance
- `[access_control]` - ABAC, IP filtering
- `[data_residency]` - Geographic restrictions

## Customization Guide

### Adjusting for Your Hardware

**CPU Cores**:
```toml
[performance]
min_threads = <num_cores>
max_threads = <num_cores * 4>
```

**Memory**:
```toml
[cache]
max_size_mb = <10-20% of RAM>

[performance]
read_buffer_size = <based on workload>
```

**Storage**:
```toml
[storage]
root = "/path/to/fast/storage"  # Use NVMe/SSD
compression = "lz4"  # or "zstd:3" for balance
```

**Network**:
```toml
[throttle]
upload_mbps = <your_bandwidth * 0.8>
download_mbps = <your_bandwidth * 0.8>
```

### Tuning for Workload

**Small Files (<1MB)**:
```toml
[zerocopy]
direct_io_threshold = 4096  # Lower threshold
```

**Large Files (>100MB)**:
```toml
[cache]
max_size_mb = 512  # Smaller cache
ttl = 60  # Shorter TTL

[performance]
read_buffer_size = 2097152  # 2MB buffers
```

**Read-Heavy**:
```toml
[cache]
max_size_mb = 8192  # Large cache
ttl = 1800  # Long TTL

[dedup]
enabled = true  # Save storage
```

**Write-Heavy**:
```toml
[dedup]
enabled = false  # Skip dedup overhead

[cache]
ttl = 60  # Short TTL
```

## Best Practices

1. **Start Conservative**: Begin with a template and adjust based on monitoring
2. **Monitor Metrics**: Use Prometheus/Grafana to track performance
3. **Load Test**: Test with your expected workload before production
4. **Incremental Changes**: Change one setting at a time
5. **Document Changes**: Track why you deviated from defaults
6. **Version Control**: Keep configurations in git
7. **Secrets Management**: Use environment variables or secrets manager
8. **Regular Review**: Revisit configuration as workload evolves

## Troubleshooting

### High CPU Usage
- Reduce `max_concurrent`
- Lower compression level
- Disable deduplication
- Check for anomaly detection overhead

### High Memory Usage
- Reduce `cache.max_size_mb`
- Lower `max_concurrent`
- Reduce thread pool sizes

### Slow Writes
- Disable `dedup`
- Use `lz4` compression
- Disable `direct_io`
- Increase `write_buffer_size`

### Slow Reads
- Increase `cache.max_size_mb`
- Enable `zerocopy.splice`
- Increase `read_buffer_size`

### Network Congestion
- Enable `throttle`
- Reduce `max_concurrent`
- Enable compression

## Support

For more information:
- Documentation: `docs/`
- Performance Tuning: `docs/performance_tuning.md`
- Production Deployment: `docs/production_deployment.md`
- Issue Tracker: https://github.com/cool-japan/rs3gw/issues
