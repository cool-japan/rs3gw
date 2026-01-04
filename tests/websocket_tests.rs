//! Integration tests for WebSocket event streaming

mod common;

use bytes::Bytes;
use common::setup_test_server;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn test_websocket_connection() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let ws_url = format!("ws://{}/events/stream", server.addr);

    // Connect to WebSocket
    let (ws_stream, _response) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");

    let (_write, mut read) = ws_stream.split();

    // Expect welcome message
    let msg = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for welcome")
        .expect("No message received")
        .expect("Error receiving message");

    if let Message::Text(text) = msg {
        let json: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(json["type"], "welcome");
        assert!(json["message"].as_str().is_some());
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn test_websocket_event_broadcast() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let ws_url = format!("ws://{}/events/stream", server.addr);

    // Connect to WebSocket
    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (_write, mut read) = ws_stream.split();

    // Skip welcome message
    let _welcome = read.next().await;

    // Create bucket (should trigger BucketCreated event)
    client
        .create_bucket()
        .bucket("test-ws-bucket")
        .send()
        .await
        .expect("Failed to create bucket");

    // Wait for bucket created event
    let event_msg = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for bucket event")
        .expect("No message received")
        .expect("Error receiving message");

    if let Message::Text(text) = event_msg {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "bucket-created");
        assert_eq!(event["bucket"], "test-ws-bucket");
    } else {
        panic!("Expected text message");
    }

    // Put object (should trigger ObjectCreated event)
    client
        .put_object()
        .bucket("test-ws-bucket")
        .key("test-file.txt")
        .body(Bytes::from_static(b"test content").into())
        .send()
        .await
        .expect("Failed to put object");

    // Wait for object created event
    let event_msg = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for object event")
        .expect("No message received")
        .expect("Error receiving message");

    if let Message::Text(text) = event_msg {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "object-created");
        assert_eq!(event["bucket"], "test-ws-bucket");
        assert_eq!(event["key"], "test-file.txt");
        assert!(event["size"].as_u64().is_some());
        assert!(event["etag"].as_str().is_some());
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn test_websocket_bucket_filter() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Connect with bucket filter
    let ws_url = format!("ws://{}/events/stream?bucket=filtered-bucket", server.addr);
    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (_write, mut read) = ws_stream.split();

    // Skip welcome message
    let welcome = read.next().await;
    if let Some(Ok(Message::Text(text))) = welcome {
        let json: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(json["filters"]["bucket"], "filtered-bucket");
    }

    // Create bucket that doesn't match filter - should NOT receive event
    client
        .create_bucket()
        .bucket("other-bucket")
        .send()
        .await
        .expect("Failed to create bucket");

    // Create bucket that matches filter - SHOULD receive event
    client
        .create_bucket()
        .bucket("filtered-bucket")
        .send()
        .await
        .expect("Failed to create bucket");

    // Wait for filtered event (should skip other-bucket)
    let event_msg = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for event")
        .expect("No message received")
        .expect("Error receiving message");

    if let Message::Text(text) = event_msg {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["bucket"], "filtered-bucket");
        // Should not receive event for other-bucket
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn test_websocket_prefix_filter() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Connect with prefix filter
    let ws_url = format!("ws://{}/events/stream?prefix=logs/", server.addr);
    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (_write, mut read) = ws_stream.split();

    // Skip welcome message
    let _welcome = read.next().await;

    client
        .create_bucket()
        .bucket("prefix-test")
        .send()
        .await
        .expect("Failed to create bucket");

    // Note: bucket-created events don't have a key, so they won't match the prefix filter
    // and won't be sent to this WebSocket connection

    // Put object that doesn't match prefix - should NOT receive event
    client
        .put_object()
        .bucket("prefix-test")
        .key("data/file.txt")
        .body(Bytes::from_static(b"data").into())
        .send()
        .await
        .expect("Failed to put object");

    // Put object that matches prefix - SHOULD receive event
    client
        .put_object()
        .bucket("prefix-test")
        .key("logs/access.log")
        .body(Bytes::from_static(b"logs").into())
        .send()
        .await
        .expect("Failed to put object");

    // Wait for filtered event (should skip data/file.txt)
    let event_msg = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for event")
        .expect("No message received")
        .expect("Error receiving message");

    if let Message::Text(text) = event_msg {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["key"], "logs/access.log");
        // Should not receive event for data/file.txt
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn test_websocket_event_type_filter() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Connect with event type filter (only object-created events)
    let ws_url = format!("ws://{}/events/stream?events=object-created", server.addr);
    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (_write, mut read) = ws_stream.split();

    // Skip welcome message
    let _welcome = read.next().await;

    client
        .create_bucket()
        .bucket("event-filter-test")
        .send()
        .await
        .expect("Failed to create bucket");

    // Bucket created event should be filtered out

    // Put object - SHOULD receive this event
    client
        .put_object()
        .bucket("event-filter-test")
        .key("file.txt")
        .body(Bytes::from_static(b"content").into())
        .send()
        .await
        .expect("Failed to put object");

    // Wait for object-created event
    let event_msg = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for event")
        .expect("No message received")
        .expect("Error receiving message");

    if let Message::Text(text) = event_msg {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "object-created");
        // Should not receive bucket-created event
    } else {
        panic!("Expected text message");
    }
}

