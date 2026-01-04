//! Storage Engine Performance Benchmarks
//!
//! This benchmark suite measures the performance of core storage operations:
//! - Object creation and retrieval
//! - Metadata operations
//! - Range requests
//! - Multipart uploads
//!
//! Run with: cargo bench --bench storage_benchmarks

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rs3gw::storage::StorageEngine;
use std::collections::HashMap;
use std::hint::black_box;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Setup a storage engine for benchmarking
fn setup_storage() -> (StorageEngine, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let engine =
        StorageEngine::new(temp_dir.path().to_path_buf()).expect("Failed to create storage engine");
    (engine, temp_dir)
}

/// Benchmark: Create bucket
fn bench_create_bucket(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");

    c.bench_function("create_bucket", |b| {
        b.iter(|| {
            let (engine, _temp_dir) = setup_storage();
            rt.block_on(async {
                engine
                    .create_bucket(black_box("benchmark-bucket"))
                    .await
                    .expect("Failed to create bucket");
            });
        });
    });
}

/// Benchmark: Put object with varying sizes
fn bench_put_object(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let (engine, _temp_dir) = setup_storage();

    rt.block_on(async {
        engine
            .create_bucket("bench-bucket")
            .await
            .expect("Failed to create bucket");
    });

    let mut group = c.benchmark_group("put_object");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = Bytes::from(vec![0u8; *size]);
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    engine
                        .put_object(
                            black_box("bench-bucket"),
                            black_box("test-object"),
                            black_box("application/octet-stream"),
                            HashMap::new(),
                            data.clone(),
                        )
                        .await
                        .expect("Failed to put object");
                });
            });
        });
    }

    group.finish();
}

/// Benchmark: Get object with varying sizes
fn bench_get_object(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let (engine, _temp_dir) = setup_storage();

    rt.block_on(async {
        engine
            .create_bucket("bench-bucket")
            .await
            .expect("Failed to create bucket");
    });

    let mut group = c.benchmark_group("get_object");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = Bytes::from(vec![0u8; *size]);
        let key = format!("test-object-{}", size);

        // Pre-populate object
        rt.block_on(async {
            engine
                .put_object(
                    "bench-bucket",
                    &key,
                    "application/octet-stream",
                    HashMap::new(),
                    data.clone(),
                )
                .await
                .expect("Failed to put object");
        });

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let (_metadata, stream) = engine
                        .get_object(black_box("bench-bucket"), black_box(&key))
                        .await
                        .expect("Failed to get object");

                    // Consume the stream
                    use futures::StreamExt;

                    let mut stream = stream;
                    let mut buffer = Vec::new();
                    while let Some(chunk) = stream.next().await {
                        let chunk = chunk.expect("Failed to read chunk");
                        buffer.extend_from_slice(&chunk);
                    }

                    black_box(buffer);
                });
            });
        });
    }

    group.finish();
}

/// Benchmark: Head object (metadata only)
fn bench_head_object(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let (engine, _temp_dir) = setup_storage();

    rt.block_on(async {
        engine
            .create_bucket("bench-bucket")
            .await
            .expect("Failed to create bucket");

        let data = Bytes::from(vec![0u8; 1024]);
        engine
            .put_object(
                "bench-bucket",
                "test-object",
                "application/octet-stream",
                HashMap::new(),
                data,
            )
            .await
            .expect("Failed to put object");
    });

    c.bench_function("head_object", |b| {
        b.iter(|| {
            rt.block_on(async {
                engine
                    .head_object(black_box("bench-bucket"), black_box("test-object"))
                    .await
                    .expect("Failed to head object");
            });
        });
    });
}

/// Benchmark: List objects with varying counts
fn bench_list_objects(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");
    let (engine, _temp_dir) = setup_storage();

    rt.block_on(async {
        engine
            .create_bucket("bench-bucket")
            .await
            .expect("Failed to create bucket");
    });

    let mut group = c.benchmark_group("list_objects");

    for count in [10, 100, 1000].iter() {
        // Pre-populate objects
        rt.block_on(async {
            for i in 0..*count {
                let data = Bytes::from(vec![0u8; 100]);
                engine
                    .put_object(
                        "bench-bucket",
                        &format!("object-{:04}", i),
                        "application/octet-stream",
                        HashMap::new(),
                        data,
                    )
                    .await
                    .expect("Failed to put object");
            }
        });

        group.throughput(Throughput::Elements(*count as u64));

        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, _| {
            b.iter(|| {
                rt.block_on(async {
                    let (objects, _prefixes) = engine
                        .list_objects(black_box("bench-bucket"), black_box(""), None, 1000)
                        .await
                        .expect("Failed to list objects");

                    black_box(objects);
                });
            });
        });
    }

    group.finish();
}

/// Benchmark: Delete object
fn bench_delete_object(c: &mut Criterion) {
    let rt = Runtime::new().expect("Failed to create runtime");

    c.bench_function("delete_object", |b| {
        b.iter(|| {
            let (engine, _temp_dir) = setup_storage();

            rt.block_on(async {
                engine
                    .create_bucket("bench-bucket")
                    .await
                    .expect("Failed to create bucket");

                let data = Bytes::from(vec![0u8; 1024]);
                engine
                    .put_object(
                        "bench-bucket",
                        "test-object",
                        "application/octet-stream",
                        HashMap::new(),
                        data,
                    )
                    .await
                    .expect("Failed to put object");

                engine
                    .delete_object(black_box("bench-bucket"), black_box("test-object"))
                    .await
                    .expect("Failed to delete object");
            });
        });
    });
}

criterion_group!(
    storage_benches,
    bench_create_bucket,
    bench_put_object,
    bench_get_object,
    bench_head_object,
    bench_list_objects,
    bench_delete_object,
);

criterion_main!(storage_benches);
