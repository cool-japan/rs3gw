// Integration tests for preprocessing API endpoints

use reqwest::Client;
use serde_json::Value;

mod common;
use common::TestServer;

#[tokio::test]
async fn test_cache_stats() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/preprocessing/cache/stats", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let result: Value = response.json().await.expect("Failed to parse JSON");
    assert!(result["cache_size_bytes"].is_number());
    assert!(result["cached_items"].is_number());
}

#[tokio::test]
async fn test_cache_clear() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/preprocessing/cache/clear", server.base_url);
    let response = client
        .post(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let result: Value = response.json().await.expect("Failed to parse JSON");
    assert_eq!(result["status"], "cleared");
}

#[tokio::test]
async fn test_list_pipelines_empty() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!("{}/api/preprocessing/pipelines", server.base_url);
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let result: Value = response.json().await.expect("Failed to parse JSON");
    assert!(result["pipelines"].is_array());
    assert_eq!(result["count"], 0);
}

#[tokio::test]
async fn test_get_nonexistent_pipeline() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!(
        "{}/api/preprocessing/pipelines/nonexistent",
        server.base_url
    );
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

// Note: The following tests require proper preprocessing step validation
// which may need adjustments to match the actual implementation.
// For now, we test the basic API structure and responses.

#[tokio::test]
async fn test_create_and_get_pipeline() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create ImageNet preprocessing pipeline
    let pipeline_json = serde_json::json!({
        "id": "test-imagenet",
        "name": "Test ImageNet Pipeline",
        "version": "1.0.0",
        "description": "Test pipeline for ImageNet preprocessing",
        "steps": [
            {
                "id": "resize",
                "step_type": "image_resize",
                "config": {
                    "width": 224,
                    "height": 224,
                    "mode": "fit",
                    "filter": "lanczos3"
                },
                "cache_results": false
            },
            {
                "id": "normalize",
                "step_type": "image_normalization",
                "config": {
                    "mean": [0.485, 0.456, 0.406],
                    "std": [0.229, 0.224, 0.225],
                    "normalize_range": true
                },
                "cache_results": true
            }
        ],
        "metadata": {
            "author": "test-team",
            "target_model": "ResNet"
        }
    });

    // Create pipeline
    let create_url = format!("{}/api/preprocessing/pipelines", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&pipeline_json)
        .send()
        .await
        .expect("Failed to create pipeline");

    assert_eq!(create_response.status(), 201);
    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    assert_eq!(create_result["status"], "created");
    assert_eq!(create_result["pipeline_id"], "test-imagenet");

    // Get the created pipeline
    let get_url = format!(
        "{}/api/preprocessing/pipelines/test-imagenet",
        server.base_url
    );
    let get_response = client
        .get(&get_url)
        .send()
        .await
        .expect("Failed to get pipeline");

    assert_eq!(get_response.status(), 200);
    let get_result: Value = get_response.json().await.expect("Failed to parse JSON");
    assert!(get_result["pipeline"].is_object());
    let pipeline = &get_result["pipeline"];
    assert_eq!(pipeline["id"], "test-imagenet");
    assert_eq!(pipeline["name"], "Test ImageNet Pipeline");
    assert_eq!(pipeline["steps"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_create_and_delete_pipeline() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create a simple pipeline
    let pipeline_json = serde_json::json!({
        "id": "test-delete",
        "name": "Test Delete Pipeline",
        "version": "1.0.0",
        "description": "Pipeline to test deletion",
        "steps": [
            {
                "id": "resize",
                "step_type": "image_resize",
                "config": {
                    "width": 128,
                    "height": 128,
                    "mode": "fit",
                    "filter": "bilinear"
                },
                "cache_results": false
            }
        ],
        "metadata": {}
    });

    // Create pipeline
    let create_url = format!("{}/api/preprocessing/pipelines", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&pipeline_json)
        .send()
        .await
        .expect("Failed to create pipeline");

    assert_eq!(create_response.status(), 201);

    // Delete the pipeline
    let delete_url = format!(
        "{}/api/preprocessing/pipelines/test-delete",
        server.base_url
    );
    let delete_response = client
        .delete(&delete_url)
        .send()
        .await
        .expect("Failed to delete pipeline");

    assert_eq!(delete_response.status(), 200);
    let delete_result: Value = delete_response.json().await.expect("Failed to parse JSON");
    assert_eq!(delete_result["status"], "deleted");
    assert_eq!(delete_result["pipeline_id"], "test-delete");

    // Verify pipeline is deleted
    let get_url = format!(
        "{}/api/preprocessing/pipelines/test-delete",
        server.base_url
    );
    let get_response = client
        .get(&get_url)
        .send()
        .await
        .expect("Failed to get pipeline");

    assert_eq!(get_response.status(), 404);
}

#[tokio::test]
async fn test_list_multiple_pipelines() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create first pipeline
    let pipeline1_json = serde_json::json!({
        "id": "pipeline-1",
        "name": "Pipeline 1",
        "version": "1.0.0",
        "description": "First test pipeline",
        "steps": [{
            "id": "step1",
            "step_type": "image_resize",
            "config": {"width": 224, "height": 224, "mode": "fit", "filter": "bilinear"},
            "cache_results": false
        }],
        "metadata": {}
    });

    // Create second pipeline
    let pipeline2_json = serde_json::json!({
        "id": "pipeline-2",
        "name": "Pipeline 2",
        "version": "1.0.0",
        "description": "Second test pipeline",
        "steps": [{
            "id": "step1",
            "step_type": "image_normalization",
            "config": {"mean": [0.5], "std": [0.5], "normalize_range": true},
            "cache_results": false
        }],
        "metadata": {}
    });

    let create_url = format!("{}/api/preprocessing/pipelines", server.base_url);

    // Create both pipelines
    let response1 = client
        .post(&create_url)
        .json(&pipeline1_json)
        .send()
        .await
        .expect("Failed to create pipeline 1");
    assert_eq!(response1.status(), 201, "Pipeline 1 creation failed");

    let response2 = client
        .post(&create_url)
        .json(&pipeline2_json)
        .send()
        .await
        .expect("Failed to create pipeline 2");
    assert_eq!(response2.status(), 201, "Pipeline 2 creation failed");

    // List all pipelines
    let list_url = format!("{}/api/preprocessing/pipelines", server.base_url);
    let list_response = client
        .get(&list_url)
        .send()
        .await
        .expect("Failed to list pipelines");

    assert_eq!(list_response.status(), 200);
    let list_result: Value = list_response.json().await.expect("Failed to parse JSON");
    assert_eq!(list_result["count"], 2);
    let pipelines = list_result["pipelines"].as_array().unwrap();
    assert_eq!(pipelines.len(), 2);

    // Verify pipeline IDs
    let ids: Vec<&str> = pipelines
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"pipeline-1"));
    assert!(ids.contains(&"pipeline-2"));
}

