# Prometheus Integration for rs3gw

rs3gw exposes Prometheus-compatible metrics at the `/metrics` endpoint in the standard text exposition format (version 0.0.4).

---

## Scrape Configuration

Add the following job to your `prometheus.yml`:

```yaml
scrape_configs:
  - job_name: rs3gw
    scrape_interval: 15s
    scrape_timeout: 10s
    static_configs:
      - targets:
          - "localhost:9000"
    metrics_path: /metrics
    # Optional: TLS configuration when rs3gw runs with TLS enabled
    # scheme: https
    # tls_config:
    #   ca_file: /path/to/ca.pem
    #   cert_file: /path/to/client.pem
    #   key_file: /path/to/client.key

  # Kubernetes service discovery example
  # - job_name: rs3gw-k8s
  #   kubernetes_sd_configs:
  #     - role: pod
  #   relabel_configs:
  #     - source_labels: [__meta_kubernetes_pod_label_app]
  #       action: keep
  #       regex: rs3gw
  #     - source_labels: [__meta_kubernetes_pod_ip]
  #       target_label: __address__
  #       replacement: "${1}:9000"
```

---

## Metrics Reference

### Request Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `s3_requests_total` | Counter | Total number of S3 API requests | `operation`, `bucket` |
| `s3_errors_total` | Counter | Total number of S3 API errors | `operation`, `bucket`, `error_code` |
| `s3_request_duration_seconds` | Histogram | Request latency distribution | `operation`, `bucket` |

### Object Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `s3_object_size_bytes` | Histogram | Distribution of object sizes for PUT/GET operations | `operation` |
| `s3_objects_total` | Gauge | Current total number of objects stored | `bucket` |
| `s3_storage_bytes_total` | Gauge | Total bytes stored across all buckets | `bucket` |
| `s3_buckets_total` | Gauge | Total number of buckets | — |

### Cache Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `s3_cache_hits_total` | Counter | Number of cache hits | `cache_type` |
| `s3_cache_misses_total` | Counter | Number of cache misses | `cache_type` |
| `s3_cache_evictions_total` | Counter | Number of cache evictions | `cache_type` |
| `s3_cache_size_bytes` | Gauge | Current in-memory cache size in bytes | `cache_type` |

### Cluster & Replication Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `rs3gw_cluster_total_nodes` | Gauge | Total number of cluster nodes | — |
| `rs3gw_cluster_healthy_nodes` | Gauge | Number of healthy cluster nodes | — |
| `rs3gw_replication_lag_ms` | Gauge | Replication lag to follower nodes in milliseconds | `destination` |
| `rs3gw_replication_queue_depth` | Gauge | Number of pending replication operations | `destination` |

### Multipart Upload Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `s3_multipart_uploads_active` | Gauge | Number of currently active multipart uploads | `bucket` |
| `s3_multipart_parts_total` | Counter | Total multipart parts uploaded | `bucket` |

### Compression Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `s3_compression_ratio` | Histogram | Compression ratio achieved for stored objects | `algorithm` |
| `s3_compression_duration_seconds` | Histogram | Time spent on compression/decompression | `operation`, `algorithm` |

### S3 Select Metrics

| Metric | Type | Description | Labels |
|--------|------|-------------|--------|
| `s3_select_queries_total` | Counter | Total number of S3 Select queries executed | `format` |
| `s3_select_query_duration_seconds` | Histogram | S3 Select query execution time | `format` |
| `s3_select_cache_hits_total` | Counter | S3 Select query result cache hits | — |
| `s3_select_bytes_scanned_total` | Counter | Total bytes scanned by S3 Select queries | — |
| `s3_select_bytes_returned_total` | Counter | Total bytes returned from S3 Select queries | — |

---

## PromQL Query Examples

### Request Rate (last 5 minutes)

```promql
# Total request rate
sum(rate(s3_requests_total[5m]))

# Request rate per operation
sum by (operation) (rate(s3_requests_total[5m]))

# PutObject throughput (bytes/sec)
sum(rate(s3_object_size_bytes_sum{operation="PutObject"}[5m]))
```

### Error Rate and Ratio

```promql
# Overall error rate
sum(rate(s3_errors_total[5m]))

# Error ratio (fraction of requests that failed)
sum(rate(s3_errors_total[5m]))
  /
sum(rate(s3_requests_total[5m]))

# Error rate by operation
sum by (operation) (rate(s3_errors_total[5m]))

# Specific error codes
sum by (error_code) (rate(s3_errors_total[5m]))
```

### Latency Percentiles

```promql
# P50 latency per operation (5m window)
histogram_quantile(0.50,
  sum by (operation, le) (rate(s3_request_duration_seconds_bucket[5m]))
)

# P99 latency per operation (5m window)
histogram_quantile(0.99,
  sum by (operation, le) (rate(s3_request_duration_seconds_bucket[5m]))
)

# P99.9 latency overall
histogram_quantile(0.999,
  sum by (le) (rate(s3_request_duration_seconds_bucket[5m]))
)
```

### Cache Performance

```promql
# Cache hit rate
sum(rate(s3_cache_hits_total[5m]))
  /
(sum(rate(s3_cache_hits_total[5m])) + sum(rate(s3_cache_misses_total[5m])))

# Cache hit rate by type
sum by (cache_type) (rate(s3_cache_hits_total[5m]))
  /
(
  sum by (cache_type) (rate(s3_cache_hits_total[5m]))
  + sum by (cache_type) (rate(s3_cache_misses_total[5m]))
)
```

