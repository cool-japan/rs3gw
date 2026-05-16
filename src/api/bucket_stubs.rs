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
use bytes::Bytes;
use tracing::info;

use crate::AppState;

use super::handlers::storage_error_to_response;
use super::utils::{
    error_response, malformed_xml_response, parse_accelerate_xml, parse_analytics_xml,
    parse_cors_xml, parse_encryption_xml, parse_intelligent_tiering_xml, parse_inventory_xml,
    parse_legal_hold_xml, parse_lifecycle_xml, parse_logging_xml, parse_metrics_xml,
    parse_notification_xml, parse_object_lock_configuration_xml, parse_ownership_controls_xml,
    parse_public_access_block_xml, parse_replication_xml, parse_request_payment_xml,
    parse_retention_xml, parse_website_xml,
};
use super::xml_responses::{
    AccelerateConfigurationXml, AnalyticsConfigurationXml, BucketLoggingStatusXml,
    CorsConfigurationXml, IntelligentTieringConfigurationXml, InventoryConfigurationXml,
    LegalHoldXml, LifecycleConfigurationXml, ListBucketAnalyticsConfigurationsResultXml,
    ListInventoryConfigurationsResultXml, ListMetricsConfigurationsResultXml,
    MetricsConfigurationXml, NotificationConfigXml, ObjectLockConfigurationXml,
    OwnershipControlsXml, PublicAccessBlockConfigurationXml, ReplicationConfigurationXml,
    RequestPaymentConfigurationXml, RetentionXml, SseConfigurationXml, WebsiteConfigurationXml,
};

use crate::storage::{LoggingConfig, RequestPaymentConfig, StorageEngine, StorageError};

/// Standard NoSuchBucket error response
fn no_such_bucket(bucket: &str) -> Response {
    error_response(
        StatusCode::NOT_FOUND,
        "NoSuchBucket",
        "The specified bucket does not exist.",
        &format!("/{}", bucket),
    )
}

// === Bucket Encryption Operations ===

