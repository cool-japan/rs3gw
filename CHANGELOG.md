# Changelog

All notable changes to rs3gw will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.2] - 2026-06-17

### Added
- New `server` Cargo feature (default-on) gating the HTTP/gRPC server stack (axum, axum-server, tonic, async-graphql, utoipa, metrics-exporter-prometheus, opentelemetry/OTLP, reqwest webhooks). Library consumers can now use `default-features = false, features = ["local"]` to depend on only the storage layer.
- New `local` Cargo feature for the storage-only profile.
- Feature-gated cloud backends: `s3` (aws-sdk-s3 + aws-config), `gcs` (google-cloud-*), `azure` (azure_*), and `formats` (parquet/arrow/apache-avro/orc-rust/prost-reflect/rmp-serde). All off by default. New `all-backends` meta-feature activates all four.
- `UsageTracker` (`src/observability/usage.rs`) — in-memory per-bucket accumulator tracking PUT/GET/DELETE request counts, bytes uploaded/downloaded, and live storage size/object count; drives the new `/api/usage` and `/api/usage/{bucket}` REST endpoints.
- `/api/usage` endpoint — returns a JSON report with per-bucket `storage_bytes`, `object_count`, `bytes_uploaded`, `bytes_downloaded`, `requests_by_op`, `total_requests`, `estimated_cost.total_usd`, plus report-level `total_objects`, `total_storage_bytes`, and `generated_at`. Accepts `?flush=true` to invoke registered cost-hook callbacks.
- `usage_tests.rs` — 2 integration tests exercising the full `/api/usage` report and `?flush=true` hook path against real S3 operations.
- `xml_parser_fuzz.rs` — deterministic fuzz harness (3 tests) for the hand-rolled XML request parsers; uses a seeded xorshift64\* PRNG so any failure reproduces identically across runs.
- Soak and observability integration test suites (`soak_tests.rs`, `observability_tests.rs`) for latency exemplars per v0.3/v0.5 roadmap.
- `protox = "0.9"` build dependency — pure-Rust `.proto` compiler; `build.rs` now calls `protox::compile` to produce a `FileDescriptorSet` consumed by `tonic-prost-build`, eliminating the need for a system `protoc` installation.

### Changed
- `scirs2-core` and `scirs2-io` updated from 0.4.4 to 0.5.0.
- `oxiarc-zstd`, `oxiarc-lz4`, `oxiarc-deflate` updated from 0.2.8 to 0.3.3.
- `opentelemetry-otlp` now uses `default-features = false` (grpc-tonic transport only) to drop the reqwest/native-tls HTTP path.
- `tonic` dependency now includes `features = ["tls-ring"]` for TLS support.
- All server-only integration test files now carry `#![cfg(feature = "server")]`; format-dependent modules carry `#[cfg(feature = "formats")]` guards — compilation is skipped cleanly when the relevant feature is off.
- `select_object_content` handler and XML select parser gated behind `#[cfg(feature = "formats")]`.
- Binary `testdata-generator` now declares `required-features = ["formats"]`; `s3-migrate` requires `["s3", "server"]`; `rs3ctl` requires `["server"]`.
- Pure-Rust default closure: with `default-features = false, features = ["local"]`, the dependency tree no longer pulls `ring`, `aws-lc-sys`, `native-tls`, `zstd-sys`, or `rustls 0.23` (C/FFI eliminated for the storage-only profile, per COOLJAPAN Pure Rust Policy).

### Fixed
- `HEAD` for SSE-encrypted objects now returns the plaintext `Content-Length` instead of the on-disk ciphertext length, making it consistent with `GET`; the sidecar is loaded to derive the plaintext size from either the sum of per-chunk `plaintext_len` fields (chunked/multipart) or `single_shot_plaintext_len(ciphertext_len)` (single-PUT), with a best-effort fallback to the on-disk size if the sidecar is unavailable.

## [0.2.1] - 2026-05-16

### Added
- Comprehensive tests for S3 Object Lock, RestoreObject, and SSE operations (961 tests total)

### Changed
- Update dependencies: quick-xml 0.40, parquet/arrow 58.x, azure_core 1.0
- Reintroduce integration smoke test workflow (`.disabled`)

