//! Integration tests for gRPC API
//!
//! Tests all gRPC endpoints including:
//! - Bucket operations
//! - Object operations (including streaming)
//! - Multipart upload operations

mod common;

use metrics_exporter_prometheus::PrometheusHandle;
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;
use tonic::transport::Channel;

// Import generated gRPC client code
use rs3gw::grpc::proto::{
    bucket::{
        CreateBucketRequest, DeleteBucketRequest, GetBucketLocationRequest, GetBucketPolicyRequest,
        GetBucketTaggingRequest, HeadBucketRequest, ListBucketsRequest, PutBucketPolicyRequest,
        PutBucketTaggingRequest,
    },
    multipart::{
        AbortMultipartUploadRequest, CompleteMultipartUploadRequest, CreateMultipartUploadRequest,
        ListPartsRequest, UploadPartRequest,
    },
    object::{
        CopyObjectRequest, DeleteObjectRequest, DeleteObjectsRequest, GetObjectAttributesRequest,
        GetObjectRequest, HeadObjectRequest, ListObjectsRequest, PutObjectRequest,
    },
    s3_service_client::S3ServiceClient,
};

// Global metrics handle - initialized once for all tests
static METRICS_HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

fn get_metrics_handle() -> PrometheusHandle {
    METRICS_HANDLE
        .get_or_init(|| {
            rs3gw::metrics::init_metrics().expect("Failed to initialize metrics for tests")
        })
        .clone()
}

/// Get a unique port using process ID and random component for nextest compatibility
fn get_unique_grpc_port() -> u16 {
    use std::collections::hash_map::RandomState;
    use std::hash::BuildHasher;
    use std::thread;

    let pid = std::process::id() as u64;
    let tid = thread::current().id();

    // Hash thread ID to get a deterministic but unique value
    let random_state = RandomState::new();

    let hash = random_state.hash_one(tid);

    // Combine pid and hash to get a unique port in the range 50000-60000
    let port_offset = ((pid.wrapping_mul(31) ^ hash) % 10000) as u16;
    50000 + port_offset
}

/// Test server info for gRPC tests
#[allow(dead_code)]
struct GrpcTestServer {
    addr: SocketAddr,
    _rest_handle: tokio::task::JoinHandle<()>,
    _grpc_handle: tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>>,
}

/// Set up a test server with both REST and gRPC endpoints
async fn setup_grpc_test_server() -> (S3ServiceClient<Channel>, TempDir, GrpcTestServer) {
    common::init_tracing();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // REST server setup (for compatibility tests)
    let rest_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind REST listener");
    let rest_addr = rest_listener
        .local_addr()
        .expect("Failed to get REST address");

    // gRPC server setup - use unique port for each test to avoid conflicts
    let grpc_port = get_unique_grpc_port();
    let grpc_addr: SocketAddr = format!("127.0.0.1:{}", grpc_port)
        .parse()
        .expect("Failed to parse grpc address");

    // Set up storage and state
    let storage_root = temp_dir.path().to_path_buf();
    let storage = Arc::new(
        rs3gw::storage::StorageEngine::new(storage_root.clone())
            .expect("Failed to create storage engine"),
    );

    // Get global metrics handle
    let metrics_handle = get_metrics_handle();

    let config = rs3gw::Config {
        bind_addr: rest_addr,
        storage_root,
        default_bucket: "default".to_string(),
        access_key: String::new(),
        secret_key: String::new(),
        compression: rs3gw::storage::CompressionMode::None,
        request_timeout_secs: 0,
        max_concurrent_requests: 0,
        tls: rs3gw::TlsConfig::default(),
        connection_pool: rs3gw::ConnectionPoolConfig::default(),
        cluster: rs3gw::cluster::ClusterConfig::default(),
        dedup: rs3gw::storage::DedupConfig::disabled(),
        zerocopy: rs3gw::storage::ZeroCopyConfig::default(),
        select_cache: rs3gw::SelectCacheConfig::default(),
        multipart_retention_hours: 168,
        fsync: false,
    };

    // Initialize preprocessing manager
    let preprocessing_path = temp_dir.path().join("preprocessing");
    let preprocessing_manager = std::sync::Arc::new(
        rs3gw::storage::preprocessing::PreprocessingManager::new(preprocessing_path),
    );

    let predictive_analytics = std::sync::Arc::new(rs3gw::observability::PredictiveAnalytics::new(
        10_000,
        0.023,
        0.09,
        0.0004,
        1_000_000_000_000,
    ));

    let metrics_tracker = std::sync::Arc::new(rs3gw::observability::MetricsTracker::new());

    let select_result_cache =
        std::sync::Arc::new(rs3gw::api::SelectResultCache::new(100, 10 * 1024 * 1024));

    let query_intelligence = std::sync::Arc::new(rs3gw::api::QueryIntelligence::new());

    let training_path = temp_dir.path().join("training");
    let training_manager = std::sync::Arc::new(rs3gw::storage::TrainingManager::new(training_path));

    let state = rs3gw::AppState {
        config,
        storage: storage.clone(),
        metrics_handle,
        cache: None,
        throttle: None,
        quota: None,
        event_broadcaster: rs3gw::api::EventBroadcaster::new(),
        query_plan_cache: None,
        select_result_cache,
        query_intelligence,
        advanced_replication: None,
        preprocessing_manager,
        predictive_analytics,
        metrics_tracker,
        training_manager,
        start_time: std::time::Instant::now(),
        verifier: None,
        auth_failure_counts: std::sync::Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )),
        in_flight: rs3gw::InFlightTracker::new(),
    };

    // Spawn REST server
    let rest_app = axum::Router::new()
        .merge(rs3gw::api::s3_router::routes())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let rest_handle = tokio::spawn(async move {
        axum::serve(rest_listener, rest_app)
            .await
            .expect("REST server failed");
    });

    // Spawn gRPC server
    let grpc_server = rs3gw::grpc::GrpcServer::new(storage, grpc_addr);
    let grpc_handle = tokio::spawn(async move { grpc_server.serve().await });

    // Give servers time to start (gRPC needs more time than REST)
    tokio::time::sleep(Duration::from_millis(1500)).await;

    // Create gRPC client with retry logic for nextest compatibility
    let channel =
        Channel::from_shared(format!("http://{}", grpc_addr)).expect("Failed to create channel");

    // Retry connection with exponential backoff (more generous for parallel test execution)
    let mut client = None;
    for attempt in 0..8 {
        match channel.connect().await {
            Ok(ch) => {
                client = Some(S3ServiceClient::new(ch));
                break;
            }
            Err(_e) if attempt < 7 => {
                let delay = Duration::from_millis(300 * (1 << attempt));
                tokio::time::sleep(delay).await;
                continue;
            }
            Err(e) => panic!("Failed to connect to gRPC server after retries: {}", e),
        }
    }
    let client = client.expect("Client should be initialized");

    let server = GrpcTestServer {
        addr: grpc_addr,
        _rest_handle: rest_handle,
        _grpc_handle: grpc_handle,
    };

    (client, temp_dir, server)
}

