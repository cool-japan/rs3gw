//! Tests for S3 API stub operations (versioning, ACL, encryption, etc.)

mod common;

use common::setup_test_server;

/// Test bucket versioning round-trip (real persisted implementation)
#[tokio::test]
async fn test_bucket_versioning() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("versioning-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 200 with empty VersioningConfiguration (no Status)
    let response = http_client
        .get(format!("{}/{}?versioning", base_url, bucket_name))
        .send()
        .await
        .expect("get versioning request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketVersioning should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("VersioningConfiguration"),
        "Expected VersioningConfiguration in response, got: {}",
        body
    );
    assert!(
        !body.contains("<Status>"),
        "Fresh bucket should not have Status element, got: {}",
        body
    );

    // PUT Enabled
    let response = http_client
        .put(format!("{}/{}?versioning", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Status>Enabled</Status>
</VersioningConfiguration>"#,
        )
        .send()
        .await
        .expect("put versioning request should complete");
    assert_eq!(response.status(), 200, "PutBucketVersioning should succeed");

    // GET — should return Enabled
    let response = http_client
        .get(format!("{}/{}?versioning", base_url, bucket_name))
        .send()
        .await
        .expect("get versioning request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketVersioning should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Status>Enabled</Status>"),
        "Expected Status Enabled after PUT, got: {}",
        body
    );

    // PUT object so there is at least one versioned object
    let response = http_client
        .put(format!("{}/{}/test-key", base_url, bucket_name))
        .body("hello")
        .send()
        .await
        .expect("put object request should complete");
    assert_eq!(response.status(), 200, "PutObject should succeed");

    // ListObjectVersions — GET /{bucket}?versions
    let response = http_client
        .get(format!("{}/{}?versions", base_url, bucket_name))
        .send()
        .await
        .expect("list object versions request should complete");
    assert_eq!(
        response.status(),
        200,
        "ListObjectVersions should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Version>") || body.contains("ListVersionsResult"),
        "Expected version listing in response, got: {}",
        body
    );

    // PUT Suspended
    let response = http_client
        .put(format!("{}/{}?versioning", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<VersioningConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Status>Suspended</Status>
</VersioningConfiguration>"#,
        )
        .send()
        .await
        .expect("put versioning request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketVersioning (Suspend) should succeed"
    );

    // GET — should return Suspended
    let response = http_client
        .get(format!("{}/{}?versioning", base_url, bucket_name))
        .send()
        .await
        .expect("get versioning request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketVersioning should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Status>Suspended</Status>"),
        "Expected Status Suspended, got: {}",
        body
    );
}