### Storage Usage

```promql
# Total bytes stored
sum(s3_storage_bytes_total)

# Total objects stored
sum(s3_objects_total)

# Storage per bucket (top 10)
topk(10, sum by (bucket) (s3_storage_bytes_total))

# Average object size
sum(s3_storage_bytes_total) / sum(s3_objects_total)
```

### Cluster Health

```promql
# Fraction of healthy nodes
rs3gw_cluster_healthy_nodes / rs3gw_cluster_total_nodes

# Maximum replication lag
max(rs3gw_replication_lag_ms)
```

### S3 Select Efficiency

```promql
# S3 Select bytes returned vs scanned ratio (data selectivity)
sum(rate(s3_select_bytes_returned_total[5m]))
  /
sum(rate(s3_select_bytes_scanned_total[5m]))

# S3 Select cache hit rate
sum(rate(s3_select_cache_hits_total[5m]))
  /
sum(rate(s3_select_queries_total[5m]))
```

---

## Recording Rules

Add these recording rules to a `rules/rs3gw.yml` file to pre-compute expensive queries:

```yaml
groups:
  - name: rs3gw_request_rates
    interval: 30s
    rules:
      # Pre-compute 5-minute request rates per operation
      - record: job:s3_requests_total:rate5m
        expr: sum by (job, operation) (rate(s3_requests_total[5m]))

      # Pre-compute 5-minute error rates per operation
      - record: job:s3_errors_total:rate5m
        expr: sum by (job, operation) (rate(s3_errors_total[5m]))

      # Pre-compute overall error ratio
      - record: job:s3_error_ratio:rate5m
        expr: >
          sum by (job) (rate(s3_errors_total[5m]))
            /
          sum by (job) (rate(s3_requests_total[5m]))

  - name: rs3gw_latency_percentiles
    interval: 30s
    rules:
      # Pre-compute P50 latency per operation
      - record: job:s3_request_duration_seconds_p50:rate5m
        expr: >
          histogram_quantile(0.50,
            sum by (job, operation, le) (rate(s3_request_duration_seconds_bucket[5m]))
          )

      # Pre-compute P99 latency per operation
      - record: job:s3_request_duration_seconds_p99:rate5m
        expr: >
          histogram_quantile(0.99,
            sum by (job, operation, le) (rate(s3_request_duration_seconds_bucket[5m]))
          )

      # Pre-compute P99.9 latency overall
      - record: job:s3_request_duration_seconds_p999:rate5m
        expr: >
          histogram_quantile(0.999,
            sum by (job, le) (rate(s3_request_duration_seconds_bucket[5m]))
          )

  - name: rs3gw_cache_metrics
    interval: 60s
    rules:
      # Cache hit rate per type
      - record: job:s3_cache_hit_ratio:rate5m
        expr: >
          sum by (job, cache_type) (rate(s3_cache_hits_total[5m]))
            /
          (
            sum by (job, cache_type) (rate(s3_cache_hits_total[5m]))
            + sum by (job, cache_type) (rate(s3_cache_misses_total[5m]))
          )

  - name: rs3gw_storage_metrics
    interval: 60s
    rules:
      # Total storage bytes
      - record: job:s3_storage_bytes:sum
        expr: sum by (job) (s3_storage_bytes_total)

      # Total object count
      - record: job:s3_objects:sum
        expr: sum by (job) (s3_objects_total)

  - name: rs3gw_alerts
    rules:
      # Alert when error ratio exceeds 1% for 5 minutes
      - alert: Rs3gwHighErrorRate
        expr: job:s3_error_ratio:rate5m > 0.01
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "rs3gw error rate above 1%"
          description: "rs3gw job {{ $labels.job }} has error ratio {{ $value | humanizePercentage }} over the last 5 minutes."

      # Alert when P99 latency exceeds 5 seconds for 5 minutes
      - alert: Rs3gwHighLatency
        expr: job:s3_request_duration_seconds_p99:rate5m > 5
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "rs3gw P99 latency above 5s"
          description: "rs3gw job {{ $labels.job }} operation {{ $labels.operation }} P99 latency is {{ $value | humanizeDuration }}."

      # Alert when cluster has unhealthy nodes
      - alert: Rs3gwClusterNodeUnhealthy
        expr: rs3gw_cluster_healthy_nodes < rs3gw_cluster_total_nodes
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "rs3gw cluster has unhealthy nodes"
          description: "{{ $value }} out of {{ query \"rs3gw_cluster_total_nodes\" | first | value }} nodes are healthy."

      # Alert when cache hit rate drops below 50%
      - alert: Rs3gwLowCacheHitRate
        expr: job:s3_cache_hit_ratio:rate5m < 0.5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "rs3gw cache hit rate below 50%"
          description: "rs3gw cache type {{ $labels.cache_type }} hit rate is {{ $value | humanizePercentage }}."
```

---

## Health and Readiness Endpoints

In addition to `/metrics`, rs3gw provides two HTTP endpoints for health probing:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Returns `{"status":"ok","version":"...","uptime_secs":N,"compression":"..."}` — always 200 |
| `/ready` | GET | Returns `{"status":"ready"}` (200) or `{"status":"unavailable","reason":"..."}` (503) |

These are suitable for use as Kubernetes `livenessProbe` (`/health`) and `readinessProbe` (`/ready`) targets.
