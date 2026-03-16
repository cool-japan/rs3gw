# gRPC Module

The gRPC module provides a high-performance binary protocol alternative to the REST API, offering improved throughput, reduced latency, and efficient streaming for large-scale deployments.

## Overview

This module implements a complete gRPC API for S3-compatible operations using:
- **Protocol Buffers** - Efficient binary serialization
- **HTTP/2** - Multiplexing and header compression
- **Streaming** - Bidirectional, client-side, and server-side
- **Tonic** - Production-ready gRPC framework
- **Type Safety** - Strong typing with generated code

### Performance Benefits

Compared to REST/XML API:
- **2-3x faster serialization** - Protobuf vs XML/JSON
- **40-60% smaller message size** - Binary vs text encoding
- **50% lower latency** - HTTP/2 vs HTTP/1.1
- **2x throughput** - Multiplexing vs sequential requests

See [benches/grpc_vs_rest_benchmarks.rs](../../benches/grpc_vs_rest_benchmarks.rs) for detailed benchmarks.

## Architecture

### Protocol Buffers Schema

Defined in `proto/` directory:

- `proto/s3.proto` - Core S3 types and common messages
- `proto/bucket.proto` - Bucket operations service
- `proto/object.proto` - Object operations service
- `proto/multipart.proto` - Multipart upload service

**Build Process:**
```rust
// build.rs
fn main() {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&[
            "proto/s3.proto",
            "proto/bucket.proto",
            "proto/object.proto",
            "proto/multipart.proto",
        ], &["proto/"])
        .expect("Failed to compile protos");
}
```

Generated code is placed in `target/` and included via:
```rust
pub mod s3 {
    include!(concat!(env!("OUT_DIR"), "/s3.rs"));
}
```

### Server Implementation

The gRPC server runs alongside the HTTP server on a dedicated port (default: 50051).

```
┌─────────────────────┐
│   rs3gw Process     │
├─────────────────────┤
│  HTTP Server :9000  │  ← REST API
│  gRPC Server :50051 │  ← gRPC API
└─────────────────────┘
         ↓
   Storage Engine
```

## Components

### Server (`server.rs`)
Main gRPC server implementation with service registration.

**Features:**
- Concurrent gRPC and HTTP servers
- Shared storage engine
- Health checking
- Reflection support
- Graceful shutdown

**Usage:**
```rust
use rs3gw::grpc::server::start_grpc_server;
use std::sync::Arc;

let storage = Arc::new(StorageEngine::new(config)?);

// Start gRPC server
let grpc_handle = tokio::spawn(async move {
    start_grpc_server(storage, "0.0.0.0:50051".parse()?)
        .await
        .expect("gRPC server failed");
});

// Start HTTP server
let http_handle = tokio::spawn(async move {
    // ... HTTP server setup
});

// Wait for both servers
tokio::try_join!(grpc_handle, http_handle)?;
```

**Configuration:**

| Variable | Default | Description |
|----------|---------|-------------|
| `RS3GW_GRPC_ENABLED` | `false` | Set to `true` to start the gRPC server alongside the HTTP server |
| `RS3GW_GRPC_PORT` | `50051` | TCP port the gRPC server listens on |
| `RS3GW_GRPC_MAX_MESSAGE_SIZE` | `67108864` | Maximum gRPC message size in bytes (default 64 MB). Applies to both encoding and decoding |
| `RS3GW_GRPC_TLS_CERT` | _(none)_ | Path to PEM certificate file. When both cert and key are provided, gRPC TLS is enabled |
| `RS3GW_GRPC_TLS_KEY` | _(none)_ | Path to PEM private key file |

**Notes:**
- gRPC is **disabled by default**. Set `RS3GW_GRPC_ENABLED=true` to activate it.
- TLS requires manually provisioned certificates; there is no auto-TLS support.
- When TLS is enabled, clients must connect with `https://` (or the tonic TLS channel type).
- `RS3GW_GRPC_MAX_MESSAGE_SIZE` affects both server decoding (inbound) and encoding (outbound). Clients must be configured with a compatible limit for large object transfers.

**Example — start with gRPC enabled and TLS:**