#[tokio::test]
async fn test_create_medical_imaging_pipeline() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create medical imaging pipeline (512x512, grayscale normalization)
    let pipeline_json = serde_json::json!({
        "id": "medical-imaging",
        "name": "Medical Imaging Pipeline",
        "version": "1.0.0",
        "description": "Pipeline for CT/MRI preprocessing",
        "steps": [
            {
                "id": "resize",
                "step_type": "image_resize",
                "config": {
                    "width": 512,
                    "height": 512,
                    "mode": "fit",
                    "filter": "lanczos3"
                },
                "cache_results": true
            },
            {
                "id": "normalize",
                "step_type": "image_normalization",
                "config": {
                    "mean": [0.5],
                    "std": [0.5],
                    "normalize_range": true
                },
                "cache_results": true
            }
        ],
        "metadata": {
            "author": "medical-ai-team",
            "compliance": "HIPAA",
            "use_case": "diagnostic"
        }
    });

    let create_url = format!("{}/api/preprocessing/pipelines", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&pipeline_json)
        .send()
        .await
        .expect("Failed to create medical pipeline");

    assert_eq!(create_response.status(), 201);
    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    assert_eq!(create_result["status"], "created");
    assert_eq!(create_result["pipeline_id"], "medical-imaging");
}

#[tokio::test]
async fn test_create_object_detection_pipeline() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create object detection pipeline (YOLO-compatible)
    let pipeline_json = serde_json::json!({
        "id": "yolo-detection",
        "name": "YOLO Object Detection Pipeline",
        "version": "1.0.0",
        "description": "Pipeline for YOLO models",
        "steps": [
            {
                "id": "resize",
                "step_type": "image_resize",
                "config": {
                    "width": 640,
                    "height": 640,
                    "mode": "fill",
                    "filter": "bilinear"
                },
                "cache_results": false
            },
            {
                "id": "normalize",
                "step_type": "image_normalization",
                "config": {
                    "mean": [0.0, 0.0, 0.0],
                    "std": [255.0, 255.0, 255.0],
                    "normalize_range": false
                },
                "cache_results": false
            }
        ],
        "metadata": {
            "author": "cv-team",
            "target_model": "YOLOv8",
            "dataset": "COCO"
        }
    });

    let create_url = format!("{}/api/preprocessing/pipelines", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&pipeline_json)
        .send()
        .await
        .expect("Failed to create YOLO pipeline");

    assert_eq!(create_response.status(), 201);
    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    assert_eq!(create_result["status"], "created");
    assert_eq!(create_result["pipeline_id"], "yolo-detection");
}
