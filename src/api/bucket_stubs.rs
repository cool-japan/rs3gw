//! Stub implementations for S3 bucket-level and object-level APIs not yet fully implemented.
//!
//! Each handler validates bucket existence (returning `NoSuchBucket` when appropriate)
//! and returns either a fixed XML response, a "not found" error, or `NotImplemented`.
//!
//! | Method   | Query / Path                      | Operation                                  |
//! |----------|-----------------------------------|--------------------------------------------|
//! | GET      | `/{bucket}?accelerate`            | GetBucketAccelerateConfiguration           |
//! | PUT      | `/{bucket}?accelerate`            | PutBucketAccelerateConfiguration           |
//! | GET      | `/{bucket}?encryption`            | GetBucketEncryption                        |
//! | PUT      | `/{bucket}?encryption`            | PutBucketEncryption                        |
//! | DELETE   | `/{bucket}?encryption`            | DeleteBucketEncryption                     |
//! | GET      | `/{bucket}?lifecycle`             | GetBucketLifecycleConfiguration            |
//! | PUT      | `/{bucket}?lifecycle`             | PutBucketLifecycleConfiguration            |
//! | DELETE   | `/{bucket}?lifecycle`             | DeleteBucketLifecycleConfiguration         |
//! | GET      | `/{bucket}?cors`                  | GetBucketCors                              |
//! | PUT      | `/{bucket}?cors`                  | PutBucketCors                              |
//! | DELETE   | `/{bucket}?cors`                  | DeleteBucketCors                           |
//! | GET      | `/{bucket}?notification`          | GetBucketNotificationConfiguration         |
//! | PUT      | `/{bucket}?notification`          | PutBucketNotificationConfiguration         |
//! | GET      | `/{bucket}?logging`               | GetBucketLogging                           |
//! | PUT      | `/{bucket}?logging`               | PutBucketLogging                           |
//! | GET      | `/{bucket}?requestPayment`        | GetBucketRequestPayment                    |
//! | PUT      | `/{bucket}?requestPayment`        | PutBucketRequestPayment                    |
//! | GET      | `/{bucket}?website`               | GetBucketWebsite                           |
//! | PUT      | `/{bucket}?website`               | PutBucketWebsite                           |
//! | DELETE   | `/{bucket}?website`               | DeleteBucketWebsite                        |
//! | GET      | `/{bucket}?replication`           | GetBucketReplication                       |
//! | PUT      | `/{bucket}?replication`           | PutBucketReplication                       |
//! | DELETE   | `/{bucket}?replication`           | DeleteBucketReplication                    |
//! | GET      | `/{bucket}?ownershipControls`     | GetBucketOwnershipControls                 |
//! | PUT      | `/{bucket}?ownershipControls`     | PutBucketOwnershipControls                 |
//! | DELETE   | `/{bucket}?ownershipControls`     | DeleteBucketOwnershipControls              |
//! | GET      | `/{bucket}?publicAccessBlock`     | GetPublicAccessBlock                       |
//! | PUT      | `/{bucket}?publicAccessBlock`     | PutPublicAccessBlock                       |
//! | DELETE   | `/{bucket}?publicAccessBlock`     | DeletePublicAccessBlock                    |
//! | GET      | `/{bucket}?intelligent-tiering`   | GetBucketIntelligentTieringConfiguration   |
//! | PUT      | `/{bucket}?intelligent-tiering`   | PutBucketIntelligentTieringConfiguration   |
//! | DELETE   | `/{bucket}?intelligent-tiering`   | DeleteBucketIntelligentTieringConfiguration|
//! | GET      | `/{bucket}?object-lock`           | GetObjectLockConfiguration                 |
//! | PUT      | `/{bucket}?object-lock`           | PutObjectLockConfiguration                 |
//! | GET      | `/{bucket}?metrics`               | GetBucketMetricsConfiguration              |
//! | PUT      | `/{bucket}?metrics`               | PutBucketMetricsConfiguration              |
//! | DELETE   | `/{bucket}?metrics`               | DeleteBucketMetricsConfiguration           |
//! | GET      | `/{bucket}?metrics&list`          | ListBucketMetricsConfigurations            |
//! | GET      | `/{bucket}?analytics`             | GetBucketAnalyticsConfiguration            |
//! | PUT      | `/{bucket}?analytics`             | PutBucketAnalyticsConfiguration            |
//! | DELETE   | `/{bucket}?analytics`             | DeleteBucketAnalyticsConfiguration         |
//! | GET      | `/{bucket}?analytics&list`        | ListBucketAnalyticsConfigurations          |
//! | GET      | `/{bucket}?inventory`             | GetBucketInventoryConfiguration            |
//! | PUT      | `/{bucket}?inventory`             | PutBucketInventoryConfiguration            |
//! | DELETE   | `/{bucket}?inventory`             | DeleteBucketInventoryConfiguration         |
//! | GET      | `/{bucket}?inventory&list`        | ListBucketInventoryConfigurations          |
//! | GET      | `/{bucket}/{key}?legal-hold`      | GetObjectLegalHold                         |
//! | PUT      | `/{bucket}/{key}?legal-hold`      | PutObjectLegalHold                         |
//! | GET      | `/{bucket}/{key}?retention`       | GetObjectRetention                         |
//! | PUT      | `/{bucket}/{key}?retention`       | PutObjectRetention                         |
//! | POST     | `/{bucket}/{key}?select`          | SelectObjectContent                        |
//! | GET      | `/{bucket}/{key}?torrent`         | GetObjectTorrent                           |
//! | POST     | `/WriteGetObjectResponse`         | WriteGetObjectResponse                     |

