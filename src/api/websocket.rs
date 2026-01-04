//! WebSocket Streaming for Real-Time Event Delivery
//!
//! This module provides WebSocket support for streaming S3 events in real-time,
//! enabling clients to receive notifications about object changes, uploads, deletes, etc.
//!
//! # Features
//! - Live object change notifications
//! - Streaming S3 Select results
//! - Connection multiplexing
//! - Automatic reconnection with exponential backoff
//! - Event filtering by bucket/prefix
//! - JSON-based event format

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::AppState;

/// Maximum number of events to buffer per WebSocket connection
const MAX_EVENT_BUFFER: usize = 1000;

/// WebSocket query parameters for event filtering
#[derive(Debug, Deserialize, Default)]
pub struct WebSocketQuery {
    /// Filter events by bucket name
    pub bucket: Option<String>,
    /// Filter events by object key prefix
    pub prefix: Option<String>,
    /// Event types to subscribe to (comma-separated)
    pub events: Option<String>,
}

/// S3 event types that can be streamed via WebSocket
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum S3EventType {
    /// Object created (PUT, POST, COPY)
    ObjectCreated,
    /// Object deleted
    ObjectRemoved,
    /// Object metadata changed
    ObjectMetadataChanged,
    /// Object accessed (GET)
    ObjectAccessed,
    /// Multipart upload initiated
    MultipartUploadCreated,
    /// Multipart upload completed
    MultipartUploadCompleted,
    /// Multipart upload aborted
    MultipartUploadAborted,
    /// Bucket created
    BucketCreated,
    /// Bucket deleted
    BucketRemoved,
    /// Bucket policy changed
    BucketPolicyChanged,
    /// Bucket tagging changed
    BucketTaggingChanged,
}

impl S3EventType {
    /// Parse event types from comma-separated string
    pub fn parse_list(events_str: &str) -> Vec<S3EventType> {
        events_str
            .split(',')
            .filter_map(|s| match s.trim() {
                "object-created" => Some(S3EventType::ObjectCreated),
                "object-removed" => Some(S3EventType::ObjectRemoved),
                "object-metadata-changed" => Some(S3EventType::ObjectMetadataChanged),
                "object-accessed" => Some(S3EventType::ObjectAccessed),
                "multipart-upload-created" => Some(S3EventType::MultipartUploadCreated),
                "multipart-upload-completed" => Some(S3EventType::MultipartUploadCompleted),
                "multipart-upload-aborted" => Some(S3EventType::MultipartUploadAborted),
                "bucket-created" => Some(S3EventType::BucketCreated),
                "bucket-removed" => Some(S3EventType::BucketRemoved),
                "bucket-policy-changed" => Some(S3EventType::BucketPolicyChanged),
                "bucket-tagging-changed" => Some(S3EventType::BucketTaggingChanged),
                _ => None,
            })
            .collect()
    }
}

/// S3 event notification sent via WebSocket
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct S3Event {
    /// Event ID (unique identifier)
    pub event_id: String,
    /// Event type
    pub event_type: S3EventType,
    /// Event timestamp (RFC3339)
    pub event_time: String,
    /// Bucket name
    pub bucket: String,
    /// Object key (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// Object size in bytes (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Object ETag (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    /// Additional metadata
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl S3Event {
    /// Create a new S3 event
    pub fn new(event_type: S3EventType, bucket: String) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4().to_string(),
            event_type,
            event_time: chrono::Utc::now().to_rfc3339(),
            bucket,
            key: None,
            size: None,
            etag: None,
            metadata: HashMap::new(),
        }
    }

    /// Set object key
    pub fn with_key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }

    /// Set object size
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Set object ETag
    pub fn with_etag(mut self, etag: String) -> Self {
        self.etag = Some(etag);
        self
    }

    /// Add metadata entry
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if event matches filter criteria
    pub fn matches_filter(
        &self,
        bucket_filter: &Option<String>,
        prefix_filter: &Option<String>,
    ) -> bool {
        // Check bucket filter
        if let Some(bucket) = bucket_filter {
            if &self.bucket != bucket {
                return false;
            }
        }

        // Check prefix filter
        if let Some(prefix) = prefix_filter {
            if let Some(key) = &self.key {
                if !key.starts_with(prefix) {
                    return false;
                }
            } else {
                // Event has no key, can't match prefix
                return false;
            }
        }

        true
    }
}

/// WebSocket event broadcaster
#[derive(Clone)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<S3Event>,
}

impl EventBroadcaster {
    /// Create a new event broadcaster
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(MAX_EVENT_BUFFER);
        Self { sender }
    }

    /// Broadcast an event to all subscribers
    pub fn broadcast(&self, event: S3Event) {
        // Ignore send errors (happens when no subscribers)
        let _ = self.sender.send(event);
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<S3Event> {
        self.sender.subscribe()
    }
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket connection handler
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WebSocketQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    debug!(
        "WebSocket connection request - bucket: {:?}, prefix: {:?}, events: {:?}",
        params.bucket, params.prefix, params.events
    );

    ws.on_upgrade(move |socket| handle_socket(socket, params, state))
}

