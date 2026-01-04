# rs3gw API Module

S3-compatible HTTP API layer built with Axum.

## Overview

This module provides the HTTP interface for S3-compatible operations. It handles request routing, XML serialization/deserialization, and maps HTTP requests to storage engine operations.

## Module Structure

```
api/
├── mod.rs                        # Module exports
├── handlers/                     # HTTP request handlers
│   ├── functions.rs             # Main S3 operation handlers
│   ├── functions_2.rs           # Additional S3 operation handlers
│   └── types.rs                 # Request/Response types
├── s3_router.rs                 # Axum router configuration
├── xml_responses.rs             # S3 XML response structures
├── select/                       # S3 Select implementation
│   ├── parser.rs                # SQL query parser
│   ├── types.rs                 # Query execution types
│   └── window_functions.rs     # SQL window functions
├── select_cache.rs              # S3 Select result caching
├── select_cache_handlers.rs    # Cache management endpoints
├── select_optimizer.rs          # Query optimization
├── query_intelligence.rs        # AI-powered query intelligence
├── query_intelligence_handlers.rs # Query intelligence API endpoints
├── preprocessing_handlers.rs    # Dataset preprocessing endpoints
├── tiering_handlers.rs          # Intelligent tiering API endpoints
├── observability_handlers.rs    # Observability endpoints
├── arrow_flight.rs              # Apache Arrow Flight integration
├── graphql.rs                   # GraphQL API
├── websocket.rs                 # WebSocket events
├── batch.rs                     # Batch operations
├── multipart.rs                 # Multipart upload handlers
├── throttle.rs                  # Rate limiting
├── bucket_stubs.rs              # S3 bucket API stubs
└── utils.rs                     # Helper utilities
```

## Components

### handlers.rs

HTTP handlers for all S3 operations:

| Handler | Method | Path | Description |
|---------|--------|------|-------------|
| `health_check` | GET | `/health` | Health check (JSON) |
| `metrics` | GET | `/metrics` | Prometheus metrics |
| `list_buckets` | GET | `/` | List all buckets |
| `head_bucket` | HEAD | `/{bucket}` | Check bucket existence |
| `create_bucket` | PUT | `/{bucket}` | Create new bucket |
| `delete_bucket` | DELETE | `/{bucket}` | Delete empty bucket |
| `list_objects_v2` | GET | `/{bucket}` | List objects with prefix/delimiter |
| `head_object` | HEAD | `/{bucket}/{key}` | Get object metadata |
| `get_object` | GET | `/{bucket}/{key}` | Download object (supports Range) |
| `put_object` | PUT | `/{bucket}/{key}` | Upload object |
| `delete_object` | DELETE | `/{bucket}/{key}` | Delete object |
| `copy_object` | PUT | `/{bucket}/{key}` | Copy object (x-amz-copy-source) |
| `create_multipart_upload` | POST | `/{bucket}/{key}?uploads` | Initiate multipart |
| `upload_part` | PUT | `/{bucket}/{key}?uploadId&partNumber` | Upload part |
| `complete_multipart_upload` | POST | `/{bucket}/{key}?uploadId` | Complete multipart |
| `abort_multipart_upload` | DELETE | `/{bucket}/{key}?uploadId` | Abort multipart |
| `list_parts` | GET | `/{bucket}/{key}?uploadId` | List uploaded parts |

### s3_router.rs

Axum router with dispatcher functions that route requests based on query parameters and headers:

- `put_object_dispatcher` - Routes to CopyObject, UploadPart, or PutObject
- `post_object_dispatcher` - Routes to CreateMultipartUpload or CompleteMultipartUpload
- `get_object_dispatcher` - Routes to ListParts or GetObject
- `delete_object_dispatcher` - Routes to AbortMultipartUpload or DeleteObject

### xml_responses.rs

S3-compatible XML response structures using `quick-xml`:

- `ErrorResponse` - Error response format
- `ListAllMyBucketsResult` - List buckets response
- `ListBucketResult` - List objects response
- `CopyObjectResult` - Copy object response
- `InitiateMultipartUploadResult` - Multipart initiation response
- `ListPartsResult` - List parts response
- `CompleteMultipartUploadResult` - Multipart completion response

### query_intelligence.rs

AI-powered query intelligence and optimization using statistical machine learning:

**Core Capabilities:**
- **Query Cost Prediction**: ML-based prediction of query execution costs (time, memory, I/O)
- **Adaptive Execution**: Dynamic strategy selection based on data characteristics
- **Semantic Caching**: Query similarity detection for intelligent cache hits
- **Statistics Collection**: Comprehensive query execution profiling

**Key Types:**
- `QueryIntelligence` - Main query intelligence engine
- `QueryCost` - Cost prediction with breakdown by operation type
- `ExecutionStrategy` - Adaptive execution strategy (FullScan, IndexScan, Cached)
- `QueryStats` - Execution statistics and profiling data
- `DataStatistics` - Data distribution and characteristics

**API Endpoints** (query_intelligence_handlers.rs):

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/query/cost` | POST | Predict query execution cost |
| `/api/query/strategy` | POST | Get adaptive execution strategy |
| `/api/query/similar` | POST | Find similar cached queries |
| `/api/query/stats` | GET | Get query statistics |

**Example:**
```rust
// Predict query cost
let cost = intelligence.predict_cost(&query, object_size).await;

