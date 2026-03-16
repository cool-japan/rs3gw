//! Apache Arrow Flight Integration
//!
//! Provides high-performance data transfer using Apache Arrow Flight protocol
//! for zero-copy data exchange with big data frameworks (Spark, Dask, Pandas).

use arrow::array::{ArrayRef, BinaryArray, RecordBatch, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::storage::StorageError;
use crate::AppState;

/// Arrow Flight error types
#[derive(Debug, thiserror::Error)]
pub enum FlightError {
    #[error("Arrow error: {0}")]
    ArrowError(#[from] arrow::error::ArrowError),

    #[error("Storage error: {0}")]
    StorageError(#[from] StorageError),

    #[error("Invalid ticket: {0}")]
    InvalidTicket(String),

    #[error("Flight not found: {0}")]
    FlightNotFound(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Flight ticket for data retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightTicket {
    /// Bucket name
    pub bucket: String,

    /// Object key (optional for bucket listing)
    pub key: Option<String>,

    /// Filter expression (optional)
    pub filter: Option<String>,

    /// Projection columns (optional)
    pub columns: Option<Vec<String>>,

    /// Maximum rows to return
    pub limit: Option<usize>,
}

impl FlightTicket {
    /// Create a new flight ticket
    pub fn new(bucket: String) -> Self {
        Self {
            bucket,
            key: None,
            filter: None,
            columns: None,
            limit: None,
        }
    }

    /// Set object key
    pub fn with_key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }

    /// Set filter expression
    pub fn with_filter(mut self, filter: String) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set projection columns
    pub fn with_columns(mut self, columns: Vec<String>) -> Self {
        self.columns = Some(columns);
        self
    }

    /// Set row limit
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Bytes, FlightError> {
        let json =
            serde_json::to_vec(self).map_err(|e| FlightError::SerializationError(e.to_string()))?;
        Ok(Bytes::from(json))
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, FlightError> {
        serde_json::from_slice(bytes).map_err(|e| FlightError::SerializationError(e.to_string()))
    }
}

/// Flight descriptor for data streams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightDescriptor {
    /// Descriptor type
    pub descriptor_type: FlightDescriptorType,

    /// Command or path
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlightDescriptorType {
    /// Command-based descriptor
    Command,

    /// Path-based descriptor
    Path,
}

/// Flight endpoint information
#[derive(Debug, Clone)]
pub struct FlightEndpoint {
    /// Ticket for this endpoint
    pub ticket: FlightTicket,

    /// Locations where data can be retrieved
    pub locations: Vec<String>,
}

/// Flight stream metadata
#[derive(Debug, Clone)]
pub struct FlightStreamMetadata {
    /// Schema for the stream
    pub schema: Arc<Schema>,

    /// Total rows (if known)
    pub total_rows: Option<usize>,

    /// Total bytes (if known)
    pub total_bytes: Option<u64>,

    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Flight data manager
pub struct FlightDataManager {
    /// Active flights
    flights: Arc<RwLock<HashMap<String, FlightStreamMetadata>>>,

    /// Application state
    state: AppState,
}

impl FlightDataManager {
    /// Create a new flight data manager
    pub fn new(state: AppState) -> Self {
        Self {
            flights: Arc::new(RwLock::new(HashMap::new())),
            state,
        }
    }

    /// Get schema for a bucket listing
    pub fn get_listing_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("key", DataType::Utf8, false),
            Field::new("size", DataType::UInt64, false),
            Field::new("last_modified", DataType::Utf8, false),
            Field::new("etag", DataType::Utf8, false),
            Field::new("content_type", DataType::Utf8, true),
        ]))
    }

    /// Get schema for object data
    pub fn get_object_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![Field::new(
            "data",
            DataType::Binary,
            false,
        )]))
    }

    /// List objects as Arrow RecordBatch
    pub async fn list_objects_as_batch(
        &self,
        ticket: &FlightTicket,
    ) -> Result<Vec<RecordBatch>, FlightError> {
        let bucket = &ticket.bucket;
        let prefix = ticket.key.as_deref().unwrap_or("");
        let limit = ticket.limit;

        let (objects, _common_prefixes) = self
            .state
            .storage
            .list_objects(bucket, prefix, None, limit.unwrap_or(1000))
            .await?;

        if objects.is_empty() {
            return Ok(vec![]);
        }

        let schema = Self::get_listing_schema();

        let mut keys = Vec::new();
        let mut sizes = Vec::new();
        let mut last_modifieds = Vec::new();
        let mut etags = Vec::new();
        let mut content_types = Vec::new();

        for obj in objects {
            keys.push(obj.key);
            sizes.push(obj.size);
            last_modifieds.push(obj.last_modified.to_rfc3339());
            etags.push(obj.etag);
            content_types.push(obj.content_type);
        }

        let batch = RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(keys)) as ArrayRef,
                Arc::new(UInt64Array::from(sizes)) as ArrayRef,
                Arc::new(StringArray::from(last_modifieds)) as ArrayRef,
                Arc::new(StringArray::from(etags)) as ArrayRef,
                Arc::new(StringArray::from(content_types)) as ArrayRef,
            ],
        )?;

        Ok(vec![batch])
    }

    /// Get object data as Arrow RecordBatch
    pub async fn get_object_as_batch(
        &self,
        ticket: &FlightTicket,
    ) -> Result<Vec<RecordBatch>, FlightError> {
        let bucket = &ticket.bucket;
        let key = ticket
            .key
            .as_ref()
            .ok_or_else(|| FlightError::InvalidTicket("Object key required".to_string()))?;

        let (_metadata, mut stream) = self.state.storage.get_object(bucket, key).await?;

        // Collect stream into bytes
        use futures::StreamExt;
        let mut data_bytes = Vec::new();
        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            data_bytes.extend_from_slice(&chunk);
        }

        let schema = Self::get_object_schema();

        let batch = RecordBatch::try_new(
            schema,
            vec![Arc::new(BinaryArray::from(vec![data_bytes.as_slice()])) as ArrayRef],
        )?;

        Ok(vec![batch])
    }

    /// Register a flight
    pub async fn register_flight(&self, flight_id: String, metadata: FlightStreamMetadata) {
        let mut flights = self.flights.write().await;
        flights.insert(flight_id, metadata);
    }

    /// Get flight metadata
    pub async fn get_flight_metadata(&self, flight_id: &str) -> Option<FlightStreamMetadata> {
        let flights = self.flights.read().await;
        flights.get(flight_id).cloned()
    }

    /// Remove flight
    pub async fn remove_flight(&self, flight_id: &str) -> Option<FlightStreamMetadata> {
        let mut flights = self.flights.write().await;
        flights.remove(flight_id)
    }

    /// List all active flights
    pub async fn list_flights(&self) -> Vec<String> {
        let flights = self.flights.read().await;
        flights.keys().cloned().collect()
    }

    /// Get Pandas-compatible listing schema with metadata
    pub fn get_pandas_listing_schema() -> Arc<Schema> {
        let fields = vec![
            Field::new("key", DataType::Utf8, false),
            Field::new("size", DataType::UInt64, false),
            Field::new("last_modified", DataType::Utf8, false),
            Field::new("etag", DataType::Utf8, false),
            Field::new("content_type", DataType::Utf8, true),
        ];

        let metadata = PyArrowMetadata::new()
            .as_pandas_dataframe()
            .with_pandas_index("key")
            .with_pandas_column_type("last_modified", "datetime64[ns]")
            .with_pandas_column_type("content_type", "category")
            .build();

        PyArrowMetadata::create_schema_with_metadata(fields, metadata)
    }

    /// Get time-series schema for metrics/logs
    pub fn get_timeseries_schema() -> Arc<Schema> {
        let fields = vec![
            Field::new("timestamp", DataType::Utf8, false),
            Field::new("metric_name", DataType::Utf8, false),
            Field::new("value", DataType::Float64, false),
            Field::new("tags", DataType::Utf8, true),
        ];

        let metadata = PyArrowMetadata::new()
            .as_pandas_dataframe()
            .as_time_series("timestamp")
            .with_pandas_index("timestamp")
            .with_pandas_column_type("timestamp", "datetime64[ns]")
            .with_pandas_column_type("metric_name", "category")
            .build();

        PyArrowMetadata::create_schema_with_metadata(fields, metadata)
    }

    /// Get Spark-compatible schema with partition metadata
    pub fn get_spark_compatible_schema(num_partitions: usize) -> Arc<Schema> {
        let fields = vec![
            Field::new("key", DataType::Utf8, false),
            Field::new("size", DataType::UInt64, false),
            Field::new("last_modified", DataType::Utf8, false),
        ];

        let metadata = PyArrowMetadata::new()
            .with_spark_metadata("sql.partitionColumns", "key")
            .with_dask_partitions(num_partitions)
            .build();

        PyArrowMetadata::create_schema_with_metadata(fields, metadata)
    }
}

