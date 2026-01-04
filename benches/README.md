# rs3gw Performance Benchmarks

This directory contains comprehensive performance benchmarks for rs3gw using [Criterion.rs](https://github.com/bheisler/criterion.rs).

## Running Benchmarks

### Run All Benchmarks
```bash
cargo bench
```

### Run Specific Benchmark Suite
```bash
# Storage operations
cargo bench --bench storage_benchmarks

# Compression algorithms
cargo bench --bench compression_benchmarks

# API operations (serialization, hashing, encoding)
cargo bench --bench api_benchmarks

# Load testing and concurrency
cargo bench --bench load_testing_benchmarks

# S3 API compatibility
cargo bench --bench s3_api_benchmarks

# gRPC vs REST protocol comparison
cargo bench --bench grpc_vs_rest_benchmarks
```

### Run Specific Benchmark
```bash
cargo bench --bench storage_benchmarks -- put_object
cargo bench --bench compression_benchmarks -- zstd
cargo bench --bench api_benchmarks -- xml
```

## Benchmark Suites

### 1. Storage Benchmarks (`storage_benchmarks.rs`)

Measures performance of core storage engine operations:

- **create_bucket** - Bucket creation overhead
- **put_object** - Object upload throughput at various sizes (1KB to 1MB)
- **get_object** - Object download throughput at various sizes
- **head_object** - Metadata-only operations (no data transfer)
- **list_objects** - Listing performance with 10, 100, and 1000 objects
- **delete_object** - Object deletion overhead

**Key Metrics:**
- Throughput (bytes/second)
- Latency (ns per operation)
- Scaling with object size and count

### 2. Compression Benchmarks (`compression_benchmarks.rs`)

Evaluates compression algorithm performance:

- **zstd_compression** - Zstd compression at levels 1, 3, 6, and 9
- **zstd_decompression** - Zstd decompression performance
- **lz4_compression** - LZ4 compression (faster, lower ratio)
- **lz4_decompression** - LZ4 decompression performance
- **compression_ratio** - Effectiveness vs data entropy (0.1 to 0.9)
- **compression_threshold** - Overhead vs benefit for small files (512B to 8KB)

**Key Metrics:**
- Compression/decompression throughput (MB/s)
- Compression ratio
- Optimal size threshold for enabling compression

**Insights:**
- Zstd level 3 provides good balance between speed and ratio
- LZ4 is 3-4x faster but achieves lower compression
- Files < 2KB often benefit from no compression
- High entropy data (random, encrypted) doesn't compress well

### 3. API Benchmarks (`api_benchmarks.rs`)

Measures performance of API-level operations:

- **xml_serialization** - ListBucketResult XML generation (10 to 1000 objects)
- **sha256_hashing** - ETag generation (1KB to 1MB)
- **hmac_sha256** - Signature computation for authentication
- **base64** - Encoding/decoding (256B to 16KB)
- **hex** - Hex encoding/decoding (32B to 1KB)
- **percent_encoding** - URL encoding/decoding
- **json** - JSON serialization/deserialization

**Key Metrics:**
- Operations per second
- Latency per operation
- Throughput for data transformations

### 4. Load Testing Benchmarks (`load_testing_benchmarks.rs`)

High-concurrency and large-scale performance testing:

- **concurrent_puts** - Concurrent PUT operations (10-500 concurrent)
- **concurrent_gets** - Concurrent GET operations with shared data
- **large_file_upload** - Large file handling (10MB-500MB)
- **large_file_download** - Large file retrieval performance
- **small_file_sequential** - Sequential small file operations (1000-10000 files)
- **small_file_parallel** - Parallel small file operations
- **latency_distribution** - p50/p95/p99/p99.9 latency tracking for PUT/GET/LIST
- **mixed_workload** - Realistic 60% reads, 30% writes, 5% lists, 5% deletes

**Key Metrics:**
- Concurrent operation throughput
- Latency percentiles (p50, p95, p99, p99.9)
- Scalability with concurrency levels
- Large file streaming performance
- Small file batch operation efficiency

### 5. S3 API Compatibility Benchmarks (`s3_api_benchmarks.rs`)

Complete S3 API operation benchmarks:

- **Bucket Operations** - CreateBucket, DeleteBucket, ListBuckets, HeadBucket, Tagging
- **Object Operations** - PutObject (1KB-1MB), GetObject, DeleteObject, HeadObject
- **Multipart Upload** - CreateMultipartUpload, UploadPart, CompleteMultipartUpload, AbortMultipartUpload
- **Copy Operations** - CopyObject across buckets
- **Metadata Operations** - Custom metadata and tagging

**Key Metrics:**
- Per-operation latency and throughput
- S3 API compatibility verification
- Operation scaling characteristics

### 6. gRPC vs REST Performance Comparison (`grpc_vs_rest_benchmarks.rs`)

Protocol efficiency comparison:

- **Serialization Overhead** - XML/JSON vs Protobuf encoding cost
- **Message Size** - HTTP/1.1 headers vs HTTP/2 HPACK compression
- **Data Encoding** - Base64 (REST) vs Binary (gRPC) efficiency
- **Deserialization** - JSON parsing vs Protobuf parsing performance
- **Connection Overhead** - HTTP/1.1 vs HTTP/2 multiplexing advantages

**Key Metrics:**
- Serialization/deserialization speed
- Message size reduction
- Protocol overhead comparison
- Expected efficiency gains (3-10x for serialization, ~33% for binary vs Base64)

**Expected Results:**
- Protobuf: 3-10x faster serialization than JSON
- Binary transfer: ~33% smaller than Base64
- HTTP/2 multiplexing: Eliminates per-request connection overhead
- HPACK: 50-70% header size reduction

## Interpreting Results

Criterion generates detailed reports in `target/criterion/`:

```
target/criterion/
├── put_object/
│   ├── base/
│   │   └── estimates.json     # Statistical estimates
│   ├── change/
│   │   └── estimates.json     # Comparison with previous run
│   └── report/
│       └── index.html         # HTML visualization
...
```

### Key Statistics

- **time** - Mean execution time with confidence intervals
- **change** - Performance change from last run (± percentage)
- **throughput** - Data processed per second (where applicable)

### Performance Targets (Indicative)

| Operation | Size | Target Throughput |
|-----------|------|-------------------|
| put_object | 1MB | > 500 MB/s |
| get_object | 1MB | > 800 MB/s |
| head_object | - | > 100k ops/s |
| zstd (level 3) | 1MB | > 300 MB/s |
| lz4 compress | 1MB | > 1 GB/s |
| sha256 hash | 1MB | > 500 MB/s |

*Note: Actual performance depends on hardware (CPU, storage speed, memory)*

## Continuous Performance Monitoring

### Baseline Recording
```bash
# Record baseline for comparison
cargo bench -- --save-baseline main
```

### Compare Against Baseline
```bash
# After making changes
cargo bench -- --baseline main
```

### CI Integration

Add to `.github/workflows/benchmark.yml`:

```yaml
name: Benchmark
on: [pull_request]

jobs:
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable

      - name: Run benchmarks
        run: cargo bench --no-fail-fast

      - name: Upload results
        uses: actions/upload-artifact@v3
        with:
          name: benchmark-results
          path: target/criterion/
```

## Profiling

For deeper performance analysis:

### CPU Profiling
```bash
# Install flamegraph
cargo install flamegraph

# Profile specific benchmark
cargo flamegraph --bench storage_benchmarks -- --bench put_object
```

### Memory Profiling
```bash
# Install valgrind and dependencies
sudo apt install valgrind

# Profile memory usage
valgrind --tool=massif cargo bench --bench storage_benchmarks
```

### perf Integration
```bash
# Linux only - detailed CPU profiling
perf record cargo bench --bench storage_benchmarks
perf report
```

## Benchmark Development Guidelines

When adding new benchmarks:

1. **Isolate measurements** - Exclude setup/teardown from timing
2. **Use `black_box()`** - Prevent compiler optimizations
3. **Multiple iterations** - Criterion handles this automatically
4. **Realistic workloads** - Mirror production scenarios
5. **Document expectations** - Add comments about expected performance
6. **Add throughput** - Use `Throughput::Bytes()` or `Throughput::Elements()`

Example:
```rust
fn bench_example(c: &mut Criterion) {
    let mut group = c.benchmark_group("example");
    let data = vec![0u8; 1024];

    group.throughput(Throughput::Bytes(1024));
    group.bench_function("operation", |b| {
        b.iter(|| {
            // Only measure this code
            process(black_box(&data))
        });
    });

    group.finish();
}
```

## Performance Regression Detection

Criterion automatically detects statistically significant changes:

- **Green** - Performance improved
- **Red** - Performance regressed
- **Yellow** - No significant change

Configure sensitivity in `Cargo.toml`:
```toml
[profile.bench]
# Optimize for benchmarking
lto = true
codegen-units = 1
```

## Resources

- [Criterion.rs Documentation](https://bheisler.github.io/criterion.rs/book/)
- [The Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Benchmarking Best Practices](https://easyperf.net/blog/2018/08/26/Estimating-performance-improvement)
