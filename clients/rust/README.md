# rs3gw Rust gRPC Client

Rust client library for rs3gw S3-compatible storage gateway using gRPC.

## Overview

The Rust gRPC client is automatically generated from the proto files during the build process using `tonic-build`. The generated client code provides type-safe, async/await-compatible interfaces for all S3 operations.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
rs3gw = { git = "https://github.com/cool-japan/rs3gw", features = ["grpc-client"] }
tonic = "0.12"
tokio = { version = "1.48", features = ["full"] }
prost = "0.13"
```

## Quick Start

```rust
use rs3gw::grpc::s3_service_client::S3ServiceClient;
use rs3gw::grpc::{ListBucketsRequest, CreateBucketRequest, PutObjectRequest};
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to rs3gw gRPC server
    let mut client = S3ServiceClient::connect("http://localhost:9001").await?;

    // List buckets
    let request = Request::new(ListBucketsRequest {});
    let response = client.list_buckets(request).await?;

    println!("Buckets: {:?}", response.into_inner().buckets);

    Ok(())
}
```

## Features

### Bucket Operations

```rust
// Create bucket
let request = CreateBucketRequest {
    bucket: "my-bucket".to_string(),
    region: "us-east-1".to_string(),
};
client.create_bucket(request).await?;

// Delete bucket
client.delete_bucket(DeleteBucketRequest {
    bucket: "my-bucket".to_string(),
}).await?;

// Head bucket (check existence)
client.head_bucket(HeadBucketRequest {
    bucket: "my-bucket".to_string(),
}).await?;
```

### Object Operations

```rust
use bytes::Bytes;

// Put object
let request = PutObjectRequest {
    bucket: "my-bucket".to_string(),
    key: "data/file.txt".to_string(),
    data: Bytes::from("Hello, world!"),
    content_type: "text/plain".to_string(),
    metadata: std::collections::HashMap::new(),
};
client.put_object(request).await?;

// Get object
let response = client.get_object(GetObjectRequest {
    bucket: "my-bucket".to_string(),
    key: "data/file.txt".to_string(),
    range: None,
}).await?;
let data = response.into_inner().data;

// Delete object
client.delete_object(DeleteObjectRequest {
    bucket: "my-bucket".to_string(),
    key: "data/file.txt".to_string(),
}).await?;

// List objects
let response = client.list_objects(ListObjectsRequest {
    bucket: "my-bucket".to_string(),
    prefix: Some("data/".to_string()),
    delimiter: None,
    max_keys: 1000,
    continuation_token: None,
}).await?;
```

### Streaming Operations

```rust
use tokio_stream::StreamExt;
use futures::stream;

// Stream object upload (for large files)
let chunks = vec![
    Bytes::from("chunk1"),
    Bytes::from("chunk2"),
    Bytes::from("chunk3"),
];
let stream = stream::iter(chunks.into_iter().map(|data| PutObjectRequest {
    bucket: "my-bucket".to_string(),
    key: "large-file.dat".to_string(),
    data,
    content_type: "application/octet-stream".to_string(),
    metadata: std::collections::HashMap::new(),
}));

let response = client.put_object_stream(Request::new(stream)).await?;

// Stream object download
let mut stream = client.get_object_stream(GetObjectRequest {
    bucket: "my-bucket".to_string(),
    key: "large-file.dat".to_string(),
    range: None,
}).await?.into_inner();

while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    // Process chunk.data
}
```

### Multipart Upload

```rust
// Create multipart upload
let create_response = client.create_multipart_upload(CreateMultipartUploadRequest {
    bucket: "my-bucket".to_string(),
    key: "huge-file.dat".to_string(),
    content_type: "application/octet-stream".to_string(),
    metadata: std::collections::HashMap::new(),
}).await?;
let upload_id = create_response.into_inner().upload_id;

// Upload parts
let mut parts = vec![];
for (i, chunk) in chunks.iter().enumerate() {
    let response = client.upload_part(UploadPartRequest {
        bucket: "my-bucket".to_string(),
        key: "huge-file.dat".to_string(),
        upload_id: upload_id.clone(),
        part_number: (i + 1) as i32,
        data: chunk.clone(),
    }).await?;

    parts.push(CompletedPart {
        part_number: (i + 1) as i32,
        etag: response.into_inner().etag,
    });
}

