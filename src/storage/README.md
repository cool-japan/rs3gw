# Storage Module

The storage module provides a comprehensive, enterprise-grade object storage engine with advanced features for performance, reliability, and data management.

## Overview

This module implements the core storage layer for rs3gw, providing:
- **High-performance object storage** with compression and deduplication
- **Multi-backend support** for local, cloud, and distributed storage
- **Advanced data management** including versioning, lifecycle, and transformations
- **Enterprise features** like encryption, audit logging, and compliance
- **ML-driven optimization** for caching and access patterns

## Core Components

### Storage Engine (`mod.rs`)
The main storage engine coordinates all storage operations and provides the primary S3-compatible API.

**Key Features:**
- Asynchronous I/O with Tokio
- Transparent compression (Zstd, LZ4)
- Metadata sidecar files (.meta)
- Custom user metadata support
- ETag generation (SHA256)
- Multipart upload support

**Main Types:**
- `StorageEngine` - Primary storage interface
- `ObjectMetadata` - Object metadata structure
- `StorageError` - Error types for storage operations

### Backend Abstraction (`backend/`)
Multi-backend architecture supporting various storage backends.

**Supported Backends:**
- **Local** - Filesystem-based storage (production-ready)
- **MinIO** - S3-compatible MinIO server (production-ready)
- **AWS S3** - Native AWS S3 integration (production-ready)
- **GCS** - Google Cloud Storage (stub, awaiting SDK stabilization)
- **Azure** - Azure Blob Storage (stub, awaiting SDK stabilization)
- **Ceph** - Ceph RADOS backend (stub, awaiting bindings)
- **GlusterFS** - GlusterFS backend (stub, awaiting bindings)

**Key Files:**
- `backend/mod.rs` - Backend trait and type definitions
- `backend/functions.rs` - Backend factory and utilities
- `backend/types.rs` - Common types and structures
- `backend/localbackend_traits.rs` - Local filesystem backend
- `backend/miniobackend_traits.rs` - MinIO backend
- `backend/s3backend_traits.rs` - AWS S3 backend

### Data Deduplication (`dedup.rs`)
Block-level deduplication using content-addressable storage.

**Features:**
- SHA256-based content addressing
- Two chunking algorithms: Fixed-size and Content-Defined (CDC)
- Reference counting with automatic garbage collection
- Configurable block size (4KB-1MB)
- Minimum object size threshold
- 30-70% storage savings for redundant data

**Configuration:**
```rust
DedupConfig {
    enabled: true,
    block_size: 65536,      // 64KB blocks
    algorithm: ChunkingAlgorithm::FixedSize,
    min_object_size: 131072, // 128KB minimum
}
```

See [DEDUP.md](DEDUP.md) for detailed documentation.

### Zero-Copy Optimizations (`zerocopy.rs`)
Kernel-level zero-copy operations for maximum performance.

**Features:**
- Direct I/O with O_DIRECT flag (Linux)
- Splice/sendfile support for zero-copy transfers
- Memory-mapped metadata files
- Aligned buffer management (512-byte alignment)
- Platform-specific optimizations

**Performance Benefits:**
- Eliminates unnecessary data copies
- Reduces CPU usage for large transfers
- Improves throughput by 2-3x for large files

### ML-Based Smart Caching (`ml_cache.rs`)
Machine learning-driven predictive cache management.

**Features:**
- Access pattern detection (periodic, bursty, trending)
- Statistical ML model using exponential moving averages
- Adaptive TTL based on access patterns
- Predictive prefetching with confidence scoring
- Priority-based LRU eviction
- Multiple cache warming strategies

**Cache Warming Strategies:**
- `MostFrequent` - Warm most frequently accessed objects
- `MostRecent` - Warm most recently accessed objects
- `HighestPriority` - Warm objects with highest priority scores
- `Predictive` - Warm objects predicted to be accessed soon

### Object Versioning (`versioning.rs`)
Complete object versioning system with version history.

**Features:**
- Multiple versions per object key
- Delete markers for soft deletes
- Version listing and retrieval
- Bucket-level versioning control
- Per-object version metadata

**API:**
```rust
// Enable versioning
manager.set_versioning_enabled(bucket, true)?;

// Add version
manager.add_version(bucket, key, data, metadata)?;

// List versions
let versions = manager.list_versions(bucket, key, limit)?;

// Get specific version
let data = manager.get_version(bucket, key, version_id)?;
```

### Storage Class Management (`storage_class.rs`)
Lifecycle-based storage class transitions.

**Storage Classes:**
- `STANDARD` - High-performance, frequently accessed
- `INTELLIGENT_TIERING` - Automatic tier optimization
- `STANDARD_IA` - Infrequent access
- `ONEZONE_IA` - Single-zone infrequent access
- `GLACIER` - Long-term archive
- `GLACIER_IR` - Glacier Instant Retrieval
- `DEEP_ARCHIVE` - Lowest-cost archival
- `REDUCED_REDUNDANCY` - Legacy reduced redundancy