/// Test bucket ACL operations (stubs)
#[tokio::test]
async fn test_bucket_acl() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("acl-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Get bucket ACL - should return FULL_CONTROL for owner
    let response = http_client
        .get(format!("{}/{}?acl", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("AccessControlPolicy"));
    assert!(body.contains("FULL_CONTROL"));

    // Put bucket ACL (stub - accepts but no-op)
    let acl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner>
    <ID>owner-id</ID>
    <DisplayName>owner</DisplayName>
  </Owner>
  <AccessControlList>
    <Grant>
      <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
        <ID>owner-id</ID>
        <DisplayName>owner</DisplayName>
      </Grantee>
      <Permission>FULL_CONTROL</Permission>
    </Grant>
  </AccessControlList>
</AccessControlPolicy>"#;

    let response = http_client
        .put(format!("{}/{}?acl", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(acl_xml)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "PutBucketAcl should succeed");

    // Delete bucket
    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();
}

/// Test object ACL operations (stubs)
#[tokio::test]
async fn test_object_acl() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("obj-acl-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Create object
    client
        .put_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .body(aws_sdk_s3::primitives::ByteStream::from_static(
            b"test content",
        ))
        .send()
        .await
        .unwrap();

    // Get object ACL - should return FULL_CONTROL for owner
    let response = http_client
        .get(format!("{}/{}/test.txt?acl", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("AccessControlPolicy"));
    assert!(body.contains("FULL_CONTROL"));

    // Put object ACL (stub - accepts but no-op)
    let acl_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<AccessControlPolicy xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Owner>
    <ID>owner-id</ID>
    <DisplayName>owner</DisplayName>
  </Owner>
  <AccessControlList>
    <Grant>
      <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="CanonicalUser">
        <ID>owner-id</ID>
        <DisplayName>owner</DisplayName>
      </Grantee>
      <Permission>FULL_CONTROL</Permission>
    </Grant>
  </AccessControlList>
</AccessControlPolicy>"#;

    let response = http_client
        .put(format!("{}/{}/test.txt?acl", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(acl_xml)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "PutObjectAcl should succeed");

    // Clean up
    client
        .delete_object()
        .bucket(&bucket_name)
        .key("test.txt")
        .send()
        .await
        .unwrap();

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();
}

/// Test bucket encryption operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_encryption() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("encrypt-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Get bucket encryption - should return 404 (not configured)
    let response = http_client
        .get(format!("{}/{}?encryption", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "GetBucketEncryption should return 404 when not configured"
    );
    let body = response.text().await.unwrap();
    assert!(
        body.contains("ServerSideEncryptionConfigurationNotFoundError"),
        "Expected ServerSideEncryptionConfigurationNotFoundError error code, got: {}",
        body
    );

    // Put bucket encryption
    let encryption_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ServerSideEncryptionConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <ApplyServerSideEncryptionByDefault>
      <SSEAlgorithm>AES256</SSEAlgorithm>
    </ApplyServerSideEncryptionByDefault>
  </Rule>
</ServerSideEncryptionConfiguration>"#;

    let response = http_client
        .put(format!("{}/{}?encryption", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(encryption_xml)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "PutBucketEncryption should succeed");

    // GET after PUT should return the persisted encryption config
    let response = http_client
        .get(format!("{}/{}?encryption", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "GetBucketEncryption should return 200 after PUT"
    );
    let body = response.text().await.unwrap();
    assert!(
        body.contains("ServerSideEncryptionConfiguration"),
        "Expected ServerSideEncryptionConfiguration in response, got: {}",
        body
    );
    assert!(
        body.contains("AES256"),
        "Expected AES256 algorithm in response, got: {}",
        body
    );

    // Delete bucket encryption
    let response = http_client
        .delete(format!("{}/{}?encryption", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketEncryption should return 204"
    );

    // GET after DELETE should return 404 again
    let response = http_client
        .get(format!("{}/{}?encryption", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "GetBucketEncryption should return 404 after DELETE"
    );

    // Delete bucket
    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();
}

/// Test bucket lifecycle operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_lifecycle() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("lifecycle-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // DELETE on fresh bucket — idempotent, no config yet
    let response = http_client
        .delete(format!("{}/{}?lifecycle", base_url, bucket_name))
        .send()
        .await
        .expect("delete lifecycle request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketLifecycle on fresh bucket should return 204"
    );

    // GET on fresh bucket — 404 with NoSuchLifecycleConfiguration
    let response = http_client
        .get(format!("{}/{}?lifecycle", base_url, bucket_name))
        .send()
        .await
        .expect("get lifecycle request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketLifecycle should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchLifecycleConfiguration"),
        "Expected NoSuchLifecycleConfiguration, got: {}",
        body
    );

    let lifecycle_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<LifecycleConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule>
    <ID>rule1</ID>
    <Status>Enabled</Status>
    <Expiration>
      <Days>30</Days>
    </Expiration>
  </Rule>
</LifecycleConfiguration>"#;

    let response = http_client
        .put(format!("{}/{}?lifecycle", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(lifecycle_xml)
        .send()
        .await
        .expect("put lifecycle request should complete");
    assert_eq!(response.status(), 200, "PutBucketLifecycle should succeed");

    // GET after PUT — 200 with persisted config
    let response = http_client
        .get(format!("{}/{}?lifecycle", base_url, bucket_name))
        .send()
        .await
        .expect("get lifecycle request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketLifecycle should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Status>Enabled</Status>"),
        "Expected Status Enabled in response, got: {}",
        body
    );
    assert!(
        body.contains("<Days>30</Days>"),
        "Expected Days 30 in response, got: {}",
        body
    );

    let response = http_client
        .delete(format!("{}/{}?lifecycle", base_url, bucket_name))
        .send()
        .await
        .expect("delete lifecycle request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketLifecycle should return 204"
    );

    // GET after DELETE — 404 again
    let response = http_client
        .get(format!("{}/{}?lifecycle", base_url, bucket_name))
        .send()
        .await
        .expect("get lifecycle request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketLifecycle should return 404 after DELETE"
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket CORS operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_cors() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("cors-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Get bucket CORS - should return 404 (not configured)
    let response = http_client
        .get(format!("{}/{}?cors", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "GetBucketCors should return 404 when not configured"
    );
    let body = response.text().await.unwrap();
    assert!(
        body.contains("NoSuchCORSConfiguration"),
        "Expected NoSuchCORSConfiguration error code, got: {}",
        body
    );

    // Put bucket CORS
    let cors_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<CORSConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <CORSRule>
    <AllowedOrigin>https://example.com</AllowedOrigin>
    <AllowedMethod>GET</AllowedMethod>
    <AllowedMethod>PUT</AllowedMethod>
    <AllowedHeader>Content-Type</AllowedHeader>
    <MaxAgeSeconds>3600</MaxAgeSeconds>
  </CORSRule>
</CORSConfiguration>"#;

    let response = http_client
        .put(format!("{}/{}?cors", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(cors_xml)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200, "PutBucketCors should succeed");

    // GET after PUT should return the persisted CORS config
    let response = http_client
        .get(format!("{}/{}?cors", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        200,
        "GetBucketCors should return 200 after PUT"
    );
    let body = response.text().await.unwrap();
    assert!(
        body.contains("CORSConfiguration"),
        "Expected CORSConfiguration in response, got: {}",
        body
    );
    assert!(
        body.contains("AllowedOrigin"),
        "Expected AllowedOrigin in response, got: {}",
        body
    );
    assert!(
        body.contains("https://example.com"),
        "Expected origin in response, got: {}",
        body
    );

    // Delete bucket CORS
    let response = http_client
        .delete(format!("{}/{}?cors", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204, "DeleteBucketCors should return 204");

    // GET after DELETE should return 404 again
    let response = http_client
        .get(format!("{}/{}?cors", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        404,
        "GetBucketCors should return 404 after DELETE"
    );

    // Delete bucket
    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();
}

/// Test bucket logging operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_logging() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("logging-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 200 with empty BucketLoggingStatus (AWS semantics: no 404)
    let response = http_client
        .get(format!("{}/{}?logging", base_url, bucket_name))
        .send()
        .await
        .expect("get logging request should complete");
    assert_eq!(response.status(), 200, "GetBucketLogging should return 200");
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("BucketLoggingStatus"),
        "Expected BucketLoggingStatus in empty response, got: {}",
        body
    );
    assert!(
        !body.contains("LoggingEnabled"),
        "Fresh bucket should not have LoggingEnabled, got: {}",
        body
    );

    let logging_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<BucketLoggingStatus xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <LoggingEnabled>
    <TargetBucket>mybucket</TargetBucket>
    <TargetPrefix>logs/</TargetPrefix>
  </LoggingEnabled>
</BucketLoggingStatus>"#;

    let response = http_client
        .put(format!("{}/{}?logging", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(logging_xml)
        .send()
        .await
        .expect("put logging request should complete");
    assert_eq!(response.status(), 200, "PutBucketLogging should succeed");

    // GET after PUT — 200 with TargetBucket in body
    let response = http_client
        .get(format!("{}/{}?logging", base_url, bucket_name))
        .send()
        .await
        .expect("get logging request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketLogging should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<TargetBucket>mybucket</TargetBucket>"),
        "Expected TargetBucket in response, got: {}",
        body
    );

    // PUT empty body disables logging
    let response = http_client
        .put(format!("{}/{}?logging", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body("<BucketLoggingStatus/>")
        .send()
        .await
        .expect("put empty logging request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketLogging with empty body should return 200"
    );

    // GET after disabling — 200 with no LoggingEnabled element
    let response = http_client
        .get(format!("{}/{}?logging", base_url, bucket_name))
        .send()
        .await
        .expect("get logging request should complete");
    assert_eq!(response.status(), 200, "GetBucketLogging should return 200");
    let body = response.text().await.expect("read response body");
    assert!(
        !body.contains("LoggingEnabled"),
        "After disabling, should not have LoggingEnabled, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket notification round-trip (real persisted implementation)
#[tokio::test]
async fn test_bucket_notification() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("notification-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 200 with empty NotificationConfiguration
    let response = http_client
        .get(format!("{}/{}?notification", base_url, bucket_name))
        .send()
        .await
        .expect("get notification request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketNotification should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NotificationConfiguration"),
        "Expected NotificationConfiguration in response, got: {}",
        body
    );

    // PUT with a TopicConfiguration
    let notification_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<NotificationConfiguration>
  <TopicConfiguration>
    <Id>test-topic</Id>
    <Topic>arn:aws:sns:us-east-1:123456789:test</Topic>
    <Event>s3:ObjectCreated:*</Event>
  </TopicConfiguration>
</NotificationConfiguration>"#;

    let response = http_client
        .put(format!("{}/{}?notification", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(notification_xml)
        .send()
        .await
        .expect("put notification request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketNotification should succeed"
    );

    // GET after PUT — body should contain test-topic
    let response = http_client
        .get(format!("{}/{}?notification", base_url, bucket_name))
        .send()
        .await
        .expect("get notification request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketNotification should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("test-topic"),
        "Expected test-topic in notification response, got: {}",
        body
    );

    // PUT empty to disable
    let response = http_client
        .put(format!("{}/{}?notification", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body("<NotificationConfiguration/>")
        .send()
        .await
        .expect("put empty notification request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketNotification (empty) should succeed"
    );

    // GET after empty PUT — should return empty-ish NotificationConfiguration
    let response = http_client
        .get(format!("{}/{}?notification", base_url, bucket_name))
        .send()
        .await
        .expect("get notification request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketNotification should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NotificationConfiguration"),
        "Expected NotificationConfiguration after clearing, got: {}",
        body
    );
    assert!(
        !body.contains("test-topic"),
        "test-topic should be gone after clearing, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket request payment operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_request_payment() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("reqpay-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 200 with BucketOwner default
    let response = http_client
        .get(format!("{}/{}?requestPayment", base_url, bucket_name))
        .send()
        .await
        .expect("get requestPayment request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketRequestPayment should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("RequestPaymentConfiguration"),
        "Expected RequestPaymentConfiguration in response, got: {}",
        body
    );
    assert!(
        body.contains("<Payer>BucketOwner</Payer>"),
        "Expected BucketOwner payer by default, got: {}",
        body
    );

    // PUT Requester
    let response = http_client
        .put(format!("{}/{}?requestPayment", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<RequestPaymentConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Payer>Requester</Payer>
</RequestPaymentConfiguration>"#,
        )
        .send()
        .await
        .expect("put requestPayment request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketRequestPayment should succeed"
    );

    // GET — should return Requester
    let response = http_client
        .get(format!("{}/{}?requestPayment", base_url, bucket_name))
        .send()
        .await
        .expect("get requestPayment request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketRequestPayment should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Payer>Requester</Payer>"),
        "Expected Requester payer after PUT, got: {}",
        body
    );

    // PUT back to BucketOwner
    let response = http_client
        .put(format!("{}/{}?requestPayment", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<RequestPaymentConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Payer>BucketOwner</Payer>
</RequestPaymentConfiguration>"#,
        )
        .send()
        .await
        .expect("put requestPayment request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketRequestPayment should succeed"
    );

    // GET — should return BucketOwner again
    let response = http_client
        .get(format!("{}/{}?requestPayment", base_url, bucket_name))
        .send()
        .await
        .expect("get requestPayment request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketRequestPayment should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Payer>BucketOwner</Payer>"),
        "Expected BucketOwner payer after restore, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket website operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_website() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("website-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 404 with NoSuchWebsiteConfiguration
    let response = http_client
        .get(format!("{}/{}?website", base_url, bucket_name))
        .send()
        .await
        .expect("get website request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketWebsite should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchWebsiteConfiguration"),
        "Expected NoSuchWebsiteConfiguration, got: {}",
        body
    );

    let response = http_client
        .put(format!("{}/{}?website", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<WebsiteConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <IndexDocument><Suffix>index.html</Suffix></IndexDocument>
  <ErrorDocument><Key>error.html</Key></ErrorDocument>
</WebsiteConfiguration>"#,
        )
        .send()
        .await
        .expect("put website request should complete");
    assert_eq!(response.status(), 200, "PutBucketWebsite should succeed");

    // GET after PUT — 200 with IndexDocument
    let response = http_client
        .get(format!("{}/{}?website", base_url, bucket_name))
        .send()
        .await
        .expect("get website request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketWebsite should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Suffix>index.html</Suffix>"),
        "Expected index.html in response, got: {}",
        body
    );

    let response = http_client
        .delete(format!("{}/{}?website", base_url, bucket_name))
        .send()
        .await
        .expect("delete website request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketWebsite should return 204"
    );

    // GET after DELETE — 404 again
    let response = http_client
        .get(format!("{}/{}?website", base_url, bucket_name))
        .send()
        .await
        .expect("get website request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketWebsite should return 404 after DELETE"
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket ownership controls operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_ownership_controls() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("ownership-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 404 with OwnershipControlsNotFoundError
    let response = http_client
        .get(format!("{}/{}?ownershipControls", base_url, bucket_name))
        .send()
        .await
        .expect("get ownershipControls request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketOwnershipControls should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("OwnershipControlsNotFoundError"),
        "Expected OwnershipControlsNotFoundError, got: {}",
        body
    );

    let response = http_client
        .put(format!("{}/{}?ownershipControls", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<OwnershipControls xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <Rule><ObjectOwnership>BucketOwnerEnforced</ObjectOwnership></Rule>
</OwnershipControls>"#,
        )
        .send()
        .await
        .expect("put ownershipControls request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketOwnershipControls should succeed"
    );

    // GET after PUT — 200 with ObjectOwnership
    let response = http_client
        .get(format!("{}/{}?ownershipControls", base_url, bucket_name))
        .send()
        .await
        .expect("get ownershipControls request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketOwnershipControls should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<ObjectOwnership>BucketOwnerEnforced</ObjectOwnership>"),
        "Expected BucketOwnerEnforced in response, got: {}",
        body
    );

    let response = http_client
        .delete(format!("{}/{}?ownershipControls", base_url, bucket_name))
        .send()
        .await
        .expect("delete ownershipControls request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketOwnershipControls should return 204"
    );

    // GET after DELETE — 404 with OwnershipControlsNotFoundError
    let response = http_client
        .get(format!("{}/{}?ownershipControls", base_url, bucket_name))
        .send()
        .await
        .expect("get ownershipControls request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketOwnershipControls should return 404 after DELETE"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("OwnershipControlsNotFoundError"),
        "Expected OwnershipControlsNotFoundError after DELETE, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket public access block operations (real persisted implementation)
#[tokio::test]
async fn test_bucket_public_access_block() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("pubblock-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 404 with NoSuchPublicAccessBlockConfiguration
    let response = http_client
        .get(format!("{}/{}?publicAccessBlock", base_url, bucket_name))
        .send()
        .await
        .expect("get publicAccessBlock request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetPublicAccessBlock should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchPublicAccessBlockConfiguration"),
        "Expected NoSuchPublicAccessBlockConfiguration, got: {}",
        body
    );

    let response = http_client
        .put(format!("{}/{}?publicAccessBlock", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<PublicAccessBlockConfiguration xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <BlockPublicAcls>true</BlockPublicAcls>
  <IgnorePublicAcls>true</IgnorePublicAcls>
  <BlockPublicPolicy>true</BlockPublicPolicy>
  <RestrictPublicBuckets>true</RestrictPublicBuckets>
</PublicAccessBlockConfiguration>"#,
        )
        .send()
        .await
        .expect("put publicAccessBlock request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutPublicAccessBlock should succeed"
    );

    // GET after PUT — 200 with all four fields true
    let response = http_client
        .get(format!("{}/{}?publicAccessBlock", base_url, bucket_name))
        .send()
        .await
        .expect("get publicAccessBlock request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetPublicAccessBlock should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<BlockPublicAcls>true</BlockPublicAcls>"),
        "Expected BlockPublicAcls true, got: {}",
        body
    );
    assert!(
        body.contains("<IgnorePublicAcls>true</IgnorePublicAcls>"),
        "Expected IgnorePublicAcls true, got: {}",
        body
    );
    assert!(
        body.contains("<BlockPublicPolicy>true</BlockPublicPolicy>"),
        "Expected BlockPublicPolicy true, got: {}",
        body
    );
    assert!(
        body.contains("<RestrictPublicBuckets>true</RestrictPublicBuckets>"),
        "Expected RestrictPublicBuckets true, got: {}",
        body
    );

    let response = http_client
        .delete(format!("{}/{}?publicAccessBlock", base_url, bucket_name))
        .send()
        .await
        .expect("delete publicAccessBlock request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeletePublicAccessBlock should return 204"
    );

    // GET after DELETE — 404 with NoSuchPublicAccessBlockConfiguration
    let response = http_client
        .get(format!("{}/{}?publicAccessBlock", base_url, bucket_name))
        .send()
        .await
        .expect("get publicAccessBlock request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetPublicAccessBlock should return 404 after DELETE"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchPublicAccessBlockConfiguration"),
        "Expected NoSuchPublicAccessBlockConfiguration after DELETE, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket replication round-trip + delete (real persisted implementation)
#[tokio::test]
async fn test_bucket_replication() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("replication-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 404 with ReplicationConfigurationNotFoundError
    let response = http_client
        .get(format!("{}/{}?replication", base_url, bucket_name))
        .send()
        .await
        .expect("get replication request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketReplication should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("ReplicationConfigurationNotFoundError"),
        "Expected ReplicationConfigurationNotFoundError, got: {}",
        body
    );

    // PUT replication configuration
    let response = http_client
        .put(format!("{}/{}?replication", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ReplicationConfiguration>
  <Role>arn:aws:iam::123456789:role/replication-role</Role>
  <Rule>
    <ID>rule-1</ID>
    <Status>Enabled</Status>
    <Destination><Bucket>arn:aws:s3:::dest-bucket</Bucket></Destination>
  </Rule>
</ReplicationConfiguration>"#,
        )
        .send()
        .await
        .expect("put replication request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketReplication should succeed"
    );

    // GET after PUT — body should contain replication-role
    let response = http_client
        .get(format!("{}/{}?replication", base_url, bucket_name))
        .send()
        .await
        .expect("get replication request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketReplication should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("replication-role"),
        "Expected replication-role in response, got: {}",
        body
    );

    // DELETE replication configuration
    let response = http_client
        .delete(format!("{}/{}?replication", base_url, bucket_name))
        .send()
        .await
        .expect("delete replication request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketReplication should return 204"
    );

    // GET after DELETE — 404 again
    let response = http_client
        .get(format!("{}/{}?replication", base_url, bucket_name))
        .send()
        .await
        .expect("get replication request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketReplication should return 404 after DELETE"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("ReplicationConfigurationNotFoundError"),
        "Expected ReplicationConfigurationNotFoundError after DELETE, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket accelerate configuration round-trip (real persisted implementation)
#[tokio::test]
async fn test_bucket_accelerate() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("accelerate-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 200 with Suspended (default)
    let response = http_client
        .get(format!("{}/{}?accelerate", base_url, bucket_name))
        .send()
        .await
        .expect("get accelerate request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketAccelerate should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("Suspended"),
        "Expected Suspended in default accelerate response, got: {}",
        body
    );

    // PUT Enabled
    let response = http_client
        .put(format!("{}/{}?accelerate", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?><AccelerateConfiguration><Status>Enabled</Status></AccelerateConfiguration>"#,
        )
        .send()
        .await
        .expect("put accelerate request should complete");
    assert_eq!(response.status(), 200, "PutBucketAccelerate should succeed");

    // GET after PUT Enabled — body should contain Enabled
    let response = http_client
        .get(format!("{}/{}?accelerate", base_url, bucket_name))
        .send()
        .await
        .expect("get accelerate request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketAccelerate should return 200 after PUT Enabled"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("Enabled"),
        "Expected Enabled in accelerate response, got: {}",
        body
    );

    // PUT Suspended
    let response = http_client
        .put(format!("{}/{}?accelerate", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?><AccelerateConfiguration><Status>Suspended</Status></AccelerateConfiguration>"#,
        )
        .send()
        .await
        .expect("put accelerate suspended request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketAccelerate (Suspended) should succeed"
    );

    // GET after PUT Suspended — body should contain Suspended
    let response = http_client
        .get(format!("{}/{}?accelerate", base_url, bucket_name))
        .send()
        .await
        .expect("get accelerate request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketAccelerate should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("Suspended"),
        "Expected Suspended in accelerate response after restore, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}

/// Test bucket intelligent tiering configuration round-trip (real persisted implementation)
#[tokio::test]
async fn test_bucket_intelligent_tiering() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("intelligent-tiering-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // GET on fresh bucket — 404 NoSuchConfiguration
    let response = http_client
        .get(format!(
            "{}/{}?intelligent-tiering&id=tier-1",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("get intelligent-tiering request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketIntelligentTiering should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchConfiguration"),
        "Expected NoSuchConfiguration, got: {}",
        body
    );

    // PUT intelligent tiering configuration
    let response = http_client
        .put(format!(
            "{}/{}?intelligent-tiering&id=tier-1",
            base_url, bucket_name
        ))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<IntelligentTieringConfiguration>
  <Id>tier-1</Id>
  <Status>Enabled</Status>
  <Tiering><Days>90</Days><AccessTier>ARCHIVE_ACCESS</AccessTier></Tiering>
</IntelligentTieringConfiguration>"#,
        )
        .send()
        .await
        .expect("put intelligent-tiering request should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketIntelligentTiering should succeed"
    );

    // GET after PUT — body should contain tier-1
    let response = http_client
        .get(format!(
            "{}/{}?intelligent-tiering&id=tier-1",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("get intelligent-tiering request should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketIntelligentTiering should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("tier-1"),
        "Expected tier-1 in intelligent-tiering response, got: {}",
        body
    );

    // DELETE
    let response = http_client
        .delete(format!(
            "{}/{}?intelligent-tiering&id=tier-1",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("delete intelligent-tiering request should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketIntelligentTiering should return 204"
    );

    // GET after DELETE — 404 again
    let response = http_client
        .get(format!(
            "{}/{}?intelligent-tiering&id=tier-1",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("get intelligent-tiering request should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketIntelligentTiering should return 404 after DELETE"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchConfiguration"),
        "Expected NoSuchConfiguration after DELETE, got: {}",
        body
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}
