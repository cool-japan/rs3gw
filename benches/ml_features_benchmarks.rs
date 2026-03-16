use bytes::Bytes;
use chrono::Utc;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rs3gw::storage::dedup::DedupConfig;
use rs3gw::storage::ml_cache::{MlCacheConfig, SmartCacheManager};
use rs3gw::storage::zerocopy::{should_mmap_metadata, should_use_direct_io, ZeroCopyConfig};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

/// Benchmark ML-based cache operations
fn bench_ml_cache(c: &mut Criterion) {
    let mut group = c.benchmark_group("ml_cache");
    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");

    // Create cache instance
    let config = MlCacheConfig::builder()
        .max_size_mb(256)
        .max_objects(10000)
        .default_ttl_secs(300)
        .build()
        .expect("Failed to build config");

    let cache = SmartCacheManager::new(config).expect("Failed to create cache");

    // Benchmark cache operations
    group.bench_function("get_miss", |b| {
        b.iter(|| {
            rt.block_on(async {
                cache
                    .get(black_box("test-bucket"), black_box("nonexistent-key"))
                    .await
            })
        });
    });

    group.bench_function("put_and_get", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key = format!(
                    "key-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0)
                );
                let value = Bytes::from(vec![0u8; 1024]); // 1KB value
                let metadata = rs3gw::storage::ObjectMetadata {
                    key: key.clone(),
                    size: 1024,
                    etag: "test-etag".to_string(),
                    last_modified: Utc::now(),
                    content_type: "application/octet-stream".to_string(),
                    metadata: HashMap::new(),
                    schema_version: 1,
                };
                let _ = cache
                    .put("test-bucket", &key, metadata.clone(), value)
                    .await;
                cache.get("test-bucket", &key).await
            })
        });
    });

    group.bench_function("cache_stats", |b| {
        b.iter(|| rt.block_on(async { cache.get_stats().await }));
    });

    group.finish();
}

/// Benchmark zero-copy decision functions
fn bench_zerocopy_decisions(c: &mut Criterion) {
    let mut group = c.benchmark_group("zerocopy_decisions");

    let config = ZeroCopyConfig::default();

    // Benchmark decision functions with different sizes
    for size in &[4096, 65536, 1048576, 10485760] {
        group.bench_with_input(
            BenchmarkId::new("should_use_direct_io", size),
            size,
            |b, &size| {
                b.iter(|| should_use_direct_io(&config, black_box(size)));
            },
        );

        group.bench_with_input(
            BenchmarkId::new("should_mmap_metadata", size),
            size,
            |b, &size| {
                b.iter(|| should_mmap_metadata(&config, black_box(size)));
            },
        );
    }

    group.finish();
}

/// Benchmark buffer alignment
fn bench_buffer_alignment(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_alignment");

    for size in &[4096, 65536, 1048576] {
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::new("align_buffer", size), size, |b, &size| {
            b.iter(|| rs3gw::storage::zerocopy::align_buffer(black_box(size)));
        });
    }

    group.finish();
}

/// Benchmark deduplication configuration
fn bench_dedup_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("dedup_config");

    group.bench_function("config_default", |b| {
        b.iter(DedupConfig::default);
    });

    group.finish();
}

/// Benchmark storage class operations
fn bench_storage_class(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_class");

    group.bench_function("manager_creation", |b| {
        b.iter(rs3gw::storage::storage_class::StorageClassManager::new);
    });

    group.finish();
}

/// Benchmark ML cache configuration builder
fn bench_ml_cache_config(c: &mut Criterion) {
    let mut group = c.benchmark_group("ml_cache_config");

    group.bench_function("builder_default", |b| {
        b.iter(|| {
            MlCacheConfig::builder()
                .max_size_mb(256)
                .max_objects(10000)
                .default_ttl_secs(300)
                .build()
        });
    });

    group.bench_function("builder_custom", |b| {
        b.iter(|| {
            MlCacheConfig::builder()
                .max_size_mb(black_box(512))
                .max_objects(black_box(20000))
                .default_ttl_secs(black_box(600))
                .pattern_window_size(black_box(100))
                .prefetch_threshold(black_box(5))
                .build()
        });
    });

    group.finish();
}

/// Benchmark hash calculations (for deduplication)
fn bench_hash_calculations(c: &mut Criterion) {
    let mut group = c.benchmark_group("hash_calculations");

    use sha2::{Digest, Sha256};

    for size in &[4096, 65536, 1048576] {
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::new("sha256", size), size, |b, &size| {
            let data = vec![0u8; size];
            b.iter(|| {
                let mut hasher = Sha256::new();
                hasher.update(black_box(&data));
                hasher.finalize()
            });
        });
    }

    group.finish();
}

criterion_group! {
    name = ml_features;
    config = Criterion::default()
        .sample_size(100)
        .measurement_time(Duration::from_secs(5));
    targets = bench_ml_cache,
              bench_zerocopy_decisions,
              bench_buffer_alignment,
              bench_dedup_config,
              bench_storage_class,
              bench_ml_cache_config,
              bench_hash_calculations
}

criterion_main!(ml_features);