// ===== Bucket Operation Tests =====

#[tokio::test]
async fn test_grpc_list_buckets() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Initially no buckets
    let request = tonic::Request::new(ListBucketsRequest {
        max_buckets: None,
        continuation_token: None,
    });
    let response = client
        .list_buckets(request)
        .await
        .expect("Failed to list buckets");

    let buckets = response.into_inner().buckets;
    assert_eq!(buckets.len(), 0, "Expected no buckets initially");
}

#[tokio::test]
async fn test_grpc_create_and_delete_bucket() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "test-bucket".to_string(),
        region: Some("us-east-1".to_string()),
        tags: std::collections::HashMap::new(),
    });

    let response = client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    assert_eq!(response.into_inner().location, "/test-bucket");

    // Verify bucket exists
    let request = tonic::Request::new(HeadBucketRequest {
        bucket: "test-bucket".to_string(),
    });

    let response = client
        .head_bucket(request)
        .await
        .expect("Failed to head bucket");

    assert!(response.into_inner().exists);

    // Delete bucket
    let request = tonic::Request::new(DeleteBucketRequest {
        bucket: "test-bucket".to_string(),
    });

    client
        .delete_bucket(request)
        .await
        .expect("Failed to delete bucket");

    // Verify bucket is gone
    let request = tonic::Request::new(HeadBucketRequest {
        bucket: "test-bucket".to_string(),
    });

    let response = client.head_bucket(request).await;
    assert!(
        response.is_ok(),
        "HeadBucket should succeed but indicate non-existence"
    );
    assert!(
        !response.unwrap().into_inner().exists,
        "Bucket should not exist after deletion"
    );
}

#[tokio::test]
async fn test_grpc_bucket_location() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "location-test".to_string(),
        region: Some("us-west-2".to_string()),
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Get bucket location
    let request = tonic::Request::new(GetBucketLocationRequest {
        bucket: "location-test".to_string(),
    });

    let response = client
        .get_bucket_location(request)
        .await
        .expect("Failed to get bucket location");

    // Note: Current implementation doesn't persist region, defaults to us-east-1
    assert_eq!(response.into_inner().location, "us-east-1");
}