/// Get bucket encryption configuration
pub async fn get_bucket_encryption(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketEncryption");
    match state.storage.get_bucket_encryption(&bucket).await {
        Ok(cfg) => {
            let xml = SseConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
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
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "ServerSideEncryptionConfigurationNotFoundError",
            "The server side encryption configuration was not found",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put bucket encryption configuration
pub async fn put_bucket_encryption(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketEncryption");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_encryption_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_encryption(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete bucket encryption configuration
pub async fn delete_bucket_encryption(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketEncryption");
    match state.storage.delete_bucket_encryption(&bucket).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Lifecycle Operations ===

pub async fn get_bucket_lifecycle(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketLifecycleConfiguration");
    match state.storage.get_bucket_lifecycle(&bucket).await {
        Ok(cfg) => {
            let xml = LifecycleConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchLifecycleConfiguration",
            "The lifecycle configuration does not exist",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn put_bucket_lifecycle(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketLifecycleConfiguration");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_lifecycle_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_lifecycle(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn delete_bucket_lifecycle(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketLifecycleConfiguration");
    match state.storage.delete_bucket_lifecycle(&bucket).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket CORS Operations ===

/// Get bucket CORS configuration
pub async fn get_bucket_cors(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketCors");
    match state.storage.get_bucket_cors(&bucket).await {
        Ok(cfg) => {
            let xml = CorsConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
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
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchCORSConfiguration",
            "The CORS configuration does not exist",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put bucket CORS configuration
pub async fn put_bucket_cors(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketCors");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_cors_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_cors(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete bucket CORS configuration
pub async fn delete_bucket_cors(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketCors");
    match state.storage.delete_bucket_cors(&bucket).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Notification Operations ===

/// Get bucket notification configuration
pub async fn get_bucket_notification(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketNotificationConfiguration");
    match state.storage.get_bucket_notification(&bucket).await {
        Ok(cfg) => {
            let xml = NotificationConfigXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put bucket notification configuration
pub async fn put_bucket_notification(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketNotificationConfiguration");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_notification_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_notification(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Logging Operations ===

pub async fn get_bucket_logging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketLogging");
    let cfg = match state.storage.get_bucket_logging(&bucket).await {
        Ok(c) => c,
        Err(StorageError::BucketNotFound) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                "The specified bucket does not exist.",
                &format!("/{}", bucket),
            )
        }
        Err(StorageError::NotFound(_)) => LoggingConfig {
            target_bucket: None,
            target_prefix: None,
        },
        Err(e) => return storage_error_to_response(e, &format!("/{}", bucket)),
    };
    let xml = BucketLoggingStatusXml::from_config(&cfg).to_xml();
    match Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml")
        .body(Body::from(xml))
    {
        Ok(resp) => resp,
        Err(_) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            "Failed to build response",
            &format!("/{}", bucket),
        ),
    }
}

pub async fn put_bucket_logging(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketLogging");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_logging_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_logging(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Request Payment Operations ===

pub async fn get_bucket_request_payment(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketRequestPayment");
    let cfg = match state.storage.get_bucket_request_payment(&bucket).await {
        Ok(c) => c,
        Err(StorageError::BucketNotFound) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "NoSuchBucket",
                "The specified bucket does not exist.",
                &format!("/{}", bucket),
            )
        }
        Err(StorageError::NotFound(_)) => RequestPaymentConfig {
            payer: "BucketOwner".to_string(),
        },
        Err(e) => return storage_error_to_response(e, &format!("/{}", bucket)),
    };
    let xml = RequestPaymentConfigurationXml::from_config(&cfg).to_xml();
    match Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/xml")
        .body(Body::from(xml))
    {
        Ok(resp) => resp,
        Err(_) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalError",
            "Failed to build response",
            &format!("/{}", bucket),
        ),
    }
}

pub async fn put_bucket_request_payment(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketRequestPayment");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_request_payment_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state
        .storage
        .put_bucket_request_payment(&bucket, &cfg)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Website Operations ===

pub async fn get_bucket_website(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketWebsite");
    match state.storage.get_bucket_website(&bucket).await {
        Ok(cfg) => {
            let xml = WebsiteConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchWebsiteConfiguration",
            "The specified bucket does not have a website configuration",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn put_bucket_website(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketWebsite");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_website_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_website(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn delete_bucket_website(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketWebsite");
    match state.storage.delete_bucket_website(&bucket).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Replication Operations ===

/// Get bucket replication configuration
pub async fn get_bucket_replication(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketReplication");
    match state.storage.get_bucket_replication(&bucket).await {
        Ok(cfg) => {
            let xml = ReplicationConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "ReplicationConfigurationNotFoundError",
            "The replication configuration was not found",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put bucket replication configuration
pub async fn put_bucket_replication(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketReplication");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_replication_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_replication(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete bucket replication configuration
pub async fn delete_bucket_replication(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketReplication");
    match state.storage.delete_bucket_replication(&bucket).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Accelerate Operations ===

/// Get bucket accelerate configuration
pub async fn get_bucket_accelerate(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketAccelerateConfiguration");
    match state.storage.get_bucket_accelerate(&bucket).await {
        Ok(cfg) => {
            let xml = AccelerateConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put bucket accelerate configuration
pub async fn put_bucket_accelerate(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketAccelerateConfiguration");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_accelerate_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_accelerate(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Ownership Controls Operations ===

pub async fn get_bucket_ownership_controls(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetBucketOwnershipControls");
    match state.storage.get_bucket_ownership_controls(&bucket).await {
        Ok(cfg) => {
            let xml = OwnershipControlsXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "OwnershipControlsNotFoundError",
            "The bucket ownership controls were not found",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn put_bucket_ownership_controls(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketOwnershipControls");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_ownership_controls_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state
        .storage
        .put_bucket_ownership_controls(&bucket, &cfg)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn delete_bucket_ownership_controls(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeleteBucketOwnershipControls");
    match state
        .storage
        .delete_bucket_ownership_controls(&bucket)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Public Access Block Operations ===

pub async fn get_public_access_block(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetPublicAccessBlock");
    match state.storage.get_bucket_public_access_block(&bucket).await {
        Ok(cfg) => {
            let xml = PublicAccessBlockConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchPublicAccessBlockConfiguration",
            "The public access block configuration was not found",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn put_public_access_block(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutPublicAccessBlock");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_public_access_block_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state
        .storage
        .put_bucket_public_access_block(&bucket, &cfg)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

pub async fn delete_public_access_block(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "DeletePublicAccessBlock");
    match state
        .storage
        .delete_bucket_public_access_block(&bucket)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Intelligent Tiering Operations ===

/// Get bucket intelligent tiering configuration
pub async fn get_bucket_intelligent_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "GetBucketIntelligentTieringConfiguration");
    match state
        .storage
        .get_bucket_intelligent_tiering(&bucket, &id)
        .await
    {
        Ok(cfg) => {
            let xml = IntelligentTieringConfigurationXml::from_config(&cfg).to_xml();
            match Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/xml")
                .body(Body::from(xml))
            {
                Ok(resp) => resp,
                Err(_) => error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "Failed to build response",
                    &format!("/{}", bucket),
                ),
            }
        }
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchConfiguration",
            "The intelligent tiering configuration does not exist",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Put bucket intelligent tiering configuration
pub async fn put_bucket_intelligent_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutBucketIntelligentTieringConfiguration");
    let xml_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_intelligent_tiering_xml(xml_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    let id = cfg.id.clone();
    match state
        .storage
        .put_bucket_intelligent_tiering(&bucket, &id, &cfg)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete bucket intelligent tiering configuration
pub async fn delete_bucket_intelligent_tiering(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "DeleteBucketIntelligentTieringConfiguration");
    match state
        .storage
        .delete_bucket_intelligent_tiering(&bucket, &id)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchBucket",
            "The specified bucket does not exist.",
            &format!("/{}", bucket),
        ),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Object Legal Hold Operations ===

/// Get the legal hold status for a specific object version.
pub async fn get_object_legal_hold(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    version_id: Option<String>,
) -> Response {
    info!(bucket = %bucket, key = %key, "GetObjectLegalHold");
    let vid = version_id.unwrap_or_else(|| "null".to_string());
    match state
        .storage
        .object_lock_manager
        .get(&bucket, &key, &vid)
        .await
    {
        Ok(meta) => {
            let status = meta.legal_hold_status.unwrap_or_else(|| "OFF".to_string());
            let xml = LegalHoldXml::from_status(&status).to_xml();
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchObjectLockConfiguration",
            "The specified object does not have an Object Lock configuration.",
            &format!("/{}/{}", bucket, key),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Set the legal hold status for a specific object version.
pub async fn put_object_legal_hold(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    version_id: Option<String>,
    _bypass: bool,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, key = %key, "PutObjectLegalHold");
    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}/{}", bucket, key),
            )
        }
    };
    let status = match parse_legal_hold_xml(body_str) {
        Ok(s) => s,
        Err(msg) => return malformed_xml_response(&msg),
    };
    let vid = version_id.unwrap_or_else(|| "null".to_string());
    match state
        .storage
        .object_lock_manager
        .put_legal_hold(&bucket, &key, &vid, &status)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

// === Object Retention Operations ===

/// Get the retention policy for a specific object version.
pub async fn get_object_retention(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    version_id: Option<String>,
) -> Response {
    info!(bucket = %bucket, key = %key, "GetObjectRetention");
    let vid = version_id.unwrap_or_else(|| "null".to_string());
    match state
        .storage
        .object_lock_manager
        .get(&bucket, &key, &vid)
        .await
    {
        Ok(meta) => match (meta.retention_mode, meta.retain_until_date) {
            (Some(mode), Some(until)) => {
                let xml = RetentionXml::new(&mode, &until).to_xml();
                (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "application/xml")],
                    xml,
                )
                    .into_response()
            }
            _ => error_response(
                StatusCode::NOT_FOUND,
                "NoSuchObjectLockConfiguration",
                "The specified object does not have a retention configuration.",
                &format!("/{}/{}", bucket, key),
            ),
        },
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchObjectLockConfiguration",
            "The specified object does not have an Object Lock configuration.",
            &format!("/{}/{}", bucket, key),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

/// Set the retention policy for a specific object version.
pub async fn put_object_retention(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
    version_id: Option<String>,
    bypass: bool,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, key = %key, bypass = %bypass, "PutObjectRetention");
    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}/{}", bucket, key),
            )
        }
    };
    let (mode, until) = match parse_retention_xml(body_str) {
        Ok(pair) => pair,
        Err(msg) => return malformed_xml_response(&msg),
    };
    let vid = version_id.unwrap_or_else(|| "null".to_string());
    match state
        .storage
        .object_lock_manager
        .put_retention(&bucket, &key, &vid, &mode, until, bypass)
        .await
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::ObjectLocked(ref msg)) => error_response(
            StatusCode::FORBIDDEN,
            "AccessDenied",
            msg,
            &format!("/{}/{}", bucket, key),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}/{}", bucket, key)),
    }
}

// === Object Lock Configuration Operations ===

/// Get the bucket-level Object Lock configuration.
pub async fn get_object_lock_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "GetObjectLockConfiguration");
    match state.storage.get_bucket_object_lock(&bucket).await {
        Ok(cfg) => {
            let xml = ObjectLockConfigurationXml::from_config(&cfg);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "ObjectLockConfigurationNotFoundError",
            "Object Lock configuration does not exist for this bucket",
            &format!("/{}", bucket),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Set the bucket-level Object Lock configuration.
pub async fn put_object_lock_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, "PutObjectLockConfiguration");
    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "MalformedXML",
                "Request body is not valid UTF-8",
                &format!("/{}", bucket),
            )
        }
    };
    let cfg = match parse_object_lock_configuration_xml(body_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    match state.storage.put_bucket_object_lock(&bucket, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::InvalidBucketState(ref msg)) => error_response(
            StatusCode::CONFLICT,
            "InvalidBucketState",
            msg,
            &format!("/{}", bucket),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Metrics Configuration Operations ===

/// Get a specific metrics configuration by ID.
pub async fn get_bucket_metrics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "GetBucketMetricsConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    match state.storage.get_bucket_metrics(&bucket, &id).await {
        Ok(cfg) => {
            let xml = MetricsConfigurationXml::from_config(&cfg);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchConfiguration",
            "The specified metrics configuration does not exist",
            &format!("/{}", bucket),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Persist a metrics configuration.
pub async fn put_bucket_metrics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, id = %id, "PutBucketMetricsConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => return malformed_xml_response("Invalid UTF-8 in request body"),
    };
    let mut cfg = match parse_metrics_xml(body_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    cfg.id = id.clone();
    match state.storage.put_bucket_metrics(&bucket, &id, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete a metrics configuration by ID.
pub async fn delete_bucket_metrics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "DeleteBucketMetricsConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    match state.storage.delete_bucket_metrics(&bucket, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// List all metrics configurations for a bucket.
pub async fn list_bucket_metrics_configurations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "ListBucketMetricsConfigurations");
    match state.storage.list_bucket_metrics(&bucket).await {
        Ok(configs) => {
            let xml = ListMetricsConfigurationsResultXml::from_configs(&configs);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Analytics Configuration Operations ===

/// Get a specific analytics configuration by ID.
pub async fn get_bucket_analytics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "GetBucketAnalyticsConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    match state.storage.get_bucket_analytics(&bucket, &id).await {
        Ok(cfg) => {
            let xml = AnalyticsConfigurationXml::from_config(&cfg);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchConfiguration",
            "The specified analytics configuration does not exist",
            &format!("/{}", bucket),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Persist an analytics configuration.
pub async fn put_bucket_analytics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, id = %id, "PutBucketAnalyticsConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => return malformed_xml_response("Invalid UTF-8 in request body"),
    };
    let mut cfg = match parse_analytics_xml(body_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    cfg.id = id.clone();
    match state.storage.put_bucket_analytics(&bucket, &id, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete an analytics configuration by ID.
pub async fn delete_bucket_analytics_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "DeleteBucketAnalyticsConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    match state.storage.delete_bucket_analytics(&bucket, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// List all analytics configurations for a bucket.
pub async fn list_bucket_analytics_configurations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "ListBucketAnalyticsConfigurations");
    match state.storage.list_bucket_analytics(&bucket).await {
        Ok(configs) => {
            let xml = ListBucketAnalyticsConfigurationsResultXml::from_configs(&configs);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

// === Bucket Inventory Configuration Operations ===

/// Get a specific inventory configuration by ID.
pub async fn get_bucket_inventory_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "GetBucketInventoryConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    match state.storage.get_bucket_inventory(&bucket, &id).await {
        Ok(cfg) => {
            let xml = InventoryConfigurationXml::from_config(&cfg);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::NotFound(_)) => error_response(
            StatusCode::NOT_FOUND,
            "NoSuchConfiguration",
            "The specified inventory configuration does not exist",
            &format!("/{}", bucket),
        ),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Persist an inventory configuration.
pub async fn put_bucket_inventory_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
    body_bytes: Bytes,
) -> Response {
    info!(bucket = %bucket, id = %id, "PutBucketInventoryConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    let body_str = match std::str::from_utf8(&body_bytes) {
        Ok(s) => s,
        Err(_) => return malformed_xml_response("Invalid UTF-8 in request body"),
    };
    let mut cfg = match parse_inventory_xml(body_str) {
        Ok(c) => c,
        Err(msg) => return malformed_xml_response(&msg),
    };
    cfg.id = id.clone();
    match state.storage.put_bucket_inventory(&bucket, &id, &cfg).await {
        Ok(()) => StatusCode::OK.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// Delete an inventory configuration by ID.
pub async fn delete_bucket_inventory_configuration(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    id: String,
) -> Response {
    info!(bucket = %bucket, id = %id, "DeleteBucketInventoryConfiguration");
    if id.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "InvalidArgument",
            "id query parameter is required",
            &format!("/{}", bucket),
        );
    }
    match state.storage.delete_bucket_inventory(&bucket, &id).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
}

/// List all inventory configurations for a bucket.
pub async fn list_bucket_inventory_configurations(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Response {
    info!(bucket = %bucket, "ListBucketInventoryConfigurations");
    match state.storage.list_bucket_inventory(&bucket).await {
        Ok(configs) => {
            let xml = ListInventoryConfigurationsResultXml::from_configs(&configs);
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/xml")],
                xml,
            )
                .into_response()
        }
        Err(StorageError::BucketNotFound) => no_such_bucket(&bucket),
        Err(e) => storage_error_to_response(e, &format!("/{}", bucket)),
    }
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
        "WriteGetObjectResponse is only valid when invoked from inside an S3 Object Lambda function. rs3gw is an S3 gateway, not Object Lambda.",
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
        "GetObjectTorrent was deprecated by AWS and is no longer part of the S3 API. rs3gw does not support BitTorrent retrieval.",
    )
    .await
}
