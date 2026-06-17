#![cfg(feature = "server")]
//! Tests for S3 ID-keyed bucket configuration APIs (metrics, analytics, inventory)

mod common;

use common::setup_test_server;

/// Test ID-keyed metrics, analytics, and inventory config round-trip with List
#[tokio::test]
async fn test_metrics_analytics_inventory_id_keyed() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let bucket_name = format!("mai-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("create_bucket should succeed");

    // ---- METRICS ----

    // GET ?metrics&id=m1 — 404 NoSuchConfiguration
    let response = http_client
        .get(format!("{}/{}?metrics&id=m1", base_url, bucket_name))
        .send()
        .await
        .expect("get metrics m1 should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketMetrics(m1) should return 404 when not configured"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("NoSuchConfiguration"),
        "Expected NoSuchConfiguration, got: {}",
        body
    );

    // PUT ?metrics&id=m1
    let response = http_client
        .put(format!("{}/{}?metrics&id=m1", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body("<MetricsConfiguration><Id>m1</Id></MetricsConfiguration>")
        .send()
        .await
        .expect("put metrics m1 should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketMetrics(m1) should succeed"
    );

    // PUT ?metrics&id=m2
    let response = http_client
        .put(format!("{}/{}?metrics&id=m2", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body("<MetricsConfiguration><Id>m2</Id></MetricsConfiguration>")
        .send()
        .await
        .expect("put metrics m2 should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketMetrics(m2) should succeed"
    );

    // GET ?metrics&id=m1 — 200, body contains m1
    let response = http_client
        .get(format!("{}/{}?metrics&id=m1", base_url, bucket_name))
        .send()
        .await
        .expect("get metrics m1 should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketMetrics(m1) should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("m1"),
        "Expected m1 in metrics response, got: {}",
        body
    );

    // GET ?metrics (List) — 200, body contains both m1 and m2
    let response = http_client
        .get(format!("{}/{}?metrics", base_url, bucket_name))
        .send()
        .await
        .expect("list metrics should complete");
    assert_eq!(
        response.status(),
        200,
        "ListBucketMetrics should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("m1"),
        "Expected m1 in list metrics response, got: {}",
        body
    );
    assert!(
        body.contains("m2"),
        "Expected m2 in list metrics response, got: {}",
        body
    );

    // DELETE ?metrics&id=m1
    let response = http_client
        .delete(format!("{}/{}?metrics&id=m1", base_url, bucket_name))
        .send()
        .await
        .expect("delete metrics m1 should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketMetrics(m1) should return 204"
    );

    // GET ?metrics (List) — 200, m2 present, m1 absent
    let response = http_client
        .get(format!("{}/{}?metrics", base_url, bucket_name))
        .send()
        .await
        .expect("list metrics after delete should complete");
    assert_eq!(
        response.status(),
        200,
        "ListBucketMetrics should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("m2"),
        "Expected m2 in list metrics response after delete m1, got: {}",
        body
    );
    assert!(
        !body.contains(">m1<"),
        "m1 should be absent from list after delete, got: {}",
        body
    );

    // GET ?metrics&id=m1 after delete — 404
    let response = http_client
        .get(format!("{}/{}?metrics&id=m1", base_url, bucket_name))
        .send()
        .await
        .expect("get metrics m1 after delete should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketMetrics(m1) should return 404 after DELETE"
    );

    // ---- ANALYTICS ----

    // GET ?analytics&id=a1 — 404
    let response = http_client
        .get(format!("{}/{}?analytics&id=a1", base_url, bucket_name))
        .send()
        .await
        .expect("get analytics a1 should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketAnalytics(a1) should return 404 when not configured"
    );

    // PUT ?analytics&id=a1
    let response = http_client
        .put(format!("{}/{}?analytics&id=a1", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body("<AnalyticsConfiguration><Id>a1</Id></AnalyticsConfiguration>")
        .send()
        .await
        .expect("put analytics a1 should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketAnalytics(a1) should succeed"
    );

    // GET ?analytics&id=a1 — 200, body contains a1
    let response = http_client
        .get(format!("{}/{}?analytics&id=a1", base_url, bucket_name))
        .send()
        .await
        .expect("get analytics a1 should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketAnalytics(a1) should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("a1"),
        "Expected a1 in analytics response, got: {}",
        body
    );

    // GET ?analytics (List) — 200, contains a1
    let response = http_client
        .get(format!("{}/{}?analytics", base_url, bucket_name))
        .send()
        .await
        .expect("list analytics should complete");
    assert_eq!(
        response.status(),
        200,
        "ListBucketAnalytics should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("a1"),
        "Expected a1 in list analytics response, got: {}",
        body
    );

    // DELETE ?analytics&id=a1
    let response = http_client
        .delete(format!("{}/{}?analytics&id=a1", base_url, bucket_name))
        .send()
        .await
        .expect("delete analytics a1 should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketAnalytics(a1) should return 204"
    );

    // GET ?analytics&id=a1 after delete — 404
    let response = http_client
        .get(format!("{}/{}?analytics&id=a1", base_url, bucket_name))
        .send()
        .await
        .expect("get analytics a1 after delete should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketAnalytics(a1) should return 404 after DELETE"
    );

    // ---- INVENTORY ----

    // GET ?inventory&id=i1 — 404
    let response = http_client
        .get(format!("{}/{}?inventory&id=i1", base_url, bucket_name))
        .send()
        .await
        .expect("get inventory i1 should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketInventory(i1) should return 404 when not configured"
    );

    // PUT ?inventory&id=i1
    let response = http_client
        .put(format!("{}/{}?inventory&id=i1", base_url, bucket_name))
        .header("Content-Type", "application/xml")
        .body(
            r#"<InventoryConfiguration>
  <Id>i1</Id>
  <IsEnabled>true</IsEnabled>
  <IncludedObjectVersions>All</IncludedObjectVersions>
  <Schedule><Frequency>Daily</Frequency></Schedule>
  <Destination><S3BucketDestination>
    <Bucket>arn:aws:s3:::inventory-dest</Bucket>
    <Format>CSV</Format>
  </S3BucketDestination></Destination>
</InventoryConfiguration>"#,
        )
        .send()
        .await
        .expect("put inventory i1 should complete");
    assert_eq!(
        response.status(),
        200,
        "PutBucketInventory(i1) should succeed"
    );

    // GET ?inventory&id=i1 — 200, body contains i1
    let response = http_client
        .get(format!("{}/{}?inventory&id=i1", base_url, bucket_name))
        .send()
        .await
        .expect("get inventory i1 should complete");
    assert_eq!(
        response.status(),
        200,
        "GetBucketInventory(i1) should return 200 after PUT"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("i1"),
        "Expected i1 in inventory response, got: {}",
        body
    );

    // GET ?inventory (List) — 200, contains i1
    let response = http_client
        .get(format!("{}/{}?inventory", base_url, bucket_name))
        .send()
        .await
        .expect("list inventory should complete");
    assert_eq!(
        response.status(),
        200,
        "ListBucketInventory should return 200"
    );
    let body = response.text().await.expect("read response body");
    assert!(
        body.contains("i1"),
        "Expected i1 in list inventory response, got: {}",
        body
    );

    // DELETE ?inventory&id=i1
    let response = http_client
        .delete(format!("{}/{}?inventory&id=i1", base_url, bucket_name))
        .send()
        .await
        .expect("delete inventory i1 should complete");
    assert_eq!(
        response.status(),
        204,
        "DeleteBucketInventory(i1) should return 204"
    );

    // GET ?inventory&id=i1 after delete — 404
    let response = http_client
        .get(format!("{}/{}?inventory&id=i1", base_url, bucket_name))
        .send()
        .await
        .expect("get inventory i1 after delete should complete");
    assert_eq!(
        response.status(),
        404,
        "GetBucketInventory(i1) should return 404 after DELETE"
    );

    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .expect("delete_bucket should succeed");
}
