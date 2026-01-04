//! gRPC Server Implementation

use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use crate::storage::StorageEngine;

use super::proto::s3_service_server::{S3Service, S3ServiceServer};
use super::proto::{bucket, multipart, object};

/// gRPC service implementation for S3 operations
#[derive(Clone)]
pub struct S3ServiceImpl {
    storage: Arc<StorageEngine>,
}

impl S3ServiceImpl {
    pub fn new(storage: Arc<StorageEngine>) -> Self {
        Self { storage }
    }
}

#[tonic::async_trait]
impl S3Service for S3ServiceImpl {
    // ===== Bucket Operations =====

    async fn list_buckets(
        &self,
        request: Request<bucket::ListBucketsRequest>,
    ) -> Result<Response<bucket::ListBucketsResponse>, Status> {
        debug!("gRPC: ListBuckets request");
        crate::grpc::bucket::list_buckets(self.storage.clone(), request).await
    }

    async fn create_bucket(
        &self,
        request: Request<bucket::CreateBucketRequest>,
    ) -> Result<Response<bucket::CreateBucketResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: CreateBucket request for bucket: {}", bucket_name);
        crate::grpc::bucket::create_bucket(self.storage.clone(), request).await
    }

    async fn delete_bucket(
        &self,
        request: Request<bucket::DeleteBucketRequest>,
    ) -> Result<Response<()>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: DeleteBucket request for bucket: {}", bucket_name);
        crate::grpc::bucket::delete_bucket(self.storage.clone(), request).await
    }

    async fn head_bucket(
        &self,
        request: Request<bucket::HeadBucketRequest>,
    ) -> Result<Response<bucket::HeadBucketResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: HeadBucket request for bucket: {}", bucket_name);
        crate::grpc::bucket::head_bucket(self.storage.clone(), request).await
    }

    async fn get_bucket_location(
        &self,
        request: Request<bucket::GetBucketLocationRequest>,
    ) -> Result<Response<bucket::GetBucketLocationResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: GetBucketLocation request for bucket: {}",
            bucket_name
        );
        crate::grpc::bucket::get_bucket_location(self.storage.clone(), request).await
    }

    async fn get_bucket_metadata(
        &self,
        request: Request<bucket::GetBucketMetadataRequest>,
    ) -> Result<Response<bucket::GetBucketMetadataResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: GetBucketMetadata request for bucket: {}",
            bucket_name
        );
        crate::grpc::bucket::get_bucket_metadata(self.storage.clone(), request).await
    }

    async fn put_bucket_tagging(
        &self,
        request: Request<bucket::PutBucketTaggingRequest>,
    ) -> Result<Response<()>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: PutBucketTagging request for bucket: {}", bucket_name);
        crate::grpc::bucket::put_bucket_tagging(self.storage.clone(), request).await
    }

    async fn get_bucket_tagging(
        &self,
        request: Request<bucket::GetBucketTaggingRequest>,
    ) -> Result<Response<bucket::GetBucketTaggingResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: GetBucketTagging request for bucket: {}", bucket_name);
        crate::grpc::bucket::get_bucket_tagging(self.storage.clone(), request).await
    }

    async fn delete_bucket_tagging(
        &self,
        request: Request<bucket::DeleteBucketTaggingRequest>,
    ) -> Result<Response<()>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: DeleteBucketTagging request for bucket: {}",
            bucket_name
        );
        crate::grpc::bucket::delete_bucket_tagging(self.storage.clone(), request).await
    }

    async fn put_bucket_policy(
        &self,
        request: Request<bucket::PutBucketPolicyRequest>,
    ) -> Result<Response<()>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: PutBucketPolicy request for bucket: {}", bucket_name);
        crate::grpc::bucket::put_bucket_policy(self.storage.clone(), request).await
    }

    async fn get_bucket_policy(
        &self,
        request: Request<bucket::GetBucketPolicyRequest>,
    ) -> Result<Response<bucket::GetBucketPolicyResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: GetBucketPolicy request for bucket: {}", bucket_name);
        crate::grpc::bucket::get_bucket_policy(self.storage.clone(), request).await
    }

    async fn delete_bucket_policy(
        &self,
        request: Request<bucket::DeleteBucketPolicyRequest>,
    ) -> Result<Response<()>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: DeleteBucketPolicy request for bucket: {}",
            bucket_name
        );
        crate::grpc::bucket::delete_bucket_policy(self.storage.clone(), request).await
    }

    async fn put_bucket_versioning(
        &self,
        request: Request<bucket::PutBucketVersioningRequest>,
    ) -> Result<Response<()>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: PutBucketVersioning request for bucket: {}",
            bucket_name
        );
        crate::grpc::bucket::put_bucket_versioning(self.storage.clone(), request).await
    }

    async fn get_bucket_versioning(
        &self,
        request: Request<bucket::GetBucketVersioningRequest>,
    ) -> Result<Response<bucket::GetBucketVersioningResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: GetBucketVersioning request for bucket: {}",
            bucket_name
        );
        crate::grpc::bucket::get_bucket_versioning(self.storage.clone(), request).await
    }

    // ===== Object Operations =====

    type ListObjectsStream = crate::grpc::object::ListObjectsStream;

    async fn list_objects(
        &self,
        request: Request<object::ListObjectsRequest>,
    ) -> Result<Response<Self::ListObjectsStream>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: ListObjects (streaming) request for bucket: {}",
            bucket_name
        );
        crate::grpc::object::list_objects_stream(self.storage.clone(), request).await
    }

    async fn list_objects_paginated(
        &self,
        request: Request<object::ListObjectsRequest>,
    ) -> Result<Response<object::ListObjectsResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: ListObjectsPaginated request for bucket: {}",
            bucket_name
        );
        crate::grpc::object::list_objects_paginated(self.storage.clone(), request).await
    }

    async fn get_object(
        &self,
        request: Request<object::GetObjectRequest>,
    ) -> Result<Response<object::GetObjectResponse>, Status> {
        let req = request.get_ref();
        debug!("gRPC: GetObject request for {}/{}", req.bucket, req.key);
        crate::grpc::object::get_object(self.storage.clone(), request).await
    }

    type GetObjectStreamStream = crate::grpc::object::GetObjectStream;

    async fn get_object_stream(
        &self,
        request: Request<object::GetObjectRequest>,
    ) -> Result<Response<Self::GetObjectStreamStream>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: GetObjectStream request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::object::get_object_stream(self.storage.clone(), request).await
    }

    async fn put_object(
        &self,
        request: Request<object::PutObjectRequest>,
    ) -> Result<Response<object::PutObjectResponse>, Status> {
        let req = request.get_ref();
        debug!("gRPC: PutObject request for {}/{}", req.bucket, req.key);
        crate::grpc::object::put_object(self.storage.clone(), request).await
    }

    async fn put_object_stream(
        &self,
        request: Request<tonic::Streaming<object::PutObjectStreamRequest>>,
    ) -> Result<Response<object::PutObjectResponse>, Status> {
        debug!("gRPC: PutObjectStream request");
        crate::grpc::object::put_object_stream(self.storage.clone(), request).await
    }

    async fn delete_object(
        &self,
        request: Request<object::DeleteObjectRequest>,
    ) -> Result<Response<object::DeleteObjectResponse>, Status> {
        let req = request.get_ref();
        debug!("gRPC: DeleteObject request for {}/{}", req.bucket, req.key);
        crate::grpc::object::delete_object(self.storage.clone(), request).await
    }

    async fn delete_objects(
        &self,
        request: Request<object::DeleteObjectsRequest>,
    ) -> Result<Response<object::DeleteObjectsResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!("gRPC: DeleteObjects request for bucket: {}", bucket_name);
        crate::grpc::object::delete_objects(self.storage.clone(), request).await
    }

    async fn head_object(
        &self,
        request: Request<object::HeadObjectRequest>,
    ) -> Result<Response<object::HeadObjectResponse>, Status> {
        let req = request.get_ref();
        debug!("gRPC: HeadObject request for {}/{}", req.bucket, req.key);
        crate::grpc::object::head_object(self.storage.clone(), request).await
    }

    async fn copy_object(
        &self,
        request: Request<object::CopyObjectRequest>,
    ) -> Result<Response<object::CopyObjectResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: CopyObject request from {}/{} to {}/{}",
            req.source_bucket, req.source_key, req.dest_bucket, req.dest_key
        );
        crate::grpc::object::copy_object(self.storage.clone(), request).await
    }

    async fn get_object_attributes(
        &self,
        request: Request<object::GetObjectAttributesRequest>,
    ) -> Result<Response<object::GetObjectAttributesResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: GetObjectAttributes request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::object::get_object_attributes(self.storage.clone(), request).await
    }

    async fn put_object_tagging(
        &self,
        request: Request<object::PutObjectTaggingRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: PutObjectTagging request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::object::put_object_tagging(self.storage.clone(), request).await
    }

    async fn get_object_tagging(
        &self,
        request: Request<object::GetObjectTaggingRequest>,
    ) -> Result<Response<object::GetObjectTaggingResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: GetObjectTagging request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::object::get_object_tagging(self.storage.clone(), request).await
    }

    async fn delete_object_tagging(
        &self,
        request: Request<object::DeleteObjectTaggingRequest>,
    ) -> Result<Response<object::DeleteObjectTaggingResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: DeleteObjectTagging request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::object::delete_object_tagging(self.storage.clone(), request).await
    }

    // ===== Multipart Upload Operations =====

    async fn create_multipart_upload(
        &self,
        request: Request<multipart::CreateMultipartUploadRequest>,
    ) -> Result<Response<multipart::CreateMultipartUploadResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: CreateMultipartUpload request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::multipart::create_multipart_upload(self.storage.clone(), request).await
    }

    async fn upload_part(
        &self,
        request: Request<multipart::UploadPartRequest>,
    ) -> Result<Response<multipart::UploadPartResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: UploadPart request for {}/{} part {}",
            req.bucket, req.key, req.part_number
        );
        crate::grpc::multipart::upload_part(self.storage.clone(), request).await
    }

    async fn upload_part_stream(
        &self,
        request: Request<tonic::Streaming<multipart::UploadPartStreamRequest>>,
    ) -> Result<Response<multipart::UploadPartResponse>, Status> {
        debug!("gRPC: UploadPartStream request");
        crate::grpc::multipart::upload_part_stream(self.storage.clone(), request).await
    }

    async fn upload_part_copy(
        &self,
        request: Request<multipart::UploadPartCopyRequest>,
    ) -> Result<Response<multipart::UploadPartCopyResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: UploadPartCopy request from {}/{} to {}/{}",
            req.source_bucket, req.source_key, req.dest_bucket, req.dest_key
        );
        crate::grpc::multipart::upload_part_copy(self.storage.clone(), request).await
    }

    async fn complete_multipart_upload(
        &self,
        request: Request<multipart::CompleteMultipartUploadRequest>,
    ) -> Result<Response<multipart::CompleteMultipartUploadResponse>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: CompleteMultipartUpload request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::multipart::complete_multipart_upload(self.storage.clone(), request).await
    }

    async fn abort_multipart_upload(
        &self,
        request: Request<multipart::AbortMultipartUploadRequest>,
    ) -> Result<Response<()>, Status> {
        let req = request.get_ref();
        debug!(
            "gRPC: AbortMultipartUpload request for {}/{}",
            req.bucket, req.key
        );
        crate::grpc::multipart::abort_multipart_upload(self.storage.clone(), request).await
    }

    async fn list_parts(
        &self,
        request: Request<multipart::ListPartsRequest>,
    ) -> Result<Response<multipart::ListPartsResponse>, Status> {
        let req = request.get_ref();
        debug!("gRPC: ListParts request for {}/{}", req.bucket, req.key);
        crate::grpc::multipart::list_parts(self.storage.clone(), request).await
    }

    async fn list_multipart_uploads(
        &self,
        request: Request<multipart::ListMultipartUploadsRequest>,
    ) -> Result<Response<multipart::ListMultipartUploadsResponse>, Status> {
        let bucket_name = &request.get_ref().bucket;
        debug!(
            "gRPC: ListMultipartUploads request for bucket: {}",
            bucket_name
        );
        crate::grpc::multipart::list_multipart_uploads(self.storage.clone(), request).await
    }
}

/// gRPC server wrapper
pub struct GrpcServer {
    storage: Arc<StorageEngine>,
    bind_addr: SocketAddr,
}

impl GrpcServer {
    pub fn new(storage: Arc<StorageEngine>, bind_addr: SocketAddr) -> Self {
        Self { storage, bind_addr }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let service = S3ServiceImpl::new(self.storage);

        info!("Starting gRPC server on {}", self.bind_addr);

        Server::builder()
            .add_service(S3ServiceServer::new(service))
            .serve(self.bind_addr)
            .await?;

        Ok(())
    }
}
