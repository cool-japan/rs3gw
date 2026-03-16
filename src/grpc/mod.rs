//! gRPC API for rs3gw
//!
//! This module provides a high-performance gRPC interface to the S3 gateway,
//! offering bi-directional streaming for large file transfers and native HTTP/2 support.
//!
//! # Features
//! - High-performance binary protocol with Protocol Buffers
//! - Bi-directional streaming for multipart uploads
//! - Server streaming for large downloads and object listings
//! - Client streaming for efficient uploads
//! - Native HTTP/2 multiplexing
//! - Language-agnostic client generation

pub mod bucket;
pub mod multipart;
pub mod object;
pub mod server;

// Re-export generated protobuf code
pub mod proto {
    tonic::include_proto!("rs3gw");

    pub mod bucket {
        tonic::include_proto!("rs3gw.bucket");
    }

    pub mod object {
        tonic::include_proto!("rs3gw.object");
    }

    pub mod multipart {
        tonic::include_proto!("rs3gw.multipart");
    }
}

pub use server::{GrpcConfig, GrpcServer};
