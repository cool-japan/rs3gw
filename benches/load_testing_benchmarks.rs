//! Load Testing Benchmarks
//!
//! Comprehensive load testing scenarios for rs3gw:
//! - High concurrency tests
//! - Large file handling
//! - Many small files
//! - Mixed workload patterns
//! - Latency percentile tracking (p50, p95, p99, p99.9)

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use futures::future::join_all;
use rs3gw::storage::StorageEngine;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Initialize storage engine for benchmarks (returns storage and temp dir)
fn init_storage() -> (StorageEngine, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let engine =
        StorageEngine::new(temp_dir.path().to_path_buf()).expect("Failed to create storage engine");
    (engine, temp_dir)
}

/// High concurrency PUT operations
fn bench_concurrent_puts(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_puts");
    let rt = Runtime::new().unwrap();

    // Test different concurrency levels
    for concurrency in [10, 50, 100, 250, 500].iter() {
        group.throughput(Throughput::Elements(*concurrency as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let (storage, _temp_dir) = init_storage();
                    let storage = Arc::new(storage);
                    let bucket = "bench-bucket";
                    storage.create_bucket(bucket).await.unwrap();

                    let data = Bytes::from(vec![0u8; 1024]); // 1KB per object
                    let mut tasks = vec![];

                    for i in 0..concurrency {
                        let storage_clone = Arc::clone(&storage);
                        let data_clone = data.clone();
                        let key = format!("object-{}", i);

                        tasks.push(tokio::spawn(async move {
                            storage_clone
                                .put_object(
                                    bucket,
                                    &key,
                                    "application/octet-stream",
                                    HashMap::new(),
                                    data_clone,
                                )
                                .await
                                .unwrap();
                        }));
                    }

                    black_box(join_all(tasks).await);
                });
            },
        );
    }

    group.finish();
}

