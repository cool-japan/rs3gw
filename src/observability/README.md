# Observability Module

The observability module provides comprehensive monitoring, profiling, and analytics capabilities for rs3gw, enabling deep insights into system performance, resource utilization, and operational health.

## Overview

This module implements enterprise-grade observability features:
- **Distributed Tracing** - OpenTelemetry integration with W3C Trace Context
- **Continuous Profiling** - CPU, memory, and I/O profiling with pprof export
- **Business Metrics** - KPI tracking and analytics for business intelligence
- **Anomaly Detection** - Statistical anomaly detection for performance issues
- **Resource Management** - Auto-scaling and adaptive resource allocation
- **REST API** - Programmatic access to observability data

## Components

### Distributed Tracing (`tracing.rs`)
OpenTelemetry integration for distributed request tracing.

**Features:**
- W3C Trace Context propagation
- OTLP gRPC export to collectors
- Configurable sampling strategies
- Resource attributes tracking
- Batch span exporter
- Integration with tracing-subscriber

**Sampling Strategies:**
- `AlwaysOn` - Trace all requests
- `AlwaysOff` - Disable tracing
- `TraceIdRatioBased` - Sample percentage of requests

**Configuration:**
```rust
use rs3gw::observability::tracing::{init_telemetry, TelemetryConfig};

let config = TelemetryConfig {
    service_name: "rs3gw".to_string(),
    service_version: "0.1.0".to_string(),
    environment: "production".to_string(),
    otlp_endpoint: "http://jaeger:4317".to_string(),
    sample_ratio: 0.1, // 10% sampling
};

init_telemetry(config)?;
```

**Environment Variables:**
- `OTEL_EXPORTER_OTLP_ENDPOINT` - OTLP collector endpoint
- `OTEL_TRACES_SAMPLER` - Sampling strategy
- `OTEL_TRACES_SAMPLER_ARG` - Sample ratio (0.0-1.0)
- `OTEL_SERVICE_NAME` - Service name
- `OTEL_RESOURCE_ATTRIBUTES` - Additional attributes

**Compatible Collectors:**
- **Jaeger** - Distributed tracing system
- **Tempo** - Grafana Tempo
- **OpenTelemetry Collector** - Generic OTLP collector
- **Cloud providers** - AWS X-Ray, Google Cloud Trace, Azure Monitor

### Continuous Profiling (`profiling.rs`)
Low-overhead runtime profiling for performance analysis.

**Profiling Types:**
- **CPU Profiling** - Sampling-based CPU usage measurement from `/proc/self/stat`
- **Memory Profiling** - RSS, virtual memory, and peak allocation tracking
- **I/O Profiling** - Read/write operation and latency tracking

**Features:**
- Automatic profile collection at configurable intervals
- Profile snapshot retention with limits
- pprof format export for flamegraph visualization
- Thread-safe metric collection with Arc/RwLock
- Platform-specific optimizations (Linux)

**Configuration:**
```rust
use rs3gw::observability::profiling::{Profiler, ProfilingConfig};
use std::time::Duration;

let config = ProfilingConfig {
    enabled: true,
    sample_interval: Duration::from_secs(60),
    max_snapshots: 100,
};

let profiler = Profiler::new(config);

// Record operations
profiler.record_cpu_time(5000); // microseconds
profiler.record_memory_usage(1024 * 1024 * 100); // 100 MB
profiler.record_read(1024 * 1024, 1000); // 1MB in 1ms
profiler.record_write(2048 * 1024, 2000); // 2MB in 2ms

// Export pprof
let pprof_data = profiler.export_pprof()?;
```

**Environment Variables:**
- `RS3GW_PROFILING_ENABLED` - Enable profiling (default: true)
- `RS3GW_PROFILING_INTERVAL` - Sample interval in seconds (default: 60)
- `RS3GW_PROFILING_MAX_SNAPSHOTS` - Maximum retained snapshots (default: 100)

**Metrics Tracked:**
- CPU usage percentage and time
- Memory RSS, virtual, and peak
- I/O read/write operations count
- I/O read/write bytes transferred
- I/O read/write latencies

**Export Formats:**
- **pprof** - Compatible with Go pprof tools
- **flamegraph** - Can be visualized with flamegraph.pl or speedscope
- **JSON** - Programmatic access via REST API

### Business Metrics (`business_metrics.rs`)
KPI tracking and business intelligence metrics.

**Metric Types:**
- **Counter** - Monotonically increasing values
- **Gauge** - Point-in-time values
- **Histogram** - Distribution of values
- **Summary** - Statistical summaries