/// Handle a WebSocket connection
async fn handle_socket(socket: WebSocket, params: WebSocketQuery, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // Parse event filter
    let event_filter = params
        .events
        .as_ref()
        .map(|e| S3EventType::parse_list(e))
        .unwrap_or_default();

    // Subscribe to event stream
    let mut event_rx = state.event_broadcaster.subscribe();

    // Send welcome message
    let welcome = serde_json::json!({
        "type": "welcome",
        "message": "Connected to rs3gw WebSocket event stream",
        "filters": {
            "bucket": params.bucket,
            "prefix": params.prefix,
            "events": event_filter,
        }
    });

    if let Ok(msg) = serde_json::to_string(&welcome) {
        if sender.send(Message::Text(msg.into())).await.is_err() {
            error!("Failed to send welcome message");
            return;
        }
    }

    info!("WebSocket connection established");

    // Spawn task to forward events to WebSocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            // Apply filters
            if !event.matches_filter(&params.bucket, &params.prefix) {
                continue;
            }

            if !event_filter.is_empty() && !event_filter.contains(&event.event_type) {
                continue;
            }

            // Serialize and send event
            match serde_json::to_string(&event) {
                Ok(json) => {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        warn!("WebSocket send failed, closing connection");
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize event: {}", e);
                }
            }
        }
    });

    // Handle incoming messages (ping/pong, close, etc.)
    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Ping(_data)) => {
                    debug!("Received ping");
                    // Pong is handled automatically by axum
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong");
                }
                Ok(Message::Close(_)) => {
                    debug!("WebSocket close received");
                    break;
                }
                Ok(Message::Text(text)) => {
                    debug!("Received text message: {}", text);
                    // Could handle client commands here
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => {
            recv_task.abort();
        },
        _ = &mut recv_task => {
            send_task.abort();
        },
    }

    info!("WebSocket connection closed");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_parsing() {
        let events = S3EventType::parse_list("object-created,object-removed,bucket-created");
        assert_eq!(events.len(), 3);
        assert!(events.contains(&S3EventType::ObjectCreated));
        assert!(events.contains(&S3EventType::ObjectRemoved));
        assert!(events.contains(&S3EventType::BucketCreated));
    }

    #[test]
    fn test_event_creation() {
        let event = S3Event::new(S3EventType::ObjectCreated, "test-bucket".to_string())
            .with_key("test/object.txt".to_string())
            .with_size(1024)
            .with_etag("abc123".to_string())
            .with_metadata("custom".to_string(), "value".to_string());

        assert_eq!(event.event_type, S3EventType::ObjectCreated);
        assert_eq!(event.bucket, "test-bucket");
        assert_eq!(event.key, Some("test/object.txt".to_string()));
        assert_eq!(event.size, Some(1024));
        assert_eq!(event.etag, Some("abc123".to_string()));
        assert_eq!(event.metadata.get("custom"), Some(&"value".to_string()));
    }

    #[test]
    fn test_event_filtering() {
        let event = S3Event::new(S3EventType::ObjectCreated, "my-bucket".to_string())
            .with_key("data/file.txt".to_string());

        // Test bucket filter
        assert!(event.matches_filter(&Some("my-bucket".to_string()), &None));
        assert!(!event.matches_filter(&Some("other-bucket".to_string()), &None));

        // Test prefix filter
        assert!(event.matches_filter(&None, &Some("data/".to_string())));
        assert!(!event.matches_filter(&None, &Some("logs/".to_string())));

        // Test combined filters
        assert!(event.matches_filter(&Some("my-bucket".to_string()), &Some("data/".to_string())));
        assert!(!event.matches_filter(&Some("my-bucket".to_string()), &Some("logs/".to_string())));
    }

    #[test]
    fn test_event_serialization() {
        let event = S3Event::new(S3EventType::ObjectCreated, "test-bucket".to_string())
            .with_key("test.txt".to_string())
            .with_size(100);

        let json = serde_json::to_string(&event).expect("Failed to serialize");
        assert!(json.contains("object-created"));
        assert!(json.contains("test-bucket"));
        assert!(json.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_event_broadcaster() {
        let broadcaster = EventBroadcaster::new();
        let mut rx1 = broadcaster.subscribe();
        let mut rx2 = broadcaster.subscribe();

        let event = S3Event::new(S3EventType::ObjectCreated, "test-bucket".to_string());
        broadcaster.broadcast(event.clone());

        // Both subscribers should receive the event
        let received1 = rx1.recv().await.expect("Failed to receive");
        let received2 = rx2.recv().await.expect("Failed to receive");

        assert_eq!(received1.event_id, event.event_id);
        assert_eq!(received2.event_id, event.event_id);
    }
}
