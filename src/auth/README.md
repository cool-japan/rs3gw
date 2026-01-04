# rs3gw Auth Module

AWS Signature Version 4 authentication for S3 API requests.

## Overview

This module implements AWS SigV4 authentication for securing S3 API requests. In the current release, authentication is optional - when credentials are not configured, all requests are allowed (development/MVP mode).

## Module Structure

```
auth/
├── mod.rs   # Module exports
└── v4.rs    # AWS Signature V4 implementation
```

## Components

### SigV4Verifier

The main authentication component that verifies AWS SigV4 signatures.

```rust
use rs3gw::auth::v4::SigV4Verifier;

// Create verifier with credentials
let verifier = SigV4Verifier::new(
    "AKIAIOSFODNN7EXAMPLE".to_string(),
    "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
    "us-east-1".to_string(),
);

// Check if auth is enabled
if verifier.is_enabled() {
    // Verify request
    verifier.verify_request(
        "GET",                    // HTTP method
        "/bucket/key",            // URI path
        "list-type=2",            // Query string
        &headers,                 // Request headers
        &payload_hash,            // SHA256 hash of body
        auth_header,              // Authorization header
    )?;
}
```

### Error Types

```rust
pub enum AuthError {
    MissingAuth,              // No Authorization header
    InvalidFormat,            // Malformed auth header
    InvalidSignature,         // Signature mismatch
    RequestExpired,           // Request timestamp too old
    InvalidAccessKey,         // Unknown access key
    MissingSignedHeader(String), // Required header not signed
}
```

## Authentication Flow

1. **Check if enabled**: If `access_key` and `secret_key` are empty, skip authentication
2. **Parse Authorization header**: Extract algorithm, credential, signed headers, signature
3. **Verify access key**: Match against configured access key
4. **Build canonical request**: Normalize method, URI, query, headers
5. **Build string to sign**: Create the signing payload
6. **Calculate signature**: HMAC-SHA256 chain with secret key
7. **Compare signatures**: Constant-time comparison

## Configuration

Authentication is configured via environment variables:

```bash
# Enable authentication
export RS3GW_ACCESS_KEY="your-access-key"
export RS3GW_SECRET_KEY="your-secret-key"

# Disable authentication (MVP mode)
export RS3GW_ACCESS_KEY=""
export RS3GW_SECRET_KEY=""
```

## Features (v0.1.0)

### Implemented
- [x] SigV4 header parsing
- [x] Canonical request building
- [x] String to sign construction
- [x] Signing key derivation
- [x] Signature calculation
- [x] Signature verification
- [x] Payload hash validation
- [x] Optional auth (disabled when no credentials)
- [x] Unit tests

### Planned (Phase 3)
- [ ] Presigned URL verification
- [ ] Query string authentication
- [ ] Timestamp validation (±15 minutes)
- [ ] Chunked upload signing
- [ ] Multi-region support
- [ ] Credential rotation

## AWS SigV4 Format

### Authorization Header

```
AWS4-HMAC-SHA256
Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request,
SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,
Signature=fe5f80f77d5fa3beca038a248ff027d0445342fe2855ddc963176630326f1024
```

### Canonical Request

```
GET
/bucket/key
list-type=2
host:s3.amazonaws.com
x-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
x-amz-date:20130524T000000Z

host;x-amz-content-sha256;x-amz-date
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
```

### String to Sign

```
AWS4-HMAC-SHA256
20130524T000000Z
20130524/us-east-1/s3/aws4_request
7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972
```

### Signing Key Derivation

```
kSecret  = "AWS4" + SecretAccessKey
kDate    = HMAC-SHA256(kSecret, Date)
kRegion  = HMAC-SHA256(kDate, Region)
kService = HMAC-SHA256(kRegion, Service)
kSigning = HMAC-SHA256(kService, "aws4_request")
```

## Helper Functions

```rust
// Hash a payload (empty body = EMPTY_SHA256)
let hash = SigV4Verifier::hash_payload(body_bytes);

// Constant for empty body hash
pub const EMPTY_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
```

## Security Considerations

- Secret keys are never logged or exposed
- Signature comparison uses constant-time operations
- Request timestamps should be validated (future work)
- HTTPS is recommended for production use