#[tokio::test]
async fn test_websocket_multiple_event_types() {
    let (client, _temp_dir, server) = setup_test_server().await;

    // Connect with multiple event type filters
    let ws_url = format!(
        "ws://{}/events/stream?events=object-created,object-removed",
        server.addr
    );
    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (_write, mut read) = ws_stream.split();

    // Skip welcome message
    let _welcome = read.next().await;

    client
        .create_bucket()
        .bucket("multi-event-test")
        .send()
        .await
        .expect("Failed to create bucket");

    // Put object
    client
        .put_object()
        .bucket("multi-event-test")
        .key("file.txt")
        .body(Bytes::from_static(b"content").into())
        .send()
        .await
        .expect("Failed to put object");

    // Receive object-created event
    let event1 = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout")
        .expect("No message")
        .expect("Error");

    if let Message::Text(text) = event1 {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "object-created");
    }

    // Delete object
    client
        .delete_object()
        .bucket("multi-event-test")
        .key("file.txt")
        .send()
        .await
        .expect("Failed to delete object");

    // Receive object-removed event
    let event2 = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout")
        .expect("No message")
        .expect("Error");

    if let Message::Text(text) = event2 {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "object-removed");
    }
}

#[tokio::test]
async fn test_websocket_ping_pong() {
    let (_client, _temp_dir, server) = setup_test_server().await;
    let ws_url = format!("ws://{}/events/stream", server.addr);

    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (mut write, mut read) = ws_stream.split();

    // Skip welcome message
    let _welcome = read.next().await;

    // Send ping
    write
        .send(Message::Ping(Bytes::from_static(&[1, 2, 3])))
        .await
        .expect("Failed to send ping");

    // Should receive pong (or welcome if not handled yet)
    let response = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout waiting for pong");

    // WebSocket should still be functional
    assert!(response.is_some());
}

#[tokio::test]
async fn test_websocket_multipart_events() {
    let (client, _temp_dir, server) = setup_test_server().await;
    let ws_url = format!(
        "ws://{}/events/stream?events=multipart-upload-created,multipart-upload-completed",
        server.addr
    );

    let (ws_stream, _response) = connect_async(&ws_url).await.expect("Failed to connect");

    let (_write, mut read) = ws_stream.split();

    // Skip welcome message
    let _welcome = read.next().await;

    client
        .create_bucket()
        .bucket("multipart-test")
        .send()
        .await
        .expect("Failed to create bucket");

    // Create multipart upload
    let upload = client
        .create_multipart_upload()
        .bucket("multipart-test")
        .key("large-file.dat")
        .send()
        .await
        .expect("Failed to create multipart upload");

    // Receive multipart-upload-created event
    let event1 = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout")
        .expect("No message")
        .expect("Error");

    if let Message::Text(text) = event1 {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "multipart-upload-created");
        assert_eq!(event["bucket"], "multipart-test");
        assert_eq!(event["key"], "large-file.dat");
    }

    // Upload a part
    let upload_id = upload.upload_id().expect("No upload ID");
    let part = client
        .upload_part()
        .bucket("multipart-test")
        .key("large-file.dat")
        .upload_id(upload_id)
        .part_number(1)
        .body(Bytes::from_static(b"part data").into())
        .send()
        .await
        .expect("Failed to upload part");

    // Complete multipart upload
    client
        .complete_multipart_upload()
        .bucket("multipart-test")
        .key("large-file.dat")
        .upload_id(upload_id)
        .multipart_upload(
            aws_sdk_s3::types::CompletedMultipartUpload::builder()
                .parts(
                    aws_sdk_s3::types::CompletedPart::builder()
                        .part_number(1)
                        .e_tag(part.e_tag().expect("No ETag"))
                        .build(),
                )
                .build(),
        )
        .send()
        .await
        .expect("Failed to complete multipart upload");

    // Receive multipart-upload-completed event
    let event2 = timeout(Duration::from_secs(2), read.next())
        .await
        .expect("Timeout")
        .expect("No message")
        .expect("Error");

    if let Message::Text(text) = event2 {
        let event: Value = serde_json::from_str(&text).expect("Invalid JSON");
        assert_eq!(event["eventType"], "multipart-upload-completed");
    }
}
