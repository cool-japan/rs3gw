//! GraphQL API for rs3gw
//!
//! Provides a flexible GraphQL interface for querying bucket and object metadata
//! with real-time subscriptions for S3 events.

use async_graphql::{Context, EmptyMutation, Object, Schema, SimpleObject, Subscription};
use futures::Stream;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::debug;

use crate::api::websocket::{EventBroadcaster, S3Event};
use crate::storage::StorageEngine;

/// GraphQL schema type
pub type Rs3gwSchema = Schema<QueryRoot, EmptyMutation, SubscriptionRoot>;

/// Bucket metadata for GraphQL
#[derive(SimpleObject, Clone)]
pub struct Bucket {
    pub name: String,
    pub created_at: String,
    pub region: String,
    pub object_count: Option<i32>,
    pub total_size: Option<i64>,
}

/// Object metadata for GraphQL
#[derive(SimpleObject, Clone)]
pub struct ObjectInfo {
    pub key: String,
    pub bucket: String,
    pub size: i64,
    pub etag: String,
    pub last_modified: String,
    pub content_type: Option<String>,
    pub storage_class: String,
    pub metadata: Vec<MetadataEntry>,
}

/// Metadata key-value pair
#[derive(SimpleObject, Clone)]
pub struct MetadataEntry {
    pub key: String,
    pub value: String,
}

/// Bucket tag
#[derive(SimpleObject, Clone)]
pub struct BucketTag {
    pub key: String,
    pub value: String,
}

/// Bucket with detailed information
#[derive(SimpleObject, Clone)]
pub struct BucketDetails {
    pub name: String,
    pub created_at: String,
    pub region: String,
    pub object_count: i32,
    pub total_size: i64,
    pub tags: Vec<BucketTag>,
    pub policy: Option<String>,
}