/// Flight info for a data stream
#[derive(Debug, Clone)]
pub struct FlightInfo {
    /// Schema for the data
    pub schema: Arc<Schema>,

    /// Descriptor for the flight
    pub descriptor: FlightDescriptor,

    /// Endpoints where data can be retrieved
    pub endpoints: Vec<FlightEndpoint>,

    /// Total rows (if known)
    pub total_rows: Option<usize>,

    /// Total bytes (if known)
    pub total_bytes: Option<u64>,
}

/// Flight action request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightAction {
    /// Action type
    pub action_type: String,

    /// Action body
    pub body: Vec<u8>,
}

/// Flight action result
#[derive(Debug, Clone)]
pub struct FlightActionResult {
    /// Result body
    pub body: Vec<u8>,
}

/// PyArrow compatibility metadata builder
///
/// Provides metadata hints for better PyArrow/Pandas interoperability
pub struct PyArrowMetadata {
    metadata: HashMap<String, String>,
}

impl PyArrowMetadata {
    /// Create new PyArrow metadata builder
    pub fn new() -> Self {
        Self {
            metadata: HashMap::new(),
        }
    }

    /// Mark schema as Pandas DataFrame compatible
    pub fn as_pandas_dataframe(mut self) -> Self {
        self.metadata
            .insert("pandas".to_string(), "true".to_string());
        self
    }

