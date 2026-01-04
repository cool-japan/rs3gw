//! gRPC Bucket Operations Handlers

use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::error;

use crate::storage::ObjectTagging;
use crate::storage::StorageEngine;

use super::proto::bucket::*;

/// Convert storage errors to gRPC Status
fn map_storage_error(err: impl std::fmt::Display) -> Status {
    Status::internal(format!("Storage error: {}", err))
}

pub async fn list_buckets(
    storage: Arc<StorageEngine>,
    request: Request<ListBucketsRequest>,
) -> Result<Response<ListBucketsResponse>, Status> {
    let req = request.into_inner();

    // List all buckets
    let buckets = storage.list_buckets().await.map_err(map_storage_error)?;

    // Apply pagination
    let max_buckets = req.max_buckets.unwrap_or(1000) as usize;
    let start_index = req
        .continuation_token
        .as_ref()
        .and_then(|t| t.parse::<usize>().ok())
        .unwrap_or(0);

    let end_index = (start_index + max_buckets).min(buckets.len());
    let page_buckets: Vec<_> = buckets[start_index..end_index]
        .iter()
        .map(|b| Bucket {
            name: b.name.clone(),
            creation_date: Some(prost_types::Timestamp {
                seconds: b.creation_date.timestamp(),
                nanos: b.creation_date.timestamp_subsec_nanos() as i32,
            }),
            region: "us-east-1".to_string(),
        })
        .collect();

    let is_truncated = end_index < buckets.len();
    let next_continuation_token = if is_truncated {
        Some(end_index.to_string())
    } else {
        None
    };

    Ok(Response::new(ListBucketsResponse {
        buckets: page_buckets,
        is_truncated,
        next_continuation_token,
    }))
}

