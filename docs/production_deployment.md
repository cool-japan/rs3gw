# rs3gw Production Deployment Guide

This guide covers production sizing, configuration, TLS, compression, monitoring, and troubleshooting for rs3gw -- an S3-compatible object storage gateway for AI/HPC workloads.

## Table of Contents

- [Production Sizing Guidance](#1-production-sizing-guidance)
- [Data Directory Layout](#2-data-directory-layout)
- [Environment Variables Reference](#3-environment-variables-reference)
- [Configuration File](#4-configuration-file)
- [TLS Configuration](#5-tls-configuration)
- [Compression Configuration](#6-compression-configuration)
- [Troubleshooting](#7-troubleshooting)
- [Monitoring](#8-monitoring)
- [Deployment Examples](#9-deployment-examples)

---

## 1. Production Sizing Guidance

| Scale | CPU | RAM | Disk | Concurrent Requests | Notes |
|-------|-----|-----|------|---------------------|-------|
| Small (< 1TB) | 2 cores | 4GB | SSD recommended | 100 | Dev/test, small teams |
| Medium (1-10TB) | 4-8 cores | 8-16GB | NVMe SSD | 500 | Production workloads |
| Large (> 10TB) | 16+ cores | 32GB+ | NVMe SSD RAID | 2000+ | HPC/AI pipelines |

**Key considerations:**

- **CPU**: rs3gw is async (tokio-based). Worker threads auto-scale between `RS3GW_MIN_THREADS` (default 4) and `RS3GW_MAX_THREADS` (default `num_cpus * 4`). Compression (zstd/lz4) adds CPU overhead proportional to write throughput.
- **RAM**: The in-memory object cache (`RS3GW_CACHE_MAX_SIZE_MB`, default 256MB), S3 Select cache (`RS3GW_SELECT_CACHE_MAX_MEMORY_MB`, default 100MB), and deduplication index all consume memory. Budget at least 2x the sum of these caches plus OS overhead.
- **Disk**: Random-read latency dominates GET performance. NVMe SSDs are strongly recommended for medium and large deployments. Use RAID-10 or ZFS mirrors for data safety at the large scale.
- **Network**: At the large scale, 10GbE or faster is recommended. Enable zero-copy optimizations (`RS3GW_ZEROCOPY_SPLICE=true`) to minimize kernel copies.
- **File descriptors**: Set `LimitNOFILE=65536` or higher for high-concurrency deployments.

---

## 2. Data Directory Layout

When rs3gw creates a bucket, it initializes the following directory structure under `<storage_root>/<bucket>/`:

```
<storage_root>/                        # RS3GW_STORAGE_ROOT (default: ./data)
├── <bucket>/
│   ├── objects/                        # Object data files (may be compressed)
│   ├── metadata/                       # JSON metadata per object (ETag, content-type, user metadata)
│   ├── sci_metadata/                   # HPC/AI scientific metadata
│   ├── tags/                           # Object tagging (key-value JSON per object)
│   ├── multipart/                      # In-progress multipart upload parts and manifests
│   ├── bucket_tags.json                # Bucket-level tags
│   └── bucket_policy.json              # Bucket policy document
├── preprocessing/                      # Data preprocessing pipeline state
├── training/                           # ML training manager state
└── dedup/                              # Deduplication block store and index (when enabled)
```

**Notes:**
- Object data in `objects/` may be compressed transparently when `RS3GW_COMPRESSION` is set. The compression algorithm is recorded in the per-object metadata.
- The `multipart/` directory holds partial uploads. Abandoned uploads are garbage-collected after `RS3GW_MULTIPART_RETENTION_HOURS` (default: 168 hours = 7 days).
- When versioning is enabled on a bucket, version metadata is tracked per-object in the metadata directory.
- When `RS3GW_FSYNC=true`, every object write calls `sync_all()` before the final rename, ensuring data durability at the cost of write throughput.

---

## 3. Environment Variables Reference

All configuration can be supplied via environment variables with the `RS3GW_` prefix. Environment variables override values from the config file.

### Core Server

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_BIND_ADDR` | `0.0.0.0:9000` | Socket address to bind the HTTP server |
| `RS3GW_STORAGE_ROOT` | `./data` | Root directory for all bucket data |
| `RS3GW_DEFAULT_BUCKET` | `default` | Default bucket name for single-bucket mode |
| `RS3GW_REQUEST_TIMEOUT` | `300` | Request timeout in seconds (0 = no timeout) |
| `RS3GW_MAX_CONCURRENT` | `0` | Maximum concurrent requests (0 = unlimited) |
| `RS3GW_FSYNC` | `false` | Call `sync_all()` on object files before rename; safer but slower |
| `RS3GW_REGION` | `us-east-1` | AWS region identifier for SigV4 authentication |
| `RS3GW_CHECKSUM_VALIDATION` | `false` | Validate checksums on read operations |

### Authentication

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_ACCESS_KEY` | *(empty)* | S3 access key. When empty, authentication is disabled (passthrough mode). |
| `RS3GW_SECRET_KEY` | *(empty)* | S3 secret key. Both access and secret keys must be set to enable SigV4 auth. |

### Compression

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_COMPRESSION` | `none` | Compression mode: `none`, `zstd`, `zstd:<level>` (1-22), or `lz4`. Aliases: `on`/`true`/`1` = zstd level 3; `off`/`false`/`0` = none. |

### TLS

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_TLS_CERT` | *(none)* | Path to TLS certificate file (PEM format) |
| `RS3GW_TLS_KEY` | *(none)* | Path to TLS private key file (PEM format) |

### Connection Pool (outbound HTTP client)

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_POOL_MAX_IDLE` | `32` | Maximum idle connections per host |
| `RS3GW_POOL_IDLE_TIMEOUT` | `90` | Idle connection timeout in seconds |
| `RS3GW_CONNECT_TIMEOUT` | `30` | Connection timeout in seconds |
| `RS3GW_CLIENT_TIMEOUT` | `300` | Outbound request timeout in seconds |

### Object Cache

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_CACHE_ENABLED` | `true` | Enable in-memory object cache |
| `RS3GW_CACHE_MAX_SIZE_MB` | `256` | Maximum cache size in megabytes |
| `RS3GW_CACHE_MAX_OBJECTS` | `10000` | Maximum number of cached objects |
| `RS3GW_CACHE_TTL` | `300` | Cache entry TTL in seconds |

### S3 Select Cache

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_SELECT_CACHE_ENABLED` | `true` | Enable S3 Select query result caching |
| `RS3GW_SELECT_CACHE_MAX_ENTRIES` | `1000` | Maximum number of cached query results |
| `RS3GW_SELECT_CACHE_MAX_MEMORY_MB` | `100` | Maximum memory for Select cache in MB |
| `RS3GW_SELECT_CACHE_TTL` | `3600` | Default TTL for cached results in seconds |

### Throttling

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_THROTTLE_ENABLED` | `false` | Enable request throttling |
| `RS3GW_THROTTLE_RPS` | `0` | Max requests per second per client (0 = unlimited) |
| `RS3GW_THROTTLE_UPLOAD_MBPS` | `0` | Upload bandwidth limit in MB/s (0 = unlimited) |
| `RS3GW_THROTTLE_DOWNLOAD_MBPS` | `0` | Download bandwidth limit in MB/s (0 = unlimited) |

### Quotas

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_QUOTA_ENABLED` | `false` | Enable storage quotas |
| `RS3GW_QUOTA_MAX_STORAGE_GB` | `0` | Default max storage per bucket in GB (0 = unlimited) |
| `RS3GW_QUOTA_MAX_OBJECTS` | `0` | Default max objects per bucket (0 = unlimited) |

### Deduplication

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_DEDUP_ENABLED` | `true` | Enable block-level deduplication |
| `RS3GW_DEDUP_BLOCK_SIZE` | `65536` | Block size in bytes (64KB default) |
| `RS3GW_DEDUP_ALGORITHM` | `fixed` | Chunking algorithm: `fixed` or `content-defined` / `cdc` |
| `RS3GW_DEDUP_MIN_SIZE` | `131072` | Minimum object size for dedup in bytes (128KB); smaller objects skip dedup |

### Zero-Copy Optimizations

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_ZEROCOPY_DIRECT_IO` | `true` | Enable direct I/O for large reads |
| `RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD` | `1048576` | Minimum file size in bytes to use direct I/O (1MB) |
| `RS3GW_ZEROCOPY_SPLICE` | `true` | Enable splice(2) for zero-copy transfers (Linux) |
| `RS3GW_ZEROCOPY_MMAP` | `true` | Enable mmap for metadata reads |

### Multipart Uploads

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_MULTIPART_RETENTION_HOURS` | `168` | Hours before abandoned multipart uploads are garbage collected (7 days) |

### Cluster / Replication

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_CLUSTER_ENABLED` | `false` | Enable cluster mode |
| `RS3GW_CLUSTER_NODE_ID` | *(auto)* | Unique node identifier |
| `RS3GW_CLUSTER_ADVERTISE_ADDR` | `127.0.0.1:9001` | Address this node advertises to peers |
| `RS3GW_CLUSTER_PORT` | `9001` | Cluster gossip port |
| `RS3GW_CLUSTER_SEED_NODES` | *(empty)* | Comma-separated list of seed node addresses |
| `RS3GW_REPLICATION_MODE` | `async` | Replication mode: `async`, `sync` (synchronous), or `quorum` |
| `RS3GW_REPLICATION_FACTOR` | `2` | Number of replicas per object |

### gRPC Server

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_GRPC_ENABLED` | `false` | Enable the gRPC interface (Arrow Flight / streaming) |
| `RS3GW_GRPC_PORT` | `50051` | gRPC server port |
| `RS3GW_GRPC_MAX_MESSAGE_SIZE` | `67108864` | Maximum message size in bytes (64MB) |
| `RS3GW_GRPC_TLS_CERT` | *(none)* | Path to gRPC TLS certificate (PEM) |
| `RS3GW_GRPC_TLS_KEY` | *(none)* | Path to gRPC TLS private key (PEM) |

### Resource Management

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_MIN_THREADS` | `4` | Minimum worker threads |
| `RS3GW_MAX_THREADS` | `num_cpus * 4` | Maximum worker threads |
| `RS3GW_TARGET_CPU` | `0.75` | Target CPU utilization (0.0-1.0) for adaptive scaling |
| `RS3GW_MEMORY_THRESHOLD` | `0.85` | Memory pressure threshold (0.0-1.0); triggers back-pressure |
| `RS3GW_ADJUSTMENT_INTERVAL` | `5` | Resource adjustment check interval in seconds |
| `RS3GW_ADAPTIVE_RATE_LIMIT` | `true` | Enable adaptive rate limiting based on system load |
| `RS3GW_INITIAL_RATE_LIMIT` | `1000` | Initial rate limit (requests/sec) |
| `RS3GW_MIN_RATE_LIMIT` | `100` | Minimum rate limit (requests/sec) |
| `RS3GW_MAX_RATE_LIMIT` | `10000` | Maximum rate limit (requests/sec) |
| `RS3GW_LOAD_SHEDDING_THRESHOLD` | `0.95` | Load shedding threshold (0.0-1.0); excess requests are rejected |

### Profiling

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_PROFILING_CPU` | `false` | Enable CPU profiling |
| `RS3GW_PROFILING_MEMORY` | `false` | Enable memory profiling |
| `RS3GW_PROFILING_IO` | `false` | Enable I/O profiling |
| `RS3GW_PROFILING_CPU_RATE` | `100` | CPU sample rate in Hz |
| `RS3GW_PROFILING_MEMORY_RATE` | `1000` | Memory sample rate (every Nth allocation) |
| `RS3GW_PROFILING_INTERVAL` | `60` | Profile collection interval in seconds |
| `RS3GW_PROFILING_MAX_PROFILES` | `24` | Maximum retained profile snapshots |
| `RS3GW_PROFILING_OUTPUT_DIR` | *(none)* | Directory for profile output files |

### Logging

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Tracing filter (`tracing-subscriber` EnvFilter syntax). Example: `rs3gw=debug,tower_http=info` |

---

## 4. Configuration File

rs3gw supports TOML configuration files in addition to environment variables. The load priority is:

1. **Config file** (`rs3gw.toml` in the working directory, or path from `--config`)
2. **Environment variables** (override file values)
3. **Defaults**

Example `rs3gw.toml`:

```toml
bind_addr = "0.0.0.0:9000"
storage_root = "/data/rs3gw"
default_bucket = "default"
access_key = "AKIAIOSFODNN7EXAMPLE"
secret_key = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
compression = "zstd"
request_timeout_secs = 300
max_concurrent_requests = 500
multipart_retention_hours = 168
fsync = false

[tls]
cert_path = "/etc/rs3gw/tls/server.crt"
key_path = "/etc/rs3gw/tls/server.key"

[connection_pool]
pool_max_idle_per_host = 32
pool_idle_timeout_secs = 90
connect_timeout_secs = 30
request_timeout_secs = 300

[dedup]
enabled = true
block_size = 65536

[select_cache]
enabled = true
max_entries = 1000
max_memory_mb = 100
ttl_seconds = 3600
```

---

## 5. TLS Configuration

### Direct TLS Termination

rs3gw supports native TLS via `rustls` (no OpenSSL dependency). To enable:

```bash
export RS3GW_TLS_CERT=/etc/rs3gw/tls/server.crt
export RS3GW_TLS_KEY=/etc/rs3gw/tls/server.key
```

Both variables must be set; otherwise the server starts in plain HTTP mode. Certificates must be PEM-encoded. The certificate file should contain the full chain (leaf certificate + intermediates).

**Certificate rotation** requires a server restart. rs3gw does not currently watch for certificate file changes at runtime.

The gRPC interface has independent TLS configuration via `RS3GW_GRPC_TLS_CERT` and `RS3GW_GRPC_TLS_KEY`, allowing different certificates for the HTTP and gRPC endpoints.

### Reverse Proxy TLS Termination (recommended)

For most production deployments, terminating TLS at a reverse proxy is preferred:

```
Client --TLS--> nginx/caddy/HAProxy --HTTP--> rs3gw (127.0.0.1:9000)
```

**Benefits:**
- Automatic certificate renewal (e.g., via certbot/ACME)
- No server restart for certificate rotation
- Centralized TLS policy management
- HTTP/2 and HTTP/3 support at the proxy layer

Example nginx upstream configuration:

```nginx
upstream rs3gw {
    server 127.0.0.1:9000;
    keepalive 64;
}

server {
    listen 443 ssl http2;
    server_name s3.example.com;

    ssl_certificate     /etc/letsencrypt/live/s3.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/s3.example.com/privkey.pem;

    client_max_body_size 5G;

    location / {
        proxy_pass http://rs3gw;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 300s;
        proxy_send_timeout 300s;
    }
}
```

---

## 6. Compression Configuration

### Modes

| Mode | Env Value | CPU Cost | Typical Ratio | Best For |
|------|-----------|----------|---------------|----------|
| None | `none`, `off`, `false`, `0` | Zero | 1:1 | Pre-compressed data (images, video, archives) |
| Zstd (default level) | `zstd`, `on`, `true`, `1` | Medium | 2-5x | General purpose; best ratio-to-speed tradeoff |
| Zstd (tuned) | `zstd:<level>` (1-22) | Low to High | Varies | Fine-grained control per workload |
| LZ4 | `lz4` | Very low | 1.5-3x | Latency-sensitive reads; fastest decompression |

### Zstd Level Tuning

| Level | Compression Speed | Decompression Speed | Ratio |
|-------|-------------------|---------------------|-------|
| 1 | Very fast | Very fast | Lower |
| 3 (default) | Fast | Very fast | Good |
| 6 | Medium | Very fast | Better |
| 12 | Slow | Very fast | High |
| 19-22 | Very slow | Very fast | Maximum |

Decompression speed is largely independent of the compression level. Higher levels only cost more on writes, not on reads.

### Workload Recommendations

- **AI/ML model checkpoints** (large, compressible tensors): `zstd:3` (default) provides a strong balance of throughput and savings.
- **HPC simulation output** (binary, moderately compressible): `zstd:1` for throughput priority, `zstd:6` for storage savings.
- **Images, video, pre-compressed archives**: `none` -- attempting to compress already-compressed data wastes CPU and may increase file size.
- **Log ingestion / JSON data**: `zstd:5` or higher for excellent ratios on text-like data.
- **Ultra-low-latency reads**: `lz4` decompresses at near-memory-bandwidth speeds.

### Usage

```bash
export RS3GW_COMPRESSION=zstd        # Default level 3
export RS3GW_COMPRESSION=zstd:6      # Level 6 for better ratio
export RS3GW_COMPRESSION=lz4         # Fastest decompression
export RS3GW_COMPRESSION=none        # No compression
```

---

## 7. Troubleshooting

### ENOSPC: Disk Full Recovery

**Symptoms**: PUT operations fail with 500 errors. Logs show "No space left on device".

**Immediate recovery**:
1. Free space by deleting temporary files or expanding the volume.
2. Clean up abandoned multipart uploads (see Multipart Cleanup below).
3. If dedup is enabled, run a dedup garbage collection pass.
4. rs3gw does not need a restart after freeing space -- it will resume accepting writes immediately.

**Prevention**:
- Monitor the `rs3gw_storage_bytes` gauge and alert at 80-85% capacity.
- Set quotas (`RS3GW_QUOTA_ENABLED=true`, `RS3GW_QUOTA_MAX_STORAGE_GB`) to enforce per-bucket limits.
- Use lifecycle/tiering policies to move cold data to cheaper or larger volumes.

### Permission Denied

**Symptoms**: Operations fail with "Permission denied" in logs.

**Diagnosis and fix**:
```bash
# Check ownership of storage root
ls -la /path/to/storage_root/

# The rs3gw process user must own the storage root recursively
chown -R rs3gw:rs3gw /path/to/storage_root/

# Minimum permissions: directories 755, files 644
find /path/to/storage_root -type d -exec chmod 755 {} \;
find /path/to/storage_root -type f -exec chmod 644 {} \;
```

For TLS certificates, ensure the rs3gw process user has read access to the certificate and key files (typically `chmod 640`).

### Corrupt Metadata: Detection and Recovery

**Symptoms**: GET/HEAD returns unexpected results. Logs show JSON parse errors for metadata files.

**Detection**:
```bash
# Find zero-byte metadata files (likely from incomplete writes)
find /path/to/storage_root -path "*/metadata/*.json" -size 0

# Validate all metadata JSON
find /path/to/storage_root -path "*/metadata/*.json" -exec sh -c \
  'python3 -m json.tool "$1" > /dev/null 2>&1 || echo "CORRUPT: $1"' _ {} \;
```

**Recovery**:
- For corrupt metadata with intact object data: delete the metadata file and re-PUT the object.
- Enable `RS3GW_FSYNC=true` to prevent metadata corruption caused by unexpected power loss. This calls `sync_all()` before renaming files, ensuring data reaches stable storage.
- Enable `RS3GW_CHECKSUM_VALIDATION=true` to detect corruption on read and return clear errors rather than serving bad data.

### Multipart Upload Cleanup

Abandoned multipart uploads consume disk space in `<bucket>/multipart/`.

**Automatic GC**: rs3gw garbage-collects multipart uploads older than `RS3GW_MULTIPART_RETENTION_HOURS` (default: 168 hours = 7 days).

**Manual cleanup**:
```bash
# List multipart upload directories older than 7 days
find /path/to/storage_root/*/multipart -mindepth 1 -maxdepth 1 -type d -mtime +7

# Remove them
find /path/to/storage_root/*/multipart -mindepth 1 -maxdepth 1 -type d -mtime +7 -exec rm -rf {} \;
```

**Tuning**: For workloads with many large multipart uploads, reduce retention to reclaim space faster:
```bash
export RS3GW_MULTIPART_RETENTION_HOURS=24  # 1 day instead of 7
```

### Performance Tuning

**High latency on reads**:
- Ensure `RS3GW_CACHE_ENABLED=true` (default) and increase `RS3GW_CACHE_MAX_SIZE_MB` if the cache hit rate (`rs3gw_cache_hit_rate` metric) is below 0.5.
- Verify zero-copy is active: `RS3GW_ZEROCOPY_SPLICE=true` and `RS3GW_ZEROCOPY_DIRECT_IO=true` (both default to true).
- Lower `RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD` below 1MB if your workload has many medium-sized objects.

**High latency on writes**:
- Switch to `RS3GW_COMPRESSION=lz4` instead of `zstd` if CPU is the bottleneck (check system CPU utilization).
- Set `RS3GW_FSYNC=false` (default) unless your durability requirements mandate fsync on every write.
- Increase `RS3GW_MAX_CONCURRENT` to allow more parallel writes if disk I/O is not saturated.

**Request queuing / 503 errors**:
- Increase `RS3GW_MAX_CONCURRENT` if the server is rejecting requests due to the concurrency limit (default 0 = unlimited).
- Tune `RS3GW_REQUEST_TIMEOUT` to shed stuck requests (default 300s may be too generous for some workloads).
- The adaptive rate limiter (`RS3GW_ADAPTIVE_RATE_LIMIT=true`, default) automatically adjusts between `RS3GW_MIN_RATE_LIMIT` (100) and `RS3GW_MAX_RATE_LIMIT` (10000) based on system load.
- Load shedding activates at `RS3GW_LOAD_SHEDDING_THRESHOLD` (default 0.95) -- excess requests receive 503.

**Memory pressure**:
- Reduce `RS3GW_CACHE_MAX_SIZE_MB` and `RS3GW_SELECT_CACHE_MAX_MEMORY_MB`.
- Adjust `RS3GW_MEMORY_THRESHOLD` (default 0.85) to trigger back-pressure earlier.
- Disable dedup for small deployments: `RS3GW_DEDUP_ENABLED=false` eliminates the in-memory dedup index.

---

## 8. Monitoring

### Prometheus Metrics Endpoint

rs3gw exposes Prometheus metrics at `GET /metrics`. This endpoint does not require authentication.

```bash
curl http://localhost:9000/metrics
```

Background metrics collection for predictive analytics runs every 60 seconds automatically, recording storage size, request rate, and bandwidth.

### Key Metrics

#### Request Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rs3gw_requests_total` | Counter | `operation`, `status` | Total requests by S3 operation and HTTP status class (2xx/3xx/4xx/5xx) |
| `rs3gw_request_duration_ms` | Histogram | `operation` | Request latency distribution in milliseconds. Buckets: 0.1, 1, 5, 10, 50, 100, 500, 1000, 5000, 60000 ms |
| `rs3gw_bytes_total` | Counter | `direction` | Bytes transferred (upload/download) |
| `rs3gw_errors_total` | Counter | `error_type`, `operation` | Errors by type and S3 operation |

#### Storage Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rs3gw_buckets_total` | Gauge | Number of buckets |
| `rs3gw_objects_total` | Gauge | Total object count |
| `rs3gw_storage_bytes` | Gauge | Total storage used in bytes |
| `rs3gw_object_size_bytes` | Histogram | Object size distribution. Buckets: 1KB, 64KB, 1MB, 10MB, 100MB, 1GB |

#### Cache Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rs3gw_cache_operations_total` | Counter | `operation`, `result` | Cache operations (hit/miss) |
| `rs3gw_cache_size_bytes` | Gauge | | Current cache memory usage |
| `rs3gw_cache_objects_total` | Gauge | | Number of cached objects |
| `rs3gw_cache_hit_rate` | Gauge | | Cache hit rate (0.0-1.0) |

#### Compression and Deduplication

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rs3gw_compression_original_bytes` | Counter | `algorithm` | Original bytes before compression |
| `rs3gw_compression_compressed_bytes` | Counter | `algorithm` | Bytes after compression |
| `rs3gw_compression_ratio` | Histogram | `algorithm` | Compression ratio (compressed/original). Buckets: 0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0, 1.25 |
| `rs3gw_dedup_total_bytes_saved` | Counter | | Total bytes saved by deduplication |
| `rs3gw_dedup_operations_total` | Counter | | Total dedup operations |
| `rs3gw_dedup_savings_ratio` | Histogram | | Dedup savings ratio. Buckets: 0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0 |

#### Multipart Upload Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rs3gw_multipart_parts` | Histogram | Parts per multipart upload |
| `rs3gw_multipart_size_bytes` | Histogram | Total size of completed multipart uploads |
| `rs3gw_multipart_duration_ms` | Histogram | Time to complete a multipart upload |

#### Cluster Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `rs3gw_cluster_nodes_total` | Gauge | Total cluster nodes |
| `rs3gw_cluster_healthy_nodes` | Gauge | Healthy cluster nodes |
| `rs3gw_cluster_replication_lag_ms` | Gauge | Replication lag in milliseconds |
| `rs3gw_cluster_operations_total` | Counter | Cluster operations (labels: `operation`, `status`) |

#### Storage Class Transitions

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rs3gw_storage_class_transitions_total` | Counter | `from_class`, `to_class` | Number of storage class transitions |
| `rs3gw_storage_class_transitioned_bytes` | Counter | `from_class`, `to_class` | Bytes moved between storage classes |

#### Batch Job Metrics

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `rs3gw_batch_jobs_total` | Counter | `job_type`, `status` | Total batch jobs executed |
| `rs3gw_batch_objects_processed` | Counter | `job_type`, `status` | Objects processed by batch jobs |

### Recommended Alerts

| Alert | PromQL Condition | Severity |
|-------|------------------|----------|
| High error rate | `rate(rs3gw_errors_total[5m]) > 1` | Warning |
| Disk nearly full | `rs3gw_storage_bytes / <total_capacity> > 0.85` | Critical |
| High P99 latency | `histogram_quantile(0.99, rate(rs3gw_request_duration_ms_bucket[5m])) > 5000` | Warning |
| Low cache hit rate | `rs3gw_cache_hit_rate < 0.5` | Info |
| Replication lag | `rs3gw_cluster_replication_lag_ms > 10000` | Warning |
| No healthy peers | `rs3gw_cluster_healthy_nodes < 2` (when cluster is enabled) | Critical |
| High compression ratio | `histogram_quantile(0.95, rs3gw_compression_ratio) > 1.0` | Info (data expanding, consider `none`) |

### Grafana Dashboard

A pre-built Grafana dashboard is available at [`docs/grafana-dashboard.json`](grafana-dashboard.json). To import:

1. Open Grafana and navigate to **Dashboards > Import**.
2. Upload or paste the contents of `docs/grafana-dashboard.json`.
3. Select your Prometheus data source.
4. The dashboard includes panels for request rate, latency percentiles, error rate, storage usage, cache hit rate, compression ratio, dedup savings, and cluster health.

See also [`docs/prometheus.md`](prometheus.md) for Prometheus scrape configuration.

---

## 9. Deployment Examples

### systemd Unit File

```ini
[Unit]
Description=rs3gw S3-compatible Object Storage Gateway
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=rs3gw
Group=rs3gw
Environment=RS3GW_BIND_ADDR=0.0.0.0:9000
Environment=RS3GW_STORAGE_ROOT=/data/rs3gw
Environment=RS3GW_COMPRESSION=zstd
Environment=RS3GW_MAX_CONCURRENT=500
Environment=RUST_LOG=info
EnvironmentFile=-/etc/rs3gw/env
ExecStart=/usr/local/bin/rs3gw
Restart=on-failure
RestartSec=5
LimitNOFILE=65536
TimeoutStopSec=40

[Install]
WantedBy=multi-user.target
```

**Key settings:**
- `LimitNOFILE=65536`: High file descriptor limit for concurrent connections.
- `TimeoutStopSec=40`: Allows the 30-second in-flight request drain timeout plus margin.
- `EnvironmentFile=-/etc/rs3gw/env`: Load secrets (access key, secret key) from a separate file with restricted permissions (`chmod 600`).

### Docker

```bash
docker run -d \
  --name rs3gw \
  -p 9000:9000 \
  -e RS3GW_STORAGE_ROOT=/data \
  -e RS3GW_BIND_ADDR=0.0.0.0:9000 \
  -e RS3GW_COMPRESSION=zstd \
  -e RS3GW_ACCESS_KEY=myaccesskey \
  -e RS3GW_SECRET_KEY=mysecretkey \
  -v /host/data:/data \
  rs3gw:latest
```

### Graceful Shutdown

rs3gw handles `SIGINT` (Ctrl+C) and `SIGTERM` gracefully:

1. The server stops accepting new connections.
2. In-flight requests are allowed to complete (up to a 30-second drain timeout).
3. The process exits cleanly.

For container orchestrators (Kubernetes, Docker Compose), ensure `terminationGracePeriodSeconds` is at least 35 seconds to account for the drain timeout plus a small buffer.