**Features:**
- Automatic transitions based on access patterns
- Age-based lifecycle policies
- Access frequency tracking
- Storage class analysis and reporting

### Encryption (`encryption.rs`)
Enterprise-grade encryption with key rotation.

**Features:**
- Envelope encryption (DEK encrypted with KEK)
- Key rotation without data re-encryption
- Multiple algorithms (AES-256-GCM, ChaCha20-Poly1305)
- Pluggable key provider interface
- Additional authenticated data (AAD) support

**Security Properties:**
- 256-bit encryption keys
- 96-bit nonces (recommended GCM size)
- Authenticated encryption (AEAD)
- Unique nonces per encryption
- Context binding via AAD

### Audit Logging (`audit.rs`)
Immutable audit trail with cryptographic verification.

**Features:**
- Blockchain-inspired event chaining
- HMAC-SHA256 chain integrity
- Real-time security event detection
- Log forwarding to SIEM systems (webhook, syslog, S3, file)
- Log rotation and compression
- Query interface with filtering

**Event Types:**
- Object operations (PUT, GET, DELETE, COPY)
- Bucket operations (CREATE, DELETE, CONFIG)
- Authentication events (SUCCESS, FAILURE)
- Authorization events (ALLOW, DENY)
- System events (STARTUP, SHUTDOWN, ERROR)

### Compliance Reporting (`compliance.rs`)
Automated compliance report generation.

**Supported Standards:**
- **SOC2** - System and Organization Controls Type 2
- **HIPAA** - Health Insurance Portability and Accountability Act
- **GDPR** - General Data Protection Regulation

**Features:**
- Automated compliance checks
- Finding and recommendation generation
- Audit log analysis
- Encryption verification
- Access control validation

### Data Transformations (`transformations.rs`)
Server-side data processing and transformations.

**Supported Transformations:**
- **Image Processing** - Resize, format conversion, quality control
  - Formats: JPEG, PNG, WebP, GIF, BMP, TIFF
  - Resize modes: by width, by height, fit, crop, exact
  - Quality control for lossy formats
- **Video Transcoding** (feature-gated) - FFmpeg integration
  - Codecs: H.264, H.265, VP8, VP9, AV1
  - Configurable bitrate, FPS, resolution
- **Compression** - On-the-fly compression/decompression
  - Algorithms: Zstd, Gzip, LZ4
  - Configurable compression levels
- **WASM Plugins** (feature-gated) - Custom transformations
  - Wasmtime runtime integration
  - Parameterized transformations

**Transformation Chain:**
```rust
let manager = TransformationManager::new();
manager.register_transformer(Arc::new(ImageTransformer::new()));
manager.register_transformer(Arc::new(CompressionTransformer::new()));

let result = manager.transform_chain(data, &[
    ("image_resize", params1),
    ("compress", params2),
])?;
```

### Object Lambda (`object_lambda.rs`)
AWS Lambda-style transformations at retrieval time.

**Built-in Transformations:**
- **PII Redaction** - Remove sensitive data
  - Email redaction
  - Phone number redaction
  - Credit card redaction
- **Format Conversion** - Data format transformations
  - JSON prettify
  - Case transformations (uppercase, lowercase)
  - Custom prefix/suffix addition

**Custom Transformations:**
```rust
manager.register_transformation("custom", Arc::new(CustomTransformation));
let result = manager.apply_transformation(data, "custom")?;
```

### Archival Management (`archival.rs`)
Policy-based archival to cold storage tiers.

**Archive Tiers:**
- `Cold` - Infrequent access cold storage
- `Glacier` - Long-term archive (hours retrieval)
- `DeepArchive` - Lowest-cost archive (12+ hours retrieval)
- `Tape` - Physical tape backup

**Features:**
- Automated archival policies
- Age-based archiving
- Size-based archiving
- Cost estimation and savings tracking
- Restore operations with SLA
- Hybrid local + cloud strategies

### Backup & Recovery (`backup.rs`)
Point-in-time recovery and snapshot management.

**Features:**
- Full and incremental snapshots
- Point-in-time recovery (PITR)
- Automated backup scheduling
- Snapshot retention policies
- Cross-region backup support
- Recovery testing automation

**Snapshot Types:**
- `Full` - Complete backup
- `Incremental` - Changes since last backup
- `Differential` - Changes since last full backup

### Self-Healing (`self_healing.rs`)
Automatic corruption detection and repair.

**Features:**
- SHA256 checksum verification
- Background integrity checking
- Automatic corruption detection
- Replica rebuilding (cluster mode)
- Age-based automatic cleanup
- Statistics and alerting

