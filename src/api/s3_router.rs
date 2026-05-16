//! S3 API Router
//!
//! Defines all S3-compatible endpoints and routes them to handlers.

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    body::Body,
    extract::{FromRequest, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, head, options, post, put},
    Router,
};
use http_body_util::BodyExt;
use serde::Deserialize;
use std::sync::Arc;
use utoipa::OpenApi;

use crate::AppState;

use super::utils::{error_response, malformed_xml_response};
use super::{
    bucket_stubs, cors, graphql, handlers, multipart, observability_handlers, openapi,
    preprocessing_handlers, query_intelligence_handlers, replication_handlers,
    select_cache_handlers, tiering_handlers, training_handlers, websocket,
};

/// Query parameters for object operations (multipart, tagging, acl, attributes, restore, etc.)
#[derive(Debug, Deserialize, Default)]
pub struct ObjectQueryParams {
    #[serde(rename = "uploadId")]
    pub upload_id: Option<String>,
    #[serde(rename = "partNumber")]
    pub part_number: Option<u32>,
    pub uploads: Option<String>,
    /// If present, this is a tagging operation
    pub tagging: Option<String>,
    /// If present, this is an ACL operation
    pub acl: Option<String>,
    /// If present, this is a GetObjectAttributes operation
    pub attributes: Option<String>,
    /// If present, this is a RestoreObject operation
    pub restore: Option<String>,
    /// If present, this is a GetObjectLegalHold/PutObjectLegalHold operation
    #[serde(rename = "legal-hold")]
    pub legal_hold: Option<String>,
    /// If present, this is a GetObjectRetention/PutObjectRetention operation
    pub retention: Option<String>,
    /// If present, this is a SelectObjectContent operation
    pub select: Option<String>,
    #[serde(rename = "select-type")]
    pub select_type: Option<String>,
    /// If present, this is a GetObjectTorrent operation
    pub torrent: Option<String>,
    /// Maximum number of parts to return for ListParts pagination
    #[serde(rename = "max-parts")]
    pub max_parts: Option<u32>,
    /// Part number marker for ListParts pagination (return parts after this number)
    #[serde(rename = "part-number-marker")]
    pub part_number_marker: Option<u32>,
    /// Version ID for Object Lock and versioned object operations
    #[serde(rename = "versionId")]
    pub version_id: Option<String>,
}

/// Query parameters for bucket-level POST operations
#[derive(Debug, Deserialize, Default)]
pub struct BucketPostQueryParams {
    /// If present, this is a DeleteObjects operation
    pub delete: Option<String>,
}

/// Query parameters for bucket-level PUT operations
#[derive(Debug, Deserialize, Default)]
pub struct BucketPutQueryParams {
    /// If present, this is a PutBucketVersioning operation
    pub versioning: Option<String>,
    /// If present, this is a PutBucketAcl operation
    pub acl: Option<String>,
    /// If present, this is a PutBucketTagging operation
    pub tagging: Option<String>,
    /// If present, this is a PutBucketPolicy operation
    pub policy: Option<String>,
    /// If present, this is a PutBucketEncryption operation
    pub encryption: Option<String>,
    /// If present, this is a PutBucketLifecycleConfiguration operation
    pub lifecycle: Option<String>,
    /// If present, this is a PutBucketCors operation
    pub cors: Option<String>,
    /// If present, this is a PutBucketNotificationConfiguration operation
    pub notification: Option<String>,
    /// If present, this is a PutBucketLogging operation
    pub logging: Option<String>,
    /// If present, this is a PutBucketRequestPayment operation
    #[serde(rename = "requestPayment")]
    pub request_payment: Option<String>,
    /// If present, this is a PutBucketWebsite operation
    pub website: Option<String>,
    /// If present, this is a PutBucketReplication operation
    pub replication: Option<String>,
    /// If present, this is a PutBucketAccelerateConfiguration operation
    pub accelerate: Option<String>,
    /// If present, this is a PutBucketOwnershipControls operation
    #[serde(rename = "ownershipControls")]
    pub ownership_controls: Option<String>,
    /// If present, this is a PutPublicAccessBlock operation
    #[serde(rename = "publicAccessBlock")]
    pub public_access_block: Option<String>,
    /// If present, this is a PutBucketIntelligentTieringConfiguration operation
    #[serde(rename = "intelligent-tiering")]
    pub intelligent_tiering: Option<String>,
    /// If present, this is a PutObjectLockConfiguration operation
    #[serde(rename = "object-lock")]
    pub object_lock: Option<String>,
    /// If present, this is a PutBucketMetricsConfiguration operation
    pub metrics: Option<String>,
    /// If present, this is a PutBucketAnalyticsConfiguration operation
    pub analytics: Option<String>,
    /// If present, this is a PutBucketInventoryConfiguration operation
    pub inventory: Option<String>,
    /// Configuration ID for metrics/analytics/inventory PUT operations
    pub id: Option<String>,
}

/// Query parameters for bucket-level DELETE operations
#[derive(Debug, Deserialize, Default)]
pub struct BucketDeleteQueryParams {
    /// If present, this is a DeleteBucketTagging operation
    pub tagging: Option<String>,
    /// If present, this is a DeleteBucketPolicy operation
    pub policy: Option<String>,
    /// If present, this is a DeleteBucketEncryption operation
    pub encryption: Option<String>,
    /// If present, this is a DeleteBucketLifecycleConfiguration operation
    pub lifecycle: Option<String>,
    /// If present, this is a DeleteBucketCors operation
    pub cors: Option<String>,
    /// If present, this is a DeleteBucketWebsite operation
    pub website: Option<String>,
    /// If present, this is a DeleteBucketReplication operation
    pub replication: Option<String>,
    /// If present, this is a DeleteBucketOwnershipControls operation
    #[serde(rename = "ownershipControls")]
    pub ownership_controls: Option<String>,
    /// If present, this is a DeletePublicAccessBlock operation
    #[serde(rename = "publicAccessBlock")]
    pub public_access_block: Option<String>,
    /// If present, this is a DeleteBucketIntelligentTieringConfiguration operation
    #[serde(rename = "intelligent-tiering")]
    pub intelligent_tiering: Option<String>,
    /// Configuration ID for intelligent-tiering, metrics, analytics, or inventory delete operations
    pub id: Option<String>,
    /// If present, this is a DeleteBucketMetricsConfiguration operation
    pub metrics: Option<String>,
    /// If present, this is a DeleteBucketAnalyticsConfiguration operation
    pub analytics: Option<String>,
    /// If present, this is a DeleteBucketInventoryConfiguration operation
    pub inventory: Option<String>,
}

