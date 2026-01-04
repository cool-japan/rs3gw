//! S3 API Compatibility Benchmarks
//!
//! Comprehensive benchmarks for all S3 API operations to ensure
//! compatibility and performance parity with AWS S3.
//!
//! Covers:
//! - Bucket operations
//! - Object operations
//! - Multipart upload
//! - Tagging
//! - Metadata operations
//! - Copy operations

use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rs3gw::storage::{ObjectTagging, StorageEngine};
use std::collections::HashMap;
use std::hint::black_box;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Initialize storage engine for benchmarks
fn init_storage() -> (StorageEngine, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let engine =
        StorageEngine::new(temp_dir.path().to_path_buf()).expect("Failed to create storage engine");
    (engine, temp_dir)
}

/// Benchmark bucket operations
fn bench_bucket_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("s3_bucket_ops");
    let rt = Runtime::new().unwrap();

    // CreateBucket
    group.bench_function("CreateBucket", |b| {
        b.to_async(&rt).iter(|| async {
            let (storage, _temp_dir) = init_storage();
            let bucket = format!("bucket-{}", uuid::Uuid::new_v4());

            let _: () = storage.create_bucket(&bucket).await.unwrap();
            black_box(());
        });
    });

    // HeadBucket
    group.bench_function("HeadBucket", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();
            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            black_box(storage.bucket_exists("test-bucket").await.unwrap());
        });
    });

    // ListBuckets
    group.bench_function("ListBuckets", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            // Create multiple buckets
            for i in 0..10 {
                storage
                    .create_bucket(&format!("bucket-{}", i))
                    .await
                    .unwrap();
            }
            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            black_box(storage.list_buckets().await.unwrap());
        });
    });

    // DeleteBucket
    group.bench_function("DeleteBucket", |b| {
        b.to_async(&rt).iter(|| async {
            let (storage, _temp_dir) = init_storage();
            let bucket = format!("bucket-{}", uuid::Uuid::new_v4());
            storage.create_bucket(&bucket).await.unwrap();

            let _: () = storage.delete_bucket(&bucket).await.unwrap();
            black_box(());
        });
    });

    // PutBucketTagging
    group.bench_function("PutBucketTagging", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();
            (storage, temp_dir)
        });

        let mut tags_map = HashMap::new();
        tags_map.insert("Environment".to_string(), "production".to_string());
        tags_map.insert("Team".to_string(), "engineering".to_string());
        let tagging = ObjectTagging { tags: tags_map };

        b.to_async(&rt).iter(|| async {
            let _: () = storage
                .put_bucket_tagging("test-bucket", &tagging)
                .await
                .unwrap();
            black_box(());
        });
    });

    // GetBucketTagging
    group.bench_function("GetBucketTagging", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let mut tags_map = HashMap::new();
            tags_map.insert("Environment".to_string(), "production".to_string());
            let tagging = ObjectTagging { tags: tags_map };
            storage
                .put_bucket_tagging("test-bucket", &tagging)
                .await
                .unwrap();

            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            black_box(storage.get_bucket_tagging("test-bucket").await.unwrap());
        });
    });

    group.finish();
}