**Tracked Metrics:**
- **Storage Utilization** - Buckets, objects, size, growth rate
- **Data Transfer** - Upload/download rates, bandwidth, peak rates
- **Request Patterns** - Latency percentiles, error rates, operation counts
- **User Activity** - Active users, engagement scores, top users
- **Cost Estimation** - Storage, bandwidth, request costs

**Features:**
- Metric registration and recording
- Historical snapshots with retention
- Prometheus and JSON export
- Trend analysis and percentage changes
- Time-series data management

**Usage:**
```rust
use rs3gw::observability::business_metrics::{BusinessMetrics, MetricType};

let mut metrics = BusinessMetrics::new();

// Register metric
metrics.register_metric(
    "request_count",
    MetricType::Counter,
    "Total number of requests"
);

// Record value
metrics.record("request_count", 1.0)?;

// Get current value
let value = metrics.get_metric("request_count")?;

// Export Prometheus
let prom = metrics.export_prometheus();

// Export JSON
let json = metrics.export_json()?;

// Get trend
let trend = metrics.get_trend("request_count", Duration::from_secs(3600))?;
```

**Prometheus Export:**
```text
# HELP request_count Total number of requests
# TYPE request_count counter
request_count 12345

# HELP storage_bytes Total storage used in bytes
# TYPE storage_bytes gauge
storage_bytes 1073741824
```

**JSON Export:**
```json
{
  "timestamp": "2025-12-31T00:00:00Z",
  "metrics": {
    "request_count": {
      "type": "counter",
      "value": 12345,
      "description": "Total number of requests"
    },
    "storage_bytes": {
      "type": "gauge",
      "value": 1073741824,
      "description": "Total storage used in bytes"
    }
  }
}
```

### Anomaly Detection (`anomaly_detection.rs`)
Statistical anomaly detection for performance monitoring.

**Anomaly Types:**
- `LatencySpike` - Unusual latency increases
- `ErrorRateIncrease` - Rising error rates
- `ThroughputDrop` - Decreasing throughput
- `CpuSpike` - CPU usage spikes
- `MemorySpike` - Memory usage spikes
- `StorageGrowthAnomaly` - Unusual storage growth
- `RequestRateAnomaly` - Abnormal request patterns

**Severity Levels:**
- `Low` - 2σ deviation
- `Medium` - 3σ deviation
- `High` - 4σ deviation
- `Critical` - 5σ+ deviation

**Features:**
- Statistical baseline calculation (mean, std dev, min/max)
- Z-score based detection with configurable thresholds
- Time series data management with sliding windows
- Anomaly history with filtering
- Automatic data aging and cleanup

**Detection Algorithm:**
```
z_score = (value - mean) / std_dev

if z_score > threshold:
    anomaly detected with severity based on z_score
```

**Usage:**
```rust
use rs3gw::observability::anomaly_detection::{AnomalyDetector, AnomalyDetectorConfig};
use std::time::Duration;

let config = AnomalyDetectorConfig {
    window_size: 100,
    low_threshold: 2.0,    // 2σ
    medium_threshold: 3.0, // 3σ
    high_threshold: 4.0,   // 4σ
    critical_threshold: 5.0, // 5σ
};

let detector = AnomalyDetector::new(config);

// Record and detect
if let Some(anomaly) = detector.record_and_detect("latency_ms", 500.0).await {
    println!("Anomaly detected: {:?}", anomaly.anomaly_type);
    println!("Severity: {:?}", anomaly.severity);
    println!("Z-score: {:.2}", anomaly.deviation_sigma);
}

// Get statistics
let stats = detector.get_statistics().await;
for (metric, stat) in stats {
    println!("{}: mean={:.2}, stddev={:.2}", metric, stat.mean, stat.std_dev);
}

// Get anomaly history
let history = detector.get_anomaly_history(None, None, None, 10).await;
```

**Anomaly Structure:**
```rust
pub struct Anomaly {
    pub metric_name: String,
    pub current_value: f64,
    pub baseline_mean: f64,
    pub deviation_sigma: f64,
    pub anomaly_type: AnomalyType,
    pub severity: AnomalySeverity,
    pub detected_at: DateTime<Utc>,
}
```

### Resource Management (`resource_manager.rs`)
Intelligent resource adaptation and auto-scaling.

**Managed Resources:**
- **Thread Pool** - Dynamic sizing based on CPU utilization
- **Rate Limiting** - Adaptive limits based on system load
- **Memory Management** - Pressure detection and backpressure
- **Load Shedding** - Graceful degradation under heavy load

**Auto-Scaling Features:**
- Target CPU utilization (default: 75%)
- Automatic scale-up when low CPU but work pending
- Automatic scale-down when CPU overloaded
- Configurable min/max thread limits