// Get execution strategy
let strategy = intelligence.get_execution_strategy(&query, &data_stats).await;

// Find similar cached query
if let Some(cached) = intelligence.find_similar_cached_query(&query).await {
    return cached;
}
```

### select_cache.rs

S3 Select query result caching with LRU eviction:

**Features:**
- **LRU Eviction**: Least Recently Used eviction policy
- **Size-Based Limits**: Configurable maximum entries and memory limits
- **ETag Validation**: Cache invalidation on object changes
- **Statistics**: Hit rate, miss rate, eviction tracking
- **TTL Support**: Time-based expiration

**Configuration:**
- `max_entries` - Maximum number of cached query results
- `max_memory_bytes` - Maximum memory usage for cache

**API Endpoints** (select_cache_handlers.rs):

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/cache/stats` | GET | Get cache statistics (hits, misses, evictions) |
| `/api/cache/clear` | POST | Clear all cached query results |
| `/api/cache/config` | GET | Get cache configuration |
| `/api/cache/config` | PUT | Update cache configuration |

**Statistics Provided:**
```json
{
  "hits": 1234,
  "misses": 567,
  "evictions": 89,
  "current_entries": 100,
  "current_memory_bytes": 52428800,
  "hit_rate": 0.685
}
```

### tiering_handlers.rs

Intelligent storage tiering API endpoints:

**Features:**
- **Automated Tiering**: ML-based access pattern analysis
- **Policy Management**: Custom tiering policies per bucket/prefix
- **Transition History**: Track object tier transitions
- **Cost Optimization**: Predictive cost analysis
- **Capacity Planning**: Automated recommendations

**API Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/tiering/analyze` | POST | Analyze tiering opportunities |
| `/api/tiering/policy/{bucket}` | GET | Get tiering policy for bucket |
| `/api/tiering/policy/{bucket}` | PUT | Set tiering policy for bucket |
| `/api/tiering/policy/{bucket}` | DELETE | Delete tiering policy |
| `/api/tiering/history/{bucket}/{key}` | GET | Get transition history for object |
| `/api/tiering/history/{bucket}` | GET | Get transition history for bucket |
| `/api/tiering/capacity` | GET | Get capacity recommendations |
| `/api/tiering/presets` | GET | Get predefined tiering policies |

**Tiering Policy Presets:**
- `aggressive` - Frequent transitions for maximum cost savings
- `balanced` - Moderate transitions balancing cost and performance
- `conservative` - Infrequent transitions prioritizing performance
- `archive` - Long-term archival strategy

### preprocessing_handlers.rs

Dataset preprocessing pipeline management:

**Features:**
- **Image Preprocessing**: Normalization, resizing, augmentation
- **Pipeline Management**: CRUD operations for preprocessing pipelines
- **Cache Management**: LRU cache for preprocessed results
- **Multi-Format Support**: Various preprocessing operations

**API Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/preprocessing/pipelines` | GET | List all pipelines |
| `/api/preprocessing/pipelines` | POST | Create new pipeline |
| `/api/preprocessing/pipelines/{id}` | GET | Get pipeline by ID |
| `/api/preprocessing/pipelines/{id}` | DELETE | Delete pipeline |
| `/api/preprocessing/cache/stats` | GET | Get cache statistics |
| `/api/preprocessing/cache/clear` | POST | Clear preprocessing cache |

### observability_handlers.rs

Comprehensive observability and monitoring endpoints:

**API Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/observability/health` | GET | Comprehensive health check |
| `/api/observability/anomalies` | GET | Detect anomalies in metrics |
| `/api/observability/business-metrics` | GET | Business-level metrics |
| `/api/observability/resource-stats` | GET | Resource usage statistics |
| `/api/observability/profiling` | GET | Performance profiling data |
| `/api/observability/predictions/storage-growth` | GET | Storage growth forecasts |
| `/api/observability/predictions/access-patterns` | GET | Access pattern predictions |
| `/api/observability/predictions/costs` | GET | Cost forecasting |
| `/api/observability/predictions/capacity` | GET | Capacity planning recommendations |

## Features (v0.1.0)

- [x] Bucket CRUD operations
- [x] Object CRUD operations
- [x] ListObjectsV2 with prefix/delimiter
- [x] Range request support (partial GET)
- [x] Multipart upload support
- [x] CopyObject with metadata directive
- [x] Custom metadata headers (x-amz-meta-*)
- [x] Proper HTTP status codes
- [x] S3-compatible XML responses
- [x] Health check endpoint
- [x] Prometheus metrics endpoint

## Usage

```rust
use rs3gw::api::s3_router;
use rs3gw::AppState;

let app = axum::Router::new()
    .merge(s3_router::routes())
    .with_state(state);
```

## Error Handling

All handlers return appropriate S3 error codes:

| Error | HTTP Status | Description |
|-------|-------------|-------------|
| NoSuchBucket | 404 | Bucket not found |
| NoSuchKey | 404 | Object not found |
| BucketAlreadyExists | 409 | Bucket already exists |
| BucketNotEmpty | 409 | Cannot delete non-empty bucket |
| InvalidRange | 416 | Invalid Range header |
| NoSuchUpload | 404 | Multipart upload not found |