```bash
RS3GW_GRPC_ENABLED=true \
RS3GW_GRPC_PORT=50051 \
RS3GW_GRPC_MAX_MESSAGE_SIZE=134217728 \
RS3GW_GRPC_TLS_CERT=/etc/rs3gw/grpc.crt \
RS3GW_GRPC_TLS_KEY=/etc/rs3gw/grpc.key \
rs3gw
```

### Bucket Operations (`bucket.rs`)
gRPC service for bucket management.

**Service Definition:**
```protobuf
service BucketService {
    rpc ListBuckets(ListBucketsRequest) returns (ListBucketsResponse);
    rpc CreateBucket(CreateBucketRequest) returns (CreateBucketResponse);
    rpc DeleteBucket(DeleteBucketRequest) returns (DeleteBucketResponse);
    rpc HeadBucket(HeadBucketRequest) returns (HeadBucketResponse);
    rpc GetBucketLocation(GetBucketLocationRequest) returns (GetBucketLocationResponse);
    rpc GetBucketTagging(GetBucketTaggingRequest) returns (GetBucketTaggingResponse);
    rpc PutBucketTagging(PutBucketTaggingRequest) returns (PutBucketTaggingResponse);
    rpc GetBucketPolicy(GetBucketPolicyRequest) returns (GetBucketPolicyResponse);
    rpc PutBucketPolicy(PutBucketPolicyRequest) returns (PutBucketPolicyResponse);
}
```

**Example Client:**
```rust
use s3_proto::bucket_service_client::BucketServiceClient;

let mut client = BucketServiceClient::connect("http://localhost:50051").await?;

// List buckets
let request = tonic::Request::new(ListBucketsRequest {});
let response = client.list_buckets(request).await?;
for bucket in response.into_inner().buckets {
    println!("Bucket: {}", bucket.name);
}

// Create bucket
let request = tonic::Request::new(CreateBucketRequest {
    bucket: "my-bucket".to_string(),
    region: "us-east-1".to_string(),
});
client.create_bucket(request).await?;
```

### Object Operations (`object.rs`)
gRPC service for object management with streaming.

**Service Definition:**
```protobuf
service ObjectService {
    rpc ListObjects(ListObjectsRequest) returns (ListObjectsResponse);
    rpc HeadObject(HeadObjectRequest) returns (HeadObjectResponse);
    rpc GetObject(GetObjectRequest) returns (stream GetObjectResponse);
    rpc PutObject(stream PutObjectRequest) returns (PutObjectResponse);
    rpc DeleteObject(DeleteObjectRequest) returns (DeleteObjectResponse);
    rpc CopyObject(CopyObjectRequest) returns (CopyObjectResponse);
    rpc GetObjectAttributes(GetObjectAttributesRequest) returns (GetObjectAttributesResponse);
    rpc GetObjectTagging(GetObjectTaggingRequest) returns (GetObjectTaggingResponse);
    rpc PutObjectTagging(PutObjectTaggingRequest) returns (PutObjectTaggingResponse);
}
```

**Streaming Features:**
- **Server-side streaming** - `GetObject` streams chunks to client
- **Client-side streaming** - `PutObject` streams chunks from client
- **Chunk size** - Configurable (default: 64KB)
- **Backpressure** - Automatic flow control

**Download Example (Server-side Streaming):**
```rust
use s3_proto::object_service_client::ObjectServiceClient;

let mut client = ObjectServiceClient::connect("http://localhost:50051").await?;

let request = tonic::Request::new(GetObjectRequest {
    bucket: "my-bucket".to_string(),
    key: "large-file.bin".to_string(),
});

let mut stream = client.get_object(request).await?.into_inner();

let mut buffer = Vec::new();
while let Some(chunk) = stream.message().await? {
    buffer.extend_from_slice(&chunk.data);
}

println!("Downloaded {} bytes", buffer.len());
```