    /// Set Pandas index column
    pub fn with_pandas_index(mut self, column: &str) -> Self {
        self.metadata
            .insert("pandas.index".to_string(), column.to_string());
        self
    }

    /// Set Pandas column types (for categorical, datetime, etc.)
    pub fn with_pandas_column_type(mut self, column: &str, pandas_type: &str) -> Self {
        self.metadata.insert(
            format!("pandas.column.{}.type", column),
            pandas_type.to_string(),
        );
        self
    }

    /// Mark as time-series data with timestamp column
    pub fn as_time_series(mut self, timestamp_column: &str) -> Self {
        self.metadata
            .insert("timeseries".to_string(), "true".to_string());
        self.metadata.insert(
            "timeseries.timestamp_column".to_string(),
            timestamp_column.to_string(),
        );
        self
    }

    /// Add categorical column metadata (for Pandas categorical dtype)
    pub fn with_categorical(mut self, column: &str, categories: Vec<String>) -> Self {
        self.metadata.insert(
            format!("pandas.column.{}.type", column),
            "categorical".to_string(),
        );
        self.metadata.insert(
            format!("pandas.column.{}.categories", column),
            serde_json::to_string(&categories).unwrap_or_default(),
        );
        self
    }

    /// Add custom metadata for Spark/Dask compatibility
    pub fn with_spark_metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata
            .insert(format!("spark.{}", key), value.to_string());
        self
    }

    /// Add Dask partition info
    pub fn with_dask_partitions(mut self, num_partitions: usize) -> Self {
        self.metadata
            .insert("dask.partitions".to_string(), num_partitions.to_string());
        self
    }

    /// Build metadata map
    pub fn build(self) -> HashMap<String, String> {
        self.metadata
    }

    /// Create enhanced schema with PyArrow metadata
    pub fn create_schema_with_metadata(
        fields: Vec<Field>,
        metadata: HashMap<String, String>,
    ) -> Arc<Schema> {
        Arc::new(Schema::new_with_metadata(fields, metadata))
    }
}