/// GraphQL Query Root
pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// List all buckets
    async fn buckets(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Bucket>> {
        let storage = ctx.data::<Arc<StorageEngine>>()?;
        debug!("GraphQL: ListBuckets");

        let buckets = storage
            .list_buckets()
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        Ok(buckets
            .into_iter()
            .map(|b| Bucket {
                name: b.name.clone(),
                created_at: b.creation_date.to_rfc3339(),
                region: "us-east-1".to_string(),
                object_count: None,
                total_size: None,
            })
            .collect())
    }

    /// Get bucket by name with detailed information
    async fn bucket(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<Option<BucketDetails>> {
        let storage = ctx.data::<Arc<StorageEngine>>()?;
        debug!("GraphQL: GetBucket({})", name);

        let exists = storage
            .bucket_exists(&name)
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        if !exists {
            return Ok(None);
        }

        let buckets = storage
            .list_buckets()
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        let bucket_meta = buckets
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| async_graphql::Error::new("Bucket not found"))?;

        let tags_result = storage.get_bucket_tagging(&name).await;
        let tags: Vec<BucketTag> = tags_result
            .map(|t| {
                t.tags
                    .into_iter()
                    .map(|(k, v)| BucketTag { key: k, value: v })
                    .collect()
            })
            .unwrap_or_default();

        let policy = storage.get_bucket_policy(&name).await.ok();

        let (objects, _) = storage
            .list_objects(&name, "", None, 10000)
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        let object_count = objects.len() as i32;
        let total_size: i64 = objects.iter().map(|o| o.size as i64).sum();

        Ok(Some(BucketDetails {
            name: name.clone(),
            created_at: bucket_meta.creation_date.to_rfc3339(),
            region: "us-east-1".to_string(),
            object_count,
            total_size,
            tags,
            policy,
        }))
    }

    /// List objects in a bucket with optional filtering
    async fn objects(
        &self,
        ctx: &Context<'_>,
        bucket: String,
        prefix: Option<String>,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<ObjectInfo>> {
        let storage = ctx.data::<Arc<StorageEngine>>()?;
        debug!("GraphQL: ListObjects({}, prefix={:?})", bucket, prefix);

        let max_keys = limit.unwrap_or(1000).min(10000) as usize;

        let (objects, _common_prefixes) = storage
            .list_objects(&bucket, prefix.as_deref().unwrap_or(""), None, max_keys)
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        let result: Vec<ObjectInfo> = objects
            .into_iter()
            .map(|obj| {
                let metadata_entries: Vec<MetadataEntry> = obj
                    .metadata
                    .into_iter()
                    .map(|(k, v)| MetadataEntry { key: k, value: v })
                    .collect();

                ObjectInfo {
                    key: obj.key.clone(),
                    bucket: bucket.clone(),
                    size: obj.size as i64,
                    etag: obj.etag.clone(),
                    last_modified: obj.last_modified.to_rfc3339(),
                    content_type: Some(obj.content_type.clone()),
                    storage_class: "STANDARD".to_string(),
                    metadata: metadata_entries,
                }
            })
            .collect();

        Ok(result)
    }

    /// Get object metadata by key
    async fn object(
        &self,
        ctx: &Context<'_>,
        bucket: String,
        key: String,
    ) -> async_graphql::Result<Option<ObjectInfo>> {
        let storage = ctx.data::<Arc<StorageEngine>>()?;
        debug!("GraphQL: GetObject({}/{})", bucket, key);

        let meta = match storage.head_object(&bucket, &key).await {
            Ok(m) => m,
            Err(_) => return Ok(None),
        };

        let metadata_entries: Vec<MetadataEntry> = meta
            .metadata
            .into_iter()
            .map(|(k, v)| MetadataEntry { key: k, value: v })
            .collect();

        Ok(Some(ObjectInfo {
            key: key.clone(),
            bucket: bucket.clone(),
            size: meta.size as i64,
            etag: meta.etag,
            last_modified: meta.last_modified.to_rfc3339(),
            content_type: Some(meta.content_type),
            storage_class: "STANDARD".to_string(),
            metadata: metadata_entries,
        }))
    }

    /// Search objects across all buckets by key pattern
    async fn search_objects(
        &self,
        ctx: &Context<'_>,
        pattern: String,
        limit: Option<i32>,
    ) -> async_graphql::Result<Vec<ObjectInfo>> {
        let storage = ctx.data::<Arc<StorageEngine>>()?;
        debug!("GraphQL: SearchObjects(pattern={})", pattern);

        let max_results = limit.unwrap_or(100).min(1000) as usize;

        let buckets = storage
            .list_buckets()
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        let mut results = Vec::new();

        for bucket_meta in buckets {
            if results.len() >= max_results {
                break;
            }

            let (objects, _) = storage
                .list_objects(&bucket_meta.name, "", None, 1000)
                .await
                .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

            for obj in objects {
                if results.len() >= max_results {
                    break;
                }

                if obj.key.contains(&pattern) {
                    let metadata_entries: Vec<MetadataEntry> = obj
                        .metadata
                        .into_iter()
                        .map(|(k, v)| MetadataEntry { key: k, value: v })
                        .collect();

                    results.push(ObjectInfo {
                        key: obj.key.clone(),
                        bucket: bucket_meta.name.clone(),
                        size: obj.size as i64,
                        etag: obj.etag.clone(),
                        last_modified: obj.last_modified.to_rfc3339(),
                        content_type: Some(obj.content_type.clone()),
                        storage_class: "STANDARD".to_string(),
                        metadata: metadata_entries,
                    });
                }
            }
        }

        Ok(results)
    }

    /// Get storage statistics
    async fn stats(&self, ctx: &Context<'_>) -> async_graphql::Result<StorageStats> {
        let storage = ctx.data::<Arc<StorageEngine>>()?;
        debug!("GraphQL: GetStats");

        let buckets = storage
            .list_buckets()
            .await
            .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

        let bucket_count = buckets.len() as i32;
        let mut total_objects = 0i32;
        let mut total_size = 0i64;

        for bucket in &buckets {
            let (objects, _) = storage
                .list_objects(&bucket.name, "", None, 10000)
                .await
                .map_err(|e| async_graphql::Error::new(format!("Storage error: {}", e)))?;

            total_objects += objects.len() as i32;
            total_size += objects.iter().map(|o| o.size as i64).sum::<i64>();
        }

        Ok(StorageStats {
            bucket_count,
            total_objects,
            total_size_bytes: total_size,
        })
    }
}

/// Storage statistics
#[derive(SimpleObject, Clone)]
pub struct StorageStats {
    pub bucket_count: i32,
    pub total_objects: i32,
    pub total_size_bytes: i64,
}

/// S3 Event for GraphQL subscriptions
#[derive(SimpleObject, Clone)]
pub struct S3EventGraphQL {
    /// Event ID
    pub event_id: String,
    /// Event type (ObjectCreated, ObjectRemoved, etc.)
    pub event_type: String,
    /// Event timestamp
    pub event_time: String,
    /// Bucket name
    pub bucket: String,
    /// Object key (if applicable)
    pub key: Option<String>,
    /// Object size in bytes (if applicable)
    pub size: Option<i64>,
    /// Object ETag (if applicable)
    pub etag: Option<String>,
}

impl From<S3Event> for S3EventGraphQL {
    fn from(event: S3Event) -> Self {
        Self {
            event_id: event.event_id,
            event_type: format!("{:?}", event.event_type),
            event_time: event.event_time,
            bucket: event.bucket,
            key: event.key,
            size: event.size.map(|s| s as i64),
            etag: event.etag,
        }
    }
}

/// GraphQL Subscription Root
pub struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    /// Subscribe to all S3 events
    async fn events(&self, ctx: &Context<'_>) -> impl Stream<Item = S3EventGraphQL> {
        let broadcaster = ctx
            .data::<Arc<EventBroadcaster>>()
            .expect("EventBroadcaster not found in context");

        let mut receiver = broadcaster.subscribe();
        async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(event) => yield S3EventGraphQL::from(event),
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }

    /// Subscribe to events for a specific bucket
    async fn bucket_events(
        &self,
        ctx: &Context<'_>,
        bucket: String,
    ) -> impl Stream<Item = S3EventGraphQL> {
        let broadcaster = ctx
            .data::<Arc<EventBroadcaster>>()
            .expect("EventBroadcaster not found in context");

        let mut receiver = broadcaster.subscribe();
        async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if event.bucket == bucket {
                            yield S3EventGraphQL::from(event);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }

    /// Subscribe to events for a specific object key prefix
    async fn prefix_events(
        &self,
        ctx: &Context<'_>,
        bucket: String,
        prefix: String,
    ) -> impl Stream<Item = S3EventGraphQL> {
        let broadcaster = ctx
            .data::<Arc<EventBroadcaster>>()
            .expect("EventBroadcaster not found in context");

        let mut receiver = broadcaster.subscribe();
        async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if event.bucket == bucket && event.key.as_ref()
                            .map(|k| k.starts_with(&prefix))
                            .unwrap_or(false)
                        {
                            yield S3EventGraphQL::from(event);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }

    /// Subscribe to specific event types
    async fn event_type_subscription(
        &self,
        ctx: &Context<'_>,
        event_types: Vec<String>,
    ) -> impl Stream<Item = S3EventGraphQL> {
        let broadcaster = ctx
            .data::<Arc<EventBroadcaster>>()
            .expect("EventBroadcaster not found in context");

        let mut receiver = broadcaster.subscribe();
        async_stream::stream! {
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        let event_type_str = format!("{:?}", event.event_type);
                        if event_types.iter().any(|et| et == &event_type_str) {
                            yield S3EventGraphQL::from(event);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                }
            }
        }
    }
}

/// Build the GraphQL schema
pub fn build_schema(
    storage: Arc<StorageEngine>,
    broadcaster: Arc<EventBroadcaster>,
) -> Rs3gwSchema {
    Schema::build(QueryRoot, EmptyMutation, SubscriptionRoot)
        .data(storage)
        .data(broadcaster)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_storage() -> (Arc<StorageEngine>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let storage = Arc::new(
            StorageEngine::new(temp_dir.path().to_path_buf())
                .expect("Failed to create storage engine"),
        );
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_graphql_list_buckets() {
        let (storage, _temp_dir) = create_test_storage().await;
        storage
            .create_bucket("test-bucket")
            .await
            .expect("Failed to create test bucket");

        let broadcaster = Arc::new(EventBroadcaster::new());
        let schema = build_schema(storage, broadcaster);
        let query = r#"{ buckets { name region } }"#;

        let result = schema.execute(query).await;
        assert!(result.errors.is_empty());

        let data = result
            .data
            .into_json()
            .expect("Failed to convert data to JSON");
        let buckets = data
            .get("buckets")
            .expect("Failed to get buckets")
            .as_array()
            .expect("Failed to convert buckets to array");
        assert_eq!(buckets.len(), 1);
        assert_eq!(
            buckets[0]
                .get("name")
                .expect("Failed to get bucket name")
                .as_str()
                .expect("Failed to convert name to str"),
            "test-bucket"
        );
    }

    #[tokio::test]
    async fn test_graphql_search_objects() {
        use bytes::Bytes;
        use std::collections::HashMap;

        let (storage, _temp_dir) = create_test_storage().await;

        storage
            .create_bucket("search-test")
            .await
            .expect("Failed to create search-test bucket");
        storage
            .put_object(
                "search-test",
                "data/file1.txt",
                "text/plain",
                HashMap::new(),
                Bytes::from("test"),
            )
            .await
            .expect("Failed to put object");

        let broadcaster = Arc::new(EventBroadcaster::new());
        let schema = build_schema(storage, broadcaster);
        let query = r#"{ searchObjects(pattern: "data") { key bucket } }"#;

        let result = schema.execute(query).await;
        assert!(result.errors.is_empty());

        let data = result
            .data
            .into_json()
            .expect("Failed to convert data to JSON");
        let objects = data
            .get("searchObjects")
            .expect("Failed to get searchObjects")
            .as_array()
            .expect("Failed to convert objects to array");
        assert_eq!(objects.len(), 1);
        assert_eq!(
            objects[0]
                .get("key")
                .expect("Failed to get object key")
                .as_str()
                .expect("Failed to convert key to str"),
            "data/file1.txt"
        );
    }
}