/// Query parameters for bucket-level GET operations
#[derive(Debug, Deserialize, Default)]
pub struct BucketGetQueryParams {
    /// If present, this is a ListMultipartUploads operation
    pub uploads: Option<String>,
    /// If present, this is a GetBucketLocation operation
    pub location: Option<String>,
    /// If present, this is a GetBucketVersioning operation
    pub versioning: Option<String>,
    /// If present, this is a GetBucketAcl operation
    pub acl: Option<String>,
    /// If present, this is a GetBucketTagging operation
    pub tagging: Option<String>,
    /// If present, this is a GetBucketPolicy operation
    pub policy: Option<String>,
    /// If present, this is a ListObjectVersions operation
    pub versions: Option<String>,
    /// If present, this is a GetBucketEncryption operation
    pub encryption: Option<String>,
    /// If present, this is a GetBucketLifecycleConfiguration operation
    pub lifecycle: Option<String>,
    /// If present, this is a GetBucketCors operation
    pub cors: Option<String>,
    /// If present, this is a GetBucketNotificationConfiguration operation
    pub notification: Option<String>,
    /// If present, this is a GetBucketLogging operation
    pub logging: Option<String>,
    /// If present, this is a GetBucketRequestPayment operation
    #[serde(rename = "requestPayment")]
    pub request_payment: Option<String>,
    /// If present, this is a GetBucketWebsite operation
    pub website: Option<String>,
    /// If present, this is a GetBucketReplication operation
    pub replication: Option<String>,
    /// If present, this is a GetBucketAccelerateConfiguration operation
    pub accelerate: Option<String>,
    /// If present, this is a GetBucketOwnershipControls operation
    #[serde(rename = "ownershipControls")]
    pub ownership_controls: Option<String>,
    /// If present, this is a GetPublicAccessBlock operation
    #[serde(rename = "publicAccessBlock")]
    pub public_access_block: Option<String>,
    /// If present, this is a GetBucketIntelligentTieringConfiguration operation
    #[serde(rename = "intelligent-tiering")]
    pub intelligent_tiering: Option<String>,
    /// If present, this is a GetObjectLockConfiguration operation
    #[serde(rename = "object-lock")]
    pub object_lock: Option<String>,
    /// If present, this is a GetBucketMetricsConfiguration or ListBucketMetricsConfigurations operation
    pub metrics: Option<String>,
    /// If present, this is a GetBucketAnalyticsConfiguration or ListBucketAnalyticsConfigurations operation
    pub analytics: Option<String>,
    /// If present, this is a GetBucketInventoryConfiguration or ListBucketInventoryConfigurations operation
    pub inventory: Option<String>,
    /// Configuration ID for metrics/analytics/inventory operations
    pub id: Option<String>,
    // ListObjects params (shared between V1 and V2)
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    #[serde(rename = "max-keys")]
    pub max_keys: Option<usize>,
    #[serde(rename = "encoding-type")]
    pub encoding_type: Option<String>,
    // ListObjectsV1-specific params
    pub marker: Option<String>,
    // ListObjectsV2-specific params
    #[serde(rename = "continuation-token")]
    pub continuation_token: Option<String>,
    #[serde(rename = "start-after")]
    pub start_after: Option<String>,
    #[serde(rename = "list-type")]
    pub list_type: Option<String>,
    // ListMultipartUploads params
    #[serde(rename = "max-uploads")]
    pub max_uploads: Option<u32>,
    #[serde(rename = "key-marker")]
    pub key_marker: Option<String>,
    #[serde(rename = "version-id-marker")]
    pub version_id_marker: Option<String>,
    #[serde(rename = "upload-id-marker")]
    pub upload_id_marker: Option<String>,
}