**Adaptive Rate Limiting:**
- Dynamic adjustment based on success rate and latency
- Scale-up when performing well (>99% success, <100ms latency)
- Scale-down when struggling (<95% success or >1000ms latency)
- Token bucket-style admission control

**Memory Pressure Detection:**
- Real-time utilization monitoring from `/proc/meminfo`
- Configurable threshold (default: 85%)
- Backpressure signaling for load shedding

**Configuration:**
```rust
use rs3gw::observability::resource_manager::{ResourceManager, ResourceConfig};
use std::time::Duration;

let config = ResourceConfig {
    min_threads: 4,
    max_threads: num_cpus::get() * 4,
    target_cpu: 0.75,
    memory_threshold: 0.85,
    adjustment_interval: Duration::from_secs(30),
    adaptive_rate_limit: true,
    initial_rate_limit: 1000,
    min_rate_limit: 100,
    max_rate_limit: 10000,
    load_shedding_threshold: 0.95,
};

let manager = ResourceManager::new(config);

// Start monitoring
manager.start_monitoring();

// Check if request should be admitted
if manager.should_admit_request() {
    // Process request
    manager.record_request(true, 50); // success, 50ms latency
} else {
    // Reject request (load shedding)
}

// Get load metrics
let metrics = manager.get_load_metrics();
println!("CPU: {:.2}%", metrics.cpu_usage * 100.0);
println!("Memory: {:.2}%", metrics.memory_usage * 100.0);
println!("Active requests: {}", metrics.active_requests);
```

**Environment Variables:**
- `RS3GW_MIN_THREADS` - Minimum thread pool size
- `RS3GW_MAX_THREADS` - Maximum thread pool size
- `RS3GW_TARGET_CPU` - Target CPU utilization (0.0-1.0)
- `RS3GW_MEMORY_THRESHOLD` - Memory pressure threshold (0.0-1.0)
- `RS3GW_ADJUSTMENT_INTERVAL` - Resource adjustment interval (seconds)
- `RS3GW_ADAPTIVE_RATE_LIMIT` - Enable adaptive rate limiting
- `RS3GW_INITIAL_RATE_LIMIT` - Initial rate limit (rps)
- `RS3GW_MIN_RATE_LIMIT` - Minimum rate limit (rps)
- `RS3GW_MAX_RATE_LIMIT` - Maximum rate limit (rps)
- `RS3GW_LOAD_SHEDDING_THRESHOLD` - Load shedding threshold (0.0-1.0)

## REST API (`observability_handlers.rs`)

Programmatic access to observability data through REST endpoints.

### Endpoints

#### GET /api/observability/profiling
Retrieve CPU, memory, and I/O profiling data.

**Query Parameters:**
- `format` - Response format (`json` or `pprof`, default: `json`)

**Response:**
```json
{
  "timestamp": "2025-12-31T00:00:00Z",
  "cpu": {
    "usage_percent": 45.2,
    "total_time_us": 123456789
  },
  "memory": {
    "rss_bytes": 134217728,
    "virtual_bytes": 268435456,
    "peak_rss_bytes": 201326592
  },
  "io": {
    "read_ops": 1234,
    "write_ops": 567,
    "read_bytes": 12345678,
    "write_bytes": 7654321,
    "read_latency_us": 100,
    "write_latency_us": 150
  }
}
```

#### GET /api/observability/business-metrics
Retrieve business KPI metrics.

**Query Parameters:**
- `format` - Response format (`json` or `prometheus`, default: `json`)

**Response:**
```json
{
  "timestamp": "2025-12-31T00:00:00Z",
  "storage_utilization": {
    "buckets": 42,
    "objects": 15234,
    "total_bytes": 1073741824,
    "growth_rate_bytes_per_day": 104857600
  },
  "data_transfer": {
    "upload_rate_bytes_per_sec": 1048576,
    "download_rate_bytes_per_sec": 5242880,
    "peak_upload_rate": 2097152,
    "peak_download_rate": 10485760
  },
  "request_patterns": {
    "p50_latency_ms": 15.2,
    "p95_latency_ms": 45.8,
    "p99_latency_ms": 123.4,
    "error_rate": 0.02,
    "total_requests": 98765
  }
}
```

#### GET /api/observability/anomalies
Retrieve detected performance anomalies.

**Query Parameters:**
- `type` - Filter by anomaly type
- `severity` - Filter by severity level
- `limit` - Maximum number of results (default: 100)