**Configuration:**
```rust
SelfHealingConfig {
    check_interval: Duration::from_secs(3600),
    repair_enabled: true,
    max_concurrent_repairs: 5,
    corruption_threshold: 0.01, // 1% alert threshold
    auto_cleanup_enabled: false,
    retention_days: 30,
}
```

### Analytics (`analytics.rs`)
Storage analytics and usage metrics.

**Metrics Tracked:**
- Request patterns (GET, PUT, DELETE, LIST)
- Storage utilization per bucket
- Access frequency distribution
- Storage class distribution
- Data transfer volumes
- Error rates and types

**Analysis Features:**
- Time-series data collection
- Metric aggregation
- Storage class analysis
- Request pattern analysis
- Cost estimation

### ML Model Detection (`ml_models.rs`)
Automatic detection and metadata extraction for machine learning models.

**Supported Formats:**
- PyTorch (.pt, .pth)
- TensorFlow (SavedModel)
- ONNX (.onnx)
- Safetensors (.safetensors) - Hugging Face format
- Keras (.h5, .keras)

**Features:**
- Automatic format detection via magic bytes
- Metadata extraction (architecture, parameters, framework version)
- Tensor shape and dtype information
- Model size and complexity metrics
- Integration via custom S3 headers (x-amz-meta-ml-*)

**Example:**
```rust
use rs3gw::storage::ml_models::{detect_ml_model_format, extract_ml_metadata};

let model_data = std::fs::read("model.pt")?;
if let Some(format) = detect_ml_model_format(&model_data).await {
    if let Some(metadata) = extract_ml_metadata(format, &model_data).await {
        println!("Framework: {}", metadata.framework);
        println!("Parameters: {}", metadata.parameter_count);
    }
}
```

### Dataset Registry (`dataset_registry.rs`)
Version-controlled registry for ML datasets with lineage tracking.

**Features:**
- Dataset versioning with immutable versions
- Dataset splits (Train/Test/Validation) management
- Lineage tracking and provenance
- Model-dataset linkage
- Metadata persistence and querying

**Key Types:**
- `DatasetRegistry` - Main registry interface
- `RegisteredDataset` - Dataset metadata
- `DatasetVersion` - Immutable dataset version
- `DatasetSplit` - Split information (Train/Test/Validation)

**Example:**
```rust
use rs3gw::storage::dataset_registry::{DatasetRegistry, DatasetSplit};

let registry = DatasetRegistry::new("/data/registry").await?;

// Register dataset
registry.register_dataset("imagenet", "ImageNet classification dataset").await?;

// Create version
let version = registry
    .create_dataset_version("imagenet", "s3://bucket/imagenet_v1/", None)
    .await?;

// Add splits
registry.add_split(
    "imagenet",
    version.version,
    DatasetSplit::Train,
    "s3://bucket/imagenet_v1/train/",
    100000
).await?;
```

### Model Registry (`model_registry.rs`)
Production-grade ML model registry with lifecycle management.

**Features:**
- Model versioning with semantic versioning support
- Stage-based lifecycle (Development → Staging → Production → Archived)
- Model lineage and provenance tracking
- Dataset linkage for reproducibility
- Metadata persistence and querying

**Model Stages:**
- `Development` - Models under active development
- `Staging` - Models ready for testing
- `Production` - Production-deployed models
- `Archived` - Retired models

**Key Types:**
- `ModelRegistry` - Main registry interface
- `RegisteredModel` - Model metadata
- `ModelVersion` - Versioned model with stage
- `ModelStage` - Lifecycle stage enum

**Example:**
```rust
use rs3gw::storage::model_registry::{ModelRegistry, ModelStage};

let registry = ModelRegistry::new("/data/models").await?;

// Register model
registry.register_model("resnet50", "ResNet-50 image classifier").await?;

// Create version
let version = registry
    .create_model_version(
        "resnet50",
        "s3://bucket/models/resnet50_v1.pt",
        Some("Initial production model")
    )
    .await?;

// Transition through stages
registry.transition_model_stage("resnet50", version.version, ModelStage::Staging).await?;
registry.transition_model_stage("resnet50", version.version, ModelStage::Production).await?;

// Get latest production version
let latest = registry.get_latest_version("resnet50", Some(ModelStage::Production)).await?;
```

## Usage Examples

### Basic Object Operations

```rust
use rs3gw::storage::StorageEngine;
use std::collections::HashMap;
use bytes::Bytes;

// Create storage engine
let storage = StorageEngine::new("/data/storage".into())?;

// Create bucket
storage.create_bucket("my-bucket").await?;

// Put object
let data = Bytes::from("Hello, World!");
let metadata = HashMap::new();
storage.put_object(
    "my-bucket",
    "hello.txt",
    "text/plain",
    metadata,
    data
).await?;

// Get object
let (data, meta) = storage.get_object("my-bucket", "hello.txt").await?;

// Delete object
storage.delete_object("my-bucket", "hello.txt").await?;

// Delete bucket
storage.delete_bucket("my-bucket").await?;
```

