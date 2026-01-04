//! Authentication module
//!
//! Implements AWS Signature V4 verification for both header-based
//! and presigned URL authentication. Includes support for aws-chunked
//! transfer encoding for streaming uploads.
//!
//! Also provides Attribute-Based Access Control (ABAC) for fine-grained
//! access policies based on time, IP address, and resource attributes.

pub mod abac;
pub mod chunked;
pub mod presigned_post;
pub mod v4;

pub use chunked::{
    encode_chunk, encode_final_chunk, get_decoded_content_length, is_aws_chunked,
    parse_chunk_header, ChunkedDecoder, ChunkedError, ChunkedSignatureContext,
};
pub use presigned_post::{
    PostCondition, PostPolicy, PresignedPost, PresignedPostBuilder, PresignedPostFields,
};
pub use v4::{
    hash_payload, AuthError, PresignedParams, PresignedUrlGenerator, SigV4Verifier,
    UNSIGNED_PAYLOAD,
};