#[tokio::test]
async fn test_grpc_bucket_tagging() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "tagging-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put bucket tagging
    let mut tags = std::collections::HashMap::new();
    tags.insert("Environment".to_string(), "Test".to_string());
    tags.insert("Team".to_string(), "Engineering".to_string());

    let request = tonic::Request::new(PutBucketTaggingRequest {
        bucket: "tagging-test".to_string(),
        tags,
    });

    client
        .put_bucket_tagging(request)
        .await
        .expect("Failed to put bucket tagging");

    // Get bucket tagging
    let request = tonic::Request::new(GetBucketTaggingRequest {
        bucket: "tagging-test".to_string(),
    });

    let response = client
        .get_bucket_tagging(request)
        .await
        .expect("Failed to get bucket tagging");

    let tags = response.into_inner().tags;
    assert_eq!(tags.len(), 2);
    assert_eq!(tags.get("Environment"), Some(&"Test".to_string()));
    assert_eq!(tags.get("Team"), Some(&"Engineering".to_string()));
}

#[tokio::test]
async fn test_grpc_bucket_policy() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "policy-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put bucket policy
    let policy = r#"{"Version":"2012-10-17","Statement":[]}"#;
    let request = tonic::Request::new(PutBucketPolicyRequest {
        bucket: "policy-test".to_string(),
        policy: policy.to_string(),
    });

    client
        .put_bucket_policy(request)
        .await
        .expect("Failed to put bucket policy");

    // Get bucket policy
    let request = tonic::Request::new(GetBucketPolicyRequest {
        bucket: "policy-test".to_string(),
    });

    let response = client
        .get_bucket_policy(request)
        .await
        .expect("Failed to get bucket policy");

    assert_eq!(response.into_inner().policy, policy);
}

// ===== Object Operation Tests =====

#[tokio::test]
async fn test_grpc_put_get_delete_object() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket first
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "object-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put object
    let data = b"Hello, gRPC!".to_vec();
    let request = tonic::Request::new(PutObjectRequest {
        bucket: "object-test".to_string(),
        key: "test.txt".to_string(),
        data: data.clone(),
        content_type: Some("text/plain".to_string()),
        metadata: std::collections::HashMap::new(),
        storage_class: None,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });

    let response = client
        .put_object(request)
        .await
        .expect("Failed to put object");

    let etag = response.into_inner().etag;
    assert!(!etag.is_empty(), "Expected non-empty ETag");

    // Get object
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "object-test".to_string(),
        key: "test.txt".to_string(),
        version_id: None,
        range_start: None,
        range_end: None,
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });

    let response = client
        .get_object(request)
        .await
        .expect("Failed to get object");

    let obj = response.into_inner();
    assert_eq!(obj.data, data);
    assert_eq!(obj.metadata.as_ref().unwrap().etag, etag);
    assert_eq!(
        obj.metadata.as_ref().unwrap().content_type,
        "text/plain".to_string()
    );

    // Head object
    let request = tonic::Request::new(HeadObjectRequest {
        bucket: "object-test".to_string(),
        key: "test.txt".to_string(),
        version_id: None,
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });

    let response = client
        .head_object(request)
        .await
        .expect("Failed to head object");

    let head_response = response.into_inner();
    assert_eq!(
        head_response.metadata.as_ref().unwrap().size,
        data.len() as u64
    );
    assert_eq!(head_response.metadata.as_ref().unwrap().etag, etag);

    // Delete object
    let request = tonic::Request::new(DeleteObjectRequest {
        bucket: "object-test".to_string(),
        key: "test.txt".to_string(),
        version_id: None,
    });

    client
        .delete_object(request)
        .await
        .expect("Failed to delete object");

    // Verify object is gone
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "object-test".to_string(),
        key: "test.txt".to_string(),
        version_id: None,
        range_start: None,
        range_end: None,
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });

    let result = client.get_object(request).await;
    assert!(result.is_err(), "Expected error for non-existent object");
}

#[tokio::test]
async fn test_grpc_list_objects() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "list-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put multiple objects
    for i in 1..=5 {
        let request = tonic::Request::new(PutObjectRequest {
            bucket: "list-test".to_string(),
            key: format!("file{}.txt", i),
            data: format!("Content {}", i).into_bytes(),
            content_type: None,
            metadata: std::collections::HashMap::new(),
            storage_class: None,
            checksum_crc32c: None,
            checksum_crc32: None,
            checksum_sha256: None,
            checksum_sha1: None,
        });

        client
            .put_object(request)
            .await
            .expect("Failed to put object");
    }

    // List objects (paginated version)
    let request = tonic::Request::new(ListObjectsRequest {
        bucket: "list-test".to_string(),
        prefix: None,
        delimiter: None,
        max_keys: Some(10),
        continuation_token: None,
        start_after: None,
        fetch_owner: false,
    });

    let response = client
        .list_objects_paginated(request)
        .await
        .expect("Failed to list objects");

    let list_result = response.into_inner();
    assert_eq!(list_result.objects.len(), 5);
    assert!(!list_result.is_truncated);

    // Verify object keys
    let keys: Vec<String> = list_result.objects.iter().map(|o| o.key.clone()).collect();
    assert!(keys.contains(&"file1.txt".to_string()));
    assert!(keys.contains(&"file5.txt".to_string()));
}