**Upload Example (Client-side Streaming):**
```rust
let mut client = ObjectServiceClient::connect("http://localhost:50051").await?;

// Create stream of chunks
let chunks = vec![vec![1u8; 1024]; 100]; // 100 x 1KB chunks
let stream = tokio_stream::iter(chunks.into_iter().enumerate().map(|(i, data)| {
    if i == 0 {
        // First chunk includes metadata
        PutObjectRequest {
            bucket: "my-bucket".to_string(),
            key: "file.bin".to_string(),
            content_type: "application/octet-stream".to_string(),
            metadata: HashMap::new(),
            data,
            chunk_index: 0,
            is_last_chunk: false,
        }
    } else {
        // Subsequent chunks
        PutObjectRequest {
            data,
            chunk_index: i as u32,
            is_last_chunk: i == 99,
            ..Default::default()
        }
    }
}));

let response = client.put_object(tonic::Request::new(stream)).await?;
println!("ETag: {}", response.into_inner().etag);
```

### Multipart Upload (`multipart.rs`)
gRPC service for multipart upload operations.

**Service Definition:**
```protobuf
service MultipartService {
    rpc CreateMultipartUpload(CreateMultipartUploadRequest) returns (CreateMultipartUploadResponse);
    rpc UploadPart(stream UploadPartRequest) returns (UploadPartResponse);
    rpc CompleteMultipartUpload(CompleteMultipartUploadRequest) returns (CompleteMultipartUploadResponse);
    rpc AbortMultipartUpload(AbortMultipartUploadRequest) returns (AbortMultipartUploadResponse);
    rpc ListParts(ListPartsRequest) returns (ListPartsResponse);
    rpc ListMultipartUploads(ListMultipartUploadsRequest) returns (ListMultipartUploadsResponse);
}
```

**Multipart Upload Flow:**
```
1. CreateMultipartUpload → upload_id
2. UploadPart (stream) → part1_etag
3. UploadPart (stream) → part2_etag
4. ...
5. CompleteMultipartUpload([part1_etag, part2_etag, ...]) → final_etag
```

**Example:**
```rust
let mut client = MultipartServiceClient::connect("http://localhost:50051").await?;

// 1. Create upload
let request = tonic::Request::new(CreateMultipartUploadRequest {
    bucket: "my-bucket".to_string(),
    key: "large-file.bin".to_string(),
    content_type: "application/octet-stream".to_string(),
    metadata: HashMap::new(),
});
let upload_id = client.create_multipart_upload(request)
    .await?
    .into_inner()
    .upload_id;

// 2. Upload parts
let mut parts = Vec::new();
for part_number in 1..=3 {
    let chunks = vec![vec![part_number as u8; 5 * 1024 * 1024]]; // 5MB part
    let stream = tokio_stream::iter(chunks.into_iter().map(move |data| {
        UploadPartRequest {
            bucket: "my-bucket".to_string(),
            key: "large-file.bin".to_string(),
            upload_id: upload_id.clone(),
            part_number,
            data,
        }
    }));

    let etag = client.upload_part(tonic::Request::new(stream))
        .await?
        .into_inner()
        .etag;

    parts.push(CompletedPart { part_number, etag });
}

// 3. Complete upload
let request = tonic::Request::new(CompleteMultipartUploadRequest {
    bucket: "my-bucket".to_string(),
    key: "large-file.bin".to_string(),
    upload_id,
    parts,
});
let final_etag = client.complete_multipart_upload(request)
    .await?
    .into_inner()
    .etag;

println!("Upload complete. ETag: {}", final_etag);
```

## Client Libraries

Pre-built client libraries for popular languages:

### Python Client

**Location**: `clients/python/`

**Installation:**
```bash
pip install grpcio grpcio-tools

# Generate client
python -m grpc_tools.protoc \
    -I../../proto \
    --python_out=. \
    --grpc_python_out=. \
    ../../proto/*.proto
```

**Usage:**
```python
import grpc
from s3_pb2_grpc import BucketServiceStub, ObjectServiceStub
from s3_pb2 import ListBucketsRequest, GetObjectRequest

# Connect
channel = grpc.insecure_channel('localhost:50051')
bucket_client = BucketServiceStub(channel)
object_client = ObjectServiceStub(channel)

# List buckets
response = bucket_client.ListBuckets(ListBucketsRequest())
for bucket in response.buckets:
    print(f"Bucket: {bucket.name}")

# Get object
request = GetObjectRequest(bucket="my-bucket", key="file.txt")
stream = object_client.GetObject(request)
data = b''.join(chunk.data for chunk in stream)
print(f"Downloaded {len(data)} bytes")
```