/// Benchmark object operations
fn bench_object_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("s3_object_ops");
    let rt = Runtime::new().unwrap();

    // PutObject with different sizes
    for size_kb in [1, 10, 100, 1024].iter() {
        let size = size_kb * 1024;
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("PutObject", format!("{}KB", size_kb)),
            &size,
            |b, &size| {
                let (storage, _temp_dir) = rt.block_on(async {
                    let (storage, temp_dir) = init_storage();
                    storage.create_bucket("test-bucket").await.unwrap();
                    (storage, temp_dir)
                });

                let data = Bytes::from(vec![0u8; size]);

                b.to_async(&rt).iter(|| async {
                    let key = format!("obj-{}", uuid::Uuid::new_v4());
                    black_box(
                        storage
                            .put_object(
                                "test-bucket",
                                &key,
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
    }

    // GetObject
    group.bench_function("GetObject", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let data = Bytes::from(vec![0u8; 10240]); // 10KB
            storage
                .put_object(
                    "test-bucket",
                    "test-object",
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
                .get_object("test-bucket", "test-object")
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
    });

    // HeadObject
    group.bench_function("HeadObject", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let data = Bytes::from(vec![0u8; 1024]);
            storage
                .put_object(
                    "test-bucket",
                    "test-object",
                    "application/octet-stream",
                    HashMap::new(),
                    data,
                )
                .await
                .unwrap();

            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            black_box(
                storage
                    .head_object("test-bucket", "test-object")
                    .await
                    .unwrap(),
            );
        });
    });

    // DeleteObject
    group.bench_function("DeleteObject", |b| {
        b.to_async(&rt).iter(|| async {
            let (storage, _temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let data = Bytes::from(vec![0u8; 1024]);
            let key = format!("obj-{}", uuid::Uuid::new_v4());
            storage
                .put_object(
                    "test-bucket",
                    &key,
                    "application/octet-stream",
                    HashMap::new(),
                    data,
                )
                .await
                .unwrap();

            let _: () = storage.delete_object("test-bucket", &key).await.unwrap();
            black_box(());
        });
    });

    // ListObjects
    for count in [10, 100, 1000].iter() {
        group.throughput(Throughput::Elements(*count as u64));

        group.bench_with_input(
            BenchmarkId::new("ListObjects", count),
            count,
            |b, &count| {
                let (storage, _temp_dir) = rt.block_on(async {
                    let (storage, temp_dir) = init_storage();
                    storage.create_bucket("test-bucket").await.unwrap();

                    let data = Bytes::from(vec![0u8; 1024]);
                    for i in 0..count {
                        storage
                            .put_object(
                                "test-bucket",
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
                            .list_objects("test-bucket", "", None, count)
                            .await
                            .unwrap(),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark copy operations
fn bench_copy_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("s3_copy_ops");
    let rt = Runtime::new().unwrap();

    for size_kb in [10, 100, 1024].iter() {
        let size = size_kb * 1024;
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_with_input(
            BenchmarkId::new("CopyObject", format!("{}KB", size_kb)),
            &size,
            |b, &size| {
                let (storage, _temp_dir) = rt.block_on(async {
                    let (storage, temp_dir) = init_storage();
                    storage.create_bucket("source-bucket").await.unwrap();
                    storage.create_bucket("dest-bucket").await.unwrap();

                    let data = Bytes::from(vec![0u8; size]);
                    storage
                        .put_object(
                            "source-bucket",
                            "source-object",
                            "application/octet-stream",
                            HashMap::new(),
                            data,
                        )
                        .await
                        .unwrap();

                    (storage, temp_dir)
                });

                b.to_async(&rt).iter(|| async {
                    let dest_key = format!("dest-{}", uuid::Uuid::new_v4());
                    black_box(
                        storage
                            .copy_object(
                                "source-bucket",
                                "source-object",
                                "dest-bucket",
                                &dest_key,
                                None, // metadata_directive
                                None, // new_metadata
                                None, // new_content_type
                            )
                            .await
                            .unwrap(),
                    );
                });
            },
        );
    }

    group.finish();
}

/// Benchmark tagging operations
fn bench_tagging_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("s3_tagging_ops");
    let rt = Runtime::new().unwrap();

    // PutObjectTagging
    group.bench_function("PutObjectTagging", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let data = Bytes::from(vec![0u8; 1024]);
            storage
                .put_object(
                    "test-bucket",
                    "test-object",
                    "application/octet-stream",
                    HashMap::new(),
                    data,
                )
                .await
                .unwrap();

            (storage, temp_dir)
        });

        let mut tags_map = HashMap::new();
        tags_map.insert("Status".to_string(), "active".to_string());
        tags_map.insert("Priority".to_string(), "high".to_string());
        let tagging = ObjectTagging { tags: tags_map };

        b.to_async(&rt).iter(|| async {
            let _: () = storage
                .put_object_tagging("test-bucket", "test-object", &tagging)
                .await
                .unwrap();
            black_box(());
        });
    });

    // GetObjectTagging
    group.bench_function("GetObjectTagging", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let data = Bytes::from(vec![0u8; 1024]);
            storage
                .put_object(
                    "test-bucket",
                    "test-object",
                    "application/octet-stream",
                    HashMap::new(),
                    data,
                )
                .await
                .unwrap();

            let mut tags_map = HashMap::new();
            tags_map.insert("Status".to_string(), "active".to_string());
            let tagging = ObjectTagging { tags: tags_map };
            storage
                .put_object_tagging("test-bucket", "test-object", &tagging)
                .await
                .unwrap();

            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            black_box(
                storage
                    .get_object_tagging("test-bucket", "test-object")
                    .await
                    .unwrap(),
            );
        });
    });

    // DeleteObjectTagging
    group.bench_function("DeleteObjectTagging", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();

            let data = Bytes::from(vec![0u8; 1024]);
            storage
                .put_object(
                    "test-bucket",
                    "test-object",
                    "application/octet-stream",
                    HashMap::new(),
                    data,
                )
                .await
                .unwrap();

            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            // Add tags
            let mut tags_map = HashMap::new();
            tags_map.insert("Status".to_string(), "active".to_string());
            let tagging = ObjectTagging { tags: tags_map };
            storage
                .put_object_tagging("test-bucket", "test-object", &tagging)
                .await
                .unwrap();

            // Delete tags
            let _: () = storage
                .delete_object_tagging("test-bucket", "test-object")
                .await
                .unwrap();
            black_box(());
        });
    });

    group.finish();
}