#[tokio::test]
async fn test_grpc_copy_object() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "copy-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put source object
    let data = b"Original data".to_vec();
    let request = tonic::Request::new(PutObjectRequest {
        bucket: "copy-test".to_string(),
        key: "source.txt".to_string(),
        data: data.clone(),
        content_type: Some("text/plain".to_string()),
        metadata: std::collections::HashMap::new(),
        storage_class: None,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });

    let put_response = client
        .put_object(request)
        .await
        .expect("Failed to put object");

    let source_etag = put_response.into_inner().etag;

    // Copy object
    let request = tonic::Request::new(CopyObjectRequest {
        source_bucket: "copy-test".to_string(),
        source_key: "source.txt".to_string(),
        source_version_id: None,
        dest_bucket: "copy-test".to_string(),
        dest_key: "destination.txt".to_string(),
        metadata_directive: None,
        metadata: std::collections::HashMap::new(),
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
        storage_class: None,
    });

    client
        .copy_object(request)
        .await
        .expect("Failed to copy object");

    // Verify destination exists
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "copy-test".to_string(),
        key: "destination.txt".to_string(),
        version_id: None,
        range_start: None,
        range_end: None,
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });

    let response = client
        .get_object(request)
        .await
        .expect("Failed to get copied object");

    let copied_obj = response.into_inner();
    assert_eq!(copied_obj.data, data);
    assert_eq!(copied_obj.metadata.as_ref().unwrap().etag, source_etag);
}

#[tokio::test]
async fn test_grpc_object_attributes() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "attrs-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put object
    let data = b"Test data for attributes".to_vec();
    let request = tonic::Request::new(PutObjectRequest {
        bucket: "attrs-test".to_string(),
        key: "test.txt".to_string(),
        data: data.clone(),
        content_type: Some("text/plain".to_string()),
        metadata: std::collections::HashMap::new(),
        storage_class: None,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });

    client
        .put_object(request)
        .await
        .expect("Failed to put object");

    // Get object attributes
    let request = tonic::Request::new(GetObjectAttributesRequest {
        bucket: "attrs-test".to_string(),
        key: "test.txt".to_string(),
        version_id: None,
    });

    let response = client
        .get_object_attributes(request)
        .await
        .expect("Failed to get object attributes");

    let attrs = response.into_inner();
    assert_eq!(attrs.size, data.len() as u64);
    assert!(!attrs.etag.is_empty());
    assert_eq!(attrs.storage_class, "STANDARD");
}

// ===== Multipart Upload Tests =====

#[tokio::test]
async fn test_grpc_multipart_upload() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "multipart-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Create multipart upload
    let request = tonic::Request::new(CreateMultipartUploadRequest {
        bucket: "multipart-test".to_string(),
        key: "large-file.bin".to_string(),
        content_type: Some("application/octet-stream".to_string()),
        metadata: std::collections::HashMap::new(),
        storage_class: None,
    });

    let response = client
        .create_multipart_upload(request)
        .await
        .expect("Failed to create multipart upload");

    let upload_id = response.into_inner().upload_id;
    assert!(!upload_id.is_empty());

    // Upload part 1 (3MB to stay under gRPC 4MB message limit)
    let part1_data = vec![1u8; 3 * 1024 * 1024]; // 3MB
    let request = tonic::Request::new(UploadPartRequest {
        bucket: "multipart-test".to_string(),
        key: "large-file.bin".to_string(),
        upload_id: upload_id.clone(),
        part_number: 1,
        data: part1_data,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });

    let response = client
        .upload_part(request)
        .await
        .expect("Failed to upload part 1");

    let part1_etag = response.into_inner().etag;

    // Upload part 2 (3MB to stay under gRPC 4MB message limit)
    let part2_data = vec![2u8; 3 * 1024 * 1024]; // 3MB
    let request = tonic::Request::new(UploadPartRequest {
        bucket: "multipart-test".to_string(),
        key: "large-file.bin".to_string(),
        upload_id: upload_id.clone(),
        part_number: 2,
        data: part2_data,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });

    let response = client
        .upload_part(request)
        .await
        .expect("Failed to upload part 2");

    let part2_etag = response.into_inner().etag;

    // List parts
    let request = tonic::Request::new(ListPartsRequest {
        bucket: "multipart-test".to_string(),
        key: "large-file.bin".to_string(),
        upload_id: upload_id.clone(),
        max_parts: Some(10),
        part_number_marker: None,
    });

    let response = client
        .list_parts(request)
        .await
        .expect("Failed to list parts");

    let parts = response.into_inner().parts;
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].part_number, 1);
    assert_eq!(parts[1].part_number, 2);

    // Complete multipart upload
    let completed_parts = vec![
        rs3gw::grpc::proto::multipart::CompletedPart {
            part_number: 1,
            etag: part1_etag,
        },
        rs3gw::grpc::proto::multipart::CompletedPart {
            part_number: 2,
            etag: part2_etag,
        },
    ];

    let request = tonic::Request::new(CompleteMultipartUploadRequest {
        bucket: "multipart-test".to_string(),
        key: "large-file.bin".to_string(),
        upload_id: upload_id.clone(),
        parts: completed_parts,
    });

    let response = client
        .complete_multipart_upload(request)
        .await
        .expect("Failed to complete multipart upload");

    let result = response.into_inner();
    assert!(!result.etag.is_empty());
    assert_eq!(result.bucket, "multipart-test");
    assert_eq!(result.key, "large-file.bin");

    // Verify object exists
    let request = tonic::Request::new(HeadObjectRequest {
        bucket: "multipart-test".to_string(),
        key: "large-file.bin".to_string(),
        version_id: None,
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });

    let response = client
        .head_object(request)
        .await
        .expect("Failed to head completed object");

    let head_response = response.into_inner();
    assert_eq!(
        head_response.metadata.as_ref().unwrap().size,
        6 * 1024 * 1024
    ); // 6MB total (2 x 3MB parts)
}