### Fixed
- Resolve OpenTelemetry version conflict: downgrade `opentelemetry*` from 0.32 to 0.31 to align with `tracing-opentelemetry` 0.32.x which internally targets opentelemetry 0.31.x

## [0.2.0] - 2026-03-16

### Added
- PUT streaming backpressure with incremental SHA-256/MD5 hashing
- Graceful shutdown with in-flight request drain (30s timeout)
- Background multipart GC scheduler (configurable retention/interval)
- Compression metrics wiring (zstd/lz4 ratio, original/compressed bytes)
- gRPC parallel delete with buffer_unordered(10) concurrency
- gRPC range GET support (range_start/range_end fields)
- InFlightTracker/InFlightGuard for request lifecycle tracking
- Filesystem error mapping (PermissionDenied → AccessDenied, StorageFull → InsufficientStorage)
- Content-Length audit and verification for all GET/HEAD/range responses
- Smoke tests for end-to-end lifecycle validation
- DeleteObjects partial failure handling
- Path traversal rejection regression tests
- Zero-byte object roundtrip tests
- SigV4 authentication with rate limiting (10 failures/60s)
- Health JSON endpoint, /ready probe
- Predictive analytics and cost forecasting
- S3 Select with query plan cache and result cache
- Arrow Flight protocol support
- Object Lambda transformations
- Data deduplication with content-addressable storage
- Intelligent tiering with storage class transitions
- Advanced replication manager
- rs3ctl CLI tool (health, metrics, gc-multipart, benchmark, diagnose)

### Changed
- PUT handler now streams body via `Body` instead of buffering `Bytes`
- StorageError::Io no longer auto-derives From; uses manual mapping
- Compression applied at storage layer with metrics recording
- Dependencies upgraded to latest compatible versions

### Documentation
- Production deployment guide (sizing, env vars, TLS, troubleshooting, monitoring)
- README with quick start, boto3/AWS CLI examples, API compatibility table
- OpenTelemetry environment variables documentation
- Bucket stubs module documentation listing all 53 stub endpoints
- rs3ctl CLI documentation

## [0.1.0] - 2026-01-04

### Added

#### Core S3 Gateway
- Complete S3-compatible API implementation with 100+ operations
- Bucket operations: create, delete, list, head, policy, tagging, lifecycle
- Object operations: get, put, delete, copy, head, tagging with streaming support
- Multipart upload: create, upload parts, complete, abort, list parts
- Range requests and conditional headers (If-Match, If-None-Match, etc.)
- AWS Signature V4 authentication with timestamp validation
- Presigned URL support (GET, PUT, POST)
- Axum web server with Tokio async runtime
- TLS/HTTPS support with rustls
- Graceful shutdown (SIGINT/SIGTERM)

#### Storage Engine
- Local filesystem backend with async I/O
- Metadata sidecar files (.meta) for custom attributes
- Transparent compression (Zstd, LZ4) with configurable levels
- ETag generation (SHA256)
- Custom user metadata support
- Checksum validation (CRC32C, CRC32, SHA256, SHA1, MD5)
- Data deduplication with SHA256 content-addressing (30-70% storage savings)
- Zero-copy optimizations (direct I/O, splice/sendfile on Linux, memory-mapped metadata)

#### Advanced Features
- **S3 Select**: SQL queries on CSV, JSON, Parquet, Avro, ORC, Protobuf, MessagePack
- **Advanced SQL**: Aggregations (SUM, AVG, COUNT, MIN, MAX), GROUP BY, ORDER BY, LIMIT, JOINs (INNER, LEFT, RIGHT, FULL OUTER, CROSS), window functions (ROW_NUMBER, RANK, DENSE_RANK, LEAD, LAG, FIRST_VALUE, LAST_VALUE, NTILE), Common Table Expressions (CTEs)
- **Query Optimization**: Predicate pushdown, column pruning (50-80% I/O reduction), parallel execution (3-4x speedup), query plan caching (LRU with 1000 entries)
- **ML-Based Smart Cache**: Access pattern tracking, predictive prefetching with confidence scoring, adaptive TTL, priority-based LRU eviction
- **Object Lambda**: PII redaction, format conversion, data masking transformations
- **Batch Operations**: S3 Batch API for copy, tag, delete, storage class transitions
- **Server-side encryption**: SSE-S3 and SSE-C modes with AES-256-GCM
- **Advanced encryption**: Envelope encryption (DEK/KEK), key rotation, AES-256-GCM + ChaCha20-Poly1305
- **Object versioning**: UUID-based version IDs, delete markers, version history
- **Object Lock**: GOVERNANCE and COMPLIANCE modes with retention policies
- **Event notifications**: Webhook and file destinations for S3 events
- **Tiered storage**: Hot/cold data management with age/size-based policies
- **Archival**: AWS Glacier, Azure Archive, tape libraries with cost optimization
- **Backup & Recovery**: Snapshots, incremental backups, point-in-time recovery