/// Dispatcher for bucket-level GET operations
/// Routes to GetBucketLocation, GetBucketVersioning, GetBucketAcl, ListMultipartUploads, ListObjectsV1, or ListObjectsV2
async fn get_bucket_dispatcher(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<BucketGetQueryParams>,
) -> Response {
    // Check for GetBucketLocation (has ?location)
    if query.location.is_some() {
        return handlers::get_bucket_location(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketVersioning (has ?versioning)
    if query.versioning.is_some() {
        return handlers::get_bucket_versioning(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketAcl (has ?acl)
    if query.acl.is_some() {
        return handlers::get_bucket_acl(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketTagging (has ?tagging)
    if query.tagging.is_some() {
        return handlers::get_bucket_tagging(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketPolicy (has ?policy)
    if query.policy.is_some() {
        return handlers::get_bucket_policy(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for ListObjectVersions (has ?versions)
    if query.versions.is_some() {
        return handlers::list_object_versions(
            State(state),
            Path(bucket),
            Query(handlers::ListObjectVersionsQuery {
                prefix: query.prefix,
                delimiter: query.delimiter,
                max_keys: query.max_keys,
                key_marker: query.key_marker,
                version_id_marker: query.version_id_marker,
            }),
        )
        .await
        .into_response();
    }

    // Check for GetBucketEncryption (has ?encryption)
    if query.encryption.is_some() {
        return bucket_stubs::get_bucket_encryption(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketLifecycleConfiguration (has ?lifecycle)
    if query.lifecycle.is_some() {
        return bucket_stubs::get_bucket_lifecycle(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketCors (has ?cors)
    if query.cors.is_some() {
        return bucket_stubs::get_bucket_cors(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketNotificationConfiguration (has ?notification)
    if query.notification.is_some() {
        return bucket_stubs::get_bucket_notification(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketLogging (has ?logging)
    if query.logging.is_some() {
        return bucket_stubs::get_bucket_logging(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketRequestPayment (has ?requestPayment)
    if query.request_payment.is_some() {
        return bucket_stubs::get_bucket_request_payment(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketWebsite (has ?website)
    if query.website.is_some() {
        return bucket_stubs::get_bucket_website(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketReplication (has ?replication)
    if query.replication.is_some() {
        return bucket_stubs::get_bucket_replication(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketAccelerateConfiguration (has ?accelerate)
    if query.accelerate.is_some() {
        return bucket_stubs::get_bucket_accelerate(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketOwnershipControls (has ?ownershipControls)
    if query.ownership_controls.is_some() {
        return bucket_stubs::get_bucket_ownership_controls(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetPublicAccessBlock (has ?publicAccessBlock)
    if query.public_access_block.is_some() {
        return bucket_stubs::get_public_access_block(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketIntelligentTieringConfiguration (has ?intelligent-tiering)
    if query.intelligent_tiering.is_some() {
        return bucket_stubs::get_bucket_intelligent_tiering(
            State(state),
            Path(bucket),
            query.id.unwrap_or_default(),
        )
        .await
        .into_response();
    }

    // Check for GetObjectLockConfiguration (has ?object-lock)
    if query.object_lock.is_some() {
        return bucket_stubs::get_object_lock_configuration(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for GetBucketMetricsConfiguration or ListBucketMetricsConfigurations (has ?metrics)
    if query.metrics.is_some() {
        // If there's an id parameter, it's GetBucketMetricsConfiguration, else List
        if let Some(ref id) = query.id {
            return bucket_stubs::get_bucket_metrics_configuration(
                State(state),
                Path(bucket),
                id.clone(),
            )
            .await
            .into_response();
        } else {
            return bucket_stubs::list_bucket_metrics_configurations(State(state), Path(bucket))
                .await
                .into_response();
        }
    }

    // Check for GetBucketAnalyticsConfiguration or ListBucketAnalyticsConfigurations (has ?analytics)
    if query.analytics.is_some() {
        if let Some(ref id) = query.id {
            return bucket_stubs::get_bucket_analytics_configuration(
                State(state),
                Path(bucket),
                id.clone(),
            )
            .await
            .into_response();
        } else {
            return bucket_stubs::list_bucket_analytics_configurations(State(state), Path(bucket))
                .await
                .into_response();
        }
    }

    // Check for GetBucketInventoryConfiguration or ListBucketInventoryConfigurations (has ?inventory)
    if query.inventory.is_some() {
        if let Some(ref id) = query.id {
            return bucket_stubs::get_bucket_inventory_configuration(
                State(state),
                Path(bucket),
                id.clone(),
            )
            .await
            .into_response();
        } else {
            return bucket_stubs::list_bucket_inventory_configurations(State(state), Path(bucket))
                .await
                .into_response();
        }
    }

    // Check for ListMultipartUploads (has ?uploads)
    if query.uploads.is_some() {
        return handlers::list_multipart_uploads(
            State(state),
            Path(bucket),
            Query(handlers::ListMultipartUploadsQuery {
                prefix: query.prefix,
                delimiter: query.delimiter,
                max_uploads: query.max_uploads,
                key_marker: query.key_marker,
                upload_id_marker: query.upload_id_marker,
            }),
        )
        .await
        .into_response();
    }

    // Check for list-type=2 to determine V1 or V2
    if query.list_type.as_deref() == Some("2") {
        // ListObjectsV2
        return handlers::list_objects_v2(
            State(state),
            Path(bucket),
            Query(handlers::ListObjectsV2Query {
                prefix: query.prefix,
                delimiter: query.delimiter,
                max_keys: query.max_keys,
                continuation_token: query.continuation_token,
                start_after: query.start_after,
                encoding_type: query.encoding_type,
                list_type: query.list_type,
            }),
        )
        .await
        .into_response();
    }

    // Default to ListObjectsV1 (original S3 API)
    handlers::list_objects_v1(
        State(state),
        Path(bucket),
        Query(handlers::ListObjectsV1Query {
            prefix: query.prefix,
            delimiter: query.delimiter,
            max_keys: query.max_keys,
            marker: query.marker,
            encoding_type: query.encoding_type,
        }),
    )
    .await
    .into_response()
}

/// Dispatcher for PUT object operations
/// Routes to CopyObject, UploadPart, PutObjectTagging, or PutObject based on headers/query params
async fn put_object_dispatcher(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<ObjectQueryParams>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // Check for PutObjectTagging (has ?tagging)
    if query.tagging.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return handlers::put_object_tagging(State(state), Path((bucket, key)), body_bytes)
            .await
            .into_response();
    }

    // Check for PutObjectAcl (has ?acl)
    if query.acl.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "IncompleteBody",
                    &format!("Failed to read request body: {}", e),
                    &format!("/{}/{}", bucket, key),
                )
                .into_response();
            }
        };
        return handlers::put_object_acl(
            State(state),
            Path((bucket, key)),
            headers.clone(),
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for PutObjectLegalHold (has ?legal-hold)
    if query.legal_hold.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        let bypass = headers
            .get("x-amz-bypass-governance-retention")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        return bucket_stubs::put_object_legal_hold(
            State(state),
            Path((bucket, key)),
            query.version_id,
            bypass,
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for PutObjectRetention (has ?retention)
    if query.retention.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        let bypass = headers
            .get("x-amz-bypass-governance-retention")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        return bucket_stubs::put_object_retention(
            State(state),
            Path((bucket, key)),
            query.version_id,
            bypass,
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for UploadPartCopy (has uploadId, partNumber, and x-amz-copy-source)
    if let (Some(upload_id), Some(part_number)) = (&query.upload_id, query.part_number) {
        if headers.contains_key("x-amz-copy-source") {
            // This is UploadPartCopy
            return multipart::upload_part_copy(
                State(state),
                Path((bucket, key)),
                Query(multipart::MultipartQuery {
                    upload_id: Some(upload_id.clone()),
                    part_number: Some(part_number),
                    uploads: None,
                    max_parts: None,
                    part_number_marker: None,
                }),
                headers,
            )
            .await
            .into_response();
        }

        // Regular UploadPart - collect body to Bytes
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };

        return multipart::upload_part(
            State(state),
            Path((bucket, key)),
            Query(multipart::MultipartQuery {
                upload_id: Some(upload_id.clone()),
                part_number: Some(part_number),
                uploads: None,
                max_parts: None,
                part_number_marker: None,
            }),
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for CopyObject (has x-amz-copy-source header)
    if headers.contains_key("x-amz-copy-source") {
        return handlers::copy_object(State(state), Path((bucket, key)), headers)
            .await
            .into_response();
    }

    // Default to PutObject — pass the raw Body for streaming
    handlers::put_object(State(state), Path((bucket, key)), headers, body)
        .await
        .into_response()
}

/// Dispatcher for POST object operations
/// Routes to CreateMultipartUpload, CompleteMultipartUpload, or RestoreObject based on query params
async fn post_object_dispatcher(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<ObjectQueryParams>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // Check for RestoreObject (has ?restore)
    if query.restore.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                tracing::error!("Failed to collect RestoreObject body: {}", e);
                return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
            }
        };
        return handlers::restore_object(State(state), Path((bucket, key)), body_bytes)
            .await
            .into_response();
    }

    // Check for SelectObjectContent (has ?select&select-type=2)
    if query.select.is_some() || query.select_type.is_some() {
        // Collect the request body
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                tracing::error!("Failed to collect request body: {}", e);
                return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
            }
        };
        return handlers::select_object_content(State(state), Path((bucket, key)), body_bytes)
            .await
            .into_response();
    }

    // Check for CreateMultipartUpload (has ?uploads)
    if query.uploads.is_some() {
        return multipart::create_multipart_upload(State(state), Path((bucket, key)), headers)
            .await
            .into_response();
    }

    // Check for CompleteMultipartUpload (has ?uploadId)
    if let Some(upload_id) = query.upload_id {
        // Collect body to Bytes
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };

        return multipart::complete_multipart_upload(
            State(state),
            Path((bucket, key)),
            Query(multipart::MultipartQuery {
                upload_id: Some(upload_id),
                part_number: None,
                uploads: None,
                max_parts: None,
                part_number_marker: None,
            }),
            body_bytes,
        )
        .await
        .into_response();
    }

    // No valid operation
    (StatusCode::BAD_REQUEST, "Invalid POST operation").into_response()
}

/// Dispatcher for GET object operations
/// Routes to GetObjectTagging, GetObjectAcl, GetObjectAttributes, ListParts, or GetObject based on query params
async fn get_object_dispatcher(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<ObjectQueryParams>,
    headers: HeaderMap,
) -> Response {
    // Check for GetObjectTagging (has ?tagging)
    if query.tagging.is_some() {
        return handlers::get_object_tagging(State(state), Path((bucket, key)))
            .await
            .into_response();
    }

    // Check for GetObjectAcl (has ?acl)
    if query.acl.is_some() {
        return handlers::get_object_acl(State(state), Path((bucket, key)))
            .await
            .into_response();
    }

    // Check for GetObjectAttributes (has ?attributes)
    if query.attributes.is_some() {
        return handlers::get_object_attributes(State(state), Path((bucket, key)), headers)
            .await
            .into_response();
    }

    // Check for GetObjectLegalHold (has ?legal-hold)
    if query.legal_hold.is_some() {
        return bucket_stubs::get_object_legal_hold(
            State(state),
            Path((bucket, key)),
            query.version_id,
        )
        .await
        .into_response();
    }

    // Check for GetObjectRetention (has ?retention)
    if query.retention.is_some() {
        return bucket_stubs::get_object_retention(
            State(state),
            Path((bucket, key)),
            query.version_id,
        )
        .await
        .into_response();
    }

    // Check for GetObjectTorrent (has ?torrent)
    if query.torrent.is_some() {
        return bucket_stubs::get_object_torrent(State(state), Path((bucket, key)))
            .await
            .into_response();
    }

    // Check for ListParts (has ?uploadId)
    if let Some(upload_id) = query.upload_id {
        return multipart::list_parts(
            State(state),
            Path((bucket, key)),
            Query(multipart::MultipartQuery {
                upload_id: Some(upload_id),
                part_number: None,
                uploads: None,
                max_parts: query.max_parts,
                part_number_marker: query.part_number_marker,
            }),
        )
        .await
        .into_response();
    }

    // Default to GetObject
    handlers::get_object(State(state), Path((bucket, key)), headers)
        .await
        .into_response()
}

/// Dispatcher for DELETE object operations
/// Routes to DeleteObjectTagging, AbortMultipartUpload, or DeleteObject based on query params
async fn delete_object_dispatcher(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    Query(query): Query<ObjectQueryParams>,
    headers: HeaderMap,
) -> Response {
    // Check for DeleteObjectTagging (has ?tagging)
    if query.tagging.is_some() {
        return handlers::delete_object_tagging(State(state), Path((bucket, key)))
            .await
            .into_response();
    }

    // Check for AbortMultipartUpload (has ?uploadId)
    if let Some(upload_id) = query.upload_id {
        return multipart::abort_multipart_upload(
            State(state),
            Path((bucket, key)),
            Query(multipart::MultipartQuery {
                upload_id: Some(upload_id),
                part_number: None,
                uploads: None,
                max_parts: None,
                part_number_marker: None,
            }),
        )
        .await
        .into_response();
    }

    // Default to DeleteObject — check Object Lock protection first.
    let version_str = query.version_id.as_deref().unwrap_or("null");
    let bypass = headers
        .get("x-amz-bypass-governance-retention")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    match state
        .storage
        .object_lock_manager
        .is_protected(&bucket, &key, version_str)
        .await
    {
        Ok(true) => {
            // Determine whether legal hold or retention is responsible
            let active_mode = state
                .storage
                .object_lock_manager
                .retention_mode(&bucket, &key, version_str)
                .await
                .ok()
                .flatten();

            let resource = format!("/{}/{}", bucket, key);
            if let Some(mode) = active_mode {
                if mode == "COMPLIANCE" {
                    return super::utils::error_response(
                        StatusCode::FORBIDDEN,
                        "AccessDenied",
                        "Object is protected by a COMPLIANCE retention policy and cannot be deleted.",
                        &resource,
                    );
                }
                // GOVERNANCE — allow with bypass header
                if bypass {
                    return handlers::delete_object(State(state), Path((bucket, key)))
                        .await
                        .into_response();
                }
                return super::utils::error_response(
                    StatusCode::FORBIDDEN,
                    "AccessDenied",
                    "Object is under GOVERNANCE retention; supply x-amz-bypass-governance-retention: true to delete.",
                    &resource,
                );
            }
            // Legal hold is active
            return super::utils::error_response(
                StatusCode::FORBIDDEN,
                "AccessDenied",
                "Object is protected by a Legal Hold and cannot be deleted.",
                &resource,
            );
        }
        Ok(false) => {}
        Err(_) => {} // Do not block deletion on lock-check I/O error
    }

    handlers::delete_object(State(state), Path((bucket, key)))
        .await
        .into_response()
}

/// Dispatcher for bucket-level POST operations
/// Routes to DeleteObjects or PostObject based on query params and content type
async fn post_bucket_dispatcher(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<BucketPostQueryParams>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // Check for DeleteObjects (has ?delete)
    if query.delete.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return handlers::delete_objects(State(state), Path(bucket), headers.clone(), body_bytes)
            .await
            .into_response();
    }

    // Check for PostObject (multipart/form-data Content-Type)
    if let Some(content_type) = headers.get("content-type") {
        if let Ok(ct) = content_type.to_str() {
            if ct.starts_with("multipart/form-data") {
                // Convert Body to Multipart - need to use axum's Multipart extractor via Request
                let req = match axum::http::Request::builder()
                    .header("content-type", ct)
                    .body(body)
                {
                    Ok(r) => r,
                    Err(e) => {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Failed to build request: {}", e),
                        )
                            .into_response();
                    }
                };
                let multipart = match axum::extract::Multipart::from_request(req, &state).await {
                    Ok(m) => m,
                    Err(e) => {
                        return (
                            StatusCode::BAD_REQUEST,
                            format!("Failed to parse multipart: {}", e),
                        )
                            .into_response();
                    }
                };
                return handlers::post_object(State(state), Path(bucket), multipart)
                    .await
                    .into_response();
            }
        }
    }

    // No valid operation
    (StatusCode::BAD_REQUEST, "Invalid POST operation on bucket").into_response()
}

/// Dispatcher for bucket-level PUT operations
/// Routes to PutBucketVersioning, PutBucketAcl, or CreateBucket based on query params
async fn put_bucket_dispatcher(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<BucketPutQueryParams>,
    headers: HeaderMap,
    body: Body,
) -> Response {
    // Check for PutBucketVersioning (has ?versioning)
    if query.versioning.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return handlers::put_bucket_versioning(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketAcl (has ?acl)
    if query.acl.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "IncompleteBody",
                    &format!("Failed to read request body: {}", e),
                    &format!("/{}", bucket),
                )
                .into_response();
            }
        };
        return handlers::put_bucket_acl(State(state), Path(bucket), headers, body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketTagging (has ?tagging)
    if query.tagging.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return handlers::put_bucket_tagging(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketPolicy (has ?policy)
    if query.policy.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return handlers::put_bucket_policy(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketEncryption (has ?encryption)
    if query.encryption.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_encryption(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketLifecycleConfiguration (has ?lifecycle)
    if query.lifecycle.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_lifecycle(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketCors (has ?cors)
    if query.cors.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_cors(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketNotificationConfiguration (has ?notification)
    if query.notification.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        return bucket_stubs::put_bucket_notification(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketLogging (has ?logging)
    if query.logging.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_logging(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketRequestPayment (has ?requestPayment)
    if query.request_payment.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_request_payment(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketWebsite (has ?website)
    if query.website.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_website(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketReplication (has ?replication)
    if query.replication.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        return bucket_stubs::put_bucket_replication(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketAccelerateConfiguration (has ?accelerate)
    if query.accelerate.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        return bucket_stubs::put_bucket_accelerate(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketOwnershipControls (has ?ownershipControls)
    if query.ownership_controls.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_bucket_ownership_controls(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutPublicAccessBlock (has ?publicAccessBlock)
    if query.public_access_block.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read body: {}", e),
                )
                    .into_response();
            }
        };
        return bucket_stubs::put_public_access_block(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketIntelligentTieringConfiguration (has ?intelligent-tiering)
    if query.intelligent_tiering.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        return bucket_stubs::put_bucket_intelligent_tiering(
            State(state),
            Path(bucket),
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for PutObjectLockConfiguration (has ?object-lock)
    if query.object_lock.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        return bucket_stubs::put_object_lock_configuration(State(state), Path(bucket), body_bytes)
            .await
            .into_response();
    }

    // Check for PutBucketMetricsConfiguration (has ?metrics)
    if query.metrics.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        let id = query.id.unwrap_or_default();
        return bucket_stubs::put_bucket_metrics_configuration(
            State(state),
            Path(bucket),
            id,
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for PutBucketAnalyticsConfiguration (has ?analytics)
    if query.analytics.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        let id = query.id.unwrap_or_default();
        return bucket_stubs::put_bucket_analytics_configuration(
            State(state),
            Path(bucket),
            id,
            body_bytes,
        )
        .await
        .into_response();
    }

    // Check for PutBucketInventoryConfiguration (has ?inventory)
    if query.inventory.is_some() {
        let body_bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(_) => return malformed_xml_response("Failed to read request body").into_response(),
        };
        let id = query.id.unwrap_or_default();
        return bucket_stubs::put_bucket_inventory_configuration(
            State(state),
            Path(bucket),
            id,
            body_bytes,
        )
        .await
        .into_response();
    }

    // Default to CreateBucket
    // If a non-empty body is provided, it must be a valid CreateBucketConfiguration XML.
    // Reject clearly malformed XML (non-empty body that lacks an opening XML tag).
    let body_bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read body: {}", e),
            )
                .into_response();
        }
    };
    if !body_bytes.is_empty() {
        let body_str = String::from_utf8_lossy(&body_bytes);
        let trimmed = body_str.trim();
        // A valid XML document must begin with '<'; anything else is malformed
        if !trimmed.starts_with('<') {
            return (
                StatusCode::BAD_REQUEST,
                [("content-type", "application/xml")],
                r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>MalformedXML</Code><Message>The XML you provided was not well-formed</Message></Error>"#,
            )
                .into_response();
        }
        // Verify basic XML well-formedness: every opened tag should have a closing counterpart.
        // We check by scanning for unclosed '<' sequences: if we find a '<' that is not
        // followed by a matching '>' anywhere in the document, the XML is malformed.
        let mut depth: i32 = 0;
        let chars: Vec<char> = trimmed.chars().collect();
        let mut i = 0;
        let mut malformed = false;
        while i < chars.len() {
            if chars[i] == '<' {
                if i + 1 >= chars.len() {
                    malformed = true;
                    break;
                }
                // Find closing '>'
                if let Some(gt) = chars[i..].iter().position(|&c| c == '>') {
                    let tag: String = chars[i..i + gt + 1].iter().collect();
                    if tag.starts_with("</") {
                        depth -= 1;
                    } else if !tag.ends_with("/>")
                        && !tag.starts_with("<?")
                        && !tag.starts_with("<!")
                    {
                        depth += 1;
                    }
                    i += gt + 1;
                } else {
                    malformed = true;
                    break;
                }
            } else {
                i += 1;
            }
        }
        if malformed || depth != 0 {
            return (
                StatusCode::BAD_REQUEST,
                [("content-type", "application/xml")],
                r#"<?xml version="1.0" encoding="UTF-8"?><Error><Code>MalformedXML</Code><Message>The XML you provided was not well-formed</Message></Error>"#,
            )
                .into_response();
        }
    }
    // Read x-amz-bucket-object-lock-enabled header for CreateBucket
    let object_lock_enabled = headers
        .get("x-amz-bucket-object-lock-enabled")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    handlers::create_bucket(State(state), Path(bucket), object_lock_enabled)
        .await
        .into_response()
}

/// Dispatcher for bucket-level DELETE operations
/// Routes to DeleteBucketTagging, DeleteBucketPolicy, or DeleteBucket based on query params
async fn delete_bucket_dispatcher(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Query(query): Query<BucketDeleteQueryParams>,
) -> Response {
    // Check for DeleteBucketTagging (has ?tagging)
    if query.tagging.is_some() {
        return handlers::delete_bucket_tagging(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketPolicy (has ?policy)
    if query.policy.is_some() {
        return handlers::delete_bucket_policy(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketEncryption (has ?encryption) - no-op (stub)
    if query.encryption.is_some() {
        return bucket_stubs::delete_bucket_encryption(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketLifecycleConfiguration (has ?lifecycle) - no-op (stub)
    if query.lifecycle.is_some() {
        return bucket_stubs::delete_bucket_lifecycle(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketCors (has ?cors) - no-op (stub)
    if query.cors.is_some() {
        return bucket_stubs::delete_bucket_cors(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketWebsite (has ?website) - no-op (stub)
    if query.website.is_some() {
        return bucket_stubs::delete_bucket_website(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketReplication (has ?replication) - no-op (stub)
    if query.replication.is_some() {
        return bucket_stubs::delete_bucket_replication(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketOwnershipControls (has ?ownershipControls) - no-op (stub)
    if query.ownership_controls.is_some() {
        return bucket_stubs::delete_bucket_ownership_controls(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeletePublicAccessBlock (has ?publicAccessBlock) - no-op (stub)
    if query.public_access_block.is_some() {
        return bucket_stubs::delete_public_access_block(State(state), Path(bucket))
            .await
            .into_response();
    }

    // Check for DeleteBucketIntelligentTieringConfiguration (has ?intelligent-tiering)
    if query.intelligent_tiering.is_some() {
        return bucket_stubs::delete_bucket_intelligent_tiering(
            State(state),
            Path(bucket),
            query.id.unwrap_or_default(),
        )
        .await
        .into_response();
    }

    // Check for DeleteBucketMetricsConfiguration (has ?metrics)
    if query.metrics.is_some() {
        let id = query.id.clone().unwrap_or_default();
        return bucket_stubs::delete_bucket_metrics_configuration(State(state), Path(bucket), id)
            .await
            .into_response();
    }

    // Check for DeleteBucketAnalyticsConfiguration (has ?analytics)
    if query.analytics.is_some() {
        let id = query.id.clone().unwrap_or_default();
        return bucket_stubs::delete_bucket_analytics_configuration(State(state), Path(bucket), id)
            .await
            .into_response();
    }

    // Check for DeleteBucketInventoryConfiguration (has ?inventory)
    if query.inventory.is_some() {
        let id = query.id.clone().unwrap_or_default();
        return bucket_stubs::delete_bucket_inventory_configuration(State(state), Path(bucket), id)
            .await
            .into_response();
    }

    // Default to DeleteBucket
    handlers::delete_bucket(State(state), Path(bucket))
        .await
        .into_response()
}

/// Handler for WriteGetObjectResponse (Lambda Object Lambda stub)
async fn write_get_object_response_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    bucket_stubs::write_get_object_response(State(state), headers)
        .await
        .into_response()
}

/// GraphQL query handler
async fn graphql_handler(State(state): State<AppState>, req: GraphQLRequest) -> GraphQLResponse {
    let schema = graphql::build_schema(
        state.storage.clone(),
        Arc::new(state.event_broadcaster.clone()),
    );
    schema.execute(req.into_inner()).await.into()
}

/// GraphQL Playground UI handler
async fn graphql_playground_handler() -> impl IntoResponse {
    Html(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="utf-8">
            <title>rs3gw GraphQL Playground</title>
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <link rel="stylesheet" href="https://unpkg.com/graphql-playground-react/build/static/css/index.css">
            <link rel="shortcut icon" href="https://unpkg.com/graphql-playground-react/build/favicon.png">
            <script src="https://unpkg.com/graphql-playground-react/build/static/js/middleware.js"></script>
        </head>
        <body>
            <div id="root"></div>
            <script>
                window.addEventListener('load', function (event) {
                    GraphQLPlayground.init(document.getElementById('root'), {
                        endpoint: '/graphql',
                        settings: {
                            'editor.theme': 'dark',
                            'editor.cursorShape': 'line'
                        },
                        tabs: [
                            {
                                endpoint: '/graphql',
                                query: `# Welcome to rs3gw GraphQL API!
#
# Example queries:

# List all buckets
query ListBuckets {
  buckets {
    name
    createdAt
    region
  }
}

# Get bucket details
query GetBucket {
  bucket(name: "my-bucket") {
    name
    objectCount
    totalSize
    tags {
      key
      value
    }
  }
}

# List objects in a bucket
query ListObjects {
  objects(bucket: "my-bucket", limit: 10) {
    key
    size
    lastModified
    contentType
  }
}

# Search objects by pattern
query SearchObjects {
  searchObjects(pattern: "data", limit: 10) {
    key
    bucket
    size
  }
}

# Get storage statistics
query GetStats {
  stats {
    bucketCount
    totalObjects
    totalSizeBytes
  }
}
`
                            }
                        ]
                    })
                })
            </script>
        </body>
        </html>
        "#,
    )
}

/// Creates the S3-compatible API router
pub fn routes() -> Router<AppState> {
    Router::new()
        // Health check endpoint
        .route("/health", get(handlers::health_check))
        // Liveness / readiness probe
        .route("/ready", get(handlers::ready_check))
        // Prometheus metrics endpoint
        .route("/metrics", get(handlers::metrics))
        // Admin: garbage-collect abandoned multipart uploads
        .route("/api/admin/gc/multipart", post(handlers::admin_gc_multipart))
        // Swagger UI endpoint with embedded OpenAPI spec (custom, non-S3 API)
        .merge(utoipa_swagger_ui::SwaggerUi::new("/swagger-ui")
            .url("/openapi.json", openapi::ApiDoc::openapi()))
        // GraphQL endpoint (custom, non-S3 API)
        .route("/graphql", get(graphql_playground_handler).post(graphql_handler))
        // WebSocket event streaming endpoint (custom, non-S3 API)
        .route("/events/stream", get(websocket::ws_handler))
        // Advanced replication management endpoints (custom, non-S3 API)
        .route("/api/replication/{bucket}/config",
            get(replication_handlers::get_replication_config)
            .put(replication_handlers::set_replication_config)
            .delete(replication_handlers::delete_replication_config))
        .route("/api/replication/metrics", get(replication_handlers::get_replication_metrics))
        .route("/api/replication/metrics/{destination}", get(replication_handlers::get_destination_metrics))
        .route("/api/replication/flush", post(replication_handlers::flush_replication_batches))
        // Observability API endpoints (v5.0.0 - custom, non-S3 API)
        .route("/api/observability/profiling", get(observability_handlers::get_profiling_data))
        .route("/api/observability/business-metrics", get(observability_handlers::get_business_metrics))
        .route("/api/observability/anomalies", get(observability_handlers::get_anomalies))
        .route("/api/observability/resources", get(observability_handlers::get_resource_stats))
        .route("/api/observability/health", get(observability_handlers::get_comprehensive_health))
        // Predictive Analytics Endpoints
        .route("/api/observability/predictions/storage-growth", get(observability_handlers::get_storage_growth_prediction))
        .route("/api/observability/predictions/access-patterns", get(observability_handlers::get_access_pattern_prediction))
        .route("/api/observability/predictions/costs", get(observability_handlers::get_cost_forecast))
        .route("/api/observability/predictions/capacity", get(observability_handlers::get_capacity_recommendations))
        // Preprocessing API endpoints (v5.0.0 - custom, non-S3 API)
        .route("/api/preprocessing/pipelines", post(preprocessing_handlers::create_pipeline))
        .route("/api/preprocessing/pipelines", get(preprocessing_handlers::list_pipelines))
        .route("/api/preprocessing/pipelines/{id}", get(preprocessing_handlers::get_pipeline))
        .route("/api/preprocessing/pipelines/{id}", delete(preprocessing_handlers::delete_pipeline))
        .route("/api/preprocessing/apply", post(preprocessing_handlers::apply_pipeline))
        .route("/api/preprocessing/validate", post(preprocessing_handlers::validate_pipeline))
        .route("/api/preprocessing/cache/stats", get(preprocessing_handlers::get_cache_stats))
        .route("/api/preprocessing/cache/clear", post(preprocessing_handlers::clear_cache))
        // Intelligent Tiering API endpoints
        .route("/api/tiering/policies/{bucket}", get(tiering_handlers::get_tiering_policy))
        .route("/api/tiering/policies/{bucket}", put(tiering_handlers::set_tiering_policy))
        .route("/api/tiering/policies/{bucket}", delete(tiering_handlers::delete_tiering_policy))
        .route("/api/tiering/analyze/{bucket}", post(tiering_handlers::analyze_tiering))
        .route("/api/tiering/analyze/{bucket}/predictive", post(tiering_handlers::analyze_tiering_predictive))
        .route("/api/tiering/recommendations/{bucket}/capacity", get(tiering_handlers::get_capacity_recommendations))
        .route("/api/tiering/apply/{bucket}", post(tiering_handlers::apply_tiering_recommendations))
        .route("/api/tiering/history", get(tiering_handlers::get_transition_history))
        .route("/api/tiering/history/{bucket}", get(tiering_handlers::get_bucket_transition_history))
        // S3 Select Query Cache API endpoints
        .route("/api/select/cache/stats", get(select_cache_handlers::get_cache_stats))
        .route("/api/select/cache/clear", post(select_cache_handlers::clear_cache))
        .route("/api/select/cache/invalidate/{etag}", delete(select_cache_handlers::invalidate_object_cache))
        // Cache warming endpoints
        .route("/api/select/cache/patterns", get(select_cache_handlers::get_pattern_stats))
        .route("/api/select/cache/patterns/top", get(select_cache_handlers::get_top_queries))
        .route("/api/select/cache/patterns/recent", get(select_cache_handlers::get_recent_queries))
        .route("/api/select/cache/patterns/clear", post(select_cache_handlers::clear_patterns))
        // Cache persistence endpoints
        .route("/api/select/cache/save", post(select_cache_handlers::save_cache))
        .route("/api/select/cache/load", post(select_cache_handlers::load_cache))
        // Query Intelligence API endpoints (AI-powered query optimization)
        .route("/api/query/intelligence/statistics", get(query_intelligence_handlers::get_statistics))
        .route("/api/query/intelligence/summary", get(query_intelligence_handlers::get_summary))
        .route("/api/query/intelligence/predict-cost", post(query_intelligence_handlers::predict_cost))
        .route("/api/query/intelligence/recommend-strategy", post(query_intelligence_handlers::recommend_strategy))
        .route("/api/query/intelligence/find-similar", post(query_intelligence_handlers::find_similar))
        .route("/api/query/intelligence/index-recommendations", get(query_intelligence_handlers::get_index_recommendations))
        .route("/api/query/intelligence/complexity-distribution", get(query_intelligence_handlers::get_complexity_distribution))
        // Distributed Training API endpoints (v5.0.0 - ML/AI training management)
        .route("/api/training/experiments", post(training_handlers::create_experiment))
        .route("/api/training/experiments/{experiment_id}", get(training_handlers::get_experiment))
        .route("/api/training/experiments/{experiment_id}/status", put(training_handlers::update_experiment_status))
        .route("/api/training/experiments/{experiment_id}/checkpoints", post(training_handlers::save_checkpoint))
        .route("/api/training/experiments/{experiment_id}/checkpoints", get(training_handlers::list_checkpoints))
        .route("/api/training/checkpoints/{checkpoint_id}", get(training_handlers::load_checkpoint))
        .route("/api/training/experiments/{experiment_id}/metrics", post(training_handlers::log_metrics))
        .route("/api/training/experiments/{experiment_id}/metrics", get(training_handlers::get_metrics))
        .route("/api/training/searches", post(training_handlers::create_search))
        .route("/api/training/searches/{search_id}", get(training_handlers::get_search))
        .route("/api/training/searches/{search_id}/trials", post(training_handlers::add_trial))
        // Presigned URL generation endpoint (custom, non-S3 API)
        .route("/presign/{bucket}/{*key}", get(handlers::generate_presigned_url))
        // S3: ListBuckets — returns all buckets owned by the authenticated sender
        .route("/", get(handlers::list_buckets))
        // S3: HeadBucket — check bucket existence and permissions (with and without trailing slash for SDK compatibility)
        .route("/{bucket}", head(handlers::head_bucket))
        .route("/{bucket}/", head(handlers::head_bucket))
        // S3: GetBucketLocation / GetBucketVersioning / GetBucketAcl / ListMultipartUploads / ListObjectsV1 / ListObjectsV2
        .route("/{bucket}", get(get_bucket_dispatcher))
        .route("/{bucket}/", get(get_bucket_dispatcher))
        // S3: CreateBucket / PutBucketVersioning / PutBucketAcl / PutBucketTagging / PutBucketPolicy / PutBucketEncryption / etc.
        .route("/{bucket}", put(put_bucket_dispatcher))
        .route("/{bucket}/", put(put_bucket_dispatcher))
        // S3: DeleteBucket / DeleteBucketTagging / DeleteBucketPolicy / DeleteBucketEncryption / etc.
        .route("/{bucket}", delete(delete_bucket_dispatcher))
        .route("/{bucket}/", delete(delete_bucket_dispatcher))
        // S3: DeleteObjects (?delete) / PostObject (multipart/form-data upload)
        .route("/{bucket}", post(post_bucket_dispatcher))
        .route("/{bucket}/", post(post_bucket_dispatcher))
        // S3: HeadObject — returns metadata without the object body
        .route("/{bucket}/{*key}", head(handlers::head_object))
        // S3: GetObject / GetObjectTagging / GetObjectAcl / GetObjectAttributes / ListParts
        .route("/{bucket}/{*key}", get(get_object_dispatcher))
        // S3: PutObject / CopyObject (x-amz-copy-source) / UploadPart (partNumber+uploadId) / UploadPartCopy / PutObjectTagging / PutObjectAcl
        .route("/{bucket}/{*key}", put(put_object_dispatcher))
        // S3: CreateMultipartUpload (?uploads) / CompleteMultipartUpload (?uploadId) / RestoreObject (?restore) / SelectObjectContent (?select)
        .route("/{bucket}/{*key}", post(post_object_dispatcher))
        // S3: DeleteObject / DeleteObjectTagging (?tagging) / AbortMultipartUpload (?uploadId)
        .route("/{bucket}/{*key}", delete(delete_object_dispatcher))
        // S3 Lambda Object Lambda: WriteGetObjectResponse (stub)
        .route("/WriteGetObjectResponse", post(write_get_object_response_handler))
        // CORS preflight (OPTIONS) for bucket and object endpoints
        .route("/{bucket}", options(cors::options_bucket_handler))
        .route("/{bucket}/", options(cors::options_bucket_handler))
        .route("/{bucket}/{*key}", options(cors::options_object_handler))
}