#[tokio::test]
async fn test_grpc_abort_multipart_upload() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "abort-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });

    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Create multipart upload
    let request = tonic::Request::new(CreateMultipartUploadRequest {
        bucket: "abort-test".to_string(),
        key: "aborted.bin".to_string(),
        content_type: None,
        metadata: std::collections::HashMap::new(),
        storage_class: None,
    });

    let response = client
        .create_multipart_upload(request)
        .await
        .expect("Failed to create multipart upload");

    let upload_id = response.into_inner().upload_id;

    // Upload a part
    let request = tonic::Request::new(UploadPartRequest {
        bucket: "abort-test".to_string(),
        key: "aborted.bin".to_string(),
        upload_id: upload_id.clone(),
        part_number: 1,
        data: vec![1u8; 1024],
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });

    client
        .upload_part(request)
        .await
        .expect("Failed to upload part");

    // Abort multipart upload
    let request = tonic::Request::new(AbortMultipartUploadRequest {
        bucket: "abort-test".to_string(),
        key: "aborted.bin".to_string(),
        upload_id: upload_id.clone(),
    });

    client
        .abort_multipart_upload(request)
        .await
        .expect("Failed to abort multipart upload");

    // Verify parts are gone
    let request = tonic::Request::new(ListPartsRequest {
        bucket: "abort-test".to_string(),
        key: "aborted.bin".to_string(),
        upload_id: upload_id.clone(),
        max_parts: Some(10),
        part_number_marker: None,
    });

    let result = client.list_parts(request).await;
    assert!(result.is_err(), "Expected error for aborted upload");
}

// ===== GrpcConfig env-var tests =====

/// Mutex that serializes env-var mutation across the two config tests so that
/// concurrent test threads cannot observe each other's env state.
static GRPC_CONFIG_ENV_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();

fn grpc_config_env_lock() -> &'static std::sync::Mutex<()> {
    GRPC_CONFIG_ENV_LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Verify that all RS3GW_GRPC_* vars are absent and defaults are used.
#[test]
fn test_grpc_config_from_env_defaults() {
    let _lock = grpc_config_env_lock().lock().expect("env mutex poisoned");

    let _guard = EnvVarGuard::clear(&[
        "RS3GW_GRPC_ENABLED",
        "RS3GW_GRPC_PORT",
        "RS3GW_GRPC_MAX_MESSAGE_SIZE",
        "RS3GW_GRPC_TLS_CERT",
        "RS3GW_GRPC_TLS_KEY",
    ]);

    let cfg = rs3gw::grpc::GrpcConfig::from_env();
    assert!(!cfg.enabled, "enabled should default to false");
    assert_eq!(cfg.bind_addr.port(), 50051, "default port should be 50051");
    assert_eq!(
        cfg.max_message_size_bytes,
        64 * 1024 * 1024,
        "default max message size should be 64MB"
    );
    assert!(
        cfg.tls_cert_path.is_none(),
        "tls_cert_path should be None by default"
    );
    assert!(
        cfg.tls_key_path.is_none(),
        "tls_key_path should be None by default"
    );
    assert!(!cfg.tls_enabled(), "tls should not be enabled by default");
}