impl Default for PyArrowMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// Flight service implementation
pub struct FlightService {
    /// Data manager
    manager: Arc<FlightDataManager>,
}

impl FlightService {
    /// Create a new flight service
    pub fn new(state: AppState) -> Self {
        Self {
            manager: Arc::new(FlightDataManager::new(state)),
        }
    }

    /// Get manager reference
    pub fn manager(&self) -> Arc<FlightDataManager> {
        Arc::clone(&self.manager)
    }

    /// Get flight info for a descriptor
    pub async fn get_flight_info(
        &self,
        descriptor: FlightDescriptor,
    ) -> Result<FlightInfo, FlightError> {
        // Parse descriptor to get bucket/key information
        let ticket = match descriptor.descriptor_type {
            FlightDescriptorType::Command => {
                // Parse command as JSON
                serde_json::from_str(&descriptor.value)
                    .map_err(|e| FlightError::SerializationError(e.to_string()))?
            }
            FlightDescriptorType::Path => {
                // Parse path as bucket/key
                let parts: Vec<&str> = descriptor.value.split('/').collect();
                if parts.is_empty() {
                    return Err(FlightError::InvalidTicket("Empty path".to_string()));
                }

                let mut ticket = FlightTicket::new(parts[0].to_string());
                if parts.len() > 1 {
                    ticket = ticket.with_key(parts[1..].join("/"));
                }
                ticket
            }
        };

        // Determine schema based on whether we're listing or getting an object
        let schema = if ticket.key.is_some() {
            FlightDataManager::get_object_schema()
        } else {
            FlightDataManager::get_listing_schema()
        };

        // Create endpoint
        let endpoint = FlightEndpoint {
            ticket: ticket.clone(),
            locations: vec!["grpc://localhost:9000".to_string()],
        };

        Ok(FlightInfo {
            schema,
            descriptor,
            endpoints: vec![endpoint],
            total_rows: None,
            total_bytes: None,
        })
    }

    /// Execute a flight action
    pub async fn do_action(
        &self,
        action: FlightAction,
    ) -> Result<Vec<FlightActionResult>, FlightError> {
        match action.action_type.as_str() {
            "list_flights" => {
                let flights = self.manager.list_flights().await;
                let body = serde_json::to_vec(&flights)
                    .map_err(|e| FlightError::SerializationError(e.to_string()))?;
                Ok(vec![FlightActionResult { body }])
            }
            "cancel_flight" => {
                let flight_id = String::from_utf8(action.body)
                    .map_err(|e| FlightError::SerializationError(e.to_string()))?;
                self.manager.remove_flight(&flight_id).await;
                Ok(vec![FlightActionResult { body: vec![] }])
            }
            _ => Err(FlightError::InvalidTicket(format!(
                "Unknown action: {}",
                action.action_type
            ))),
        }
    }