use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use tracing::{info, warn};

use crate::AppState;

use super::handlers::storage_error_to_response;
use super::utils::error_response;

use crate::storage::StorageEngine;

// === Helper functions for common stub patterns ===

/// Helper for bucket delete stubs - returns NO_CONTENT if bucket exists
async fn delete_stub(storage: &StorageEngine, bucket: &str) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => no_such_bucket(bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Helper for bucket put stubs - returns OK if bucket exists (with warning logged externally)
async fn put_stub(storage: &StorageEngine, bucket: &str) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => StatusCode::OK.into_response(),
        Ok(false) => no_such_bucket(bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Helper for bucket get stubs that return "not found" errors
async fn get_not_found_stub(
    storage: &StorageEngine,
    bucket: &str,
    error_code: &str,
    error_msg: &str,
) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => error_response(
            StatusCode::NOT_FOUND,
            error_code,
            error_msg,
            &format!("/{}", bucket),
        ),
        Ok(false) => no_such_bucket(bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Helper for bucket get stubs that return XML responses
async fn get_xml_stub(storage: &StorageEngine, bucket: &str, xml: &str) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => match Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "application/xml")
            .body(Body::from(xml.to_string()))
        {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("Failed to build response: {}", e);
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                )
            }
        },
        Ok(false) => no_such_bucket(bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Standard NoSuchBucket error response
fn no_such_bucket(bucket: &str) -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "NoSuchBucket",
        "The specified bucket does not exist.",
        &format!("/{}", bucket),
    )
}

// === Bucket Encryption Operations (stubs) ===

/// Get bucket encryption configuration (stub - returns ServerSideEncryptionConfigurationNotFoundError)
pub async fn get_bucket_encryption(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketEncryption");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "ServerSideEncryptionConfigurationNotFoundError",
        "The server side encryption configuration was not found",
    )
    .await
}