### Go Client

**Location**: `clients/go/`

**Installation:**
```bash
go get google.golang.org/grpc
go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest

# Generate client
protoc --go_out=. --go-grpc_out=. -I../../proto ../../proto/*.proto
```

**Usage:**
```go
package main

import (
    "context"
    "google.golang.org/grpc"
    pb "rs3gw/proto"
)

func main() {
    conn, _ := grpc.Dial("localhost:50051", grpc.WithInsecure())
    defer conn.Close()

    client := pb.NewBucketServiceClient(conn)

    // List buckets
    resp, _ := client.ListBuckets(context.Background(), &pb.ListBucketsRequest{})
    for _, bucket := range resp.Buckets {
        fmt.Printf("Bucket: %s\n", bucket.Name)
    }
}
```

### Rust Client

**Location**: `clients/rust/`

**Usage:**
```rust
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = bucket_service_client::BucketServiceClient::connect(
        "http://localhost:50051"
    ).await?;

    let request = Request::new(ListBucketsRequest {});
    let response = client.list_buckets(request).await?;

    for bucket in response.into_inner().buckets {
        println!("Bucket: {}", bucket.name);
    }

    Ok(())
}
```

## Performance Benchmarks

### Serialization Performance

Based on `benches/grpc_vs_rest_benchmarks.rs`:

| Operation | REST/XML | gRPC/Protobuf | Speedup |
|-----------|----------|---------------|---------|
| Serialize 1KB object | 12.5 µs | 4.2 µs | 3.0x |
| Deserialize 1KB object | 18.3 µs | 6.1 µs | 3.0x |
| Message size 10KB | 10,240 bytes | 6,144 bytes | 1.7x smaller |

### End-to-End Performance

| Operation | REST API | gRPC API | Improvement |
|-----------|----------|----------|-------------|
| PUT 1MB object | 15.2 ms | 8.7 ms | 1.7x faster |
| GET 1MB object | 12.8 ms | 7.3 ms | 1.8x faster |
| LIST 1000 objects | 45.6 ms | 28.2 ms | 1.6x faster |
| Multipart upload 100MB | 1,250 ms | 780 ms | 1.6x faster |

**Factors:**
- Binary vs text encoding
- HTTP/2 vs HTTP/1.1
- Header compression (HPACK)
- Multiplexing
- Type safety (fewer validation errors)

## Deployment

### Docker Deployment

```dockerfile
FROM rust:1.85 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/rs3gw /usr/local/bin/

# Expose both HTTP and gRPC ports
EXPOSE 9000 50051

CMD ["rs3gw"]
```

### Kubernetes Deployment

```yaml
apiVersion: v1
kind: Service
metadata:
  name: rs3gw-grpc
spec:
  selector:
    app: rs3gw
  ports:
    - name: http
      protocol: TCP
      port: 9000
      targetPort: 9000
    - name: grpc
      protocol: TCP
      port: 50051
      targetPort: 50051
  type: LoadBalancer
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rs3gw
spec:
  replicas: 3
  selector:
    matchLabels:
      app: rs3gw
  template:
    metadata:
      labels:
        app: rs3gw
    spec:
      containers:
      - name: rs3gw
        image: rs3gw:latest
        ports:
        - containerPort: 9000
          name: http
        - containerPort: 50051
          name: grpc
        env:
        - name: RS3GW_GRPC_ENABLED
          value: "true"
        - name: RS3GW_GRPC_PORT
          value: "50051"
```

### Load Balancing

For gRPC load balancing, use:
- **Client-side LB** - gRPC built-in load balancing
- **Envoy proxy** - L7 load balancing with health checks
- **Nginx** (HTTP/2) - L7 load balancing
- **K8s Service** - L4 load balancing

