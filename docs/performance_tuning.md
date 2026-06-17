# rs3gw Performance Tuning Guide

This guide provides comprehensive recommendations for optimizing rs3gw performance across different workloads and deployment scenarios.

## Table of Contents

- [Hardware Requirements](#hardware-requirements)
- [Environment Variables](#environment-variables)
- [Compression Tuning](#compression-tuning)
- [Caching Configuration](#caching-configuration)
- [Network Optimization](#network-optimization)
- [Storage Optimization](#storage-optimization)
- [Concurrent Operations](#concurrent-operations)
- [Memory Management](#memory-management)
- [I/O Optimization](#io-optimization)
- [Cluster Configuration](#cluster-configuration)
- [Monitoring & Profiling](#monitoring--profiling)
- [Workload-Specific Tuning](#workload-specific-tuning)

---

## Hardware Requirements

### Minimum Requirements
- **CPU**: 2 cores
- **RAM**: 4 GB
- **Disk**: 20 GB SSD
- **Network**: 100 Mbps

### Recommended for Production
- **CPU**: 8+ cores (16+ for high-throughput scenarios)
- **RAM**: 32 GB (64+ GB for large caching workloads)
- **Disk**: NVMe SSD with high IOPS (10,000+ IOPS recommended)
- **Network**: 10 Gbps (25/40/100 Gbps for HPC workloads)

### High-Performance Configuration
- **CPU**: 32+ cores with AVX2/AVX-512 support
- **RAM**: 128+ GB
- **Disk**: Multiple NVMe SSDs in RAID configuration
- **Network**: 100 Gbps InfiniBand or RoCE for HPC/AI workloads

---

## Environment Variables

### Core Configuration

```bash
# Server binding
export RS3GW_BIND_ADDR="0.0.0.0:9000"

# Storage location (use fast SSD/NVMe)
export RS3GW_STORAGE_ROOT="/mnt/nvme/rs3gw/data"

# Compression (see Compression Tuning section)
export RS3GW_COMPRESSION="zstd:3"

# Request timeout (adjust based on object sizes)
export RS3GW_REQUEST_TIMEOUT="600"  # 10 minutes for large files
```

### Performance-Critical Settings

```bash
# Concurrent request handling
export RS3GW_MAX_CONCURRENT="1000"  # Increase for high-throughput

# Connection pooling
export RS3GW_POOL_MAX_IDLE="64"           # More connections for high load
export RS3GW_POOL_IDLE_TIMEOUT="120"      # Keep connections longer
export RS3GW_CONNECT_TIMEOUT="10"         # Fast connection establishment
export RS3GW_CLIENT_TIMEOUT="600"         # Longer for large transfers
```

### Caching Configuration

```bash
# Enable and configure cache
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="4096"     # 4 GB cache
export RS3GW_CACHE_MAX_OBJECTS="50000"    # Max cached objects
export RS3GW_CACHE_TTL="600"              # 10 minute TTL
```

### Deduplication Settings

```bash
# Enable deduplication
export RS3GW_DEDUP_ENABLED="true"
export RS3GW_DEDUP_BLOCK_SIZE="65536"     # 64 KB blocks
export RS3GW_DEDUP_ALGORITHM="content-defined"  # Better dedup ratio
export RS3GW_DEDUP_MIN_SIZE="131072"      # 128 KB minimum
```

### Zero-Copy Optimizations (Linux)

```bash
# Direct I/O for large objects
export RS3GW_ZEROCOPY_DIRECT_IO="true"
export RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD="1048576"  # 1 MB

# Splice/sendfile for kernel-level zero-copy
export RS3GW_ZEROCOPY_SPLICE="true"

# Memory-mapped metadata
export RS3GW_ZEROCOPY_MMAP="true"
```

### Resource Management

```bash
# Auto-scaling thread pool
export RS3GW_MIN_THREADS="8"
export RS3GW_MAX_THREADS="128"            # num_cpus * 4
export RS3GW_TARGET_CPU="75"              # Target 75% CPU utilization

# Adaptive rate limiting
export RS3GW_ADAPTIVE_RATE_LIMIT="true"
export RS3GW_INITIAL_RATE_LIMIT="5000"    # 5000 req/sec
export RS3GW_MIN_RATE_LIMIT="1000"
export RS3GW_MAX_RATE_LIMIT="50000"

# Memory pressure management
export RS3GW_MEMORY_THRESHOLD="85"        # Start backpressure at 85%

# Load shedding
export RS3GW_LOAD_SHEDDING_THRESHOLD="95" # Shed load at 95% capacity
```

---

## Compression Tuning

### Compression Algorithms

**Zstd (Recommended for most workloads)**
```bash
# Balanced compression (default)
export RS3GW_COMPRESSION="zstd:3"   # Level 3, ~250 MB/s compression

# Fast compression (high-throughput)
export RS3GW_COMPRESSION="zstd:1"   # Level 1, ~500 MB/s compression

# High compression (storage-optimized)
export RS3GW_COMPRESSION="zstd:9"   # Level 9, ~100 MB/s compression
```

**LZ4 (Maximum speed)**
```bash
# Ultra-fast compression for hot data
export RS3GW_COMPRESSION="lz4"      # ~1-2 GB/s compression speed
```

**No Compression**
```bash
# For already-compressed data (images, video, archives)
export RS3GW_COMPRESSION="none"
```

### Compression Selection Guide

| Workload | Recommended Setting | Rationale |
|----------|-------------------|-----------|
| Text/Logs | `zstd:5` | Good compression ratio, moderate speed |
| JSON/XML | `zstd:3` | Balance of speed and compression |
| Binary Data | `zstd:1` or `lz4` | Fast compression, some benefit |
| Images/Video | `none` | Already compressed, skip overhead |
| Archives | `none` | Already compressed |
| Cold Storage | `zstd:9` | Maximize space savings |
| Hot Data | `lz4` | Minimize latency |

### Object-Size Considerations (Threshold)

Compression is **off by default** (`RS3GW_COMPRESSION=none`). When enabled it is
applied to every stored object — there is no built-in minimum-size gate — so the
trade-off below matters most for workloads dominated by tiny objects.

The `bench_compression_threshold` group in `benches/compression_benchmarks.rs`
sweeps object sizes from 512 B to 8 KiB, comparing `none` vs `zstd:1` vs `lz4`;
`bench_compression_ratio` sweeps payload entropy. Reproduce with:

```bash
cargo bench --bench compression_benchmarks
```

Guidance derived from the sweep:

- **Very small objects (≲ 1 KiB):** compression rarely pays off — framing/CPU
  overhead dominates and the ratio gain is marginal. Prefer `none`, or `lz4` if
  the payloads are highly compressible (e.g. small JSON).
- **Small–medium objects (1 KiB–1 MiB):** `zstd:1`–`zstd:3` is the sweet spot for
  mixed text/JSON/XML; `lz4` when latency is the priority.
- **Large objects (≳ 1 MiB):** `zstd:3` (default) balances ratio and throughput;
  `zstd:9` for cold storage where space dominates CPU.
- **Already-compressed payloads** (images, video, archives): always `none`.

If a per-object size threshold is needed, gate it at the application/bucket
policy layer; the storage engine intentionally keeps the compression decision a
single global mode for predictability.

---

## Caching Configuration

### ML-Based Smart Caching

The ML cache automatically learns access patterns and optimizes caching decisions.

```bash
# Enable smart caching
export RS3GW_CACHE_ENABLED="true"

# Cache sizing (adjust based on available RAM)
export RS3GW_CACHE_MAX_SIZE_MB="8192"     # 8 GB for 32 GB RAM system
export RS3GW_CACHE_MAX_OBJECTS="100000"   # Adjust based on object sizes

# TTL configuration
export RS3GW_CACHE_TTL="300"              # 5 minutes default TTL
```

### Cache Sizing Guidelines

| Available RAM | Recommended Cache Size | Max Objects |
|--------------|----------------------|-------------|
| 8 GB | 1-2 GB | 10,000 |
| 16 GB | 4 GB | 25,000 |
| 32 GB | 8 GB | 50,000 |
| 64 GB | 16-24 GB | 100,000 |
| 128 GB | 32-64 GB | 200,000+ |

### Cache Performance Tips

1. **Warm the cache on startup** - Use cache warming strategies
2. **Monitor cache hit rates** - Aim for >80% hit rate for hot data
3. **Adjust TTL based on update frequency** - Longer TTL for static data
4. **Size appropriately** - Don't use all available RAM for cache

---

## Network Optimization

### TCP Tuning (Linux)

```bash
# Edit /etc/sysctl.conf
net.core.rmem_max = 134217728          # 128 MB receive buffer
net.core.wmem_max = 134217728          # 128 MB send buffer
net.ipv4.tcp_rmem = 4096 87380 67108864   # TCP receive buffer
net.ipv4.tcp_wmem = 4096 65536 67108864   # TCP send buffer
net.core.netdev_max_backlog = 5000      # Backlog queue size
net.ipv4.tcp_max_syn_backlog = 8192     # SYN backlog
net.ipv4.tcp_slow_start_after_idle = 0  # Disable slow start after idle

# Apply settings
sudo sysctl -p
```

### Connection Pooling

```bash
# HTTP/2 multiplexing
export RS3GW_POOL_MAX_IDLE="128"       # Keep many connections ready
export RS3GW_POOL_IDLE_TIMEOUT="300"   # 5 minute timeout
```

### Bandwidth Throttling

```bash
# For QoS scenarios
export RS3GW_THROTTLE_ENABLED="true"
export RS3GW_THROTTLE_RPS="10000"              # 10k req/sec
export RS3GW_THROTTLE_UPLOAD_MBPS="1000"       # 1 Gbps upload
export RS3GW_THROTTLE_DOWNLOAD_MBPS="2000"     # 2 Gbps download
```

---

## Storage Optimization

### Filesystem Selection

**Best Choices:**
- **ext4** - Reliable, well-tested (recommended for most users)
- **XFS** - Better for large files and high concurrency
- **ZFS** - Built-in compression, snapshots, and checksumming
- **Btrfs** - COW filesystem with compression support

**Configuration Examples:**

```bash
# ext4 with optimal mount options
sudo mount -o noatime,nodiratime,data=writeback /dev/nvme0n1 /mnt/rs3gw

# XFS with large block size
sudo mkfs.xfs -b size=4096 -d agcount=8 /dev/nvme0n1
sudo mount -o noatime,nodiratime,logbufs=8,logbsize=256k /dev/nvme0n1 /mnt/rs3gw

# ZFS with compression
sudo zfs create -o compression=lz4 -o atime=off tank/rs3gw
```

### I/O Scheduler

```bash
# For SSDs/NVMe (use none or mq-deadline)
echo none > /sys/block/nvme0n1/queue/scheduler

# For HDDs (use mq-deadline or bfq)
echo mq-deadline > /sys/block/sda/queue/scheduler
```

### RAID Configuration

```bash
# RAID 0 for maximum performance (no redundancy)
sudo mdadm --create /dev/md0 --level=0 --raid-devices=4 /dev/nvme{0..3}n1

# RAID 10 for balance of performance and redundancy
sudo mdadm --create /dev/md0 --level=10 --raid-devices=4 /dev/nvme{0..3}n1

# RAID 6 for maximum redundancy (slower writes)
sudo mdadm --create /dev/md0 --level=6 --raid-devices=6 /dev/nvme{0..5}n1
```

---

## Concurrent Operations

### Tuning Concurrency Limits

```bash
# High-throughput configuration
export RS3GW_MAX_CONCURRENT="2000"     # Allow 2000 concurrent requests

# CPU-bound workloads
export RS3GW_MIN_THREADS="16"
export RS3GW_MAX_THREADS="64"

# I/O-bound workloads
export RS3GW_MIN_THREADS="32"
export RS3GW_MAX_THREADS="256"
```

### Tokio Runtime Tuning

```bash
# Set worker threads (default: num_cpus)
export TOKIO_WORKER_THREADS="32"       # Explicit worker count
```

---

## Memory Management

### Memory Allocation

```bash
# Use jemalloc for better performance
export LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so.2
```

### Huge Pages (Linux)

```bash
# Enable transparent huge pages
echo always > /sys/kernel/mm/transparent_hugepage/enabled
echo always > /sys/kernel/mm/transparent_hugepage/defrag

# Or use explicit huge pages
sudo sysctl -w vm.nr_hugepages=1024    # Allocate 2 GB (2048 KB pages)
```

---

## I/O Optimization

### io_uring Support (Linux 5.11+)

Build with io_uring for maximum I/O performance:

```bash
cargo build --release --features io_uring
```

**Performance Gains:**
- Up to 60% improvement for I/O-bound workloads
- Reduced CPU usage for file operations
- Lower syscall overhead

### Direct I/O

```bash
# Enable direct I/O for large files
export RS3GW_ZEROCOPY_DIRECT_IO="true"
export RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD="1048576"  # 1 MB

# Benefits:
# - Bypass page cache for large sequential I/O
# - Reduce memory pressure
# - Better predictable latency
```

---

## Cluster Configuration

### Multi-Node Deployment

```bash
# Node configuration
export RS3GW_CLUSTER_ENABLED="true"
export RS3GW_CLUSTER_NODE_ID="node-01"
export RS3GW_CLUSTER_ADVERTISE_ADDR="10.0.1.10:9001"
export RS3GW_CLUSTER_PORT="9001"

# Seed nodes for discovery
export RS3GW_CLUSTER_SEED_NODES="10.0.1.10:9001,10.0.1.11:9001,10.0.1.12:9001"

# Replication configuration
export RS3GW_REPLICATION_MODE="quorum"    # quorum, sync, or async
export RS3GW_REPLICATION_FACTOR="3"       # 3 replicas
```

### Replication Mode Selection

| Mode | Consistency | Performance | Use Case |
|------|------------|-------------|----------|
| `sync` | Strong | Slower | Critical data, compliance |
| `async` | Eventual | Fastest | Analytics, logs |
| `quorum` | Strong | Balanced | General-purpose (recommended) |

---

## Monitoring & Profiling

### Prometheus Metrics

```bash
# Enable comprehensive metrics
# Metrics endpoint: http://localhost:9000/metrics
```

**Key Metrics to Monitor:**
- `rs3gw_requests_total` - Request throughput
- `rs3gw_request_duration_seconds` - Latency distribution
- `rs3gw_storage_bytes_total` - Storage utilization
- `rs3gw_cache_hit_ratio` - Cache effectiveness
- `rs3gw_replication_lag_seconds` - Replication health

### OpenTelemetry Tracing

```bash
# Configure OTLP endpoint for distributed tracing
export OTEL_EXPORTER_OTLP_ENDPOINT="http://jaeger:4317"
export OTEL_TRACES_SAMPLER_ARG="0.1"      # 10% sampling rate
```

### Continuous Profiling

```bash
# Enable profiling
export RS3GW_PROFILING_ENABLED="true"
export RS3GW_PROFILING_INTERVAL="60"      # Profile every 60 seconds
export RS3GW_PROFILING_RETENTION="100"    # Keep 100 profiles
```

---

## Workload-Specific Tuning

### Small Files (< 1 MB)

```bash
# Optimize for throughput
export RS3GW_COMPRESSION="lz4"            # Fast compression
export RS3GW_CACHE_ENABLED="true"         # Cache aggressively
export RS3GW_CACHE_MAX_OBJECTS="200000"   # Many small objects
export RS3GW_MAX_CONCURRENT="2000"        # High concurrency
export RS3GW_DEDUP_ENABLED="false"        # Skip dedup overhead
export RS3GW_ZEROCOPY_DIRECT_IO="false"   # Page cache helpful
```

### Large Files (> 100 MB)

```bash
# Optimize for streaming
export RS3GW_COMPRESSION="none"           # Skip compression
export RS3GW_CACHE_ENABLED="false"        # Don't cache large files
export RS3GW_REQUEST_TIMEOUT="3600"       # 1 hour timeout
export RS3GW_MAX_CONCURRENT="100"         # Lower concurrency
export RS3GW_ZEROCOPY_DIRECT_IO="true"    # Direct I/O bypass
export RS3GW_ZEROCOPY_SPLICE="true"       # Zero-copy transfers
```

### Read-Heavy Workload

```bash
# Maximize read performance
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="16384"    # Large cache (16 GB)
export RS3GW_CACHE_TTL="1800"             # Long TTL (30 min)
export RS3GW_MAX_CONCURRENT="1000"        # High read concurrency
export RS3GW_COMPRESSION="lz4"            # Fast decompression
```

### Write-Heavy Workload

```bash
# Optimize for writes
export RS3GW_COMPRESSION="zstd:1"         # Fast compression
export RS3GW_CACHE_ENABLED="false"        # Skip cache on writes
export RS3GW_MAX_CONCURRENT="500"         # Moderate concurrency
export RS3GW_DEDUP_ENABLED="true"         # Save storage
export RS3GW_ZEROCOPY_DIRECT_IO="true"    # Direct writes
```

### Analytics Workload (S3 Select)

```bash
# Optimize for query performance
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="8192"
export RS3GW_COMPRESSION="zstd:3"         # Good compression for data
export RS3GW_MAX_CONCURRENT="200"         # Parallel queries

# Use Parquet format for best performance
# Enable query optimization
```

### HPC/AI Workload

```bash
# Maximum performance configuration
export RS3GW_COMPRESSION="none"           # Zero compression overhead
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="65536"    # 64 GB cache
export RS3GW_MAX_CONCURRENT="500"
export RS3GW_ZEROCOPY_DIRECT_IO="true"
export RS3GW_ZEROCOPY_SPLICE="true"
export RS3GW_POOL_MAX_IDLE="256"
export RS3GW_MIN_THREADS="32"
export RS3GW_MAX_THREADS="128"

# Build with io_uring
cargo build --release --features io_uring

# Use 100 Gbps network
# Use NVMe RAID 0 for storage
# Enable huge pages
```

---

## Performance Benchmarking

### Built-in Benchmarks

```bash
# Run storage benchmarks
cargo bench --bench storage_benchmarks

# Run S3 API benchmarks
cargo bench --bench s3_api_benchmarks

# Run load testing benchmarks
cargo bench --bench load_testing_benchmarks

# Run gRPC vs REST comparison
cargo bench --bench grpc_vs_rest_benchmarks
```

### External Benchmarking Tools

**S3 Performance Testing:**
```bash
# Using s3-benchmark
s3-benchmark -bucket=test -objects=1000 -size=1M -threads=32

# Using COSBench
cosbench-start.sh workload.xml

# Using MinIO warp
warp get --obj.size=1MiB --concurrent=32 --duration=60s
```

---

## Troubleshooting Performance Issues

### High CPU Usage

**Diagnosis:**
```bash
# Check profiling data
rs3ctl metrics get | grep cpu

# Use perf on Linux
sudo perf top -p $(pidof rs3gw)
```

**Solutions:**
- Reduce `RS3GW_MAX_CONCURRENT`
- Enable compression to reduce I/O
- Scale horizontally with clustering

### High Memory Usage

**Diagnosis:**
```bash
# Check memory metrics
rs3ctl metrics get | grep memory
```

**Solutions:**
- Reduce `RS3GW_CACHE_MAX_SIZE_MB`
- Lower `RS3GW_CACHE_MAX_OBJECTS`
- Enable memory pressure management

### High I/O Wait

**Diagnosis:**
```bash
# Check I/O stats
iostat -x 1

# Check disk latency
sudo iotop
```

**Solutions:**
- Use faster storage (NVMe SSD)
- Enable `io_uring` feature
- Use RAID for better IOPS
- Enable direct I/O

### Network Bottleneck

**Diagnosis:**
```bash
# Check network throughput
iftop -i eth0

# Check connection stats
ss -s
```

**Solutions:**
- Increase `RS3GW_POOL_MAX_IDLE`
- Use 10 Gbps+ network
- Enable TCP tuning
- Use multiple network interfaces

---

## Recommended Configurations

### Development

```bash
export RS3GW_BIND_ADDR="127.0.0.1:9000"
export RS3GW_STORAGE_ROOT="./data"
export RS3GW_COMPRESSION="zstd:1"
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="512"
export RS3GW_MAX_CONCURRENT="100"
```

### Production (General Purpose)

```bash
export RS3GW_BIND_ADDR="0.0.0.0:9000"
export RS3GW_STORAGE_ROOT="/mnt/nvme/rs3gw/data"
export RS3GW_COMPRESSION="zstd:3"
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="4096"
export RS3GW_CACHE_MAX_OBJECTS="50000"
export RS3GW_MAX_CONCURRENT="1000"
export RS3GW_POOL_MAX_IDLE="64"
export RS3GW_DEDUP_ENABLED="true"
export RS3GW_ZEROCOPY_DIRECT_IO="true"
export RS3GW_ADAPTIVE_RATE_LIMIT="true"
```

### Production (High Performance)

```bash
export RS3GW_BIND_ADDR="0.0.0.0:9000"
export RS3GW_STORAGE_ROOT="/mnt/raid/rs3gw/data"
export RS3GW_COMPRESSION="lz4"
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="16384"
export RS3GW_CACHE_MAX_OBJECTS="200000"
export RS3GW_MAX_CONCURRENT="2000"
export RS3GW_POOL_MAX_IDLE="256"
export RS3GW_MIN_THREADS="32"
export RS3GW_MAX_THREADS="256"
export RS3GW_ZEROCOPY_DIRECT_IO="true"
export RS3GW_ZEROCOPY_SPLICE="true"
export RS3GW_ZEROCOPY_MMAP="true"
export RS3GW_ADAPTIVE_RATE_LIMIT="true"
export RS3GW_INITIAL_RATE_LIMIT="10000"

# Build with io_uring
cargo build --release --features io_uring
```

---

## Summary

Performance tuning rs3gw involves:

1. **Hardware**: Use fast storage (NVMe SSD), sufficient RAM, and fast network
2. **Compression**: Choose appropriate algorithm for your workload
3. **Caching**: Size cache appropriately and monitor hit rates
4. **Concurrency**: Balance thread count with system resources
5. **I/O**: Enable zero-copy optimizations and io_uring on Linux
6. **Monitoring**: Track metrics and use profiling to identify bottlenecks
7. **Workload-Specific**: Tune for your specific use case

Always benchmark with your actual workload and iterate on configuration settings.

---

**Last Updated:** 2025-12-30
**Version:** 4.0.0