#### Multi-Backend Support
- Storage backend abstraction with 40+ methods
- LocalBackend (wraps StorageEngine)
- MinIOBackend (AWS SDK integration, S3-compatible)
- S3Backend (AWS S3 with native SDK)
- Stub implementations for GCS, Azure Blob, Ceph/RADOS, GlusterFS (ready for integration)
- Backend factory for dynamic backend instantiation from configuration

#### Multiple API Protocols
- **REST API**: Full S3 compatibility with 100+ operations
- **gRPC API**: Protocol buffer schema (661 lines), bi-directional streaming, 40+ operations, client libraries (Python, Go, Rust)
- **GraphQL API**: Flexible queries, real-time subscriptions (WebSocket), Playground UI at /graphql
- **WebSocket Streaming**: Live event delivery with filtering by bucket/prefix/event-type
- **OpenAPI/Swagger**: Auto-generated API documentation at /swagger-ui

#### Security & Compliance
- **ABAC (Attribute-Based Access Control)**: Time-based restrictions, IP whitelist/blacklist (IPv4/IPv6 CIDR), policy evaluation engine
- **Audit Logging**: Immutable HMAC-SHA256 chain, cryptographic verification, tamper detection
- **Security Event Detection**: Brute force detection, privilege escalation monitoring
- **Compliance Reports**: SOC2, HIPAA, GDPR automated reporting
- **Log Forwarding**: Webhook, syslog (RFC 5424 UDP/TCP/TLS), S3, file destinations
- **Log Rotation**: Size-based rotation with zstd compression

#### Observability & Performance
- **Prometheus Metrics**: 50+ metrics (throughput, latency, storage size, cache stats)
- **Distributed Tracing**: OpenTelemetry OTLP integration, W3C Trace Context, compatible with Jaeger/Tempo
- **Continuous Profiling**: CPU, memory, I/O profiling with pprof format export
- **Business Metrics**: Storage utilization, data transfer rates, cost estimation, trend analysis
- **Anomaly Detection**: Statistical baseline (Z-score), 7 anomaly types (LatencySpike, ErrorRateIncrease, ThroughputDrop, etc.)
- **Predictive Analytics**: Capacity forecasting, hotspot prediction, workload classification
- **Health Check**: `/health` endpoint for load balancers
- **Auto-Scaling**: Dynamic thread pool sizing, adaptive rate limiting, memory pressure detection, load shedding

#### Cluster & High Availability
- Multi-node deployment with multi-leader architecture
- Data replication (sync, async, quorum modes)
- Gossip protocol for node discovery
- Conflict resolution with vector clocks (last-writer-wins)
- Configurable replication factor

#### AI/ML Integration
- **ML Model Registry**: Model versioning, metadata tracking, deployment management
- **Dataset Registry**: Dataset versioning, preprocessing pipelines, lineage tracking
- **Distributed Training**: Checkpoint management, gradient storage, metadata tracking
- **Preprocessing Pipelines**: Image operations (resize, crop, normalize, augmentation), format conversion
- **WASM Plugins**: User-defined transformations with sandboxed execution (Wasmtime)
- **Query Intelligence**: Pattern recognition, index recommendations, adaptive caching

#### Scientific Format Support
- HDF5 metadata extraction (superblock parsing, attributes)
- NetCDF metadata extraction (dimensions, variables, attributes)
- Parquet metadata extraction (schema, row counts, compression)
- Auto-detection via file magic bytes
- Custom x-amz-meta-sci-* headers

#### CLI Tools
- **rs3ctl**: Administrative CLI for rs3gw management
- **s3-migrate**: Migration tool for S3-compatible storage
- **testdata-generator**: Generate test datasets for benchmarking