    /// Get data stream for a ticket
    pub async fn do_get(&self, ticket: FlightTicket) -> Result<Vec<RecordBatch>, FlightError> {
        if ticket.key.is_some() {
            // Get single object
            self.manager.get_object_as_batch(&ticket).await
        } else {
            // List objects
            self.manager.list_objects_as_batch(&ticket).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{StorageEngine, TrainingManager};
    use std::env;

    async fn create_test_state() -> AppState {
        let temp_dir = env::temp_dir().join(format!("rs3gw_flight_test_{}", uuid::Uuid::new_v4()));
        let storage = Arc::new(StorageEngine::new(temp_dir.clone()).expect("storage"));

        use crate::api::EventBroadcaster;
        use crate::Config;

        // Use shared test metrics handle to avoid conflicts with other test modules
        let metrics_handle = crate::test_helpers::get_test_metrics_handle();

        let config = Config::default();
        let event_broadcaster = EventBroadcaster::new();

        // Initialize preprocessing manager
        let preprocessing_path = temp_dir.join("preprocessing");
        let preprocessing_manager = Arc::new(
            crate::storage::preprocessing::PreprocessingManager::new(preprocessing_path),
        );

        let predictive_analytics = Arc::new(crate::observability::PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000_000,
        ));

        let metrics_tracker = Arc::new(crate::observability::MetricsTracker::new());

        let select_result_cache =
            Arc::new(crate::api::SelectResultCache::new(100, 10 * 1024 * 1024));

        let query_intelligence = std::sync::Arc::new(crate::api::QueryIntelligence::new());

        AppState {
            config,
            storage,
            metrics_handle,
            cache: None,
            throttle: None,
            quota: None,
            event_broadcaster,
            query_plan_cache: None,
            select_result_cache,
            query_intelligence,
            advanced_replication: None,
            preprocessing_manager,
            predictive_analytics,
            metrics_tracker,
            training_manager: Arc::new(TrainingManager::new(temp_dir.join("training"))),
            start_time: std::time::Instant::now(),
            verifier: None,
            auth_failure_counts: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            in_flight: crate::InFlightTracker::new(),
        }
    }

    #[tokio::test]
    async fn test_flight_ticket_serialization() {
        let ticket = FlightTicket::new("my-bucket".to_string())
            .with_key("my-object.txt".to_string())
            .with_limit(100);

        let bytes = ticket.to_bytes().expect("serialize");
        let deserialized = FlightTicket::from_bytes(&bytes).expect("deserialize");

        assert_eq!(ticket.bucket, deserialized.bucket);
        assert_eq!(ticket.key, deserialized.key);
        assert_eq!(ticket.limit, deserialized.limit);
    }

    #[tokio::test]
    async fn test_flight_service_creation() {
        let state = create_test_state().await;
        let service = FlightService::new(state);
        let manager = service.manager();

        let flights = manager.list_flights().await;
        assert!(flights.is_empty());
    }

    #[tokio::test]
    async fn test_get_listing_schema() {
        let schema = FlightDataManager::get_listing_schema();
        assert_eq!(schema.fields().len(), 5);
        assert_eq!(schema.field(0).name(), "key");
        assert_eq!(schema.field(1).name(), "size");
        assert_eq!(schema.field(2).name(), "last_modified");
    }

    #[tokio::test]
    async fn test_get_object_schema() {
        let schema = FlightDataManager::get_object_schema();
        assert_eq!(schema.fields().len(), 1);
        assert_eq!(schema.field(0).name(), "data");
        assert_eq!(schema.field(0).data_type(), &DataType::Binary);
    }

    #[tokio::test]
    async fn test_list_objects_as_batch() {
        let state = create_test_state().await;
        let manager = FlightDataManager::new(state.clone());

        // Create test bucket and objects
        state
            .storage
            .create_bucket("test-bucket")
            .await
            .expect("create bucket");
        state
            .storage
            .put_object(
                "test-bucket",
                "obj1.txt",
                "text/plain",
                HashMap::new(),
                b"data1".to_vec().into(),
            )
            .await
            .expect("put object 1");
        state
            .storage
            .put_object(
                "test-bucket",
                "obj2.txt",
                "text/plain",
                HashMap::new(),
                b"data2".to_vec().into(),
            )
            .await
            .expect("put object 2");

        let ticket = FlightTicket::new("test-bucket".to_string());
        let batches = manager.list_objects_as_batch(&ticket).await.expect("list");

        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 5);
    }

    #[tokio::test]
    async fn test_get_object_as_batch() {
        let state = create_test_state().await;
        let manager = FlightDataManager::new(state.clone());

        // Create test bucket and object
        state
            .storage
            .create_bucket("test-bucket")
            .await
            .expect("create bucket");
        let test_data = b"Hello, Arrow Flight!";
        state
            .storage
            .put_object(
                "test-bucket",
                "test.txt",
                "text/plain",
                HashMap::new(),
                test_data.to_vec().into(),
            )
            .await
            .expect("put object");

        let ticket = FlightTicket::new("test-bucket".to_string()).with_key("test.txt".to_string());

        let batches = manager.get_object_as_batch(&ticket).await.expect("get");

        assert_eq!(batches.len(), 1);
        let batch = &batches[0];
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 1);

