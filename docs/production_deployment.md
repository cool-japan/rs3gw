# rs3gw Production Deployment Guide

This guide provides comprehensive instructions for deploying rs3gw in production environments with best practices for security, high availability, and operational excellence.

## Table of Contents

- [Deployment Options](#deployment-options)
- [Security Best Practices](#security-best-practices)
- [High Availability Setup](#high-availability-setup)
- [Monitoring & Observability](#monitoring--observability)
- [Backup & Disaster Recovery](#backup--disaster-recovery)
- [Performance Optimization](#performance-optimization)
- [Troubleshooting](#troubleshooting)
- [ML/AI Features & Dataset Preprocessing](#mlai-features--dataset-preprocessing)

---

## Deployment Options

### 1. Docker Deployment (Recommended for Getting Started)

#### Single Node Deployment

```bash
# Build the Docker image
docker build -t rs3gw:latest .

# Run with basic configuration
docker run -d \
  --name rs3gw \
  -p 9000:9000 \
  -e RS3GW_STORAGE_ROOT=/data \
  -e RS3GW_BIND_ADDR=0.0.0.0:9000 \
  -v /path/to/storage:/data \
  rs3gw:latest
```

#### Docker Compose for Production

See `docker-compose.dev.yml` for a complete stack including:
- rs3gw service
- Prometheus for metrics
- Grafana for dashboards
- Jaeger for distributed tracing
- MinIO for backend integration

```bash
# Start the entire stack
docker-compose -f docker-compose.dev.yml up -d

# View logs
docker-compose -f docker-compose.dev.yml logs -f rs3gw

# Stop the stack
docker-compose -f docker-compose.dev.yml down
```

### 2. Kubernetes Deployment (Recommended for Production)

#### Helm Chart Deployment

```bash
# Add the Helm repository (if available)
# helm repo add rs3gw https://charts.rs3gw.io
# helm repo update

# Install with default values
helm install rs3gw ./k8s/helm/rs3gw

# Install with custom values
helm install rs3gw ./k8s/helm/rs3gw \
  --set replicaCount=3 \
  --set persistence.size=1Ti \
  --set resources.requests.cpu=4 \
  --set resources.requests.memory=16Gi

# Upgrade the deployment
helm upgrade rs3gw ./k8s/helm/rs3gw \
  --set image.tag=v1.0.0

# Uninstall
helm uninstall rs3gw
```

#### Kustomize Deployment

```bash
# Deploy using kustomize
kubectl apply -k k8s/

# Check deployment status
kubectl get pods -l app=rs3gw
kubectl get svc rs3gw

# View logs
kubectl logs -f deployment/rs3gw
```

### 3. Bare Metal / VM Deployment

#### System Requirements

- **OS**: Linux (Ubuntu 20.04+, RHEL 8+, or equivalent)
- **CPU**: 8+ cores for production workloads
- **RAM**: 32+ GB
- **Storage**: NVMe SSD recommended (10,000+ IOPS)
- **Network**: 10 Gbps network interface

#### Installation Steps

```bash
# 1. Install Rust (if building from source)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 2. Clone the repository
git clone https://github.com/cool-japan/rs3gw.git
cd rs3gw

# 3. Build the release binary
cargo build --release

# 4. Install the binary
sudo cp target/release/rs3gw /usr/local/bin/
sudo chmod +x /usr/local/bin/rs3gw

# 5. Create systemd service
sudo tee /etc/systemd/system/rs3gw.service > /dev/null <<EOF
[Unit]
Description=rs3gw Object Storage Gateway
After=network.target

[Service]
Type=simple
User=rs3gw
Group=rs3gw
EnvironmentFile=/etc/rs3gw/rs3gw.env
ExecStart=/usr/local/bin/rs3gw
Restart=on-failure
RestartSec=5s
LimitNOFILE=65536

[Install]
WantedBy=multi-user.target
EOF

# 6. Create configuration directory and user
sudo useradd -r -s /bin/false rs3gw
sudo mkdir -p /etc/rs3gw /var/lib/rs3gw/data
sudo chown -R rs3gw:rs3gw /var/lib/rs3gw

# 7. Create environment file
sudo tee /etc/rs3gw/rs3gw.env > /dev/null <<EOF
RS3GW_BIND_ADDR=0.0.0.0:9000
RS3GW_STORAGE_ROOT=/var/lib/rs3gw/data
RS3GW_ACCESS_KEY=your-access-key
RS3GW_SECRET_KEY=your-secret-key
RS3GW_COMPRESSION=zstd:3
RS3GW_CACHE_ENABLED=true
RS3GW_CACHE_MAX_SIZE_MB=4096
RS3GW_DEDUP_ENABLED=true
EOF

# 8. Start the service
sudo systemctl daemon-reload
sudo systemctl enable rs3gw
sudo systemctl start rs3gw

# 9. Check status
sudo systemctl status rs3gw
sudo journalctl -u rs3gw -f
```

---

## Security Best Practices

### 1. Authentication & Access Control

#### Enable AWS Signature V4 Authentication

```bash
# Set access credentials
export RS3GW_ACCESS_KEY="your-secure-access-key"
export RS3GW_SECRET_KEY="your-secure-secret-key"

# Never use empty credentials in production!
```

#### Configure ABAC (Attribute-Based Access Control)

Create bucket policies with time windows and IP restrictions:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": "*",
      "Action": ["s3:GetObject"],
      "Resource": "arn:aws:s3:::public-bucket/*"
    },
    {
      "Effect": "Deny",
      "Principal": "*",
      "Action": ["s3:PutObject", "s3:DeleteObject"],
      "Resource": "arn:aws:s3:::production-data/*",
      "Condition": {
        "IpAddress": {
          "aws:SourceIp": ["10.0.0.0/8", "192.168.0.0/16"]
        },
        "DateGreaterThan": {"aws:CurrentTime": "2024-01-01T09:00:00Z"},
        "DateLessThan": {"aws:CurrentTime": "2024-01-01T17:00:00Z"}
      }
    }
  ]
}
```

### 2. TLS/HTTPS Configuration

#### Generate TLS Certificates

```bash
# Self-signed certificate (development only)
openssl req -x509 -newkey rsa:4096 -nodes \
  -keyout server-key.pem \
  -out server-cert.pem \
  -days 365 \
  -subj "/CN=rs3gw.example.com"

# Production: Use Let's Encrypt
certbot certonly --standalone -d rs3gw.example.com
```

#### Enable TLS in rs3gw

```bash
export RS3GW_TLS_CERT="/path/to/server-cert.pem"
export RS3GW_TLS_KEY="/path/to/server-key.pem"
```

### 3. Encryption at Rest

```bash
# Enable AES-256-GCM encryption
export RS3GW_ENCRYPTION_ENABLED="true"
export RS3GW_ENCRYPTION_ALGORITHM="aes-256-gcm"

# Key rotation (recommended: monthly)
# Use rs3ctl to rotate encryption keys
rs3ctl maintenance rotate-keys --backup-old-keys
```

### 4. Audit Logging

```bash
# Enable comprehensive audit logging
export RS3GW_AUDIT_ENABLED="true"
export RS3GW_AUDIT_LOG_PATH="/var/log/rs3gw/audit.log"
export RS3GW_AUDIT_ROTATION_SIZE="100MB"
export RS3GW_AUDIT_SYSLOG_ENABLED="true"
export RS3GW_AUDIT_SYSLOG_ENDPOINT="syslog.example.com:514"

# Enable S3 log forwarding for long-term retention
export RS3GW_AUDIT_S3_ENABLED="true"
export RS3GW_AUDIT_S3_BUCKET="audit-logs"
export RS3GW_AUDIT_S3_PREFIX="rs3gw/"
```

### 5. Network Security

#### Firewall Configuration

```bash
# Allow only necessary ports
sudo ufw allow 9000/tcp  # rs3gw API
sudo ufw allow 9001/tcp  # Cluster communication (if cluster mode enabled)
sudo ufw deny from any to any
sudo ufw enable
```

#### Reverse Proxy with Nginx

```nginx
upstream rs3gw_backend {
    server 127.0.0.1:9000;
    keepalive 64;
}

server {
    listen 443 ssl http2;
    server_name s3.example.com;

    ssl_certificate /etc/letsencrypt/live/s3.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/s3.example.com/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Frame-Options "DENY" always;
    add_header X-Content-Type-Options "nosniff" always;

    # Increase timeouts for large uploads
    client_max_body_size 10G;
    client_body_timeout 600s;
    proxy_read_timeout 600s;
    proxy_send_timeout 600s;

    location / {
        proxy_pass http://rs3gw_backend;
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

---

## High Availability Setup

### 1. Multi-Node Cluster Configuration

```bash
# Node 1
export RS3GW_CLUSTER_ENABLED="true"
export RS3GW_CLUSTER_NODE_ID="node-1"
export RS3GW_CLUSTER_ADVERTISE_ADDR="10.0.1.10:9001"
export RS3GW_CLUSTER_PORT="9001"
export RS3GW_CLUSTER_SEED_NODES="10.0.1.11:9001,10.0.1.12:9001"
export RS3GW_REPLICATION_MODE="quorum"
export RS3GW_REPLICATION_FACTOR="3"

# Node 2
export RS3GW_CLUSTER_ENABLED="true"
export RS3GW_CLUSTER_NODE_ID="node-2"
export RS3GW_CLUSTER_ADVERTISE_ADDR="10.0.1.11:9001"
export RS3GW_CLUSTER_PORT="9001"
export RS3GW_CLUSTER_SEED_NODES="10.0.1.10:9001,10.0.1.12:9001"
export RS3GW_REPLICATION_MODE="quorum"
export RS3GW_REPLICATION_FACTOR="3"

# Node 3
export RS3GW_CLUSTER_ENABLED="true"
export RS3GW_CLUSTER_NODE_ID="node-3"
export RS3GW_CLUSTER_ADVERTISE_ADDR="10.0.1.12:9001"
export RS3GW_CLUSTER_PORT="9001"
export RS3GW_CLUSTER_SEED_NODES="10.0.1.10:9001,10.0.1.11:9001"
export RS3GW_REPLICATION_MODE="quorum"
export RS3GW_REPLICATION_FACTOR="3"
```

### 2. Load Balancing

#### HAProxy Configuration

```haproxy
global
    maxconn 50000
    log /dev/log local0
    user haproxy
    group haproxy
    daemon

defaults
    log global
    mode http
    option httplog
    option dontlognull
    timeout connect 5000
    timeout client 600000
    timeout server 600000

frontend s3_frontend
    bind *:9000
    bind *:443 ssl crt /etc/haproxy/certs/
    default_backend s3_backend

backend s3_backend
    balance roundrobin
    option httpchk GET /health
    http-check expect status 200
    server node1 10.0.1.10:9000 check inter 5s
    server node2 10.0.1.11:9000 check inter 5s
    server node3 10.0.1.12:9000 check inter 5s

listen stats
    bind *:8404
    stats enable
    stats uri /stats
    stats refresh 5s
    stats admin if TRUE
```

### 3. Cross-Region Replication

```bash
# Configure replication from US-WEST to US-EAST
rs3ctl replication configure my-bucket \
  --destination us-east-1.example.com:9000 \
  --filter prefix=important/ \
  --wan-optimization \
  --compression \
  --bandwidth-limit 100MB/s
```

---

## Monitoring & Observability

### 1. Prometheus Integration

#### Prometheus Configuration

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'rs3gw'
    static_configs:
      - targets:
        - 'rs3gw-node1:9000'
        - 'rs3gw-node2:9000'
        - 'rs3gw-node3:9000'
    metrics_path: '/metrics'
    scrape_interval: 5s
```

#### Key Metrics to Monitor

- **Request Rate**: `rs3gw_requests_total`
- **Latency**: `rs3gw_request_duration_seconds`
- **Error Rate**: `rs3gw_errors_total`
- **Storage Usage**: `rs3gw_storage_bytes_total`
- **Object Count**: `rs3gw_objects_total`
- **Cache Hit Rate**: `rs3gw_cache_hits_total / rs3gw_cache_requests_total`

### 2. Grafana Dashboards

Pre-built dashboards are available in `deploy/grafana/`:
- **Overview Dashboard**: Request rates, latency, error rates
- **Storage Dashboard**: Storage usage, object counts, bucket statistics
- **Performance Dashboard**: Cache hit rates, deduplication savings, throughput
- **Cluster Dashboard**: Node health, replication lag, consistency

### 3. Distributed Tracing with Jaeger

```bash
# Configure OpenTelemetry export
export OTEL_EXPORTER_OTLP_ENDPOINT="http://jaeger:4317"
export OTEL_TRACES_SAMPLER="traceidratio"
export OTEL_TRACES_SAMPLER_ARG="0.1"  # Sample 10% of traces
export RS3GW_SERVICE_NAME="rs3gw-production"
export RS3GW_SERVICE_VERSION="1.0.0"
export RS3GW_ENVIRONMENT="production"
```

### 4. Observability API Endpoints

rs3gw provides dedicated observability endpoints:

```bash
# Get profiling data (CPU, memory, I/O)
curl http://rs3gw:9000/api/observability/profiling

# Get profiling data in pprof format for flamegraphs
curl http://rs3gw:9000/api/observability/profiling?format=pprof

# Get business metrics
curl http://rs3gw:9000/api/observability/business-metrics

# Get detected anomalies
curl http://rs3gw:9000/api/observability/anomalies

# Filter anomalies by severity
curl http://rs3gw:9000/api/observability/anomalies?severity=high

# Get resource manager statistics
curl http://rs3gw:9000/api/observability/resources

# Comprehensive health check
curl http://rs3gw:9000/api/observability/health
```

### 5. Alerts

#### Prometheus Alert Rules

```yaml
# alerts.yml
groups:
  - name: rs3gw_alerts
    interval: 30s
    rules:
      - alert: HighErrorRate
        expr: rate(rs3gw_errors_total[5m]) > 0.05
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "High error rate detected"
          description: "Error rate is {{ $value }} errors/sec"

      - alert: HighLatency
        expr: histogram_quantile(0.99, rate(rs3gw_request_duration_seconds_bucket[5m])) > 1.0
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "High latency detected"
          description: "P99 latency is {{ $value }}s"

      - alert: LowDiskSpace
        expr: (rs3gw_storage_bytes_total / 1e12) > 0.9 * rs3gw_storage_capacity_bytes
        for: 15m
        labels:
          severity: warning
        annotations:
          summary: "Low disk space"
          description: "Storage usage is at {{ $value }}%"

      - alert: NodeDown
        expr: up{job="rs3gw"} == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "rs3gw node is down"
          description: "Node {{ $labels.instance }} has been down for >1 minute"
```

---

## Backup & Disaster Recovery

### 1. Point-in-Time Snapshots

```bash
# Create a snapshot
rs3ctl backup create --bucket my-bucket --snapshot-id snapshot-20240101

# List snapshots
rs3ctl backup list --bucket my-bucket

# Restore from snapshot
rs3ctl backup restore --bucket my-bucket --snapshot-id snapshot-20240101

# Delete old snapshots
rs3ctl backup cleanup --bucket my-bucket --retention-days 30
```

### 2. Automated Backup Schedule

```bash
# Create systemd timer for daily backups
sudo tee /etc/systemd/system/rs3gw-backup.service > /dev/null <<EOF
[Unit]
Description=rs3gw Daily Backup
After=network.target

[Service]
Type=oneshot
User=rs3gw
ExecStart=/usr/local/bin/rs3ctl backup create --all-buckets --snapshot-id daily-\$(date +\%Y\%m\%d)
EOF

sudo tee /etc/systemd/system/rs3gw-backup.timer > /dev/null <<EOF
[Unit]
Description=rs3gw Daily Backup Timer
Requires=rs3gw-backup.service

[Timer]
OnCalendar=daily
OnCalendar=02:00
Persistent=true

[Install]
WantedBy=timers.target
EOF

sudo systemctl enable rs3gw-backup.timer
sudo systemctl start rs3gw-backup.timer
```

### 3. Cross-Region Backup

```bash
# Replicate backups to remote site
rs3ctl backup replicate \
  --source-bucket my-bucket \
  --destination s3://backup-bucket/rs3gw/ \
  --destination-endpoint https://s3.remote.example.com
```

### 4. Disaster Recovery Plan

1. **Backup Verification**: Test restores monthly
2. **RTO Target**: < 1 hour for critical data
3. **RPO Target**: < 15 minutes (replication lag)
4. **Multi-Region**: Maintain replicas in 2+ regions
5. **Documentation**: Keep runbooks updated

---

## Performance Optimization

See [Performance Tuning Guide](performance_tuning.md) for detailed optimization recommendations.

### Quick Wins

```bash
# Enable all performance features
export RS3GW_COMPRESSION="zstd:3"
export RS3GW_CACHE_ENABLED="true"
export RS3GW_CACHE_MAX_SIZE_MB="8192"
export RS3GW_DEDUP_ENABLED="true"
export RS3GW_ZEROCOPY_DIRECT_IO="true"
export RS3GW_ZEROCOPY_SPLICE="true"
export RS3GW_ZEROCOPY_MMAP="true"
```

---

## Troubleshooting

### 1. Common Issues

#### High Memory Usage

```bash
# Check memory pressure
curl http://rs3gw:9000/api/observability/resources

# Reduce cache size
export RS3GW_CACHE_MAX_SIZE_MB="2048"

# Enable memory pressure detection
export RS3GW_MEMORY_THRESHOLD="0.85"
```

#### Slow Requests

```bash
# Check profiling data
curl http://rs3gw:9000/api/observability/profiling

# Enable query optimization
export RS3GW_SELECT_CACHE_ENABLED="true"
export RS3GW_SELECT_PARALLEL_THRESHOLD="10485760"  # 10MB

# Increase worker threads
export RS3GW_MAX_THREADS="32"
```

#### Replication Lag

```bash
# Check replication metrics
rs3ctl replication metrics

# Increase batch size for WAN optimization
rs3ctl replication configure my-bucket \
  --batch-size 100 \
  --wan-optimization \
  --compression
```

### 2. Debugging Tools

```bash
# Enable debug logging
export RUST_LOG="rs3gw=debug"

# Get detailed profiling with flamegraphs
curl http://rs3gw:9000/api/observability/profiling?format=pprof > cpu.pprof
go tool pprof -http=:8080 cpu.pprof

# Check anomaly detection
curl http://rs3gw:9000/api/observability/anomalies?severity=high

# Verify integrity
rs3ctl maintenance check-integrity --bucket my-bucket
```

### 3. Support Resources

- **Documentation**: https://rs3gw.io/docs
- **GitHub Issues**: https://github.com/cool-japan/rs3gw/issues
- **Community Forum**: https://community.rs3gw.io
- **Security**: security@rs3gw.io

---

## ML/AI Features & Dataset Preprocessing

rs3gw v5.0.0+ includes comprehensive ML/AI features for model registry, dataset version control, and data preprocessing pipelines.

### 1. Model Registry

Store and version ML models with full lineage tracking:

```bash
# Register a model via S3 API (using special prefix)
aws s3 cp model.pt s3://models/my-model/v1.0.0/model.pt \
  --metadata framework=pytorch,architecture=resnet50,params=25M

# List model versions
rs3ctl models list my-model

# Get model metadata
rs3ctl models get my-model --version v1.0.0
```

### 2. Dataset Version Control

Track dataset versions with split management and model lineage:

```bash
# Register a dataset
rs3ctl datasets register training-data \
  --description "ImageNet training subset" \
  --author "ml-team"

# Create a dataset version
rs3ctl datasets version training-data \
  --version v1.0.0 \
  --uri s3://datasets/imagenet-train \
  --samples 1000000

# Add dataset splits
rs3ctl datasets split training-data v1.0.0 \
  --split train --uri s3://datasets/imagenet-train/train \
  --samples 900000

rs3ctl datasets split training-data v1.0.0 \
  --split val --uri s3://datasets/imagenet-train/val \
  --samples 100000

# Link dataset to trained model
rs3ctl datasets link training-data v1.0.0 \
  --model my-model --model-version v1.0.0 \
  --split train
```

### 3. Dataset Preprocessing API

Create and apply preprocessing pipelines via HTTP API:

#### Create a Preprocessing Pipeline

```bash
curl -X POST http://rs3gw:9000/api/preprocessing/pipelines \
  -H "Content-Type: application/json" \
  -d '{
    "id": "imagenet-pipeline",
    "name": "ImageNet Preprocessing",
    "version": "1.0.0",
    "description": "Standard ImageNet preprocessing with normalization",
    "steps": [
      {
        "id": "resize",
        "step_type": "image_resize",
        "config": {
          "width": 224,
          "height": 224,
          "mode": "fit",
          "filter": "lanczos3"
        },
        "cache": true
      },
      {
        "id": "normalize",
        "step_type": "image_normalization",
        "config": {
          "mean": [0.485, 0.456, 0.406],
          "std": [0.229, 0.224, 0.225],
          "normalize_range": true
        },
        "cache": true
      }
    ],
    "metadata": {
      "author": "ml-team",
      "purpose": "inference"
    }
  }'
```

#### Apply Pipeline to Objects

```bash
# Apply preprocessing to a single image
curl -X POST http://rs3gw:9000/api/preprocessing/apply \
  -H "Content-Type: application/json" \
  -d '{
    "pipeline_id": "imagenet-pipeline",
    "bucket": "raw-images",
    "key": "photo.jpg",
    "output_bucket": "processed-images",
    "output_key": "photo_preprocessed.jpg"
  }'
```

#### Manage Pipelines

```bash
# List all pipelines
curl http://rs3gw:9000/api/preprocessing/pipelines

# Get a specific pipeline
curl http://rs3gw:9000/api/preprocessing/pipelines/imagenet-pipeline

# Validate a pipeline definition
curl -X POST http://rs3gw:9000/api/preprocessing/validate \
  -H "Content-Type: application/json" \
  -d '{ "pipeline": { ... } }'

# Delete a pipeline
curl -X DELETE http://rs3gw:9000/api/preprocessing/pipelines/imagenet-pipeline

# Get cache statistics
curl http://rs3gw:9000/api/preprocessing/cache/stats

# Clear preprocessing cache
curl -X POST http://rs3gw:9000/api/preprocessing/cache/clear
```

#### CLI Alternative

```bash
# Create a pipeline from JSON file
rs3ctl preprocessing create examples/imagenet_pipeline.json

# List pipelines
rs3ctl preprocessing list

# Apply pipeline
rs3ctl preprocessing apply imagenet-pipeline \
  --bucket raw-images \
  --key photo.jpg \
  --output-bucket processed-images \
  --output-key photo_preprocessed.jpg

# Validate pipeline
rs3ctl preprocessing validate examples/pipeline.json
```

### 4. Production Configuration

Configure preprocessing features in production:

```bash
# Environment variables
export RS3GW_STORAGE_ROOT=/data
export RS3GW_CACHE_ENABLED=true
export RS3GW_CACHE_MAX_SIZE_MB=4096  # 4GB for preprocessing cache

# In rs3gw.toml
[preprocessing]
cache_max_size_mb = 4096
cache_max_objects = 50000
default_cache_ttl_secs = 3600
```

### 5. Integration with ML Workflows

#### PyTorch Integration Example

```python
import boto3
import json

s3 = boto3.client('s3', endpoint_url='http://rs3gw:9000')

# Register model
s3.upload_file(
    'model.pt',
    'models',
    'my-model/v1.0.0/model.pt',
    ExtraArgs={
        'Metadata': {
            'framework': 'pytorch',
            'architecture': 'resnet50'
        }
    }
)

# Apply preprocessing via API
import requests

response = requests.post('http://rs3gw:9000/api/preprocessing/apply', json={
    'pipeline_id': 'imagenet-pipeline',
    'bucket': 'raw-images',
    'key': 'batch/image_001.jpg',
    'output_bucket': 'processed',
    'output_key': 'batch/image_001.jpg'
})

print(f"Processed: {response.json()['output_key']}")
```

#### Batch Processing

```bash
# Batch preprocessing using GNU parallel
aws s3 ls s3://raw-images/ --recursive | \
  awk '{print $4}' | \
  parallel -j 10 \
    'curl -X POST http://rs3gw:9000/api/preprocessing/apply \
      -H "Content-Type: application/json" \
      -d "{\"pipeline_id\":\"imagenet-pipeline\",\"bucket\":\"raw-images\",\"key\":\"{}\",\"output_bucket\":\"processed\"}"'
```

### 6. Monitoring Preprocessing Operations

```bash
# Monitor cache performance
curl http://rs3gw:9000/api/preprocessing/cache/stats

# Watch preprocessing metrics in Grafana
# Navigate to: http://grafana:3000/d/preprocessing-dashboard

# Check preprocessing logs
docker logs rs3gw 2>&1 | grep preprocessing

# Monitor via observability API
curl http://rs3gw:9000/api/observability/business-metrics | \
  jq '.preprocessing'
```

---

### 7. Apache Arrow Flight for High-Performance Data Transfer

rs3gw implements the Apache Arrow Flight protocol for zero-copy, high-performance data transfer with Python data science tools.

#### Features

- **Zero-copy data transfer** between rs3gw and Python/Spark/Dask
- **19x faster** than REST API for listing operations
- **4.3x faster** for large file downloads
- **3.7x lower** memory usage
- **Pandas DataFrame** integration with type hints
- **Time-series** metadata for temporal analysis
- **Categorical columns** for memory efficiency

#### Configuration

Arrow Flight runs on the same port as the main HTTP server (default: 9000):

```bash
export RS3GW_BIND_ADDR=0.0.0.0:9000
# Arrow Flight automatically available on port 9000
```

#### Python Client Example

```python
from arrow_flight_pandas import RS3FlightClient
import pandas as pd

# Connect to rs3gw
client = RS3FlightClient(host="rs3gw.example.com", port=9000)

# List objects as optimized Pandas DataFrame
df = client.list_objects_as_dataframe("my-bucket", prefix="data/")

# DataFrame has optimized types:
# - 'last_modified': datetime64[ns]
# - 'content_type': category (90% memory reduction)
# - 'key': index

print(df.head())
print(f"Total size: {df['size'].sum()} bytes")
print(f"Content types: {df['content_type'].value_counts()}")
```

#### Spark Integration

```python
from pyspark.sql import SparkSession

spark = SparkSession.builder.appName("RS3GW").getOrCreate()

# Get data via Arrow Flight (zero-copy)
df_pandas = client.list_objects_as_dataframe("data-lake")
df_spark = spark.createDataFrame(df_pandas)

# Distributed processing
result = df_spark.groupBy("content_type").count()
result.show()
```

#### Dask Integration

```python
import dask.dataframe as dd

# Get large dataset via Arrow Flight
df_pandas = client.list_objects_as_dataframe("big-data")

# Create Dask DataFrame with automatic partitioning
df_dask = dd.from_pandas(df_pandas, npartitions=10)

# Distributed operations
result = df_dask.groupby('content_type').size().compute()
```

#### Performance Benchmarks

| Operation | REST API | Arrow Flight | Speedup |
|-----------|----------|--------------|---------|
| List 1000 objects | 850ms | 45ms | **19x** |
| Download 100MB | 1200ms | 280ms | **4.3x** |
| Memory usage | 450MB | 120MB | **3.7x** |

*Benchmarks on local network, single-node rs3gw*

#### Production Tips

1. **Firewall Configuration**: Ensure port 9000 allows both HTTP and gRPC (Arrow Flight)
2. **Load Balancing**: Arrow Flight works with standard HTTP/2 load balancers
3. **TLS**: Arrow Flight automatically uses TLS when `RS3GW_TLS_CERT` is configured
4. **Monitoring**: Arrow Flight operations appear in Prometheus metrics as `grpc_*`

For detailed examples and API reference, see [examples/ARROW_FLIGHT_README.md](../examples/ARROW_FLIGHT_README.md).

---

### 8. gRPC API for High-Performance Binary Protocol

rs3gw provides a full gRPC API for low-latency, high-throughput operations using Protocol Buffers.

#### Features

- **Binary protocol** with 60% smaller message size vs. JSON
- **HTTP/2 multiplexing** for concurrent streams
- **Bi-directional streaming** for large file transfers
- **Type-safe** clients in Python, Go, Java, and Rust
- **40+ S3 operations** supported

#### Configuration

gRPC runs on the same port as the HTTP server:

```bash
export RS3GW_BIND_ADDR=0.0.0.0:9000
# gRPC automatically available on port 9000
```

#### Client Libraries

Pre-generated clients are available in `clients/` directory:

**Python Client:**
```python
import grpc
from proto import s3_pb2, s3_pb2_grpc

# Connect to rs3gw
channel = grpc.insecure_channel('rs3gw.example.com:9000')
stub = s3_pb2_grpc.S3ServiceStub(channel)

# List buckets
response = stub.ListBuckets(s3_pb2.ListBucketsRequest())
for bucket in response.buckets:
    print(f"Bucket: {bucket.name}, Created: {bucket.creation_date}")

# Upload object with streaming
def generate_chunks():
    with open('large-file.bin', 'rb') as f:
        while chunk := f.read(1024 * 1024):  # 1MB chunks
            yield s3_pb2.PutObjectRequest(
                bucket='my-bucket',
                key='large-file.bin',
                body=chunk
            )

response = stub.PutObject(generate_chunks())
print(f"Uploaded: {response.etag}")
```

**Go Client:**
```go
import (
    "context"
    pb "github.com/cool-japan/rs3gw/proto"
    "google.golang.org/grpc"
)

conn, _ := grpc.Dial("rs3gw.example.com:9000", grpc.WithInsecure())
defer conn.Close()
client := pb.NewS3ServiceClient(conn)

// List objects
resp, _ := client.ListObjectsV2(context.Background(), &pb.ListObjectsV2Request{
    Bucket: "my-bucket",
    MaxKeys: 1000,
})

for _, obj := range resp.Contents {
    fmt.Printf("Key: %s, Size: %d\n", obj.Key, obj.Size)
}
```

#### Performance vs REST

| Metric | REST API | gRPC | Improvement |
|--------|----------|------|-------------|
| Message size (JSON vs Protobuf) | 1000 bytes | 400 bytes | **60% smaller** |
| Latency (p50) | 45ms | 12ms | **3.7x faster** |
| Throughput (ops/sec) | 2500 | 8500 | **3.4x higher** |
| Connection overhead | 150ms | 5ms | **30x faster** |

#### Production Deployment

**With TLS:**
```bash
export RS3GW_TLS_CERT="/path/to/cert.pem"
export RS3GW_TLS_KEY="/path/to/key.pem"
# gRPC automatically uses TLS
```

**Load Balancing (Nginx):**
```nginx
upstream grpc_backend {
    server rs3gw-1:9000;
    server rs3gw-2:9000;
    server rs3gw-3:9000;
}

server {
    listen 443 ssl http2;

    ssl_certificate /etc/ssl/cert.pem;
    ssl_certificate_key /etc/ssl/key.pem;

    location / {
        grpc_pass grpcs://grpc_backend;
        grpc_set_header Host $host;
    }
}
```

For client generation scripts and examples, see `clients/` directory.

---

### 9. WebSocket Event Streaming

rs3gw provides real-time event notifications via WebSocket for monitoring object changes.

#### Features

- **Real-time notifications** for all S3 operations
- **Event filtering** by bucket, prefix, and event type
- **Connection multiplexing** with broadcast channels
- **Automatic ping/pong** for connection health
- **JSON event format** for easy parsing

#### Configuration

WebSocket endpoint is available at `/events/stream`:

```bash
# No special configuration required
# WebSocket runs on same port as HTTP server
```

#### Connecting with JavaScript

```javascript
const ws = new WebSocket('ws://rs3gw.example.com:9000/events/stream?bucket=my-bucket&prefix=logs/');

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);

    if (data.type === 'welcome') {
        console.log('Connected:', data.message);
    } else if (data.type === 'event') {
        console.log('Event:', data.event_type, data.bucket, data.key);
    }
};

ws.onerror = (error) => console.error('WebSocket error:', error);
ws.onclose = () => console.log('WebSocket closed');
```

#### Connecting with Python

```python
import websocket
import json

def on_message(ws, message):
    data = json.loads(message)
    if data.get('type') == 'event':
        print(f"Event: {data['event_type']} - {data['bucket']}/{data['key']}")

ws = websocket.WebSocketApp(
    "ws://rs3gw.example.com:9000/events/stream?bucket=my-bucket",
    on_message=on_message
)
ws.run_forever()
```

#### Event Filtering

Filter events using query parameters:

```
# Filter by bucket
ws://rs3gw:9000/events/stream?bucket=my-bucket

# Filter by prefix
ws://rs3gw:9000/events/stream?bucket=my-bucket&prefix=logs/2025/

# Filter by event types (comma-separated)
ws://rs3gw:9000/events/stream?event_types=ObjectCreated,ObjectRemoved
```

#### Event Types

- `ObjectCreated:Put` - Object uploaded
- `ObjectCreated:Post` - Object created via POST
- `ObjectCreated:Copy` - Object copied
- `ObjectRemoved:Delete` - Object deleted
- `ObjectRemoved:DeleteMarkerCreated` - Delete marker created
- `BucketCreated` - Bucket created
- `BucketRemoved` - Bucket deleted
- `MultipartUploadStarted` - Multipart upload initiated
- `MultipartUploadCompleted` - Multipart upload completed
- `MultipartUploadAborted` - Multipart upload aborted

#### Production Deployment

**Nginx WebSocket Proxy:**
```nginx
location /events/stream {
    proxy_pass http://rs3gw_backend;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_set_header Host $host;
    proxy_read_timeout 3600s;  # Keep connection alive
}
```

**Monitoring:**
```bash
# Check active WebSocket connections
curl http://rs3gw:9000/api/observability/resources | jq '.websocket_connections'

# Monitor event broadcast rate
curl http://rs3gw:9000/metrics | grep ws_events_total
```

---

### 10. GraphQL API for Flexible Queries

rs3gw provides a GraphQL API for flexible metadata queries and aggregations.

#### Features

- **Flexible queries** - Request exactly the data you need
- **Aggregations** - Storage statistics across buckets
- **Schema introspection** - Self-documenting API
- **GraphQL Playground** - Interactive query builder
- **Batch operations** - Multiple queries in one request

#### Endpoints

- **Playground**: `GET http://rs3gw:9000/graphql` (interactive UI)
- **Query**: `POST http://rs3gw:9000/graphql` (API endpoint)

#### Example Queries

**List Buckets with Statistics:**
```graphql
query {
  buckets {
    name
    creationDate
    objectCount
    totalSize
  }
}
```

**Search Objects Across Buckets:**
```graphql
query {
  searchObjects(pattern: "*.json", limit: 10) {
    key
    size
    lastModified
    contentType
  }
}
```

**Get Bucket Details:**
```graphql
query {
  bucket(name: "my-bucket") {
    name
    objectCount
    totalSize
    tagging {
      key
      value
    }
    policy
  }
}
```

**Storage Statistics:**
```graphql
query {
  storageStats {
    totalBuckets
    totalObjects
    totalSize
    averageObjectSize
  }
}
```

#### Using with cURL

```bash
curl -X POST http://rs3gw:9000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { buckets { name objectCount totalSize } }"
  }'
```

#### Using with Python

```python
import requests

query = """
query {
  bucket(name: "my-bucket") {
    name
    objectCount
    totalSize
  }
}
"""

response = requests.post(
    'http://rs3gw:9000/graphql',
    json={'query': query}
)

data = response.json()
bucket = data['data']['bucket']
print(f"Bucket: {bucket['name']}, Objects: {bucket['objectCount']}")
```

#### GraphQL Playground

Access the interactive playground at `http://rs3gw:9000/graphql` in your browser:

- **Autocomplete** - Schema-aware query building
- **Documentation** - Built-in schema explorer
- **History** - Query history tracking
- **Variables** - Parameterized queries

#### Production Configuration

**Enable CORS for GraphQL:**
```bash
export RS3GW_CORS_ENABLED="true"
export RS3GW_CORS_ALLOWED_ORIGINS="https://dashboard.example.com"
```

**Rate Limiting:**
```bash
export RS3GW_THROTTLE_RPS="100"  # Limit to 100 queries/sec
```

**Monitoring:**
```bash
# GraphQL query metrics
curl http://rs3gw:9000/metrics | grep graphql_
```

---

## Advanced Performance Features

### 1. S3 Select Query Result Caching

rs3gw v5.1.0+ includes intelligent caching for S3 Select queries, providing 100x-1000x faster repeated queries.

#### How It Works

- **ETag-based cache keys**: Results are automatically invalidated when objects are modified
- **LRU eviction**: Least recently used entries are evicted when cache is full
- **TTL expiration**: Configurable time-to-live for cache entries (default: 1 hour)
- **Memory limits**: Prevents cache from consuming excessive memory

#### Configuration

The S3 Select cache can be configured via environment variables:

```bash
# Enable/disable S3 Select result caching (default: true)
export RS3GW_SELECT_CACHE_ENABLED="true"

# Maximum number of cached query results (default: 1000)
export RS3GW_SELECT_CACHE_MAX_ENTRIES="1000"

# Maximum memory usage for cache in MB (default: 100MB)
export RS3GW_SELECT_CACHE_MAX_MEMORY_MB="100"

# Default TTL for cached results in seconds (default: 3600 = 1 hour)
export RS3GW_SELECT_CACHE_TTL="3600"
```

**Recommended Production Settings**:

```bash
# High-traffic production environment
export RS3GW_SELECT_CACHE_MAX_ENTRIES="5000"
export RS3GW_SELECT_CACHE_MAX_MEMORY_MB="500"
export RS3GW_SELECT_CACHE_TTL="7200"  # 2 hours

# Low-memory environment
export RS3GW_SELECT_CACHE_MAX_ENTRIES="500"
export RS3GW_SELECT_CACHE_MAX_MEMORY_MB="50"
export RS3GW_SELECT_CACHE_TTL="1800"  # 30 minutes
```

#### Cache Management API

**Get cache statistics:**
```bash
curl http://rs3gw:9000/api/select/cache/stats

{
  "stats": {
    "gets": 1500,
    "hits": 1200,
    "misses": 300,
    "evictions": 5,
    "expirations": 2,
    "current_entries": 850,
    "memory_bytes": 45678900,
    "max_entries": 1000,
    "max_memory_bytes": 104857600
  },
  "timestamp": "2026-01-02T12:34:56.789Z"
}
```

**Clear all cached results:**
```bash
curl -X POST http://rs3gw:9000/api/select/cache/clear

{
  "status": "success",
  "message": "Cache cleared successfully"
}
```

**Invalidate cache for specific object:**
```bash
curl -X DELETE http://rs3gw:9000/api/select/cache/invalidate/etag123

{
  "status": "success",
  "etag": "etag123",
  "message": "Cache invalidated for object"
}
```

#### Cache Headers

S3 Select responses include a custom header indicating cache status:

- `x-amz-select-cache: HIT` - Result served from cache
- `x-amz-select-cache: MISS` - Query executed and result cached

#### Performance Benefits

| Scenario | Without Cache | With Cache | Improvement |
|----------|---------------|------------|-------------|
| Repeated query on 10MB CSV | 250ms | 2ms | **125x faster** |
| Dashboard with 20 queries | 5 seconds | 40ms | **125x faster** |
| Analytics on static dataset | 500ms | 3ms | **167x faster** |

#### Use Cases

- **Dashboards**: Frequent queries on relatively static data
- **Analytics**: Repeated analysis on historical datasets
- **Data Exploration**: Users running similar queries iteratively
- **Reporting**: Scheduled reports with consistent queries

#### Monitoring

rs3gw exposes Prometheus metrics for S3 Select cache monitoring:

**Available Metrics**:

- `select_cache_hits` (counter) - Total number of cache hits
- `select_cache_misses` (counter) - Total number of cache misses
- `select_cache_evictions` (counter) - Total number of LRU evictions
- `select_cache_expirations` (counter) - Total number of TTL expirations
- `select_cache_entries` (gauge) - Current number of entries in cache
- `select_cache_memory_bytes` (gauge) - Current memory usage in bytes

**Query Metrics**:

```bash
# Get all cache metrics
curl http://rs3gw:9000/metrics | grep select_cache

# Calculate hit rate
select_cache_hits / (select_cache_hits + select_cache_misses) * 100

# Check memory utilization
select_cache_memory_bytes / max_memory_bytes * 100
```

**Prometheus Queries** (PromQL):

```promql
# Cache hit rate over 5 minutes
rate(select_cache_hits[5m]) / (rate(select_cache_hits[5m]) + rate(select_cache_misses[5m]))

# Cache memory usage percentage
select_cache_memory_bytes / (100 * 1024 * 1024) * 100

# Eviction rate (evictions per second)
rate(select_cache_evictions[5m])

# Cache efficiency score (hits per query)
rate(select_cache_hits[5m]) / rate(select_cache_gets[5m])
```

**Grafana Dashboard**:

Example dashboard panels:

1. **Cache Hit Rate** - Line graph showing hit rate over time
2. **Memory Usage** - Gauge showing current memory utilization
3. **Cache Operations** - Stacked area chart (hits, misses, evictions)
4. **Entry Count** - Single stat showing current cache entries

### 2. Intelligent Data Tiering

rs3gw provides automated data tiering based on access patterns and business rules.

#### Tiering API

**Get tiering policy for a bucket:**
```bash
curl http://rs3gw:9000/api/tiering/policies/my-bucket

{
  "bucket": "my-bucket",
  "policy": {
    "enabled": true,
    "rules": [
      {
        "id": "archive-old-data",
        "filter": {
          "prefix": "logs/",
          "min_age_days": 90
        },
        "transitions": [
          {
            "storage_class": "GLACIER",
            "days": 90
          }
        ]
      }
    ]
  },
  "status": "success"
}
```

**Set tiering policy:**
```bash
curl -X PUT http://rs3gw:9000/api/tiering/policies/my-bucket \
  -H "Content-Type: application/json" \
  -d '{
    "enabled": true,
    "rules": [
      {
        "id": "frequent-to-infrequent",
        "transitions": [
          {
            "storage_class": "STANDARD_IA",
            "days": 30
          },
          {
            "storage_class": "GLACIER",
            "days": 90
          }
        ]
      }
    ]
  }'
```

**Analyze tiering recommendations:**
```bash
curl -X POST http://rs3gw:9000/api/tiering/analyze/my-bucket

{
  "analysis": {
    "total_objects": 10000,
    "potential_savings": 45.2,
    "recommendations": [
      {
        "object_count": 3500,
        "current_class": "STANDARD",
        "recommended_class": "STANDARD_IA",
        "estimated_savings_pct": 50.0
      }
    ]
  },
  "timestamp": "2026-01-02T12:34:56.789Z"
}
```

**Get tiering history:**
```bash
curl http://rs3gw:9000/api/tiering/history/my-bucket

{
  "bucket": "my-bucket",
  "transitions": [
    {
      "key": "logs/2025-01-01.log",
      "from_class": "STANDARD",
      "to_class": "GLACIER",
      "timestamp": "2026-01-02T00:00:00Z",
      "reason": "age-based-policy"
    }
  ],
  "total_count": 1250
}
```

#### Tiering Policies

rs3gw supports several pre-configured tiering policies:

**Balanced Policy** (Default):
```bash
curl -X PUT http://rs3gw:9000/api/tiering/policies/my-bucket \
  -H "Content-Type: application/json" \
  -d '{"preset": "balanced"}'

# Transitions:
# - 30 days → STANDARD_IA
# - 90 days → INTELLIGENT_TIERING
# - 180 days → GLACIER
```

**Aggressive Archival:**
```bash
curl -X PUT http://rs3gw:9000/api/tiering/policies/my-bucket \
  -H "Content-Type: application/json" \
  -d '{"preset": "aggressive"}'

# Transitions:
# - 7 days → STANDARD_IA
# - 30 days → GLACIER
# - 365 days → DEEP_ARCHIVE
```

**Cost-Optimized:**
```bash
curl -X PUT http://rs3gw:9000/api/tiering/policies/my-bucket \
  -H "Content-Type: application/json" \
  -d '{"preset": "cost-optimized"}'

# Transitions:
# - 14 days → INTELLIGENT_TIERING
# - 60 days → GLACIER
```

#### Predictive Tiering

rs3gw uses ML-based access pattern analysis for predictive tiering:

```bash
curl -X POST http://rs3gw:9000/api/tiering/analyze/my-bucket/predictive

{
  "analysis": {
    "access_patterns": {
      "periodic": 450,
      "bursty": 230,
      "trending": 120,
      "declining": 3200
    },
    "recommendations": [
      {
        "pattern": "declining",
        "object_count": 3200,
        "suggested_action": "transition_to_ia",
        "confidence": 0.92
      }
    ]
  },
  "timestamp": "2026-01-02T12:34:56.789Z"
}
```

#### Benefits

- **Cost Savings**: Automatically move infrequently accessed data to cheaper storage tiers
- **Performance**: Keep hot data in fast storage classes
- **Compliance**: Meet data retention requirements automatically
- **Predictive**: ML-based access pattern analysis

#### Monitoring

```bash
# Check tiering statistics
curl http://rs3gw:9000/api/tiering/stats/my-bucket

# View capacity recommendations
curl http://rs3gw:9000/api/tiering/recommendations/my-bucket/capacity

# Monitor via Prometheus
curl http://rs3gw:9000/metrics | grep tiering_
```

---

## Conclusion

This guide covers the essential aspects of deploying rs3gw in production. For additional details, see:

- [Performance Tuning Guide](performance_tuning.md)
- [rs3ctl CLI Reference](rs3ctl.md)
- [WebSocket Event Streaming](websocket.md)
- [Transformations Guide](transformations.md)
- [Arrow Flight Examples](../examples/ARROW_FLIGHT_README.md)
- [Preprocessing Pipelines](../examples/PREPROCESSING_PIPELINES.md)
- [API Documentation](../src/api/README.md)
- [Storage Documentation](../src/storage/README.md)

Remember to regularly review logs, metrics, and alerts to ensure optimal operation and preemptively address potential issues.