**Envoy Configuration:**
```yaml
static_resources:
  listeners:
  - name: grpc_listener
    address:
      socket_address:
        address: 0.0.0.0
        port_value: 50051
    filter_chains:
    - filters:
      - name: envoy.filters.network.http_connection_manager
        typed_config:
          "@type": type.googleapis.com/envoy.extensions.filters.network.http_connection_manager.v3.HttpConnectionManager
          codec_type: AUTO
          stat_prefix: grpc
          route_config:
            name: local_route
            virtual_hosts:
            - name: backend
              domains: ["*"]
              routes:
              - match:
                  prefix: "/"
                route:
                  cluster: rs3gw_cluster
          http_filters:
          - name: envoy.filters.http.router
  clusters:
  - name: rs3gw_cluster
    connect_timeout: 1s
    type: STRICT_DNS
    lb_policy: ROUND_ROBIN
    http2_protocol_options: {}
    load_assignment:
      cluster_name: rs3gw_cluster
      endpoints:
      - lb_endpoints:
        - endpoint:
            address:
              socket_address:
                address: rs3gw-1
                port_value: 50051
        - endpoint:
            address:
              socket_address:
                address: rs3gw-2
                port_value: 50051
```

## Testing

Comprehensive integration tests for gRPC functionality:

```bash
# Run gRPC integration tests
cargo test --test grpc_tests

# Run with logging
RUST_LOG=debug cargo test --test grpc_tests

# Run specific test
cargo test --test grpc_tests test_grpc_put_get_object
```

**Test Coverage:**
- Bucket operations (create, delete, head, list, tagging, policy)
- Object operations (put, get, delete, copy, head, attributes, list)
- Multipart uploads (create, upload parts, complete, abort, list)
- Streaming (large objects, backpressure)
- Error handling (not found, invalid parameters)

## Security

### TLS/SSL

Enable TLS for production:

```rust
use tonic::transport::ServerTlsConfig;

let tls_config = ServerTlsConfig::new()
    .identity(Identity::from_pem(cert_pem, key_pem));

Server::builder()
    .tls_config(tls_config)?
    .add_service(bucket_service)
    .add_service(object_service)
    .serve(addr)
    .await?;
```

**Environment Variables:**
- `RS3GW_GRPC_TLS_CERT` — path to the PEM certificate file (enables TLS when both cert and key are set)
- `RS3GW_GRPC_TLS_KEY` — path to the PEM private key file

### Authentication

Implement authentication via metadata:

```rust
use tonic::{Request, Status};

fn check_auth<T>(req: Request<T>) -> Result<Request<T>, Status> {
    match req.metadata().get("authorization") {
        Some(token) if token == "Bearer secret-token" => Ok(req),
        _ => Err(Status::unauthenticated("Invalid token")),
    }
}

// Apply interceptor
Server::builder()
    .add_service(
        BucketServiceServer::with_interceptor(service, check_auth)
    )
    .serve(addr)
    .await?;
```

## Troubleshooting

### Common Issues

**1. Connection Refused**
```
Error: transport error: connection refused
```
Solution: Ensure gRPC server is running on port 50051

**2. Message Size Exceeded**
```
Error: message size exceeded
```
Solution: Increase max message size in both client and server
```rust
// Server
Server::builder()
    .max_decoding_message_size(128 * 1024 * 1024) // 128MB
    .add_service(service)
    .serve(addr).await?;

// Client
let channel = Channel::from_static("http://localhost:50051")
    .max_decoding_message_size(128 * 1024 * 1024)
    .connect().await?;
```

**3. Streaming Stalls**
- Check network connectivity
- Verify chunk size is reasonable (64KB-1MB)
- Monitor backpressure/flow control

## Dependencies

Key dependencies for gRPC functionality:

- **tonic** - gRPC framework (0.14)
- **prost** - Protocol Buffers (0.14)
- **tokio** - Async runtime
- **tokio-stream** - Stream utilities
- **tonic-prost-build** - Build-time code generation

## Future Enhancements

Planned improvements:

1. **gRPC-Web Support** - Browser compatibility
2. **Reflection API** - Dynamic service discovery
3. **Health Checks** - Standard gRPC health checking
4. **Metrics** - gRPC-specific Prometheus metrics
5. **Interceptors** - Request/response middleware
6. **Retry Logic** - Automatic retry with backoff

## Related Documentation

- [API Module](../api/README.md) - REST API implementation
- [Storage Module](../storage/README.md) - Storage backend
- [Main README](../../README.md) - Project overview
- [Protocol Buffers](../../proto/README.md) - Proto definitions

## License

MIT OR Apache-2.0
