//! Bucket operation tests for rs3gw

mod common;

use common::{setup_test_server, setup_test_server_with_auth};

#[tokio::test]
async fn test_bucket_operations() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create a bucket
    let create_result = client.create_bucket().bucket("test-bucket").send().await;
    assert!(
        create_result.is_ok(),
        "Failed to create bucket: {:?}",
        create_result.err()
    );

    // Head bucket (check it exists)
    let head_result = client.head_bucket().bucket("test-bucket").send().await;
    assert!(head_result.is_ok(), "Failed to head bucket");

    // List buckets
    let list_result = client.list_buckets().send().await;
    assert!(list_result.is_ok(), "Failed to list buckets");
    let list_output = list_result.unwrap();
    let buckets = list_output.buckets();
    assert!(buckets.iter().any(|b| b.name() == Some("test-bucket")));

    // Delete bucket
    let delete_result = client.delete_bucket().bucket("test-bucket").send().await;
    assert!(delete_result.is_ok(), "Failed to delete bucket");
}

#[tokio::test]
async fn test_bucket_tagging() {
    let (client, _temp_dir, _server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("bucket-tagging-test")
        .send()
        .await
        .unwrap();

    // Get tagging (should be empty initially)
    let get_result = client
        .get_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    // Note: AWS returns an error for no tags, but we return empty for simplicity
    // Either is acceptable - check for success or empty tag set
    if let Ok(tagging) = get_result {
        assert!(tagging.tag_set().is_empty());
    }

    // Put tagging
    use aws_sdk_s3::types::{Tag, Tagging};
    let tag1 = Tag::builder()
        .key("Environment")
        .value("Test")
        .build()
        .unwrap();
    let tag2 = Tag::builder().key("Owner").value("rs3gw").build().unwrap();
    let tagging = Tagging::builder()
        .tag_set(tag1)
        .tag_set(tag2)
        .build()
        .unwrap();

    let put_result = client
        .put_bucket_tagging()
        .bucket("bucket-tagging-test")
        .tagging(tagging)
        .send()
        .await;
    assert!(
        put_result.is_ok(),
        "Failed to put bucket tagging: {:?}",
        put_result.err()
    );

    // Get tagging (should have 2 tags now)
    let get_result = client
        .get_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    assert!(
        get_result.is_ok(),
        "Failed to get bucket tagging: {:?}",
        get_result.err()
    );
    let tags = get_result.unwrap();
    let tag_set = tags.tag_set();
    assert_eq!(tag_set.len(), 2);

    // Delete tagging
    let delete_result = client
        .delete_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    assert!(delete_result.is_ok(), "Failed to delete bucket tagging");

    // Verify tagging is deleted
    let get_result = client
        .get_bucket_tagging()
        .bucket("bucket-tagging-test")
        .send()
        .await;
    if let Ok(tags) = get_result {
        assert!(tags.tag_set().is_empty());
    }
}

#[tokio::test]
async fn test_bucket_policy() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Create bucket
    client
        .create_bucket()
        .bucket("policy-test")
        .send()
        .await
        .unwrap();

    // Use reqwest for raw HTTP calls since aws-sdk-s3 bucket policy
    // methods require proper IAM signing
    let http_client = reqwest::Client::new();
    let base_url = format!("http://{}", server.addr);

    // Initially there should be no policy
    let get_response = http_client
        .get(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(get_response.status(), 404);

    // Put a bucket policy
    let policy = serde_json::json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Sid": "PublicReadGetObject",
            "Effect": "Allow",
            "Principal": "*",
            "Action": "s3:GetObject",
            "Resource": "arn:aws:s3:::policy-test/*"
        }]
    });

    let put_response = http_client
        .put(format!("{}/policy-test?policy", base_url))
        .header("Content-Type", "application/json")
        .body(policy.to_string())
        .send()
        .await
        .unwrap();
    assert_eq!(put_response.status(), 204);

    // Get the policy back
    let get_response = http_client
        .get(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(get_response.status(), 200);

    let retrieved_policy: serde_json::Value = get_response.json().await.unwrap();
    assert_eq!(retrieved_policy["Version"], "2012-10-17");
    assert_eq!(
        retrieved_policy["Statement"][0]["Sid"],
        "PublicReadGetObject"
    );

    // Test putting invalid JSON (should fail)
    let invalid_response = http_client
        .put(format!("{}/policy-test?policy", base_url))
        .body("not valid json")
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_response.status(), 400);

    // Delete the policy
    let delete_response = http_client
        .delete(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(delete_response.status(), 204);

    // Verify policy is deleted
    let get_response = http_client
        .get(format!("{}/policy-test?policy", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(get_response.status(), 404);
}

/// Test bucket location operations
#[tokio::test]
async fn test_bucket_location() {
    let (client, _temp_dir, _server) = setup_test_server().await;
    let bucket_name = format!("location-{}", uuid::Uuid::new_v4());

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Get bucket location
    let location_result = client
        .get_bucket_location()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Location should be us-east-1 (our default)
    let location = location_result.location_constraint();
    // us-east-1 is represented as None in S3 API
    assert!(
        location.is_none()
            || location
                .map(|l| l.as_str() == "us-east-1" || l.as_str().is_empty())
                .unwrap_or(true),
        "Location should be us-east-1 or empty, got {:?}",
        location
    );

    // Delete bucket
    client
        .delete_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();
}

/// Test presigned URL generation and usage
#[tokio::test]
async fn test_presigned_urls() {
    let (client, _temp_dir, server) = setup_test_server_with_auth().await;
    let bucket_name = format!("presign-{}", uuid::Uuid::new_v4());
    let base_url = format!("http://{}", server.addr);
    let http_client = reqwest::Client::new();

    // Create bucket
    client
        .create_bucket()
        .bucket(&bucket_name)
        .send()
        .await
        .unwrap();

    // Test 1: Generate presigned PUT URL (no object required for PUT)
    let presign_response = http_client
        .get(format!(
            "{}/presign/{}/upload.txt?method=PUT&expires=3600",
            base_url, bucket_name
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(presign_response.status(), 200, "Presign PUT request failed");
    let presign_json: serde_json::Value = presign_response.json().await.unwrap();
    let put_url = presign_json["url"].as_str().unwrap();
    assert!(put_url.contains("X-Amz-Signature"));
    assert_eq!(presign_json["method"], "PUT");
    assert_eq!(presign_json["expires_in"], 3600);

    // Test 2: Use presigned PUT URL to upload an object
    let content = b"presigned upload content";
    let put_response = http_client
        .put(put_url)
        .body(content.to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(
        put_response.status(),
        200,
        "Presigned PUT should succeed: {}",
        put_response.text().await.unwrap_or_default()
    );

    // Test 3: Verify object was uploaded via standard GET
    let get_result = client
        .get_object()
        .bucket(&bucket_name)
        .key("upload.txt")
        .send()
        .await
        .unwrap();
    let body = get_result.body.collect().await.unwrap().into_bytes();
    assert_eq!(&body[..], content);

    // Test 4: Generate presigned GET URL (now object exists)
    let presign_response = http_client
        .get(format!("{}/presign/{}/upload.txt", base_url, bucket_name))
        .send()
        .await
        .unwrap();
    assert_eq!(presign_response.status(), 200, "Presign GET request failed");
    let presign_json: serde_json::Value = presign_response.json().await.unwrap();
    let get_url = presign_json["url"].as_str().unwrap();
    assert!(get_url.contains("X-Amz-Signature"));
    assert_eq!(presign_json["method"], "GET");

    // Test 5: Use presigned GET URL to download the object
    let get_response = http_client.get(get_url).send().await.unwrap();
    assert_eq!(get_response.status(), 200);
    let body = get_response.bytes().await.unwrap();
    assert_eq!(&body[..], content);

    // Test 6: Invalid method should return error
    let presign_response = http_client
        .get(format!(
            "{}/presign/{}/test.txt?method=DELETE",
            base_url, bucket_name
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(
        presign_response.status(),
        400,
        "Invalid method should return 400"
    );
}
