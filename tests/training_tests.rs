#![cfg(feature = "server")]
// Integration tests for distributed training API endpoints

use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};

mod common;
use common::TestServer;

#[tokio::test]
async fn test_create_experiment() {
    let server = TestServer::new().await;
    let client = Client::new();

    let experiment_json = json!({
        "name": "test-experiment-1",
        "description": "Integration test experiment",
        "tags": ["test", "integration"],
        "hyperparameters": {
            "learning_rate": 0.001,
            "batch_size": 32,
            "epochs": 10
        }
    });

    let url = format!("{}/api/training/experiments", server.base_url);
    let response = client
        .post(&url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let result: Value = response.json().await.expect("Failed to parse JSON");
    assert!(result["experiment"]["id"].is_string());
    assert_eq!(result["experiment"]["name"], "test-experiment-1");
    assert_eq!(
        result["experiment"]["description"],
        "Integration test experiment"
    );
    assert_eq!(result["experiment"]["status"], "Running");
    assert!(result["experiment"]["tags"].is_array());
    assert_eq!(result["experiment"]["tags"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_create_duplicate_experiment() {
    let server = TestServer::new().await;
    let client = Client::new();

    let experiment_json = json!({
        "name": "duplicate-test",
        "hyperparameters": {}
    });

    let url = format!("{}/api/training/experiments", server.base_url);

    // First creation should succeed
    let response1 = client
        .post(&url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response1.status(), 200);

    // Second creation with same name should fail
    let response2 = client
        .post(&url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to send request");
    assert_eq!(response2.status(), 400);

    let error: Value = response2.json().await.expect("Failed to parse JSON");
    assert!(error["error"].as_str().unwrap().contains("already exists"));
}

#[tokio::test]
async fn test_get_experiment() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create experiment first
    let experiment_json = json!({
        "name": "get-test-exp",
        "hyperparameters": {"lr": 0.01}
    });

    let create_url = format!("{}/api/training/experiments", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to create experiment");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let exp_id = create_result["experiment"]["id"].as_str().unwrap();

    // Get experiment by ID
    let get_url = format!("{}/api/training/experiments/{}", server.base_url, exp_id);
    let get_response = client
        .get(&get_url)
        .send()
        .await
        .expect("Failed to get experiment");

    assert_eq!(get_response.status(), 200);

    let result: Value = get_response.json().await.expect("Failed to parse JSON");
    assert_eq!(result["experiment"]["id"], exp_id);
    assert_eq!(result["experiment"]["name"], "get-test-exp");
}

#[tokio::test]
async fn test_get_nonexistent_experiment() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!(
        "{}/api/training/experiments/nonexistent-id",
        server.base_url
    );
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_update_experiment_status() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create experiment
    let experiment_json = json!({
        "name": "status-test-exp",
        "hyperparameters": {}
    });

    let create_url = format!("{}/api/training/experiments", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to create experiment");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let exp_id = create_result["experiment"]["id"].as_str().unwrap();

    // Update status
    let status_json = json!({
        "status": "Completed"
    });

    let update_url = format!(
        "{}/api/training/experiments/{}/status",
        server.base_url, exp_id
    );
    let update_response = client
        .put(&update_url)
        .json(&status_json)
        .send()
        .await
        .expect("Failed to update status");

    assert_eq!(update_response.status(), 200);

    let result: Value = update_response.json().await.expect("Failed to parse JSON");
    assert_eq!(result["experiment"]["status"], "Completed");
}

#[tokio::test]
async fn test_save_and_load_checkpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create experiment
    let experiment_json = json!({
        "name": "checkpoint-test-exp",
        "hyperparameters": {"lr": 0.001}
    });

    let create_url = format!("{}/api/training/experiments", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to create experiment");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let exp_id = create_result["experiment"]["id"].as_str().unwrap();

    // Save checkpoint
    let model_data = b"fake_model_weights_data";
    let model_state_b64 = base64::engine::general_purpose::STANDARD.encode(model_data);

    let checkpoint_json = json!({
        "epoch": 5,
        "model_state": model_state_b64,
        "optimizer_state": null,
        "metrics": {
            "loss": 0.234,
            "accuracy": 0.89
        }
    });

    let save_url = format!(
        "{}/api/training/experiments/{}/checkpoints",
        server.base_url, exp_id
    );
    let save_response = client
        .post(&save_url)
        .json(&checkpoint_json)
        .send()
        .await
        .expect("Failed to save checkpoint");

    assert_eq!(save_response.status(), 200);

    let save_result: Value = save_response.json().await.expect("Failed to parse JSON");
    let checkpoint_id = save_result["checkpoint"]["id"].as_str().unwrap();
    assert_eq!(save_result["checkpoint"]["epoch"], 5);

    // Load checkpoint
    let load_url = format!(
        "{}/api/training/checkpoints/{}",
        server.base_url, checkpoint_id
    );
    let load_response = client
        .get(&load_url)
        .send()
        .await
        .expect("Failed to load checkpoint");

    assert_eq!(load_response.status(), 200);

    let load_result: Value = load_response.json().await.expect("Failed to parse JSON");
    assert_eq!(load_result["checkpoint"]["id"], checkpoint_id);
    assert_eq!(load_result["checkpoint"]["epoch"], 5);
    assert_eq!(load_result["model_state"], model_state_b64);
}

#[tokio::test]
async fn test_list_checkpoints() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create experiment
    let experiment_json = json!({
        "name": "list-ckpt-test-exp",
        "hyperparameters": {}
    });

    let create_url = format!("{}/api/training/experiments", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to create experiment");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let exp_id = create_result["experiment"]["id"].as_str().unwrap();

    // Save multiple checkpoints
    for epoch in 1..=3 {
        let checkpoint_json = json!({
            "epoch": epoch,
            "model_state": base64::engine::general_purpose::STANDARD.encode(format!("model_{}", epoch)),
            "metrics": {"loss": 1.0 / epoch as f64}
        });

        let save_url = format!(
            "{}/api/training/experiments/{}/checkpoints",
            server.base_url, exp_id
        );
        client
            .post(&save_url)
            .json(&checkpoint_json)
            .send()
            .await
            .expect("Failed to save checkpoint");
    }

    // List checkpoints
    let list_url = format!(
        "{}/api/training/experiments/{}/checkpoints",
        server.base_url, exp_id
    );
    let list_response = client
        .get(&list_url)
        .send()
        .await
        .expect("Failed to list checkpoints");

    assert_eq!(list_response.status(), 200);

    let result: Value = list_response.json().await.expect("Failed to parse JSON");
    assert!(result["checkpoints"].is_array());
    assert_eq!(result["count"], 3);

    let checkpoints = result["checkpoints"].as_array().unwrap();
    assert_eq!(checkpoints.len(), 3);
}

#[tokio::test]
async fn test_log_and_get_metrics() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create experiment
    let experiment_json = json!({
        "name": "metrics-test-exp",
        "hyperparameters": {}
    });

    let create_url = format!("{}/api/training/experiments", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to create experiment");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let exp_id = create_result["experiment"]["id"].as_str().unwrap();

    // Log metrics for multiple steps
    for step in 1..=5 {
        let metrics_json = json!({
            "step": step,
            "metrics": {
                "loss": 1.0 / step as f64,
                "accuracy": 0.5 + (step as f64 * 0.1)
            }
        });

        let log_url = format!(
            "{}/api/training/experiments/{}/metrics",
            server.base_url, exp_id
        );
        let log_response = client
            .post(&log_url)
            .json(&metrics_json)
            .send()
            .await
            .expect("Failed to log metrics");

        assert_eq!(log_response.status(), 201);
    }

    // Get metrics
    let get_url = format!(
        "{}/api/training/experiments/{}/metrics",
        server.base_url, exp_id
    );
    let get_response = client
        .get(&get_url)
        .send()
        .await
        .expect("Failed to get metrics");

    assert_eq!(get_response.status(), 200);

    let result: Value = get_response.json().await.expect("Failed to parse JSON");
    assert!(result["metrics"].is_array());
    assert_eq!(result["count"], 5);

    let metrics = result["metrics"].as_array().unwrap();
    assert_eq!(metrics.len(), 5);
    assert_eq!(metrics[0]["step"], 1);
}

#[tokio::test]
async fn test_create_hyperparameter_search() {
    let server = TestServer::new().await;
    let client = Client::new();

    let search_json = json!({
        "search_space": {
            "learning_rate": [0.001, 0.01, 0.1],
            "batch_size": [16, 32, 64],
            "dropout": [0.1, 0.2, 0.3]
        },
        "optimization_metric": "accuracy"
    });

    let url = format!("{}/api/training/searches", server.base_url);
    let response = client
        .post(&url)
        .json(&search_json)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let result: Value = response.json().await.expect("Failed to parse JSON");
    assert!(result["search"]["id"].is_string());
    assert_eq!(result["search"]["optimization_metric"], "accuracy");
    assert_eq!(result["search"]["trials"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_add_trial_to_search() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create search
    let search_json = json!({
        "search_space": {"lr": [0.001, 0.01]},
        "optimization_metric": "loss"
    });

    let create_url = format!("{}/api/training/searches", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&search_json)
        .send()
        .await
        .expect("Failed to create search");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let search_id = create_result["search"]["id"].as_str().unwrap();

    // Add trial
    let trial_json = json!({
        "params": {"lr": 0.001},
        "metrics": {"loss": 0.45, "accuracy": 0.82},
        "status": "Completed"
    });

    let add_url = format!(
        "{}/api/training/searches/{}/trials",
        server.base_url, search_id
    );
    let add_response = client
        .post(&add_url)
        .json(&trial_json)
        .send()
        .await
        .expect("Failed to add trial");

    assert_eq!(add_response.status(), 200);

    let result: Value = add_response.json().await.expect("Failed to parse JSON");
    assert!(result["search"]["trials"].is_array());
    assert_eq!(result["search"]["trials"].as_array().unwrap().len(), 1);

    let trial = &result["search"]["trials"][0];
    assert_eq!(trial["status"], "Completed");
    assert_eq!(trial["params"]["lr"], 0.001);
}

#[tokio::test]
async fn test_multiple_trials_best_result() {
    let server = TestServer::new().await;
    let client = Client::new();

    // Create search
    let search_json = json!({
        "search_space": {"lr": [0.001, 0.01, 0.1]},
        "optimization_metric": "accuracy"
    });

    let create_url = format!("{}/api/training/searches", server.base_url);
    let create_response = client
        .post(&create_url)
        .json(&search_json)
        .send()
        .await
        .expect("Failed to create search");

    let create_result: Value = create_response.json().await.expect("Failed to parse JSON");
    let search_id = create_result["search"]["id"].as_str().unwrap();

    // Add multiple trials with different accuracies
    let trials = vec![
        (0.001, 0.75),
        (0.01, 0.89), // Best
        (0.1, 0.82),
    ];

    for (lr, acc) in trials {
        let trial_json = json!({
            "params": {"lr": lr},
            "metrics": {"accuracy": acc},
            "status": "Completed"
        });

        let add_url = format!(
            "{}/api/training/searches/{}/trials",
            server.base_url, search_id
        );
        client
            .post(&add_url)
            .json(&trial_json)
            .send()
            .await
            .expect("Failed to add trial");
    }

    // Get search results - should have best result
    let get_url = format!("{}/api/training/searches/{}", server.base_url, search_id);
    let get_response = client
        .get(&get_url)
        .send()
        .await
        .expect("Failed to get search");

    assert_eq!(get_response.status(), 200);

    let result: Value = get_response.json().await.expect("Failed to parse JSON");
    assert_eq!(result["search"]["trials"].as_array().unwrap().len(), 3);

    if let Some(best) = result["search"]["best_result"].as_object() {
        assert_eq!(best["params"]["lr"], 0.01);
        assert_eq!(best["metrics"]["accuracy"], 0.89);
    }
}

#[tokio::test]
async fn test_load_nonexistent_checkpoint() {
    let server = TestServer::new().await;
    let client = Client::new();

    let url = format!(
        "{}/api/training/checkpoints/nonexistent-checkpoint-id",
        server.base_url
    );
    let response = client
        .get(&url)
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_full_training_workflow() {
    let server = TestServer::new().await;
    let client = Client::new();

    // 1. Create experiment
    let experiment_json = json!({
        "name": "full-workflow-test",
        "description": "Complete training workflow test",
        "tags": ["test", "integration", "workflow"],
        "hyperparameters": {
            "learning_rate": 0.001,
            "batch_size": 32,
            "epochs": 10
        }
    });

    let create_exp_url = format!("{}/api/training/experiments", server.base_url);
    let create_exp_response = client
        .post(&create_exp_url)
        .json(&experiment_json)
        .send()
        .await
        .expect("Failed to create experiment");

    assert_eq!(create_exp_response.status(), 200);
    let exp_result: Value = create_exp_response
        .json()
        .await
        .expect("Failed to parse JSON");
    let exp_id = exp_result["experiment"]["id"].as_str().unwrap();

    // 2. Log metrics for several epochs
    for epoch in 1..=5 {
        let metrics_json = json!({
            "step": epoch,
            "metrics": {
                "loss": 1.0 / (epoch as f64 + 1.0),
                "accuracy": 0.5 + (epoch as f64 * 0.08)
            }
        });

        let log_url = format!(
            "{}/api/training/experiments/{}/metrics",
            server.base_url, exp_id
        );
        client
            .post(&log_url)
            .json(&metrics_json)
            .send()
            .await
            .expect("Failed to log metrics");
    }

    // 3. Save checkpoints at specific epochs
    for epoch in [2, 4, 5] {
        let checkpoint_json = json!({
            "epoch": epoch,
            "model_state": base64::engine::general_purpose::STANDARD.encode(format!("model_epoch_{}", epoch)),
            "optimizer_state": base64::engine::general_purpose::STANDARD.encode(format!("optim_epoch_{}", epoch)),
            "metrics": {
                "loss": 1.0 / (epoch as f64 + 1.0),
                "accuracy": 0.5 + (epoch as f64 * 0.08)
            }
        });

        let save_url = format!(
            "{}/api/training/experiments/{}/checkpoints",
            server.base_url, exp_id
        );
        client
            .post(&save_url)
            .json(&checkpoint_json)
            .send()
            .await
            .expect("Failed to save checkpoint");
    }

    // 4. Complete the experiment
    let status_json = json!({"status": "Completed"});
    let update_url = format!(
        "{}/api/training/experiments/{}/status",
        server.base_url, exp_id
    );
    let update_response = client
        .put(&update_url)
        .json(&status_json)
        .send()
        .await
        .expect("Failed to update status");

    assert_eq!(update_response.status(), 200);

    // 5. Verify final state
    let get_exp_url = format!("{}/api/training/experiments/{}", server.base_url, exp_id);
    let get_exp_response = client
        .get(&get_exp_url)
        .send()
        .await
        .expect("Failed to get experiment");

    let final_exp: Value = get_exp_response.json().await.expect("Failed to parse JSON");
    assert_eq!(final_exp["experiment"]["status"], "Completed");

    // 6. Verify metrics
    let get_metrics_url = format!(
        "{}/api/training/experiments/{}/metrics",
        server.base_url, exp_id
    );
    let get_metrics_response = client
        .get(&get_metrics_url)
        .send()
        .await
        .expect("Failed to get metrics");

    let metrics_result: Value = get_metrics_response
        .json()
        .await
        .expect("Failed to parse JSON");
    assert_eq!(metrics_result["count"], 5);

    // 7. Verify checkpoints
    let list_ckpt_url = format!(
        "{}/api/training/experiments/{}/checkpoints",
        server.base_url, exp_id
    );
    let list_ckpt_response = client
        .get(&list_ckpt_url)
        .send()
        .await
        .expect("Failed to list checkpoints");

    let ckpt_result: Value = list_ckpt_response
        .json()
        .await
        .expect("Failed to parse JSON");
    assert_eq!(ckpt_result["count"], 3);
}