/// Benchmark multipart upload operations
fn bench_multipart_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("s3_multipart_ops");
    group.sample_size(10); // Fewer samples due to complexity
    let rt = Runtime::new().unwrap();

    // CreateMultipartUpload
    group.bench_function("CreateMultipartUpload", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();
            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            let key = format!("mp-{}", uuid::Uuid::new_v4());
            black_box(
                storage
                    .create_multipart_upload(
                        "test-bucket",
                        &key,
                        "application/octet-stream",
                        HashMap::new(),
                    )
                    .await
                    .unwrap(),
            );
        });
    });

    // UploadPart + CompleteMultipartUpload
    group.bench_function("CompleteMultipartUpload_3parts", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();
            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            let key = format!("mp-{}", uuid::Uuid::new_v4());
            let upload_id = storage
                .create_multipart_upload(
                    "test-bucket",
                    &key,
                    "application/octet-stream",
                    HashMap::new(),
                )
                .await
                .unwrap();

            // Upload 3 parts (5MB each minimum for S3 compatibility simulation)
            let part_data = Bytes::from(vec![0u8; 5 * 1024 * 1024]);
            let mut parts = vec![];

            for part_num in 1..=3 {
                let etag = storage
                    .upload_part("test-bucket", &key, &upload_id, part_num, part_data.clone())
                    .await
                    .unwrap();
                parts.push((part_num, etag));
            }

            black_box(
                storage
                    .complete_multipart_upload("test-bucket", &key, &upload_id, &parts)
                    .await
                    .unwrap(),
            );
        });
    });

    // AbortMultipartUpload
    group.bench_function("AbortMultipartUpload", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();
            (storage, temp_dir)
        });

        b.to_async(&rt).iter(|| async {
            let key = format!("mp-{}", uuid::Uuid::new_v4());
            let upload_id = storage
                .create_multipart_upload(
                    "test-bucket",
                    &key,
                    "application/octet-stream",
                    HashMap::new(),
                )
                .await
                .unwrap();

            let _: () = storage
                .abort_multipart_upload("test-bucket", &key, &upload_id)
                .await
                .unwrap();
            black_box(());
        });
    });

    group.finish();
}

/// Benchmark metadata operations
fn bench_metadata_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("s3_metadata_ops");
    let rt = Runtime::new().unwrap();

    // PUT with custom metadata
    group.bench_function("PutObject_with_metadata", |b| {
        let (storage, _temp_dir) = rt.block_on(async {
            let (storage, temp_dir) = init_storage();
            storage.create_bucket("test-bucket").await.unwrap();
            (storage, temp_dir)
        });

        let data = Bytes::from(vec![0u8; 4096]);

        b.to_async(&rt).iter(|| async {
            let mut metadata = HashMap::new();
            metadata.insert("x-amz-meta-custom1".to_string(), "value1".to_string());
            metadata.insert("x-amz-meta-custom2".to_string(), "value2".to_string());
            metadata.insert("x-amz-meta-custom3".to_string(), "value3".to_string());

            let key = format!("obj-{}", uuid::Uuid::new_v4());
            black_box(
                storage
                    .put_object(
                        "test-bucket",
                        &key,
                        "application/json",
                        metadata,
                        data.clone(),
                    )
                    .await
                    .unwrap(),
            );
        });
    });

    group.finish();
}

criterion_group!(
    s3_api_tests,
    bench_bucket_operations,
    bench_object_operations,
    bench_copy_operations,
    bench_tagging_operations,
    bench_multipart_operations,
    bench_metadata_operations
);
criterion_main!(s3_api_tests);