/// Put bucket encryption configuration (stub - accepts but no-op)
pub async fn put_bucket_encryption(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketEncryption (stub)");
    warn!(bucket = %bucket, "Bucket encryption configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket encryption configuration (stub - no-op)
pub async fn delete_bucket_encryption(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketEncryption (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Bucket Lifecycle Operations (stubs) ===

/// Get bucket lifecycle configuration (stub - returns NoSuchLifecycleConfiguration)
pub async fn get_bucket_lifecycle(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketLifecycleConfiguration");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchLifecycleConfiguration",
        "The lifecycle configuration does not exist",
    )
    .await
}

/// Put bucket lifecycle configuration (stub - accepts but no-op)
pub async fn put_bucket_lifecycle(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketLifecycleConfiguration (stub)");
    warn!(bucket = %bucket, "Bucket lifecycle configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket lifecycle configuration (stub - no-op)
pub async fn delete_bucket_lifecycle(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketLifecycleConfiguration (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Bucket CORS Operations (stubs) ===

/// Get bucket CORS configuration (stub - returns NoSuchCORSConfiguration)
pub async fn get_bucket_cors(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketCors");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchCORSConfiguration",
        "The CORS configuration does not exist",
    )
    .await
}

/// Put bucket CORS configuration (stub - accepts but no-op)
pub async fn put_bucket_cors(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketCors (stub)");
    warn!(bucket = %bucket, "Bucket CORS configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket CORS configuration (stub - no-op)
pub async fn delete_bucket_cors(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketCors (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Bucket Notification Operations (stubs) ===

const EMPTY_NOTIFICATION_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<NotificationConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/"></NotificationConfiguration>"#;

/// Get bucket notification configuration (stub - returns empty configuration)
pub async fn get_bucket_notification(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketNotificationConfiguration");
    get_xml_stub(&state.storage, &bucket, EMPTY_NOTIFICATION_XML).await
}

/// Put bucket notification configuration (stub - accepts but no-op)
pub async fn put_bucket_notification(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketNotificationConfiguration (stub)");
    warn!(bucket = %bucket, "Bucket notification configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

// === Bucket Logging Operations (stubs) ===

const EMPTY_LOGGING_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<BucketLoggingStatus xmlns="http://s3.amazonaws.com/doc/2006-03-01/"></BucketLoggingStatus>"#;

/// Get bucket logging configuration (stub - returns empty logging config)
pub async fn get_bucket_logging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketLogging");
    get_xml_stub(&state.storage, &bucket, EMPTY_LOGGING_XML).await
}

/// Put bucket logging configuration (stub - accepts but no-op)
pub async fn put_bucket_logging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketLogging (stub)");
    warn!(bucket = %bucket, "Bucket logging configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

// === Bucket Request Payment Operations (stubs) ===

const REQUEST_PAYMENT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<RequestPaymentConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Payer>BucketOwner</Payer>
</RequestPaymentConfiguration>"#;

/// Get bucket request payment configuration (stub - returns BucketOwner)
pub async fn get_bucket_request_payment(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketRequestPayment");
    get_xml_stub(&state.storage, &bucket, REQUEST_PAYMENT_XML).await
}

/// Put bucket request payment configuration (stub - accepts but no-op)
pub async fn put_bucket_request_payment(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketRequestPayment (stub)");
    warn!(bucket = %bucket, "Bucket request payment configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

// === Bucket Website Operations (stubs) ===

/// Get bucket website configuration (stub - returns NoSuchWebsiteConfiguration)
pub async fn get_bucket_website(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketWebsite");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchWebsiteConfiguration",
        "The specified bucket does not have a website configuration",
    )
    .await
}

/// Put bucket website configuration (stub - accepts but no-op)
pub async fn put_bucket_website(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketWebsite (stub)");
    warn!(bucket = %bucket, "Bucket website configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket website configuration (stub - no-op)
pub async fn delete_bucket_website(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketWebsite (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Bucket Replication Operations (stubs) ===

/// Get bucket replication configuration (stub - returns ReplicationConfigurationNotFoundError)
pub async fn get_bucket_replication(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketReplication");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "ReplicationConfigurationNotFoundError",
        "The replication configuration was not found",
    )
    .await
}

/// Put bucket replication configuration (stub - accepts but no-op)
pub async fn put_bucket_replication(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketReplication (stub)");
    warn!(bucket = %bucket, "Bucket replication configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket replication configuration (stub - no-op)
pub async fn delete_bucket_replication(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketReplication (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Bucket Accelerate Operations (stubs) ===

const ACCELERATE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccelerateConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Status>Suspended</Status>
</AccelerateConfiguration>"#;

/// Get bucket accelerate configuration (stub - returns Suspended)
pub async fn get_bucket_accelerate(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketAccelerateConfiguration");
    get_xml_stub(&state.storage, &bucket, ACCELERATE_XML).await
}

/// Put bucket accelerate configuration (stub - accepts but no-op)
pub async fn put_bucket_accelerate(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketAccelerateConfiguration (stub)");
    warn!(bucket = %bucket, "Bucket accelerate configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

// === Bucket Ownership Controls Operations (stubs) ===

const OWNERSHIP_CONTROLS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<OwnershipControls xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<Rule><ObjectOwnership>BucketOwnerEnforced</ObjectOwnership></Rule>
</OwnershipControls>"#;

/// Get bucket ownership controls (stub - returns BucketOwnerEnforced)
pub async fn get_bucket_ownership_controls(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketOwnershipControls");
    get_xml_stub(&state.storage, &bucket, OWNERSHIP_CONTROLS_XML).await
}

/// Put bucket ownership controls (stub - accepts but no-op)
pub async fn put_bucket_ownership_controls(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketOwnershipControls (stub)");
    warn!(bucket = %bucket, "Bucket ownership controls accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket ownership controls (stub - no-op)
pub async fn delete_bucket_ownership_controls(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketOwnershipControls (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Public Access Block Operations (stubs) ===

const PUBLIC_ACCESS_BLOCK_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<PublicAccessBlockConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<BlockPublicAcls>true</BlockPublicAcls>
<IgnorePublicAcls>true</IgnorePublicAcls>
<BlockPublicPolicy>true</BlockPublicPolicy>
<RestrictPublicBuckets>true</RestrictPublicBuckets>
</PublicAccessBlockConfiguration>"#;

/// Get public access block configuration (stub - returns all blocked)
pub async fn get_public_access_block(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetPublicAccessBlock");
    get_xml_stub(&state.storage, &bucket, PUBLIC_ACCESS_BLOCK_XML).await
}

/// Put public access block configuration (stub - accepts but no-op)
pub async fn put_public_access_block(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutPublicAccessBlock (stub)");
    warn!(bucket = %bucket, "Public access block configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete public access block configuration (stub - no-op)
pub async fn delete_public_access_block(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeletePublicAccessBlock (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Intelligent Tiering Operations (stubs) ===

/// Get bucket intelligent tiering configuration (stub - returns not found)
pub async fn get_bucket_intelligent_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketIntelligentTieringConfiguration");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchConfiguration",
        "The intelligent tiering configuration does not exist",
    )
    .await
}

/// Put bucket intelligent tiering configuration (stub - accepts but no-op)
pub async fn put_bucket_intelligent_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketIntelligentTieringConfiguration (stub)");
    warn!(bucket = %bucket, "Intelligent tiering configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket intelligent tiering configuration (stub - no-op)
pub async fn delete_bucket_intelligent_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketIntelligentTieringConfiguration (stub)");
    delete_stub(&state.storage, &bucket).await
}

// === Object Legal Hold Operations (stubs) ===

/// Helper for object operations that return ObjectLock errors
async fn object_lock_error_stub(storage: &StorageEngine, bucket: &str, key: &str) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => error_response(
            StatusCode::BAD_REQUEST,
            "InvalidRequest",
            "Object Lock must be enabled to use this operation",
            &format!("/{}/{}", bucket, key),
        ),
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}/{}", bucket, key),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Get object legal hold (stub - returns not found)
pub async fn get_object_legal_hold(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = %bucket, key = %key, "GetObjectLegalHold");
    object_lock_error_stub(&state.storage, &bucket, &key).await
}

/// Put object legal hold (stub - returns error since Object Lock not enabled)
pub async fn put_object_legal_hold(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = %bucket, key = %key, "PutObjectLegalHold (stub)");
    object_lock_error_stub(&state.storage, &bucket, &key).await
}

// === Object Retention Operations (stubs) ===

/// Get object retention (stub - returns error since Object Lock not enabled)
pub async fn get_object_retention(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = %bucket, key = %key, "GetObjectRetention");
    object_lock_error_stub(&state.storage, &bucket, &key).await
}

/// Put object retention (stub - returns error since Object Lock not enabled)
pub async fn put_object_retention(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = %bucket, key = %key, "PutObjectRetention (stub)");
    object_lock_error_stub(&state.storage, &bucket, &key).await
}

// === Object Lock Configuration Operations (stubs) ===

/// Get object lock configuration (stub - returns ObjectLockConfigurationNotFoundError)
pub async fn get_object_lock_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetObjectLockConfiguration");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "ObjectLockConfigurationNotFoundError",
        "Object Lock configuration does not exist for this bucket",
    )
    .await
}

/// Helper for put object lock config that returns conflict error
async fn put_object_lock_stub(storage: &StorageEngine, bucket: &str) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => error_response(
            StatusCode::CONFLICT,
            "InvalidBucketState",
            "Object Lock cannot be enabled on existing buckets",
            &format!("/{}", bucket),
        ),
        Ok(false) => no_such_bucket(bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put object lock configuration (stub - returns error)
pub async fn put_object_lock_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutObjectLockConfiguration (stub)");
    put_object_lock_stub(&state.storage, &bucket).await
}

// === Bucket Metrics Configuration Operations (stubs) ===

/// Get bucket metrics configuration (stub - returns NoSuchConfiguration)
pub async fn get_bucket_metrics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketMetricsConfiguration");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchConfiguration",
        "The metrics configuration does not exist",
    )
    .await
}

/// Put bucket metrics configuration (stub - accepts but no-op)
pub async fn put_bucket_metrics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketMetricsConfiguration (stub)");
    warn!(bucket = %bucket, "Bucket metrics configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket metrics configuration (stub - no-op)
pub async fn delete_bucket_metrics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketMetricsConfiguration (stub)");
    delete_stub(&state.storage, &bucket).await
}

const LIST_METRICS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListMetricsConfigurationsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<IsTruncated>false</IsTruncated>
</ListMetricsConfigurationsResult>"#;

/// List bucket metrics configurations (stub - returns empty list)
pub async fn list_bucket_metrics_configurations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "ListBucketMetricsConfigurations");
    get_xml_stub(&state.storage, &bucket, LIST_METRICS_XML).await
}

// === Bucket Analytics Configuration Operations (stubs) ===

/// Get bucket analytics configuration (stub - returns NoSuchConfiguration)
pub async fn get_bucket_analytics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketAnalyticsConfiguration");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchConfiguration",
        "The analytics configuration does not exist",
    )
    .await
}

/// Put bucket analytics configuration (stub - accepts but no-op)
pub async fn put_bucket_analytics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketAnalyticsConfiguration (stub)");
    warn!(bucket = %bucket, "Bucket analytics configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket analytics configuration (stub - no-op)
pub async fn delete_bucket_analytics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketAnalyticsConfiguration (stub)");
    delete_stub(&state.storage, &bucket).await
}

const LIST_ANALYTICS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListBucketAnalyticsConfigurationsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<IsTruncated>false</IsTruncated>
</ListBucketAnalyticsConfigurationsResult>"#;

/// List bucket analytics configurations (stub - returns empty list)
pub async fn list_bucket_analytics_configurations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "ListBucketAnalyticsConfigurations");
    get_xml_stub(&state.storage, &bucket, LIST_ANALYTICS_XML).await
}

// === Bucket Inventory Configuration Operations (stubs) ===

/// Get bucket inventory configuration (stub - returns NoSuchConfiguration)
pub async fn get_bucket_inventory_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketInventoryConfiguration");
    get_not_found_stub(
        &state.storage,
        &bucket,
        "NoSuchConfiguration",
        "The inventory configuration does not exist",
    )
    .await
}

/// Put bucket inventory configuration (stub - accepts but no-op)
pub async fn put_bucket_inventory_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "PutBucketInventoryConfiguration (stub)");
    warn!(bucket = %bucket, "Bucket inventory configuration accepted but not implemented");
    put_stub(&state.storage, &bucket).await
}

/// Delete bucket inventory configuration (stub - no-op)
pub async fn delete_bucket_inventory_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketInventoryConfiguration (stub)");
    delete_stub(&state.storage, &bucket).await
}

const LIST_INVENTORY_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListInventoryConfigurationsResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
<IsTruncated>false</IsTruncated>
</ListInventoryConfigurationsResult>"#;

/// List bucket inventory configurations (stub - returns empty list)
pub async fn list_bucket_inventory_configurations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "ListBucketInventoryConfigurations");
    get_xml_stub(&state.storage, &bucket, LIST_INVENTORY_XML).await
}

// === SelectObjectContent Operation (stub) ===

/// Helper for object operations that return NotImplemented errors
async fn not_implemented_object_stub(
    storage: &StorageEngine,
    bucket: &str,
    key: &str,
    error_msg: &str,
) -> Response {
    match storage.bucket_exists(bucket).await {
        Ok(true) => error_response(
            StatusCode::NOT_IMPLEMENTED,
            "NotImplemented",
            error_msg,
            &format!("/{}/{}", bucket, key),
        ),
        Ok(false) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// SelectObjectContent - S3 Select API (stub)
///
/// Returns NotImplemented error as S3 Select is not supported.
/// S3 Select allows running SQL queries against CSV, JSON, or Parquet objects.
pub async fn select_object_content(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = %bucket, key = %key, "SelectObjectContent (stub)");
    not_implemented_object_stub(
        &state.storage,
        &bucket,
        &key,
        "S3 Select is not implemented in this gateway.",
    )
    .await
}

// === WriteGetObjectResponse Operation (stub) ===

/// WriteGetObjectResponse - Lambda Object Lambda integration (stub)
///
/// Returns NotImplemented error as Lambda integration is not supported.
/// This operation is used by Lambda functions to return modified objects.
pub async fn write_get_object_response(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let request_route = headers
        .get("x-amz-request-route")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    info!(request_route = %request_route, "WriteGetObjectResponse (stub)");

    error_response(
        StatusCode::NOT_IMPLEMENTED,
        "NotImplemented",
        "Lambda Object Lambda is not implemented in this gateway.",
        "/WriteGetObjectResponse",
    )
}

// === GetObjectTorrent Operation (stub) ===

/// GetObjectTorrent - Get torrent file for object (stub)
///
/// Returns NotImplemented error as BitTorrent support is not available.
pub async fn get_object_torrent(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    info!(bucket = %bucket, key = %key, "GetObjectTorrent (stub)");
    not_implemented_object_stub(
        &state.storage,
        &bucket,
        &key,
        "BitTorrent distribution is not implemented in this gateway.",
    )
    .await
}