/// High concurrency GET operations
fn bench_concurrent_gets(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_gets");
    let rt = Runtime::new().unwrap();

    for concurrency in [10, 50, 100, 250, 500].iter() {
        group.throughput(Throughput::Elements(*concurrency as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(concurrency),
            concurrency,
            |b, &concurrency| {
                // Setup: Create objects first
                let (storage, _temp_dir) = init_storage();
                let storage = Arc::new(rt.block_on(async {
                    let bucket = "bench-bucket";
                    storage.create_bucket(bucket).await.unwrap();

                    let data = Bytes::from(vec![0u8; 1024]); // 1KB per object
                    for i in 0..concurrency {
                        storage
                            .put_object(
                                bucket,
                                &format!("object-{}", i),
                                "application/octet-stream",
                                HashMap::new(),
                                data.clone(),
                            )
                            .await
                            .unwrap();
                    }

                    storage
                }));

                b.to_async(&rt).iter(|| async {
                    let mut tasks = vec![];

                    for i in 0..concurrency {
                        let storage_clone = Arc::clone(&storage);
                        let key = format!("object-{}", i);

                        tasks.push(tokio::spawn(async move {
                            let (_meta, _stream) = storage_clone
                                .get_object("bench-bucket", &key)
                                .await
                                .unwrap();
                        }));
                    }

                    black_box(join_all(tasks).await);
                });
            },
        );
    }

    group.finish();
}

/// Large file handling (10MB, 50MB, 100MB, 500MB)
fn bench_large_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_files");
    group.sample_size(10); // Fewer samples for large files
    let rt = Runtime::new().unwrap();

    for size_mb in [10, 50, 100, 500].iter() {
        let size = size_mb * 1024 * 1024;
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::new("put", format!("{}MB", size_mb)),
            &size,
            |b, &size| {
                let data = Bytes::from(vec![0u8; size]);

                b.to_async(&rt).iter(|| async {
                    let (storage, _temp_dir) = init_storage();
                    storage.create_bucket("bench-bucket").await.unwrap();

                    black_box(
                        storage
                            .put_object(
                                "bench-bucket",
                                "large-object",
                                "application/octet-stream",
                                HashMap::new(),
                                data.clone(),
                            )
                            .await
                            .unwrap(),
                    );
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("get", format!("{}MB", size_mb)),
            &size,
            |b, &size| {
                let (storage, _temp_dir) = rt.block_on(async {
                    let (storage, temp_dir) = init_storage();
                    storage.create_bucket("bench-bucket").await.unwrap();

                    let data = Bytes::from(vec![0u8; size]);
                    storage
                        .put_object(
                            "bench-bucket",
                            "large-object",
                            "application/octet-stream",
                            HashMap::new(),
                            data,
                        )
                        .await
                        .unwrap();

                    (storage, temp_dir)
                });

                b.to_async(&rt).iter(|| async {
                    use futures::StreamExt;
                    let (meta, mut stream) = storage
                        .get_object("bench-bucket", "large-object")
                        .await
                        .unwrap();

                    // Consume the stream to measure realistic performance
                    let mut buffer = Vec::new();
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.expect("Failed to read chunk");
                        buffer.extend_from_slice(&chunk);
                    }

                    black_box((meta, buffer));
                });
            },
        );
    }

    group.finish();
}

/// Many small files (1000, 5000, 10000 files of 1KB each)
fn bench_many_small_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_small_files");
    group.sample_size(10); // Fewer samples due to setup time
    let rt = Runtime::new().unwrap();

    for count in [1000, 5000, 10000].iter() {
        group.throughput(Throughput::Elements(*count as u64));
        group.bench_with_input(
            BenchmarkId::new("sequential_put", count),
            count,
            |b, &count| {
                let data = Bytes::from(vec![0u8; 1024]); // 1KB per file

                b.to_async(&rt).iter(|| async {
                    let (storage, _temp_dir) = init_storage();
                    storage.create_bucket("bench-bucket").await.unwrap();

                    for i in 0..count {
                        storage
                            .put_object(
                                "bench-bucket",
                                &format!("small-{:06}", i),
                                "application/octet-stream",
                                HashMap::new(),
                                data.clone(),
                            )
                            .await
                            .unwrap();
                    }
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("list_all", count), count, |b, &count| {
            let (storage, _temp_dir) = rt.block_on(async {
                let (storage, temp_dir) = init_storage();
                storage.create_bucket("bench-bucket").await.unwrap();

                let data = Bytes::from(vec![0u8; 1024]);
                for i in 0..count {
                    storage
                        .put_object(
                            "bench-bucket",
                            &format!("small-{:06}", i),
                            "application/octet-stream",
                            HashMap::new(),
                            data.clone(),
                        )
                        .await
                        .unwrap();
                }

                (storage, temp_dir)
            });

            b.to_async(&rt).iter(|| async {
                black_box(
                    storage
                        .list_objects("bench-bucket", "", None, count)
                        .await
                        .unwrap(),
                );
            });
        });
    }

    group.finish();
}

/// Mixed workload: Realistic pattern with reads, writes, lists, deletes
fn bench_mixed_workload(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_workload");
    let rt = Runtime::new().unwrap();

    group.bench_function("realistic_pattern", |b| {
        b.to_async(&rt).iter(|| async {
            let (storage, _temp_dir) = init_storage();
            let storage = Arc::new(storage);
            storage.create_bucket("bench-bucket").await.unwrap();

            let mut tasks = vec![];

            // 60% reads, 30% writes, 5% lists, 5% deletes
            for i in 0..100 {
                let storage_clone = Arc::clone(&storage);

                tasks.push(tokio::spawn(async move {
                    let op_type = i % 20;

                    match op_type {
                        // 60% reads (0-11)
                        0..=11 => {
                            let _ = storage_clone
                                .get_object("bench-bucket", &format!("obj-{}", i % 50))
                                .await;
                        }
                        // 30% writes (12-17)
                        12..=17 => {
                            let data = Bytes::from(vec![0u8; 4096]); // 4KB
                            let _ = storage_clone
                                .put_object(
                                    "bench-bucket",
                                    &format!("obj-{}", i),
                                    "application/octet-stream",
                                    HashMap::new(),
                                    data,
                                )
                                .await;
                        }
                        // 5% lists (18)
                        18 => {
                            let _ = storage_clone
                                .list_objects("bench-bucket", "", None, 100)
                                .await;
                        }
                        // 5% deletes (19)
                        _ => {
                            let _ = storage_clone
                                .delete_object("bench-bucket", &format!("obj-{}", i % 50))
                                .await;
                        }
                    }
                }));
            }

            black_box(join_all(tasks).await);
        });
    });

    group.finish();
}

/// Latency percentile tracking
fn bench_latency_percentiles(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_percentiles");
    group.sample_size(1000); // Large sample for accurate percentiles
    let rt = Runtime::new().unwrap();

    // PUT latency distribution
    group.bench_function("put_latency", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("bench-bucket").await.unwrap();
            (Arc::new(storage), temp_dir)
        });

        let data = Bytes::from(vec![0u8; 4096]); // 4KB
        let counter = Arc::new(AtomicUsize::new(0));

        b.to_async(&rt).iter(|| {
            let counter_clone = Arc::clone(&counter);
            let storage_clone = Arc::clone(&storage);
            let data_clone = data.clone();
            async move {
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                black_box(
                    storage_clone
                        .put_object(
                            "bench-bucket",
                            &format!("obj-{}", count),
                            "application/octet-stream",
                            HashMap::new(),
                            data_clone,
                        )
                        .await
                        .unwrap(),
                );
            }
        });
    });

    // GET latency distribution
    group.bench_function("get_latency", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("bench-bucket").await.unwrap();

            // Pre-populate with 100 objects
            let data = Bytes::from(vec![0u8; 4096]);
            for i in 0..100 {
                storage
                    .put_object(
                        "bench-bucket",
                        &format!("obj-{}", i),
                        "application/octet-stream",
                        HashMap::new(),
                        data.clone(),
                    )
                    .await
                    .unwrap();
            }

            (Arc::new(storage), temp_dir)
        });

        let counter = Arc::new(AtomicUsize::new(0));
        b.to_async(&rt).iter(|| {
            let counter_clone = Arc::clone(&counter);
            let storage_clone = Arc::clone(&storage);
            async move {
                use futures::StreamExt;
                let count = counter_clone.fetch_add(1, Ordering::SeqCst);
                let key = format!("obj-{}", count % 100);

                let (meta, mut stream) = storage_clone
                    .get_object("bench-bucket", &key)
                    .await
                    .unwrap();

                // Consume the stream to measure realistic performance
                let mut buffer = Vec::new();
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.expect("Failed to read chunk");
                    buffer.extend_from_slice(&chunk);
                }

                black_box((meta, buffer));
            }
        });
    });

    // LIST latency distribution
    group.bench_function("list_latency", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("bench-bucket").await.unwrap();

            // Pre-populate with 1000 objects
            let data = Bytes::from(vec![0u8; 1024]);
            for i in 0..1000 {
                storage
                    .put_object(
                        "bench-bucket",
                        &format!("obj-{:04}", i),
                        "application/octet-stream",
                        HashMap::new(),
                        data.clone(),
                    )
                    .await
                    .unwrap();
            }

            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            black_box(
                storage
                    .list_objects("bench-bucket", "", None, 100)
                    .await
                    .unwrap(),
            );
        });
    });

    group.finish();
}

criterion_group!(
    load_tests,
    bench_concurrent_puts,
    bench_concurrent_gets,
    bench_large_files,
    bench_many_small_files,
    bench_mixed_workload,
    bench_latency_percentiles
);
criterion_main!(load_tests);