        // Verify data
        let data_array = batch
            .column(0)
            .as_any()
            .downcast_ref::<BinaryArray>()
            .expect("binary array");
        assert_eq!(data_array.value(0), test_data);
    }

    #[tokio::test]
    async fn test_flight_info_creation() {
        let state = create_test_state().await;
        let service = FlightService::new(state);

        let descriptor = FlightDescriptor {
            descriptor_type: FlightDescriptorType::Path,
            value: "test-bucket".to_string(),
        };

        let info = service.get_flight_info(descriptor).await.expect("get info");
        assert_eq!(info.endpoints.len(), 1);
        assert_eq!(info.schema.fields().len(), 5); // Listing schema
    }

    #[tokio::test]
    async fn test_do_action_list_flights() {
        let state = create_test_state().await;
        let service = FlightService::new(state);

        let action = FlightAction {
            action_type: "list_flights".to_string(),
            body: vec![],
        };

        let results = service.do_action(action).await.expect("do action");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_pyarrow_metadata_builder() {
        let metadata = PyArrowMetadata::new()
            .as_pandas_dataframe()
            .with_pandas_index("id")
            .with_pandas_column_type("timestamp", "datetime64[ns]")
            .build();

        assert_eq!(metadata.get("pandas"), Some(&"true".to_string()));
        assert_eq!(metadata.get("pandas.index"), Some(&"id".to_string()));
        assert_eq!(
            metadata.get("pandas.column.timestamp.type"),
            Some(&"datetime64[ns]".to_string())
        );
    }

    #[test]
    fn test_pyarrow_categorical_metadata() {
        let categories = vec!["cat1".to_string(), "cat2".to_string(), "cat3".to_string()];
        let metadata = PyArrowMetadata::new()
            .with_categorical("status", categories.clone())
            .build();

        assert_eq!(
            metadata.get("pandas.column.status.type"),
            Some(&"categorical".to_string())
        );
        let categories_json = metadata
            .get("pandas.column.status.categories")
            .expect("categories");
        let parsed: Vec<String> = serde_json::from_str(categories_json).expect("parse");
        assert_eq!(parsed, categories);
    }

    #[test]
    fn test_pyarrow_timeseries_metadata() {
        let metadata = PyArrowMetadata::new().as_time_series("timestamp").build();

        assert_eq!(metadata.get("timeseries"), Some(&"true".to_string()));
        assert_eq!(
            metadata.get("timeseries.timestamp_column"),
            Some(&"timestamp".to_string())
        );
    }

    #[test]
    fn test_pandas_listing_schema() {
        let schema = FlightDataManager::get_pandas_listing_schema();
        assert_eq!(schema.fields().len(), 5);

        let metadata = schema.metadata();
        assert_eq!(metadata.get("pandas"), Some(&"true".to_string()));
        assert_eq!(metadata.get("pandas.index"), Some(&"key".to_string()));
    }

    #[test]
    fn test_timeseries_schema() {
        let schema = FlightDataManager::get_timeseries_schema();
        assert_eq!(schema.fields().len(), 4);

        let metadata = schema.metadata();
        assert_eq!(metadata.get("timeseries"), Some(&"true".to_string()));
        assert_eq!(
            metadata.get("timeseries.timestamp_column"),
            Some(&"timestamp".to_string())
        );
    }

    #[test]
    fn test_spark_compatible_schema() {
        let num_partitions = 10;
        let schema = FlightDataManager::get_spark_compatible_schema(num_partitions);
        assert_eq!(schema.fields().len(), 3);

        let metadata = schema.metadata();
        assert_eq!(
            metadata.get("spark.sql.partitionColumns"),
            Some(&"key".to_string())
        );
        assert_eq!(
            metadata.get("dask.partitions"),
            Some(&num_partitions.to_string())
        );
    }
}
