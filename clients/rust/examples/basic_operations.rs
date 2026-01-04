// Basic operations example for rs3gw Rust gRPC client.
// This example demonstrates CRUD operations for buckets and objects.

use rs3gw::grpc::bucket_service_client::BucketServiceClient;
use rs3gw::grpc::object_service_client::ObjectServiceClient;
use rs3gw::grpc::{
    CreateBucketRequest, DeleteBucketRequest, DeleteObjectRequest, GetObjectRequest,
    GetObjectTaggingRequest, HeadBucketRequest, HeadObjectRequest, ListBucketsRequest,
    ListObjectsRequest, PutObjectRequest, PutObjectTaggingRequest,
};
use std::collections::HashMap;
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("rs3gw Rust gRPC Client - Basic Operations Example\n");
    println!("Make sure rs3gw server is running on localhost:9001\n");

    // Connect to rs3gw gRPC server
    let mut bucket_client = BucketServiceClient::connect("http://localhost:9001").await?;
    let mut object_client = ObjectServiceClient::connect("http://localhost:9001").await?;

    // Bucket operations
    println!("=== Bucket Operations ===\n");

    // List buckets
    println!("Listing buckets...");
    let response = bucket_client
        .list_buckets(Request::new(ListBucketsRequest {}))
        .await?;
    let buckets = response.into_inner().buckets;
    println!("Found {} buckets:", buckets.len());
    for bucket in buckets {
        println!("  - {} (created: {})", bucket.name, bucket.creation_date);
    }

    // Create bucket
    let bucket_name = "test-bucket-rust";
    println!("\nCreating bucket: {}", bucket_name);
    match bucket_client
        .create_bucket(Request::new(CreateBucketRequest {
            bucket: bucket_name.to_string(),
            region: "us-east-1".to_string(),
        }))
        .await
    {
        Ok(_) => println!("Bucket '{}' created successfully", bucket_name),
        Err(e) if e.code() == tonic::Code::AlreadyExists => {
            println!("Bucket already exists")
        }
        Err(e) => return Err(e.into()),
    }

    // Head bucket
    println!("\nChecking if bucket exists: {}", bucket_name);
    let response = bucket_client
        .head_bucket(Request::new(HeadBucketRequest {
            bucket: bucket_name.to_string(),
        }))
        .await?;
    println!("Bucket exists: {}", response.into_inner().exists);

    // Object operations
    println!("\n=== Object Operations ===\n");

    // Put object
    let object_key = "data/hello.txt";
    let object_data = b"Hello from Rust gRPC client!".to_vec();
    println!("Uploading object: {}", object_key);

    let mut metadata = HashMap::new();
    metadata.insert("author".to_string(), "rust-client".to_string());
    metadata.insert("version".to_string(), "1.0".to_string());

    let response = object_client
        .put_object(Request::new(PutObjectRequest {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            data: object_data.clone(),
            content_type: "text/plain".to_string(),
            metadata,
        }))
        .await?;
    let put_resp = response.into_inner();
    println!("Object uploaded successfully");
    println!("  ETag: {}", put_resp.etag);
    println!("  Size: {} bytes", object_data.len());

    // Get object
    println!("\nDownloading object: {}", object_key);
    let response = object_client
        .get_object(Request::new(GetObjectRequest {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            range: None,
        }))
        .await?;
    let get_resp = response.into_inner();
    println!("Object downloaded successfully");
    println!("  Content-Type: {}", get_resp.content_type);
    println!("  Size: {} bytes", get_resp.size);
    println!("  Data: {}", String::from_utf8_lossy(&get_resp.data));
    println!("  Metadata: {:?}", get_resp.metadata);

    // Head object
    println!("\nGetting object metadata: {}", object_key);
    let response = object_client
        .head_object(Request::new(HeadObjectRequest {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
        }))
        .await?;
    let head_resp = response.into_inner();
    println!("Object metadata:");
    println!("  Content-Type: {}", head_resp.content_type);
    println!("  Size: {} bytes", head_resp.size);
    println!("  ETag: {}", head_resp.etag);
    println!("  Last-Modified: {}", head_resp.last_modified);

    // List objects
    println!("\nListing objects in bucket: {}", bucket_name);
    let response = object_client
        .list_objects(Request::new(ListObjectsRequest {
            bucket: bucket_name.to_string(),
            prefix: Some("data/".to_string()),
            delimiter: None,
            max_keys: 1000,
            continuation_token: None,
        }))
        .await?;
    let list_resp = response.into_inner();
    println!("Found {} objects:", list_resp.contents.len());
    for obj in list_resp.contents {
        println!("  - {} ({} bytes, ETag: {})", obj.key, obj.size, obj.etag);
    }

    // Tagging operations
    println!("\n=== Tagging Operations ===\n");

    // Put object tags
    println!("Adding tags to object: {}", object_key);
    let mut tags = HashMap::new();
    tags.insert("environment".to_string(), "development".to_string());
    tags.insert("project".to_string(), "rs3gw-test".to_string());

    object_client
        .put_object_tagging(Request::new(PutObjectTaggingRequest {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            tags: tags.clone(),
        }))
        .await?;
    println!("Tags added successfully");

    // Get object tags
    println!("\nGetting tags for object: {}", object_key);
    let response = object_client
        .get_object_tagging(Request::new(GetObjectTaggingRequest {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
        }))
        .await?;
    println!("Object tags: {:?}", response.into_inner().tags);

    // Cleanup
    println!("\n=== Cleanup ===\n");

    // Delete object
    println!("Deleting object: {}", object_key);
    object_client
        .delete_object(Request::new(DeleteObjectRequest {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
        }))
        .await?;
    println!("Object deleted successfully");

    // Delete bucket
    println!("\nDeleting bucket: {}", bucket_name);
    bucket_client
        .delete_bucket(Request::new(DeleteBucketRequest {
            bucket: bucket_name.to_string(),
        }))
        .await?;
    println!("Bucket deleted successfully");

    println!("\n=== Example completed successfully ===");
    Ok(())
}