**Response:**
```json
{
  "timestamp": "2025-12-31T00:00:00Z",
  "anomalies": [
    {
      "metric_name": "latency_ms",
      "current_value": 500.0,
      "baseline_mean": 50.0,
      "deviation_sigma": 4.5,
      "anomaly_type": "LatencySpike",
      "severity": "High",
      "detected_at": "2025-12-31T00:00:00Z"
    }
  ]
}
```

#### GET /api/observability/resources
Retrieve resource manager statistics.

**Response:**
```json
{
  "timestamp": "2025-12-31T00:00:00Z",
  "cpu_usage": 0.452,
  "memory_usage": 0.621,
  "active_requests": 42,
  "pending_requests": 5,
  "current_rate_limit": 1500,
  "is_under_memory_pressure": false,
  "should_shed_load": false
}
```

#### GET /api/observability/health
Comprehensive health check with all metrics.

**Response:**
```json
{
  "status": "healthy",
  "timestamp": "2025-12-31T00:00:00Z",
  "profiling": { ... },
  "business_metrics": { ... },
  "anomalies": { ... },
  "resources": { ... }
}
```

## Integration with Monitoring Stack

### Prometheus Integration

Configure Prometheus to scrape metrics:

```yaml
scrape_configs:
  - job_name: 'rs3gw'
    static_configs:
      - targets: ['localhost:9000']
    metrics_path: '/metrics'
    scrape_interval: 15s
```

### Grafana Dashboards

Import pre-configured dashboards for:
- **Storage Metrics** - Bucket/object counts, sizes, growth
- **Performance Metrics** - Latency, throughput, error rates
- **Resource Utilization** - CPU, memory, I/O
- **Anomaly Detection** - Real-time anomaly visualization

### Jaeger Tracing

Configure rs3gw to send traces to Jaeger:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317
export OTEL_TRACES_SAMPLER=traceidratio
export OTEL_TRACES_SAMPLER_ARG=0.1
```

View traces in Jaeger UI at `http://localhost:16686`

## Performance Characteristics

Based on benchmarks:

- **Profiling Overhead**: <1% CPU, <10MB memory
- **Metric Collection**: <100µs per metric
- **Anomaly Detection**: <1ms per data point
- **REST API Latency**: <5ms per request
- **Trace Export**: Batched, <10ms per batch

## Usage Examples

### Complete Observability Setup

```rust
use rs3gw::observability::{
    tracing::{init_telemetry, TelemetryConfig},
    profiling::{Profiler, ProfilingConfig},
    business_metrics::BusinessMetrics,
    anomaly_detection::{AnomalyDetector, AnomalyDetectorConfig},
    resource_manager::{ResourceManager, ResourceConfig},
};

// Initialize distributed tracing
let telemetry_config = TelemetryConfig::from_env();
init_telemetry(telemetry_config)?;

// Start profiler
let profiler = Profiler::new(ProfilingConfig::default());

// Initialize business metrics
let mut metrics = BusinessMetrics::new();
metrics.register_metric("requests", MetricType::Counter, "Total requests");

// Start anomaly detector
let anomaly_detector = AnomalyDetector::new(AnomalyDetectorConfig::default());

// Start resource manager
let resource_manager = ResourceManager::new(ResourceConfig::default());
resource_manager.start_monitoring();

// Use in request handler
async fn handle_request() -> Result<Response> {
    // Check admission
    if !resource_manager.should_admit_request() {
        return Err(ServiceUnavailable);
    }

    // Record request
    metrics.record("requests", 1.0)?;

    // Process request
    let start = Instant::now();
    let result = process_request().await;
    let latency_ms = start.elapsed().as_millis() as f64;

    // Detect anomalies
    if let Some(anomaly) = anomaly_detector.record_and_detect("latency_ms", latency_ms).await {
        warn!("Anomaly detected: {:?}", anomaly);
    }

    // Record metrics
    profiler.record_cpu_time(latency_ms as u64 * 1000);
    resource_manager.record_request(result.is_ok(), latency_ms as u64);

    result
}
```

## Testing

Comprehensive test coverage for all observability components:

```bash
# All observability tests
cargo test --lib observability::

# Specific component
cargo test --lib observability::anomaly_detection::

# Integration tests
cargo test --test observability_tests
```

## Dependencies

Key dependencies for observability functionality:

- **opentelemetry** - Distributed tracing
- **opentelemetry-otlp** - OTLP export
- **tracing** - Structured logging
- **metrics** - Metric collection
- **serde** - Serialization
- **chrono** - Time handling

## Related Documentation

- [API Module](../api/README.md) - HTTP handlers including observability endpoints
- [Storage Module](../storage/README.md) - Storage metrics and monitoring
- [Main README](../../README.md) - Project overview

## License

Apache-2.0
