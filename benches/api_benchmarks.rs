//! API Handler Performance Benchmarks
//!
//! This benchmark suite measures the performance of S3 API handlers:
//! - Authentication operations
//! - Request parsing
//! - Encoding/decoding operations
//!
//! Run with: cargo bench --bench api_benchmarks

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

/// Benchmark: SHA256 hashing for ETag generation
fn bench_sha256_hashing(c: &mut Criterion) {
    use sha2::{Digest, Sha256};

    let mut group = c.benchmark_group("sha256_hashing");

    for size in [1024, 10_240, 102_400, 1_048_576].iter() {
        let data = vec![0u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| {
                let mut hasher = Sha256::new();
                hasher.update(black_box(data));
                let hash = hasher.finalize();
                black_box(hex::encode(hash));
            });
        });
    }

    group.finish();
}

/// Benchmark: HMAC-SHA256 for signature verification
fn bench_hmac_sha256(c: &mut Criterion) {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut group = c.benchmark_group("hmac_sha256");

    let key = b"test-secret-key-for-benchmarking";

    for size in [256, 1024, 4096].iter() {
        let data = vec![0u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        group.bench_with_input(BenchmarkId::from_parameter(size), &data, |b, data| {
            b.iter(|| {
                let mut mac = HmacSha256::new_from_slice(key).expect("Failed to create HMAC");
                mac.update(black_box(data));
                let result = mac.finalize();
                black_box(hex::encode(result.into_bytes()));
            });
        });
    }

    group.finish();
}

/// Benchmark: Base64 encoding/decoding
fn bench_base64(c: &mut Criterion) {
    use base64::{engine::general_purpose, Engine};

    let mut group = c.benchmark_group("base64");

    for size in [256, 1024, 4096, 16384].iter() {
        let data = vec![0u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        // Encoding
        group.bench_with_input(BenchmarkId::new("encode", size), &data, |b, data| {
            b.iter(|| {
                let encoded = general_purpose::STANDARD.encode(black_box(data));
                black_box(encoded);
            });
        });

        // Decoding
        let encoded = general_purpose::STANDARD.encode(&data);
        group.bench_with_input(
            BenchmarkId::new("decode", size),
            &encoded,
            |b, encoded_data| {
                b.iter(|| {
                    let decoded = general_purpose::STANDARD
                        .decode(black_box(encoded_data))
                        .expect("Failed to decode");
                    black_box(decoded);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Hex encoding/decoding
fn bench_hex(c: &mut Criterion) {
    let mut group = c.benchmark_group("hex");

    for size in [32, 64, 256, 1024].iter() {
        let data = vec![0u8; *size];
        group.throughput(Throughput::Bytes(*size as u64));

        // Encoding
        group.bench_with_input(BenchmarkId::new("encode", size), &data, |b, data| {
            b.iter(|| {
                let encoded = hex::encode(black_box(data));
                black_box(encoded);
            });
        });

        // Decoding
        let encoded = hex::encode(&data);
        group.bench_with_input(
            BenchmarkId::new("decode", size),
            &encoded,
            |b, encoded_data| {
                b.iter(|| {
                    let decoded = hex::decode(black_box(encoded_data)).expect("Failed to decode");
                    black_box(decoded);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: URL percent encoding/decoding
fn bench_percent_encoding(c: &mut Criterion) {
    use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};

    let mut group = c.benchmark_group("percent_encoding");

    let test_strings = [
        ("simple", "simple-string"),
        ("spaces", "string with spaces"),
        ("special", "string/with:special@characters!"),
        ("unicode", "string-with-unicode-文字"),
    ];

    for (name, test_str) in test_strings.iter() {
        group.throughput(Throughput::Bytes(test_str.len() as u64));

        // Encoding
        group.bench_with_input(BenchmarkId::new("encode", name), test_str, |b, s| {
            b.iter(|| {
                let encoded = utf8_percent_encode(black_box(s), NON_ALPHANUMERIC).to_string();
                black_box(encoded);
            });
        });

        // Decoding
        let encoded = utf8_percent_encode(test_str, NON_ALPHANUMERIC).to_string();
        group.bench_with_input(BenchmarkId::new("decode", name), &encoded, |b, s| {
            b.iter(|| {
                let decoded = percent_decode_str(black_box(s))
                    .decode_utf8()
                    .expect("Failed to decode");
                black_box(decoded);
            });
        });
    }

    group.finish();
}

/// Benchmark: JSON serialization/deserialization
fn bench_json(c: &mut Criterion) {
    use serde_json::json;

    let mut group = c.benchmark_group("json");

    for count in [10, 100, 1000].iter() {
        let data = json!({
            "objects": (0..*count).map(|i| json!({
                "key": format!("object-{:04}", i),
                "size": 1024 * i,
                "etag": format!("etag-{}", i),
            })).collect::<Vec<_>>(),
        });

        group.throughput(Throughput::Elements(*count as u64));

        // Serialization
        group.bench_with_input(BenchmarkId::new("serialize", count), &data, |b, data| {
            b.iter(|| {
                let json_str = serde_json::to_string(black_box(data)).expect("Failed to serialize");
                black_box(json_str);
            });
        });

        // Deserialization
        let json_str = serde_json::to_string(&data).expect("Failed to serialize");
        group.bench_with_input(
            BenchmarkId::new("deserialize", count),
            &json_str,
            |b, json_str| {
                b.iter(|| {
                    let value: serde_json::Value =
                        serde_json::from_str(black_box(json_str)).expect("Failed to deserialize");
                    black_box(value);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    api_benches,
    bench_sha256_hashing,
    bench_hmac_sha256,
    bench_base64,
    bench_hex,
    bench_percent_encoding,
    bench_json,
);

criterion_main!(api_benches);