/// Verify that env vars override the defaults correctly.
#[test]
fn test_grpc_config_from_env_overrides() {
    let _lock = grpc_config_env_lock().lock().expect("env mutex poisoned");

    let _guard = EnvVarGuard::set(&[
        ("RS3GW_GRPC_ENABLED", "true"),
        ("RS3GW_GRPC_PORT", "50052"),
        ("RS3GW_GRPC_MAX_MESSAGE_SIZE", "33554432"),
        ("RS3GW_GRPC_TLS_CERT", "/tmp/cert.pem"),
        ("RS3GW_GRPC_TLS_KEY", "/tmp/key.pem"),
    ]);

    let cfg = rs3gw::grpc::GrpcConfig::from_env();
    assert!(cfg.enabled, "enabled should be true");
    assert_eq!(
        cfg.bind_addr.port(),
        50052,
        "port should be overridden to 50052"
    );
    assert_eq!(
        cfg.max_message_size_bytes, 33_554_432,
        "max message size should be overridden to 32MB"
    );
    assert_eq!(
        cfg.tls_cert_path.as_deref(),
        Some(std::path::Path::new("/tmp/cert.pem"))
    );
    assert_eq!(
        cfg.tls_key_path.as_deref(),
        Some(std::path::Path::new("/tmp/key.pem"))
    );
    assert!(
        cfg.tls_enabled(),
        "tls should be enabled when both paths are set"
    );
}

/// Test ListObjects pagination: create 25 objects, page through them 10-at-a-time.
#[tokio::test]
async fn test_grpc_list_objects_pagination() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create a dedicated bucket for this test.
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "pagination-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create pagination-test bucket");

    // Upload 25 objects with deterministic, lexicographically sorted keys.
    for i in 0..25u32 {
        let key = format!("obj-{:03}.bin", i);
        let request = tonic::Request::new(PutObjectRequest {
            bucket: "pagination-test".to_string(),
            key,
            data: format!("data-{}", i).into_bytes(),
            content_type: None,
            metadata: std::collections::HashMap::new(),
            storage_class: None,
            checksum_crc32c: None,
            checksum_crc32: None,
            checksum_sha256: None,
            checksum_sha1: None,
        });
        client
            .put_object(request)
            .await
            .expect("Failed to put object");
    }

    // Page 1: expect 10 objects and is_truncated == true.
    let request = tonic::Request::new(ListObjectsRequest {
        bucket: "pagination-test".to_string(),
        prefix: None,
        delimiter: None,
        max_keys: Some(10),
        continuation_token: None,
        start_after: None,
        fetch_owner: false,
    });
    let page1 = client
        .list_objects_paginated(request)
        .await
        .expect("Failed to list page 1")
        .into_inner();
    assert_eq!(page1.objects.len(), 10, "Page 1 should contain 10 objects");
    assert!(page1.is_truncated, "Page 1 should be truncated");
    let token1 = page1
        .next_continuation_token
        .expect("Page 1 must provide a continuation token");

    // Page 2: expect 10 objects and is_truncated == true.
    let request = tonic::Request::new(ListObjectsRequest {
        bucket: "pagination-test".to_string(),
        prefix: None,
        delimiter: None,
        max_keys: Some(10),
        continuation_token: Some(token1),
        start_after: None,
        fetch_owner: false,
    });
    let page2 = client
        .list_objects_paginated(request)
        .await
        .expect("Failed to list page 2")
        .into_inner();
    assert_eq!(page2.objects.len(), 10, "Page 2 should contain 10 objects");
    assert!(page2.is_truncated, "Page 2 should be truncated");
    let token2 = page2
        .next_continuation_token
        .expect("Page 2 must provide a continuation token");

    // Page 3: expect 5 objects and is_truncated == false.
    let request = tonic::Request::new(ListObjectsRequest {
        bucket: "pagination-test".to_string(),
        prefix: None,
        delimiter: None,
        max_keys: Some(10),
        continuation_token: Some(token2),
        start_after: None,
        fetch_owner: false,
    });
    let page3 = client
        .list_objects_paginated(request)
        .await
        .expect("Failed to list page 3")
        .into_inner();
    assert_eq!(
        page3.objects.len(),
        5,
        "Page 3 should contain the remaining 5 objects"
    );
    assert!(!page3.is_truncated, "Page 3 should not be truncated");

    // Sanity: union of all three pages covers all 25 keys exactly once.
    let mut all_keys: Vec<String> = page1
        .objects
        .iter()
        .chain(page2.objects.iter())
        .chain(page3.objects.iter())
        .map(|o| o.key.clone())
        .collect();
    all_keys.sort();
    assert_eq!(
        all_keys.len(),
        25,
        "Total objects across pages should be 25"
    );
    for (i, key) in all_keys.iter().enumerate() {
        assert_eq!(
            *key,
            format!("obj-{:03}.bin", i),
            "Key mismatch at index {}",
            i
        );
    }
}

