# rs3ctl - Command-Line Management Tool for rs3gw

`rs3ctl` is a powerful command-line interface for managing rs3gw S3-compatible object storage servers. It provides comprehensive control over buckets, objects, replication, metrics, and maintenance operations.

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Commands](#commands)
  - [Bucket Management](#bucket-management)
  - [Object Management](#object-management)
  - [Replication Management](#replication-management)
  - [Metrics & Monitoring](#metrics--monitoring)
  - [Maintenance Operations](#maintenance-operations)
- [Examples](#examples)
- [Configuration](#configuration)
- [Troubleshooting](#troubleshooting)

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/cool-japan/rs3gw
cd rs3gw

# Build rs3ctl
cargo build --release --bin rs3ctl

# Install to system path (optional)
cargo install --path . --bin rs3ctl
```

### Binary Location

After building, the binary will be located at:
```
target/release/rs3ctl
```

## Quick Start

### Basic Usage

```bash
# Check server health
rs3ctl health

# List all buckets
rs3ctl bucket list

# Create a new bucket
rs3ctl bucket create my-bucket

# Upload a file
rs3ctl object upload my-bucket my-key.txt /path/to/file.txt

# Download a file
rs3ctl object download my-bucket my-key.txt ./downloaded-file.txt
```

### Specify Custom Endpoint

```bash
# Connect to a remote rs3gw server
rs3ctl --endpoint http://rs3gw.example.com:9000 bucket list

# Connect via HTTPS
rs3ctl --endpoint https://rs3gw.example.com bucket list
```

## Commands

### Global Options

- `--endpoint, -e <URL>` - rs3gw server endpoint (default: http://localhost:9000)
- `--access-key, -a <KEY>` - Access key for authentication (future feature)
- `--secret-key, -s <KEY>` - Secret key for authentication (future feature)
- `--help, -h` - Show help information
- `--version, -V` - Show version information

### Health Check

Check server health status:

```bash
rs3ctl health
```

**Output:**
```
✓ Server is healthy
OK
```

---

### Bucket Management

#### List Buckets

List all buckets on the server:

```bash
rs3ctl bucket list
```

#### Create Bucket

Create a new bucket:

```bash
rs3ctl bucket create <BUCKET_NAME> [OPTIONS]

Options:
  -r, --region <REGION>    Bucket region (default: us-east-1)
```

**Examples:**
```bash
# Create bucket with default region
rs3ctl bucket create my-bucket

# Create bucket with specific region
rs3ctl bucket create my-bucket --region us-west-2
```

**Output:**
```
✓ Bucket 'my-bucket' created successfully
```

#### Delete Bucket

Delete an existing bucket:

```bash
rs3ctl bucket delete <BUCKET_NAME> [OPTIONS]

Options:
  -f, --force    Force delete even if bucket is not empty (future feature)
```

**Examples:**
```bash
# Delete empty bucket
rs3ctl bucket delete my-bucket

# Force delete (future feature)
rs3ctl bucket delete my-bucket --force
```

**Output:**
```
✓ Bucket 'my-bucket' deleted successfully
```

#### Get Bucket Information

Get detailed information about a bucket:

```bash
rs3ctl bucket info <BUCKET_NAME>
```

**Example:**
```bash
rs3ctl bucket info my-bucket
```

**Output:**
```
Bucket: my-bucket
Status: Exists
  content-length: 0
  date: Mon, 30 Dec 2025 12:00:00 GMT
  x-amz-bucket-region: us-east-1
```

#### Get Bucket Policy

Retrieve the bucket policy:

```bash
rs3ctl bucket get-policy <BUCKET_NAME>
```

**Example:**
```bash
rs3ctl bucket get-policy my-bucket
```

**Output:**
```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::my-bucket/*"
    }
  ]
}
```

#### Set Bucket Policy

Set or update a bucket policy from a JSON file:

```bash
rs3ctl bucket set-policy <BUCKET_NAME> <POLICY_FILE>
```

**Example:**
```bash
# Create policy file
cat > policy.json << 'EOF'
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": "*",
      "Action": "s3:GetObject",
      "Resource": "arn:aws:s3:::my-bucket/*"
    }
  ]
}
EOF

# Apply policy
rs3ctl bucket set-policy my-bucket policy.json
```

**Output:**
```
✓ Bucket policy updated successfully
```

---

### Object Management

#### List Objects

List objects in a bucket:

```bash
rs3ctl object list <BUCKET_NAME> [OPTIONS]

Options:
  -p, --prefix <PREFIX>        Filter by prefix
  -m, --max-keys <MAX_KEYS>    Maximum keys to return (default: 1000)
```

**Examples:**
```bash
# List all objects
rs3ctl object list my-bucket

# List with prefix filter
rs3ctl object list my-bucket --prefix logs/

# Limit results
rs3ctl object list my-bucket --max-keys 100
```

#### Upload Object

Upload a file as an object:

```bash
rs3ctl object upload <BUCKET_NAME> <KEY> <FILE> [OPTIONS]

Options:
  -t, --content-type <TYPE>    Content type (MIME type)
```

**Examples:**
```bash
# Basic upload
rs3ctl object upload my-bucket data.json /path/to/data.json

# Upload with content type
rs3ctl object upload my-bucket image.png /path/to/image.png \
  --content-type image/png

# Upload with different key
rs3ctl object upload my-bucket backups/2025-12-30.tar.gz ./backup.tar.gz
```

**Output:**
```
[00:00:05] ========================================> 1.2MB/1.2MB Done
✓ Object uploaded successfully
ETag: "e8b7dd25f1d3f7e4c8c9e8a5b3d7e4c8"
```

#### Download Object

Download an object to a local file:

```bash
rs3ctl object download <BUCKET_NAME> <KEY> <OUTPUT_PATH>
```

**Examples:**
```bash
# Download object
rs3ctl object download my-bucket data.json ./downloaded-data.json

# Download to specific directory
rs3ctl object download my-bucket logs/app.log ./logs/app.log
```

**Output:**
```
[00:00:03] ========================================> 512KB/512KB Done
✓ Object downloaded to ./downloaded-data.json
```

#### Delete Object

Delete an object from a bucket:

```bash
rs3ctl object delete <BUCKET_NAME> <KEY>
```

**Examples:**
```bash
rs3ctl object delete my-bucket old-data.json
```

**Output:**
```
✓ Object deleted successfully
```

#### Get Object Information

Get metadata about an object:

```bash
rs3ctl object info <BUCKET_NAME> <KEY>
```

**Example:**
```bash
rs3ctl object info my-bucket data.json
```

**Output:**
```
Object: my-bucket/data.json
  content-length: 1234567
  content-type: application/json
  etag: "e8b7dd25f1d3f7e4c8c9e8a5b3d7e4c8"
  last-modified: Mon, 30 Dec 2025 12:00:00 GMT
  x-amz-storage-class: STANDARD
```

---

### Replication Management

#### Get Replication Configuration

Retrieve replication configuration for a bucket:

```bash
rs3ctl replication get-config <BUCKET_NAME>
```

**Example:**
```bash
rs3ctl replication get-config my-bucket
```

#### Set Replication Configuration

Configure replication from a JSON file:

```bash
rs3ctl replication set-config <BUCKET_NAME> <CONFIG_FILE>
```

**Example:**
```bash
# Create replication config
cat > replication.json << 'EOF'
{
  "destinations": [
    {
      "name": "backup-region",
      "endpoint": "http://backup.example.com:9000",
      "bucket": "my-bucket-backup"
    }
  ],
  "filter": {
    "prefix": "important/"
  },
  "wan_optimization": {
    "enabled": true,
    "compression_level": 3
  }
}
EOF

# Apply configuration
rs3ctl replication set-config my-bucket replication.json
```

**Output:**
```
✓ Replication configuration updated successfully
```

#### Get Replication Metrics

View replication metrics:

```bash
rs3ctl replication metrics [OPTIONS]

Options:
  -d, --destination <NAME>    Specific destination metrics
```

**Examples:**
```bash
# Get all metrics
rs3ctl replication metrics

# Get specific destination metrics
rs3ctl replication metrics --destination backup-region
```

**Output:**
```json
{
  "total_objects": 1500,
  "total_bytes": 15728640,
  "success_count": 1498,
  "failure_count": 2,
  "last_replication": "2025-12-30T12:00:00Z"
}
```

---

### Metrics & Monitoring

#### Get Prometheus Metrics

Retrieve Prometheus-formatted metrics:

```bash
rs3ctl metrics get
```

**Output:**
```
# HELP rs3gw_requests_total Total number of requests
# TYPE rs3gw_requests_total counter
rs3gw_requests_total{operation="GetObject"} 1523
rs3gw_requests_total{operation="PutObject"} 842
...
```

#### Get Storage Statistics

View storage statistics (future feature):

```bash
rs3ctl metrics storage
```

#### Get Operation Statistics

View operation statistics (future feature):

```bash
rs3ctl metrics operations
```

---

### Maintenance Operations

#### Create Backup Snapshot

Create a backup snapshot (requires server-side API):

```bash
rs3ctl maintenance backup [OPTIONS]

Options:
  -n, --name <NAME>                 Snapshot name
  -t, --snapshot-type <TYPE>        Snapshot type (full, incremental) [default: full]
```

**Example:**
```bash
rs3ctl maintenance backup --name daily-backup --snapshot-type full
```

#### Run Integrity Check

Trigger integrity check (requires server-side API):

```bash
rs3ctl maintenance integrity-check [OPTIONS]

Options:
  -b, --bucket <BUCKET>    Check specific bucket (checks all if not specified)
```

**Examples:**
```bash
# Check all buckets
rs3ctl maintenance integrity-check

# Check specific bucket
rs3ctl maintenance integrity-check --bucket my-bucket
```

#### Trigger Cleanup

Cleanup old objects (requires server-side API):

```bash
rs3ctl maintenance cleanup [OPTIONS]

Options:
  -r, --retention-days <DAYS>    Retention period in days [default: 30]
```

**Example:**
```bash
rs3ctl maintenance cleanup --retention-days 90
```

---

## Examples

### Complete Workflow Example

```bash
# 1. Check server health
rs3ctl health

# 2. Create a bucket
rs3ctl bucket create my-data --region us-east-1

# 3. Upload some files
rs3ctl object upload my-data file1.txt ./data/file1.txt
rs3ctl object upload my-data file2.json ./data/file2.json
rs3ctl object upload my-data images/photo.jpg ./photos/photo.jpg

# 4. List objects
rs3ctl object list my-data

# 5. List objects with prefix
rs3ctl object list my-data --prefix images/

# 6. Get object info
rs3ctl object info my-data file2.json

# 7. Download an object
rs3ctl object download my-data file2.json ./backup/file2.json

# 8. Set bucket policy
cat > policy.json << 'EOF'
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject", "s3:ListBucket"],
      "Resource": ["arn:aws:s3:::my-data", "arn:aws:s3:::my-data/*"]
    }
  ]
}
EOF
rs3ctl bucket set-policy my-data policy.json

# 9. Get metrics
rs3ctl metrics get

# 10. Delete object
rs3ctl object delete my-data file1.txt

# 11. Delete bucket (must be empty)
rs3ctl bucket delete my-data
```

### Bulk Upload Script

```bash
#!/bin/bash
# Upload all files from a directory

BUCKET="my-bucket"
SOURCE_DIR="./data"
ENDPOINT="http://localhost:9000"

for file in "$SOURCE_DIR"/*; do
    filename=$(basename "$file")
    echo "Uploading $filename..."
    rs3ctl --endpoint "$ENDPOINT" object upload "$BUCKET" "$filename" "$file"
done

echo "Upload complete!"
```

### Backup Script

```bash
#!/bin/bash
# Backup script for rs3gw buckets

ENDPOINT="http://localhost:9000"
BACKUP_DIR="./backups/$(date +%Y%m%d)"
BUCKET="important-data"

mkdir -p "$BACKUP_DIR"

# Download all objects
echo "Downloading objects from $BUCKET..."
# Note: This is a simplified example - for production use s3-migrate tool

rs3ctl --endpoint "$ENDPOINT" object list "$BUCKET" | \
  while read -r key; do
    rs3ctl --endpoint "$ENDPOINT" object download "$BUCKET" "$key" "$BACKUP_DIR/$key"
  done

echo "Backup complete: $BACKUP_DIR"
```

---

## Configuration

### Environment Variables

While rs3ctl doesn't currently read environment variables, you can use shell aliases for convenience:

```bash
# Add to ~/.bashrc or ~/.zshrc
alias rs3='rs3ctl --endpoint http://my-rs3gw-server.com:9000'

# Usage
rs3 bucket list
rs3 object upload my-bucket key.txt file.txt
```

### Shell Completion

Generate shell completion scripts:

```bash
# Bash
rs3ctl --help | grep -A 100 "USAGE" > /dev/null

# Note: Built-in completion support coming in future version
```

---

## Troubleshooting

### Common Issues

#### Connection Refused

**Problem:** `Failed to connect to rs3gw server`

**Solutions:**
- Verify the server is running: `curl http://localhost:9000/health`
- Check the endpoint URL is correct
- Ensure firewall allows connections to port 9000

#### Bucket Already Exists

**Problem:** `BucketAlreadyExists error`

**Solution:** Choose a different bucket name or delete the existing bucket first

#### Permission Denied

**Problem:** `403 Forbidden` or `Access Denied`

**Solutions:**
- Check authentication credentials (when authentication is enabled)
- Verify bucket policy allows the operation
- Ensure you have necessary permissions

#### File Not Found

**Problem:** `NoSuchKey error`

**Solution:** Verify the object key is correct using `rs3ctl object list`

### Debug Mode

For detailed request/response information, use verbose logging:

```bash
# Enable Rust logging
RUST_LOG=debug rs3ctl object upload my-bucket key.txt file.txt
```

### Getting Help

```bash
# General help
rs3ctl --help

# Command-specific help
rs3ctl bucket --help
rs3ctl object upload --help
```

---

## Future Features

The following features are planned for future versions:

- **AWS SigV4 Authentication**: Full support for signed requests
- **Parallel Uploads/Downloads**: Multi-threaded file transfers
- **Resume Support**: Resume interrupted transfers
- **Configuration File**: Store commonly used settings
- **Shell Completion**: Auto-completion for bash/zsh/fish
- **Progress Tracking**: Enhanced progress bars with ETA
- **Batch Operations**: Upload/download multiple files efficiently
- **Sync Command**: Synchronize directories with buckets
- **Watch Mode**: Monitor and sync changes automatically

---

## Environment Variables

All runtime configuration for `rs3gw` is controlled through environment variables. There is no separate config file for rs3ctl itself, but the server it connects to reads the variables below.

### Core

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_BIND_ADDR` | `0.0.0.0:9000` | Server listen address (host:port) |
| `RS3GW_STORAGE_ROOT` | `./data` | Root directory for all stored objects and metadata |
| `RS3GW_DEFAULT_BUCKET` | `default` | Default bucket name (single-bucket mode) |
| `RS3GW_REQUEST_TIMEOUT` | `300` | Per-request timeout in seconds (0 = no timeout) |
| `RS3GW_MAX_CONCURRENT` | `0` | Maximum concurrent requests (0 = unlimited) |
| `RS3GW_MULTIPART_RETENTION_HOURS` | `168` | Hours before abandoned multipart uploads are garbage-collected (7 days) |
| `RS3GW_REGION` | `us-east-1` | Region string returned by `GetBucketLocation` and used in SigV4 validation |
| `RS3GW_CHECKSUM_VALIDATION` | `false` | When `true`, SHA-256 digest is verified on every `GetObject` before streaming |
| `RS3GW_FSYNC` | `false` | When `true`, calls `sync_all()` on temp files before atomic rename (safer but slower) |

### TLS (HTTP)

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_TLS_CERT` | _(none)_ | Path to PEM certificate file; enables HTTPS when both cert and key are set |
| `RS3GW_TLS_KEY` | _(none)_ | Path to PEM private key file |

### Auth

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_ACCESS_KEY` | _(empty)_ | AWS access key ID; when empty, SigV4 auth is disabled (passthrough mode) |
| `RS3GW_SECRET_KEY` | _(empty)_ | AWS secret access key; must be set together with `RS3GW_ACCESS_KEY` |

### Cache

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_CACHE_ENABLED` | `true` | Enable in-memory object cache |
| `RS3GW_CACHE_MAX_SIZE_MB` | `256` | Maximum cache size in megabytes |
| `RS3GW_CACHE_MAX_OBJECTS` | `10000` | Maximum number of cached objects |
| `RS3GW_CACHE_TTL` | `300` | Cache entry TTL in seconds |
| `RS3GW_SELECT_CACHE_ENABLED` | `true` | Enable S3 Select result caching |
| `RS3GW_SELECT_CACHE_MAX_ENTRIES` | `1000` | Maximum number of cached query results |
| `RS3GW_SELECT_CACHE_MAX_MEMORY_MB` | `100` | Maximum memory for S3 Select cache in megabytes |
| `RS3GW_SELECT_CACHE_TTL` | `3600` | S3 Select cache TTL in seconds |

### Throttle

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_THROTTLE_ENABLED` | `false` | Enable per-client request throttling |
| `RS3GW_THROTTLE_RPS` | `0` | Max requests per second per client (0 = unlimited) |
| `RS3GW_THROTTLE_UPLOAD_MBPS` | `0` | Upload bandwidth limit in MB/s (0 = unlimited) |
| `RS3GW_THROTTLE_DOWNLOAD_MBPS` | `0` | Download bandwidth limit in MB/s (0 = unlimited) |

### Dedup / Compression

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_COMPRESSION` | _(none)_ | Compression mode: `none`, `lz4`, `zstd`, or `zstd:LEVEL` (level 1-22) |
| `RS3GW_DEDUP_ENABLED` | `true` | Enable block-level data deduplication |
| `RS3GW_DEDUP_BLOCK_SIZE` | `65536` | Dedup chunk size in bytes (default 64 KB) |
| `RS3GW_DEDUP_ALGORITHM` | `fixed` | Chunking algorithm: `fixed` or `content-defined` (CDC) |
| `RS3GW_DEDUP_MIN_SIZE` | `131072` | Minimum object size to deduplicate in bytes (default 128 KB) |

### Storage

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_ZEROCOPY_DIRECT_IO` | `true` | Enable direct I/O (kernel bypass) for large objects |
| `RS3GW_ZEROCOPY_DIRECT_IO_THRESHOLD` | `1048576` | Minimum object size for direct I/O in bytes (default 1 MB) |
| `RS3GW_ZEROCOPY_SPLICE` | `true` | Enable splice(2) zero-copy on Linux |
| `RS3GW_ZEROCOPY_MMAP` | `true` | Enable mmap for metadata reads |
| `RS3GW_QUOTA_ENABLED` | `false` | Enable per-bucket storage quotas |
| `RS3GW_QUOTA_MAX_STORAGE_GB` | `0` | Default maximum storage per bucket in GB (0 = unlimited) |
| `RS3GW_QUOTA_MAX_OBJECTS` | `0` | Default maximum objects per bucket (0 = unlimited) |

### gRPC

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_GRPC_ENABLED` | `false` | Enable the gRPC API server |
| `RS3GW_GRPC_PORT` | `50051` | gRPC server listen port |
| `RS3GW_GRPC_MAX_MESSAGE_SIZE` | `67108864` | Maximum gRPC message size in bytes (default 64 MB) |
| `RS3GW_GRPC_TLS_CERT` | _(none)_ | Path to PEM certificate for gRPC TLS; enables TLS when both cert and key are set |
| `RS3GW_GRPC_TLS_KEY` | _(none)_ | Path to PEM private key for gRPC TLS |

### Cluster

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_CLUSTER_ENABLED` | `false` | Enable cluster/gossip mode |
| `RS3GW_CLUSTER_NODE_ID` | _(auto)_ | Unique node identifier within the cluster |
| `RS3GW_CLUSTER_ADVERTISE_ADDR` | `127.0.0.1:9001` | Address advertised to peers for cluster traffic |
| `RS3GW_CLUSTER_PORT` | `9001` | Local port for cluster communication |
| `RS3GW_CLUSTER_SEED_NODES` | _(none)_ | Comma-separated list of peer addresses to bootstrap cluster membership |
| `RS3GW_REPLICATION_MODE` | `async` | Replication mode: `async`, `sync`, or `quorum` |
| `RS3GW_REPLICATION_FACTOR` | `2` | Number of replicas per object |

### Connection Pool (outbound HTTP)

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_POOL_MAX_IDLE` | `32` | Maximum idle connections per host in the outbound HTTP pool |
| `RS3GW_POOL_IDLE_TIMEOUT` | `90` | Idle connection timeout in seconds |
| `RS3GW_CONNECT_TIMEOUT` | `30` | TCP connection timeout in seconds |
| `RS3GW_CLIENT_TIMEOUT` | `300` | Overall outbound request timeout in seconds |

### Observability

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_PROFILING_CPU` | `false` | Enable CPU profiling |
| `RS3GW_PROFILING_MEMORY` | `false` | Enable memory profiling |
| `RS3GW_PROFILING_IO` | `false` | Enable I/O profiling |
| `RS3GW_PROFILING_CPU_RATE` | _(default)_ | CPU sample rate in Hz |
| `RS3GW_PROFILING_MEMORY_RATE` | _(default)_ | Memory sample rate |
| `RS3GW_PROFILING_INTERVAL` | _(default)_ | Profile collection interval in seconds |
| `RS3GW_PROFILING_MAX_PROFILES` | _(default)_ | Maximum number of profiles to retain in memory |
| `RS3GW_PROFILING_OUTPUT_DIR` | _(none)_ | Directory to write flamegraph/profile output files |
| `RS3GW_ADAPTIVE_RATE_LIMIT` | `false` | Enable adaptive rate limiting based on load |
| `RS3GW_INITIAL_RATE_LIMIT` | _(default)_ | Initial request rate limit (requests/sec) |
| `RS3GW_MIN_RATE_LIMIT` | _(default)_ | Minimum rate limit floor |
| `RS3GW_MAX_RATE_LIMIT` | _(default)_ | Maximum rate limit ceiling |
| `RS3GW_LOAD_SHEDDING_THRESHOLD` | _(default)_ | CPU utilization fraction at which load shedding activates |
| `RS3GW_MEMORY_THRESHOLD` | _(default)_ | Memory pressure fraction at which load shedding activates |
| `RS3GW_TARGET_CPU` | _(default)_ | Target CPU utilization fraction for adaptive scaling |
| `RS3GW_MIN_THREADS` | _(default)_ | Minimum Tokio worker threads |
| `RS3GW_MAX_THREADS` | _(default)_ | Maximum Tokio worker threads |
| `RS3GW_ADJUSTMENT_INTERVAL` | _(default)_ | Interval in seconds between adaptive resource adjustments |

---

## Configuration Precedence

Environment variables are the primary configuration mechanism. A TOML config file (`rs3gw.toml`) is also supported. The load order is:

1. **Compiled defaults** — hardcoded values in `Config::default()`.
2. **Config file** — `rs3gw.toml` (or the path supplied via `--config`), if the file exists. File values override compiled defaults.
3. **Environment variable overrides** — any `RS3GW_*` variable that is set overrides the corresponding file or default value.

YAML configuration is not currently supported. TOML/YAML file support with hot-reload is planned for a future release.

**Example — override bind address at startup:**

```bash
RS3GW_BIND_ADDR=0.0.0.0:8080 rs3gw
```

**Example — enable gRPC with TLS:**

```bash
RS3GW_GRPC_ENABLED=true \
RS3GW_GRPC_TLS_CERT=/etc/rs3gw/grpc.crt \
RS3GW_GRPC_TLS_KEY=/etc/rs3gw/grpc.key \
rs3gw
```

---

## On-Disk Format v1

rs3gw stores all objects and metadata under `RS3GW_STORAGE_ROOT` using a simple, human-inspectable directory tree.

### Directory Structure

```
<root>/
  <bucket>/
    objects/<encoded_key>              # object data (may be zstd/lz4 compressed)
    metadata/<encoded_key>.json        # JSON: ObjectMetadata (schema_version=1)
    sci_metadata/<encoded_key>.json    # JSON: SciMetadata (HPC/AI fields)
    tags/<encoded_key>.json            # JSON: ObjectTagging
    bucket_tags.json                   # JSON: bucket-level tags
    bucket_policy.json                 # JSON: bucket access policy
    multipart/<upload_id>/
      metadata.json                    # JSON: MultipartMetadata
      part-00001, part-00002, ...      # raw part data files
```

### Key Points

- **`schema_version: 1`** — every `metadata/<key>.json` file contains `"schema_version": 1`. Future breaking metadata changes will increment this value so older binaries can refuse incompatible data.
- **`__DIROBJ__` sentinel** — S3 keys that end with `/` (directory-placeholder objects) are stored by appending `__DIROBJ__` as the final path component, e.g. `folder/` → `folder/__DIROBJ__`. This allows the keys to round-trip cleanly on filesystems that disallow trailing slashes in filenames.
- **Atomic-rename durability** — writes go to a temp file (`<path>.tmp.<nanos>`) and are `rename`d into place, so readers never see partial writes.
- **Forward compatibility with pre-0.2.0 data** — if `schema_version` is absent in a metadata file it defaults to `1`, making all pre-0.2.0 metadata forward-compatible. No migration tooling is required.
- **Checksum storage** — when `RS3GW_CHECKSUM_VALIDATION=true` the SHA-256 digest (base64-encoded) is stored in the `__checksum_value__` metadata field and verified on every `GetObject`.

---

## Regions and LocationConstraint

- `GetBucketLocation` always returns the region configured via `RS3GW_REGION` (default: `us-east-1`).
- All standard AWS region strings (e.g. `eu-west-1`, `ap-northeast-1`) are accepted in the SigV4 credential scope without error; rs3gw does not validate the region string.
- The region value affects SigV4 signature verification — the client and server must agree on the region string.
- Multi-region routing (routing requests to different nodes based on region) is planned for a future release.

---

## See Also

- [rs3gw Main Documentation](../README.md)
- [s3-migrate Tool](./s3_migrate.md)
- [API Documentation](./api.md)
- [Performance Tuning Guide](./performance_tuning.md)

---

## License

rs3ctl is part of the rs3gw project and is distributed under the same license (MIT OR Apache-2.0).

## Contributing

Contributions are welcome! Please see the main rs3gw repository for contribution guidelines.

---

**Version:** 0.1.0
**Last Updated:** 2025-12-30