pub async fn create_bucket(
    storage: Arc<StorageEngine>,
    request: Request<CreateBucketRequest>,
) -> Result<Response<CreateBucketResponse>, Status> {
    let req = request.into_inner();

    storage
        .create_bucket(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    // Set tags if provided
    if !req.tags.is_empty() {
        if let Err(e) = storage
            .put_bucket_tagging(&req.bucket, &ObjectTagging { tags: req.tags })
            .await
        {
            error!("Failed to set bucket tags: {}", e);
        }
    }

    Ok(Response::new(CreateBucketResponse {
        location: format!("/{}", req.bucket),
    }))
}

pub async fn delete_bucket(
    storage: Arc<StorageEngine>,
    request: Request<DeleteBucketRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    storage
        .delete_bucket(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

pub async fn head_bucket(
    storage: Arc<StorageEngine>,
    request: Request<HeadBucketRequest>,
) -> Result<Response<HeadBucketResponse>, Status> {
    let req = request.into_inner();

    let exists = storage
        .bucket_exists(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    if exists {
        // Get bucket metadata from list
        let buckets = storage.list_buckets().await.map_err(map_storage_error)?;
        let metadata = buckets
            .iter()
            .find(|b| b.name == req.bucket)
            .ok_or_else(|| Status::not_found("Bucket not found"))?;

        Ok(Response::new(HeadBucketResponse {
            exists: true,
            region: Some("us-east-1".to_string()),
            creation_date: Some(prost_types::Timestamp {
                seconds: metadata.creation_date.timestamp(),
                nanos: metadata.creation_date.timestamp_subsec_nanos() as i32,
            }),
        }))
    } else {
        Ok(Response::new(HeadBucketResponse {
            exists: false,
            region: None,
            creation_date: None,
        }))
    }
}

pub async fn get_bucket_location(
    storage: Arc<StorageEngine>,
    request: Request<GetBucketLocationRequest>,
) -> Result<Response<GetBucketLocationResponse>, Status> {
    let req = request.into_inner();

    if !storage
        .bucket_exists(&req.bucket)
        .await
        .map_err(map_storage_error)?
    {
        return Err(Status::not_found("Bucket not found"));
    }

    Ok(Response::new(GetBucketLocationResponse {
        location: "us-east-1".to_string(),
    }))
}

pub async fn get_bucket_metadata(
    storage: Arc<StorageEngine>,
    request: Request<GetBucketMetadataRequest>,
) -> Result<Response<GetBucketMetadataResponse>, Status> {
    let req = request.into_inner();

    let buckets = storage.list_buckets().await.map_err(map_storage_error)?;
    let metadata = buckets
        .iter()
        .find(|b| b.name == req.bucket)
        .ok_or_else(|| Status::not_found("Bucket not found"))?;

    let tags = storage
        .get_bucket_tagging(&req.bucket)
        .await
        .map(|t| t.tags)
        .unwrap_or_default();

    let policy = storage.get_bucket_policy(&req.bucket).await.ok();

    Ok(Response::new(GetBucketMetadataResponse {
        metadata: Some(BucketMetadata {
            name: metadata.name.clone(),
            creation_date: Some(prost_types::Timestamp {
                seconds: metadata.creation_date.timestamp(),
                nanos: metadata.creation_date.timestamp_subsec_nanos() as i32,
            }),
            region: "us-east-1".to_string(),
            tags,
            policy,
            versioning_enabled: {
                let versioning_config = storage
                    .get_bucket_versioning(&req.bucket)
                    .await
                    .map_err(map_storage_error)?;
                versioning_config.status.is_enabled()
            },
        }),
    }))
}

pub async fn put_bucket_tagging(
    storage: Arc<StorageEngine>,
    request: Request<PutBucketTaggingRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    storage
        .put_bucket_tagging(&req.bucket, &ObjectTagging { tags: req.tags })
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

pub async fn get_bucket_tagging(
    storage: Arc<StorageEngine>,
    request: Request<GetBucketTaggingRequest>,
) -> Result<Response<GetBucketTaggingResponse>, Status> {
    let req = request.into_inner();

    let tagging = storage
        .get_bucket_tagging(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(GetBucketTaggingResponse {
        tags: tagging.tags,
    }))
}

pub async fn delete_bucket_tagging(
    storage: Arc<StorageEngine>,
    request: Request<DeleteBucketTaggingRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    storage
        .delete_bucket_tagging(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

pub async fn put_bucket_policy(
    storage: Arc<StorageEngine>,
    request: Request<PutBucketPolicyRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    storage
        .put_bucket_policy(&req.bucket, &req.policy)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

pub async fn get_bucket_policy(
    storage: Arc<StorageEngine>,
    request: Request<GetBucketPolicyRequest>,
) -> Result<Response<GetBucketPolicyResponse>, Status> {
    let req = request.into_inner();

    let policy = storage
        .get_bucket_policy(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(GetBucketPolicyResponse { policy }))
}

pub async fn delete_bucket_policy(
    storage: Arc<StorageEngine>,
    request: Request<DeleteBucketPolicyRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    storage
        .delete_bucket_policy(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(()))
}

pub async fn put_bucket_versioning(
    storage: Arc<StorageEngine>,
    request: Request<PutBucketVersioningRequest>,
) -> Result<Response<()>, Status> {
    let req = request.into_inner();

    if req.enabled {
        storage
            .enable_bucket_versioning(&req.bucket)
            .await
            .map_err(map_storage_error)?;
    } else {
        storage
            .suspend_bucket_versioning(&req.bucket)
            .await
            .map_err(map_storage_error)?;
    }

    Ok(Response::new(()))
}

pub async fn get_bucket_versioning(
    storage: Arc<StorageEngine>,
    request: Request<GetBucketVersioningRequest>,
) -> Result<Response<GetBucketVersioningResponse>, Status> {
    let req = request.into_inner();

    let versioning_config = storage
        .get_bucket_versioning(&req.bucket)
        .await
        .map_err(map_storage_error)?;

    Ok(Response::new(GetBucketVersioningResponse {
        enabled: versioning_config.status.is_enabled(),
    }))
}
