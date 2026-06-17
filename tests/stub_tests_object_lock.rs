#![cfg(feature = "server")]
//! Tests for S3 Object Lock API operations

mod common;

use common::setup_test_server;

/// Test Object Lock configuration at the bucket level
#[tokio::test]
async fn test_object_lock_bucket_level() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("no-lock-{}", uuid::Uuid::new_v4());
    let bucket_name_locked = format!("locked-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // 1. Create bucket WITHOUT object lock (normal create)
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket (no lock) should succeed");

    // PUT ?object-lock on non-lock-enabled bucket → 409 InvalidBucketState
    let response = http_client
        .put(format!("{}/{}?object-lock", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ObjectLockConfiguration>
  <ObjectLockEnabled>Enabled</ObjectLockEnabled>
</ObjectLockConfiguration>"#,
        )
        .send()
        .await
        .expect("put object-lock should complete");
    assert_eq!(
        response.status(),
        409,
        "PutObjectLockConfiguration on non-lock bucket should return 409"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("InvalidBucketState"),
        "Expected InvalidBucketState error, got: {}",
        body
    );

    // 2. Create a second bucket WITH object lock enabled
    let response = http_client
        .put(format!("{}/{}", base_url, bucket_name_locked))
        .header("x-amz-bucket-object-lock-enabled", "true")
        .send()
        .await
        .expect("create locked bucket should complete");
    assert_eq!(
        response.status(),
        200,
        "CreateBucket with object lock should succeed"
    );

    // GET ?object-lock on lock-enabled bucket → 200 with ObjectLockEnabled=Enabled
    let response = http_client
        .get(format!("{}/{}?object-lock", base_url, bucket_name_locked))
        .send()
        .await
        .expect("get object-lock should complete");
    assert_eq!(
        response.status(),
        200,
        "GetObjectLockConfiguration should return 200 on lock-enabled bucket"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("ObjectLockEnabled"),
        "Expected ObjectLockEnabled in response, got: {}",
        body
    );
    assert!(
        body.contains("Enabled"),
        "Expected Enabled status in response, got: {}",
        body
    );

    // PUT ?object-lock on lock-enabled bucket with GOVERNANCE rule → 200
    let response = http_client
        .put(format!("{}/{}?object-lock", base_url, bucket_name_locked))
        .header("Content-Type", "application/xml")
        .body(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ObjectLockConfiguration>
  <ObjectLockEnabled>Enabled</ObjectLockEnabled>
  <Rule><DefaultRetention><Mode>GOVERNANCE</Mode><Days>30</Days></DefaultRetention></Rule>
</ObjectLockConfiguration>"#,
        )
        .send()
        .await
        .expect("put object-lock rule should complete");
    assert_eq!(
        response.status(),
        200,
        "PutObjectLockConfiguration with rule should succeed"
    );

    // GET after PUT — body should contain GOVERNANCE and 30
    let response = http_client
        .get(format!("{}/{}?object-lock", base_url, bucket_name_locked))
        .send()
        .await
        .expect("get object-lock should complete");
    assert_eq!(
        response.status(),
        200,
        "GetObjectLockConfiguration should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("GOVERNANCE"),
        "Expected GOVERNANCE mode in response, got: {}",
        body
    );
    assert!(
        body.contains("30"),
        "Expected 30 days in response, got: {}",
        body
    );

    // Clean up
    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
    client
        .delete_bucket()
        .bucket(&bucket_name_locked)
        .send()
        .await
        .expect("delete_bucket (locked) should succeed");
}

/// Test that a Legal Hold blocks deletion and can be lifted
#[tokio::test]
async fn test_object_lock_legal_hold_blocks_delete() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("legalhold-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // 1. Create bucket with object lock enabled
    let response = http_client
        .put(format!("{}/{}", base_url, bucket_name))
        .header("x-amz-bucket-object-lock-enabled", "true")
        .send()
        .await
        .expect("create locked bucket should complete");
    assert_eq!(
        response.status(),
        200,
        "CreateBucket with object lock should succeed"
    );

    // 2. PUT object
    let response = http_client
        .put(format!("{}/{}/locked-object.txt", base_url, bucket_name))
        .body("hello")
        .send()
        .await
        .expect("put object should complete");
    assert_eq!(response.status(), 200, "PutObject should succeed");

    // 3. PUT legal hold ON
    let response = http_client
        .put(format!(
            "{}/{}/locked-object.txt?legal-hold",
            base_url, bucket_name
        ))
        .header("Content-Type", "application/xml")
        .body("<LegalHold><Status>ON</Status></LegalHold>")
        .send()
        .await
        .expect("put legal-hold should complete");
    assert_eq!(
        response.status(),
        200,
        "PutObjectLegalHold ON should succeed"
    );

    // 4. GET legal hold — should show ON
    let response = http_client
        .get(format!(
            "{}/{}/locked-object.txt?legal-hold",
            base_url, bucket_name
        ))
        .send()
        .await
        .expect("get legal-hold should complete");
    assert_eq!(
        response.status(),
        200,
        "GetObjectLegalHold should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("<Status>ON</Status>"),
        "Expected Status ON in legal-hold response, got: {}",
        body
    );

    // 5. DELETE with legal hold active — should be blocked (403)
    let response = http_client
        .delete(format!("{}/{}/locked-object.txt", base_url, bucket_name))
        .send()
        .await
        .expect("delete object should complete");
    assert_eq!(
        response.status(),
        403,
        "DELETE with active legal hold should return 403"
    );

    // 6. PUT legal hold OFF
    let response = http_client
        .put(format!(
            "{}/{}/locked-object.txt?legal-hold",
            base_url, bucket_name
        ))
        .header("Content-Type", "application/xml")
        .body("<LegalHold><Status>OFF</Status></LegalHold>")
        .send()
        .await
        .expect("put legal-hold OFF should complete");
    assert_eq!(
        response.status(),
        200,
        "PutObjectLegalHold OFF should succeed"
    );

    // 7. DELETE after lifting hold — should succeed (204)
    let response = http_client
        .delete(format!("{}/{}/locked-object.txt", base_url, bucket_name))
        .send()
        .await
        .expect("delete object after lift should complete");
    assert_eq!(
        response.status(),
        204,
        "DELETE after legal hold lifted should return 204"
    );
}

/// Test GOVERNANCE retention enforcement and bypass
#[tokio::test]
async fn test_object_lock_governance_with_bypass() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("governance-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // 1. Create bucket with object lock
    let response = http_client
        .put(format!("{}/{}", base_url, bucket_name))
        .header("x-amz-bucket-object-lock-enabled", "true")
        .send()
        .await
        .expect("create locked bucket should complete");
    assert_eq!(
        response.status(),
        200,
        "CreateBucket with object lock should succeed"
    );

    // 2. PUT object
    let response = http_client
        .put(format!("{}/{}/governed.txt", base_url, bucket_name))
        .body("data")
        .send()
        .await
        .expect("put object should complete");
    assert_eq!(response.status(), 200, "PutObject should succeed");

    // 3. PUT GOVERNANCE retention far in the future
    let response = http_client
        .put(format!("{}/{}/governed.txt?retention", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<Retention><Mode>GOVERNANCE</Mode><RetainUntilDate>2099-01-01T00:00:00.000Z</RetainUntilDate></Retention>"#,
        )
        .send()
        .await
        .expect("put retention should complete");
    assert_eq!(
        response.status(),
        200,
        "PutObjectRetention (GOVERNANCE) should succeed"
    );

    // 4. DELETE without bypass — should be blocked (403)
    let response = http_client
        .delete(format!("{}/{}/governed.txt", base_url, bucket_name))
        .send()
        .await
        .expect("delete without bypass should complete");
    assert_eq!(
        response.status(),
        403,
        "DELETE under GOVERNANCE without bypass should return 403"
    );

    // 5. DELETE with bypass — should succeed (204)
    let response = http_client
        .delete(format!("{}/{}/governed.txt", base_url, bucket_name))
        .header("x-amz-bypass-governance-retention", "true")
        .send()
        .await
        .expect("delete with bypass should complete");
    assert_eq!(
        response.status(),
        204,
        "DELETE under GOVERNANCE with bypass should return 204"
    );
}

/// Test COMPLIANCE retention immutability (cannot shorten, deletion blocked even with bypass)
#[tokio::test]
async fn test_object_lock_compliance_immutable() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("compliance-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // 1. Create bucket with object lock
    let response = http_client
        .put(format!("{}/{}", base_url, bucket_name))
        .header("x-amz-bucket-object-lock-enabled", "true")
        .send()
        .await
        .expect("create locked bucket should complete");
    assert_eq!(
        response.status(),
        200,
        "CreateBucket with object lock should succeed"
    );

    // 2. PUT object
    let response = http_client
        .put(format!("{}/{}/compliant.txt", base_url, bucket_name))
        .body("secure data")
        .send()
        .await
        .expect("put object should complete");
    assert_eq!(response.status(), 200, "PutObject should succeed");

    // 3. PUT COMPLIANCE retention far in the future
    let response = http_client
        .put(format!("{}/{}/compliant.txt?retention", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<Retention><Mode>COMPLIANCE</Mode><RetainUntilDate>2099-01-01T00:00:00.000Z</RetainUntilDate></Retention>"#,
        )
        .send()
        .await
        .expect("put COMPLIANCE retention should complete");
    assert_eq!(
        response.status(),
        200,
        "PutObjectRetention (COMPLIANCE) should succeed"
    );

    // 4. PUT shorter date under COMPLIANCE — should be blocked (403)
    let response = http_client
        .put(format!("{}/{}/compliant.txt?retention", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<Retention><Mode>COMPLIANCE</Mode><RetainUntilDate>2050-01-01T00:00:00.000Z</RetainUntilDate></Retention>"#,
        )
        .send()
        .await
        .expect("put shorter COMPLIANCE retention should complete");
    assert_eq!(
        response.status(),
        403,
        "Shortening COMPLIANCE retention should return 403"
    );

    // 5. DELETE even with bypass — COMPLIANCE blocks deletion (403)
    let response = http_client
        .delete(format!("{}/{}/compliant.txt", base_url, bucket_name))
        .header("x-amz-bypass-governance-retention", "true")
        .send()
        .await
        .expect("delete under COMPLIANCE should complete");
    assert_eq!(
        response.status(),
        403,
        "DELETE under COMPLIANCE (even with bypass) should return 403"
    );

    // 6. PUT longer date — should succeed (200)
    let response = http_client
        .put(format!("{}/{}/compliant.txt?retention", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<Retention><Mode>COMPLIANCE</Mode><RetainUntilDate>2199-01-01T00:00:00.000Z</RetainUntilDate></Retention>"#,
        )
        .send()
        .await
        .expect("put longer COMPLIANCE retention should complete");
    assert_eq!(
        response.status(),
        200,
        "Lengthening COMPLIANCE retention should return 200"
    );
}