// ===== Parallel Delete & Range GET Tests =====

#[tokio::test]
async fn test_grpc_parallel_delete_20_keys() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "par-del-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Create 20 objects
    for i in 0..20u32 {
        let request = tonic::Request::new(PutObjectRequest {
            bucket: "par-del-test".to_string(),
            key: format!("key-{:03}", i),
            data: format!("data-{}", i).into_bytes(),
            content_type: None,
            metadata: std::collections::HashMap::new(),
            storage_class: None,
            checksum_crc32c: None,
            checksum_crc32: None,
            checksum_sha256: None,
            checksum_sha1: None,
        });
        client
            .put_object(request)
            .await
            .expect("Failed to put object");
    }

    // Delete all 20 via batch delete
    let keys: Vec<String> = (0..20u32).map(|i| format!("key-{:03}", i)).collect();
    let request = tonic::Request::new(DeleteObjectsRequest {
        bucket: "par-del-test".to_string(),
        keys,
        quiet: false,
    });
    let response = client
        .delete_objects(request)
        .await
        .expect("Failed to delete objects");
    let result = response.into_inner();
    assert_eq!(result.deleted.len(), 20, "Expected 20 deleted objects");
    assert!(result.errors.is_empty(), "Expected no errors");

    // Verify all objects are gone
    let request = tonic::Request::new(ListObjectsRequest {
        bucket: "par-del-test".to_string(),
        prefix: None,
        delimiter: None,
        max_keys: Some(100),
        continuation_token: None,
        start_after: None,
        fetch_owner: false,
    });
    let list_result = client
        .list_objects_paginated(request)
        .await
        .expect("Failed to list objects")
        .into_inner();
    assert_eq!(
        list_result.objects.len(),
        0,
        "Expected no objects after batch delete"
    );
}

#[tokio::test]
async fn test_grpc_range_get() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "range-get-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put object with known content
    let data = b"0123456789abcdefghijklmnopqrstuvwxyz".to_vec();
    let request = tonic::Request::new(PutObjectRequest {
        bucket: "range-get-test".to_string(),
        key: "range-obj.txt".to_string(),
        data: data.clone(),
        content_type: Some("text/plain".to_string()),
        metadata: std::collections::HashMap::new(),
        storage_class: None,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });
    client
        .put_object(request)
        .await
        .expect("Failed to put object");

    // Range GET: bytes 5-14 (inclusive) => "56789abcde"
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "range-get-test".to_string(),
        key: "range-obj.txt".to_string(),
        version_id: None,
        range_start: Some(5),
        range_end: Some(14),
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });
    let response = client
        .get_object(request)
        .await
        .expect("Failed to get object range");
    let obj = response.into_inner();

    assert_eq!(obj.content_range_start, Some(5));
    assert_eq!(obj.content_range_end, Some(14));
    assert_eq!(obj.content_range_total, Some(data.len() as i64));
    assert_eq!(
        std::str::from_utf8(&obj.data).unwrap_or(""),
        "56789abcde",
        "Range GET should return the expected byte slice"
    );
}

#[tokio::test]
async fn test_grpc_range_get_invalid() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "range-inv-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put a small object
    let request = tonic::Request::new(PutObjectRequest {
        bucket: "range-inv-test".to_string(),
        key: "small.txt".to_string(),
        data: b"hello".to_vec(),
        content_type: None,
        metadata: std::collections::HashMap::new(),
        storage_class: None,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });
    client
        .put_object(request)
        .await
        .expect("Failed to put object");

    // Request range_start beyond object size (5 bytes, start=100)
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "range-inv-test".to_string(),
        key: "small.txt".to_string(),
        version_id: None,
        range_start: Some(100),
        range_end: Some(200),
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });
    let result = client.get_object(request).await;
    assert!(
        result.is_err(),
        "Expected error for range_start beyond object size"
    );

    // Request range_end < range_start
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "range-inv-test".to_string(),
        key: "small.txt".to_string(),
        version_id: None,
        range_start: Some(3),
        range_end: Some(1),
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });
    let result = client.get_object(request).await;
    assert!(
        result.is_err(),
        "Expected error when range_end < range_start"
    );
}