// Complete multipart upload
client.complete_multipart_upload(CompleteMultipartUploadRequest {
    bucket: "my-bucket".to_string(),
    key: "huge-file.dat".to_string(),
    upload_id,
    parts,
}).await?;
```

### Tagging Operations

```rust
use std::collections::HashMap;

// Put bucket tags
let mut tags = HashMap::new();
tags.insert("Environment".to_string(), "Production".to_string());
tags.insert("Owner".to_string(), "Engineering".to_string());

client.put_bucket_tagging(PutBucketTaggingRequest {
    bucket: "my-bucket".to_string(),
    tags,
}).await?;

// Get bucket tags
let response = client.get_bucket_tagging(GetBucketTaggingRequest {
    bucket: "my-bucket".to_string(),
}).await?;
let tags = response.into_inner().tags;

// Put object tags
client.put_object_tagging(PutObjectTaggingRequest {
    bucket: "my-bucket".to_string(),
    key: "data/file.txt".to_string(),
    tags: tags.clone(),
}).await?;
```

### Policy Operations

```rust
// Put bucket policy (JSON)
let policy = r#"{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": "*",
    "Action": "s3:GetObject",
    "Resource": "arn:aws:s3:::my-bucket/*"
  }]
}"#;

client.put_bucket_policy(PutBucketPolicyRequest {
    bucket: "my-bucket".to_string(),
    policy: policy.to_string(),
}).await?;

// Get bucket policy
let response = client.get_bucket_policy(GetBucketPolicyRequest {
    bucket: "my-bucket".to_string(),
}).await?;
```

## Error Handling

```rust
use tonic::Status;

match client.get_object(request).await {
    Ok(response) => {
        let object = response.into_inner();
        // Process object
    }
    Err(status) => match status.code() {
        tonic::Code::NotFound => {
            println!("Object not found");
        }
        tonic::Code::PermissionDenied => {
            println!("Access denied");
        }
        _ => {
            println!("Error: {:?}", status);
        }
    }
}
```

## Connection Management

```rust
// Configure connection with timeouts and retries
use tonic::transport::{Channel, Endpoint};
use std::time::Duration;

let endpoint = Endpoint::from_static("http://localhost:9001")
    .timeout(Duration::from_secs(30))
    .connect_timeout(Duration::from_secs(5))
    .tcp_keepalive(Some(Duration::from_secs(60)));

let channel = endpoint.connect().await?;
let client = S3ServiceClient::new(channel);
```

## TLS/HTTPS Support

```rust
use tonic::transport::{Certificate, ClientTlsConfig, Identity};

// Load CA certificate
let ca_cert = std::fs::read_to_string("ca.pem")?;
let ca = Certificate::from_pem(ca_cert);

// Configure TLS
let tls_config = ClientTlsConfig::new()
    .ca_certificate(ca)
    .domain_name("rs3gw.example.com");

let endpoint = Endpoint::from_static("https://rs3gw.example.com:9001")
    .tls_config(tls_config)?;

let channel = endpoint.connect().await?;
let client = S3ServiceClient::new(channel);
```

## Interceptors (Middleware)

```rust
use tonic::metadata::MetadataValue;
use tonic::service::Interceptor;
use tonic::Request;

// Add authentication token to all requests
#[derive(Clone)]
struct AuthInterceptor {
    token: String,
}

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let token = MetadataValue::try_from(&self.token)
            .map_err(|_| Status::unauthenticated("Invalid token"))?;
        request.metadata_mut().insert("authorization", token);
        Ok(request)
    }
}

// Use interceptor with client
let interceptor = AuthInterceptor {
    token: "Bearer my-secret-token".to_string(),
};
let client = S3ServiceClient::with_interceptor(channel, interceptor);
```

## Performance Tips

1. **Connection Pooling**: Reuse the same `Channel` across multiple requests
2. **Streaming**: Use streaming operations for large files to reduce memory usage
3. **Compression**: Enable gRPC compression for bandwidth-constrained networks
4. **Concurrency**: Use `tokio::spawn` for parallel operations
5. **Batch Operations**: Group multiple operations when possible

## Examples

See the `examples/` directory for complete working examples:

- `examples/basic_operations.rs` - CRUD operations
- `examples/streaming.rs` - Streaming uploads and downloads
- `examples/multipart.rs` - Multipart upload workflow
- `examples/async_parallel.rs` - Concurrent operations

## Requirements

- Rust 1.75+
- tokio runtime with full features
- tonic 0.12+
- prost 0.13+

## License

Apache-2.0