#### Development & Deployment
- Multi-stage Dockerfile for minimal images (50MB)
- Docker Compose example with full observability stack (Prometheus, Grafana, Jaeger, MinIO)
- Kubernetes manifests (kustomize)
- Helm chart (k8s/helm/rs3gw/)
- ARM64 multi-arch builds
- TOML and environment variable configuration
- 50+ configuration options

#### Testing & Quality
- **594 tests total**: 527 unit tests + 67 integration tests
- **Comprehensive benchmarking suite**:
  - Storage operations (put, get, list, delete)
  - Compression algorithms (Zstd vs LZ4)
  - API operations (hashing, encoding, serialization)
  - Load testing (10-500 concurrent ops, 10MB-500MB files, 1000-10000 small files)
  - S3 API compatibility benchmarks
  - gRPC vs REST performance comparison
- **Criterion.rs integration**: HTML reports, baseline tracking, regression detection
- Zero clippy warnings
- All files < 2000 lines (refactoring policy compliance)
- No unwrap() in production code (safety policy)

#### Optional Features
- `io_uring`: Linux io_uring support (kernel 5.11+, 60% I/O improvement)
- `video-transcoding`: Video transcoding with ffmpeg (feature-gated due to C dependencies)
- `wasm-plugins`: WASM plugin runtime (pure Rust via Wasmtime)

### Performance

**Object Operations** (local filesystem):
- Small (1KB): 199-205 KiB/s write, 18 GiB/s read
- Medium (10KB): 1.55-1.60 MiB/s write, 18 GiB/s read
- Large (100KB): 15.3-16.4 MiB/s write, 18 GiB/s read
- XL (1MB): 97-102 MiB/s write, 18 GiB/s read

**Metadata Operations**:
- HeadObject: 18µs (53 GiB/s)
- Bucket tagging: 40µs (put), 17µs (get)
- Object tagging: 1.2ms (put), 19µs (get)

**S3 Select**:
- CSV: 50-100 MB/s parsing
- Parquet (optimized): 200-400 MB/s with column pruning
- Parallel execution: 3-4x speedup on multi-core

**gRPC vs REST**:
- Serialization: Protobuf 2-3x faster than XML/JSON
- Message size: Protobuf 30-50% smaller (HPACK compression)

### Dependencies

Built with 80+ high-quality Rust crates including:
- Tokio (async runtime)
- Axum (web framework)
- Tonic (gRPC)
- Arrow/Parquet (columnar data processing)
- AWS SDK for Rust (S3 backend integration)
- OpenTelemetry (distributed tracing)
- scirs2-io (high-performance storage engine)

### Documentation

- Comprehensive README with architecture diagrams
- Module-level README files (API, Storage, Auth, Cluster, Observability, gRPC)
- TODO.md with detailed roadmap and configuration reference
- Inline documentation with examples
- Client library documentation (Python, Go, Rust)
- Docker and Kubernetes deployment guides

### Fixed

- All compilation warnings eliminated
- Test compilation errors resolved (Arc imports, type annotations)
- Visibility warnings in training_handlers.rs
- Unused import warnings across modules
- Clippy warnings in advanced_replication.rs

### Notes

This is the initial release of rs3gw, representing 9 months of development and ~63,000 lines of Rust code. The project is production-ready for most use cases, with comprehensive testing, documentation, and enterprise features.

For detailed feature status and future roadmap, see [TODO.md](TODO.md).

## Upgrade Guide

N/A (initial release)

## Security Notes

- Default configuration has authentication disabled for development ease
- For production use, **always** set `RS3GW_ACCESS_KEY` and `RS3GW_SECRET_KEY`
- Enable TLS/HTTPS with `RS3GW_TLS_CERT` and `RS3GW_TLS_KEY`
- Review ABAC policies for fine-grained access control
- Enable audit logging for compliance requirements

[0.2.2]: https://github.com/cool-japan/rs3gw/releases/tag/v0.2.2
[0.2.1]: https://github.com/cool-japan/rs3gw/releases/tag/v0.2.1
[0.2.0]: https://github.com/cool-japan/rs3gw/releases/tag/v0.2.0
[0.1.0]: https://github.com/cool-japan/rs3gw/releases/tag/v0.1.0
