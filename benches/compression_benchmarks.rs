//! Compression Performance Benchmarks
//!
//! This benchmark suite measures the performance of different compression algorithms:
//! - Zstd compression at various levels
//! - LZ4 compression
//! - Compression vs no compression tradeoffs
//!
//! Run with: cargo bench --bench compression_benchmarks

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

/// Generate test data with varying entropy
fn generate_test_data(size: usize, entropy: f32) -> Vec<u8> {
    use scirs2_core::random::quick::random_f64;

    if entropy < 0.1 {
        // Low entropy - highly compressible (repeated pattern)
        vec![0u8; size]
    } else if entropy < 0.5 {
        // Medium entropy - somewhat compressible (pattern with some variation)
        (0..size).map(|i| (i % 256) as u8).collect()
    } else {
        // High entropy - incompressible (random data)
        (0..size).map(|_| (random_f64() * 256.0) as u8).collect()
    }
}

/// Benchmark: Zstd compression with different levels
fn bench_zstd_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("zstd_compression");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = generate_test_data(*size, 0.3); // Medium entropy
        group.throughput(Throughput::Bytes(*size as u64));

        for level in [1, 3, 6, 9].iter() {
            group.bench_with_input(
                BenchmarkId::from_parameter(format!("{}bytes_level{}", size, level)),
                &(*size, *level),
                |b, _| {
                    b.iter(|| {
                        let compressed = zstd::encode_all(black_box(&data[..]), *level)
                            .expect("Failed to compress");
                        black_box(compressed);
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark: Zstd decompression
fn bench_zstd_decompression(c: &mut Criterion) {
    let mut group = c.benchmark_group("zstd_decompression");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = generate_test_data(*size, 0.3);
        let compressed = zstd::encode_all(&data[..], 3).expect("Failed to compress");

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &compressed,
            |b, compressed_data| {
                b.iter(|| {
                    let decompressed = zstd::decode_all(black_box(&compressed_data[..]))
                        .expect("Failed to decompress");
                    black_box(decompressed);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: LZ4 compression
fn bench_lz4_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4_compression");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = generate_test_data(*size, 0.3);
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| {
                let compressed = lz4_flex::compress_prepend_size(black_box(data));
                black_box(compressed);
            });
        });
    }

    group.finish();
}

/// Benchmark: LZ4 decompression
fn bench_lz4_decompression(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4_decompression");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = generate_test_data(*size, 0.3);
        let compressed = lz4_flex::compress_prepend_size(&data);

        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &compressed,
            |b, compressed_data| {
                b.iter(|| {
                    let decompressed =
                        lz4_flex::decompress_size_prepended(black_box(compressed_data))
                            .expect("Failed to decompress");
                    black_box(decompressed);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Compression ratio vs entropy
fn bench_compression_ratio(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_ratio");
    let size = 102_400; // 100KB

    for entropy in [0.1, 0.3, 0.5, 0.7, 0.9].iter() {
        let data = generate_test_data(size, *entropy);
        group.throughput(Throughput::Bytes(size as u64));

        // Zstd level 3
        group.bench_with_input(
            BenchmarkId::new("zstd_level3", format!("entropy_{}", entropy)),
            &data,
            |b, data| {
                b.iter(|| {
                    let compressed =
                        zstd::encode_all(black_box(&data[..]), 3).expect("Failed to compress");
                    black_box(compressed);
                });
            },
        );

        // LZ4
        group.bench_with_input(
            BenchmarkId::new("lz4", format!("entropy_{}", entropy)),
            &data,
            |b, data| {
                b.iter(|| {
                    let compressed = lz4_flex::compress_prepend_size(black_box(data));
                    black_box(compressed);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Compression speedup vs file size threshold
fn bench_compression_threshold(c: &mut Criterion) {
    let mut group = c.benchmark_group("compression_threshold");

    for size in [512, 1024, 2048, 4096, 8192].iter() {
        let data = generate_test_data(*size, 0.3);
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(
            BenchmarkId::new("no_compression", size),
            &data,
            |b, data| {
                b.iter(|| {
                    let result = Bytes::copy_from_slice(black_box(data));
                    black_box(result);
                });
            },
        );

        group.bench_with_input(BenchmarkId::new("zstd_level1", size), &data, |b, data| {
            b.iter(|| {
                let compressed =
                    zstd::encode_all(black_box(&data[..]), 1).expect("Failed to compress");
                black_box(compressed);
            });
        });

        group.bench_with_input(BenchmarkId::new("lz4", size), &data, |b, data| {
            b.iter(|| {
                let compressed = lz4_flex::compress_prepend_size(black_box(data));
                black_box(compressed);
            });
        });
    }

    group.finish();
}

criterion_group!(
    compression_benches,
    bench_zstd_compression,
    bench_zstd_decompression,
    bench_lz4_compression,
    bench_lz4_decompression,
    bench_compression_ratio,
    bench_compression_threshold,
);

criterion_main!(compression_benches);