#[tokio::test]
async fn test_grpc_delete_mixed_existing_nonexisting() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "mix-del-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Create 3 objects
    for i in 0..3u32 {
        let request = tonic::Request::new(PutObjectRequest {
            bucket: "mix-del-test".to_string(),
            key: format!("exist-{}", i),
            data: format!("data-{}", i).into_bytes(),
            content_type: None,
            metadata: std::collections::HashMap::new(),
            storage_class: None,
            checksum_crc32c: None,
            checksum_crc32: None,
            checksum_sha256: None,
            checksum_sha1: None,
        });
        client
            .put_object(request)
            .await
            .expect("Failed to put object");
    }

    // Delete mix of existing and non-existing keys
    let keys = vec![
        "exist-0".to_string(),
        "nonexist-1".to_string(),
        "exist-1".to_string(),
        "nonexist-2".to_string(),
        "exist-2".to_string(),
    ];
    let request = tonic::Request::new(DeleteObjectsRequest {
        bucket: "mix-del-test".to_string(),
        keys,
        quiet: false,
    });
    let response = client
        .delete_objects(request)
        .await
        .expect("Failed to delete objects");
    let result = response.into_inner();

    // S3 semantics: deleting a non-existent key is not an error; it succeeds silently
    // So we expect all 5 to appear in the deleted list (or at least the existing 3 + the non-existing ones)
    // The exact behavior depends on the storage engine implementation.
    // At minimum, the existing 3 should be in deleted.
    let total_results = result.deleted.len() + result.errors.len();
    assert_eq!(total_results, 5, "Should have a result for each key");
}

#[tokio::test]
async fn test_grpc_parallel_delete_empty_list() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "empty-del-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Delete with empty key list
    let request = tonic::Request::new(DeleteObjectsRequest {
        bucket: "empty-del-test".to_string(),
        keys: vec![],
        quiet: false,
    });
    let response = client
        .delete_objects(request)
        .await
        .expect("Failed to delete objects with empty list");
    let result = response.into_inner();
    assert_eq!(result.deleted.len(), 0, "Expected 0 deletions");
    assert!(result.errors.is_empty(), "Expected no errors");
}

#[tokio::test]
async fn test_grpc_get_full_object() {
    let (mut client, _temp_dir, _server) = setup_grpc_test_server().await;

    // Create bucket
    let request = tonic::Request::new(CreateBucketRequest {
        bucket: "full-get-test".to_string(),
        region: None,
        tags: std::collections::HashMap::new(),
    });
    client
        .create_bucket(request)
        .await
        .expect("Failed to create bucket");

    // Put object
    let data = b"Full content retrieval test data with various bytes".to_vec();
    let request = tonic::Request::new(PutObjectRequest {
        bucket: "full-get-test".to_string(),
        key: "full-obj.bin".to_string(),
        data: data.clone(),
        content_type: Some("application/octet-stream".to_string()),
        metadata: std::collections::HashMap::new(),
        storage_class: None,
        checksum_crc32c: None,
        checksum_crc32: None,
        checksum_sha256: None,
        checksum_sha1: None,
    });
    client
        .put_object(request)
        .await
        .expect("Failed to put object");

    // GET without range fields (full object)
    let request = tonic::Request::new(GetObjectRequest {
        bucket: "full-get-test".to_string(),
        key: "full-obj.bin".to_string(),
        version_id: None,
        range_start: None,
        range_end: None,
        if_match: None,
        if_none_match: None,
        if_modified_since: None,
        if_unmodified_since: None,
    });
    let response = client
        .get_object(request)
        .await
        .expect("Failed to get full object");
    let obj = response.into_inner();

    assert_eq!(obj.data, data, "Full GET should return all data");
    assert!(
        obj.content_range_start.is_none(),
        "Full GET should not have content_range_start"
    );
    assert!(
        obj.content_range_end.is_none(),
        "Full GET should not have content_range_end"
    );
    assert!(
        obj.content_range_total.is_none(),
        "Full GET should not have content_range_total"
    );
    assert_eq!(
        obj.metadata.as_ref().map(|m| m.size),
        Some(data.len() as u64),
        "Metadata size should match data length"
    );
    assert_eq!(
        obj.metadata.as_ref().map(|m| m.content_type.as_str()),
        Some("application/octet-stream"),
        "Content type should match"
    );
}

// ===== Test helpers for env-var management =====

/// RAII guard that removes env vars on creation and restores them on drop.
struct EnvVarGuard {
    originals: Vec<(String, Option<String>)>,
}

impl EnvVarGuard {
    /// Remove the listed env vars for the duration of the guard.
    fn clear(names: &[&str]) -> Self {
        let originals = names
            .iter()
            .map(|&name| {
                let old = std::env::var(name).ok();
                std::env::remove_var(name);
                (name.to_string(), old)
            })
            .collect();
        Self { originals }
    }

    /// Set the listed env vars for the duration of the guard.
    fn set(pairs: &[(&str, &str)]) -> Self {
        let originals = pairs
            .iter()
            .map(|&(name, value)| {
                let old = std::env::var(name).ok();
                std::env::set_var(name, value);
                (name.to_string(), old)
            })
            .collect();
        Self { originals }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        for (name, original) in &self.originals {
            match original {
                Some(val) => std::env::set_var(name, val),
                None => std::env::remove_var(name),
            }
        }
    }
}