### Multipart Upload

```rust
// Create multipart upload
let upload_id = storage.create_multipart_upload(
    "my-bucket",
    "large-file.bin",
    "application/octet-stream",
    HashMap::new()
).await?;

// Upload parts
let part1 = storage.upload_part(
    "my-bucket",
    "large-file.bin",
    &upload_id,
    1,
    Bytes::from(vec![0u8; 5 * 1024 * 1024])
).await?;

let part2 = storage.upload_part(
    "my-bucket",
    "large-file.bin",
    &upload_id,
    2,
    Bytes::from(vec![1u8; 5 * 1024 * 1024])
).await?;

// Complete upload
let parts = vec![part1, part2];
storage.complete_multipart_upload(
    "my-bucket",
    "large-file.bin",
    &upload_id,
    parts
).await?;
```

### With Deduplication

```rust
use rs3gw::storage::dedup::{DedupManager, DedupConfig};

let config = DedupConfig::default();
let dedup = DedupManager::new("/data/dedup".into(), config).await?;

// Store with deduplication
dedup.store_object("bucket", "key", &data).await?;

// Retrieve
let data = dedup.get_object("bucket", "key").await?;

// Stats
let stats = dedup.get_stats().await;
println!("Dedup ratio: {:.2}%", stats.dedup_ratio * 100.0);
```

### With Encryption

```rust
use rs3gw::storage::encryption::{EncryptionService, LocalKeyProvider};

let key_provider = Arc::new(LocalKeyProvider::new());
let service = EncryptionService::new(key_provider);

// Encrypt
let encrypted = service.encrypt(&data, Some("context"))?;

// Decrypt
let decrypted = service.decrypt(&encrypted, Some("context"))?;

// Rotate key
service.rotate_key("key-id", "new-key-id")?;
```

## Configuration

Storage configuration is managed through environment variables and TOML files:

```toml
[storage]
root = "/data/storage"
compression = "zstd:3"

[storage.dedup]
enabled = true
block_size = 65536
algorithm = "content-defined"
min_object_size = 131072

[storage.zerocopy]
direct_io = true
direct_io_threshold = 1048576
splice = true
mmap = true

[storage.cache]
enabled = true
max_size_mb = 256
max_objects = 10000
ttl_seconds = 300
```

See the main [README.md](../../README.md) for complete configuration reference.

## Performance Characteristics

Based on comprehensive benchmarks:

- **GET Operations**: 746 MiB/s (1MB objects)
- **PUT Operations**: 102 MiB/s (1MB objects)
- **HEAD Operations**: 17.8 µs (ultra-fast metadata)
- **LIST Operations**: 51.5 Kelem/s (1000 objects)
- **Deduplication**: 30-70% storage savings
- **Zero-Copy**: 2-3x throughput improvement for large files
- **ML Cache**: 80-95% hit rate for predictable patterns

## Testing

The storage module includes comprehensive test coverage:

- **Unit Tests**: 336+ tests across all components
- **Integration Tests**: Backend integration, multipart uploads
- **Performance Tests**: Criterion.rs benchmarks
- **Reliability Tests**: Corruption detection, self-healing

Run tests:
```bash
# All storage tests
cargo test --lib storage::

# Specific module
cargo test --lib storage::dedup::

# With all features
cargo test --lib --all-features
```

## Dependencies

Key dependencies for storage functionality:

- **tokio** - Async runtime
- **bytes** - Efficient byte buffers
- **serde** - Serialization
- **sha2** - Cryptographic hashing
- **zstd** - Compression
- **parquet/arrow** - Columnar data
- **image** - Image processing
- **ffmpeg-next** - Video transcoding (optional)
- **wasmtime** - WASM runtime (optional)

## Future Enhancements

Planned improvements:

1. **Additional Backends**
   - Complete Ceph RADOS integration
   - Complete GlusterFS integration
   - Azure and GCS production implementations

2. **Performance Optimizations**
   - io_uring support for ultra-fast I/O (Linux)
   - SIMD-accelerated checksums
   - GPU-accelerated image/video processing

3. **Advanced Features**
   - HSM (Hardware Security Module) integration
   - Cloud KMS integration (AWS KMS, Vault)
   - Bring Your Own Key (BYOK) support

4. **Data Processing**
   - Additional format support (Avro, ORC enhancements)
   - Real-time analytics pipelines
   - Stream processing integration

## Related Documentation

- [API Module](../api/README.md) - HTTP handlers and routing
- [Auth Module](../auth/README.md) - Authentication and authorization
- [Deduplication](DEDUP.md) - Detailed dedup documentation
- [Main README](../../README.md) - Project overview and getting started

## License

Apache-2.0
