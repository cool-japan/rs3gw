//! S3 API layer
//!
//! This module implements the S3-compatible REST API.

pub mod arrow_flight;
// Disabled: arrow_flight_sql needs API updates (moved to .bak)
// pub mod arrow_flight_sql;
pub mod batch;
pub mod bucket_stubs;
pub mod graphql;
pub mod handlers;
pub mod multipart;
pub mod observability_handlers;
pub mod openapi;
pub mod preprocessing_handlers;
pub mod query_intelligence;
pub mod query_intelligence_handlers;
pub mod replication_handlers;
pub mod s3_router;
pub mod select;
pub mod select_cache;
pub mod select_cache_handlers;
pub mod select_optimizer;
pub mod throttle;
pub mod tiering_handlers;
pub mod training_handlers;
pub mod utils;
pub mod websocket;
pub mod xml_responses;

pub use arrow_flight::{
    FlightAction, FlightActionResult, FlightDataManager, FlightDescriptor, FlightDescriptorType,
    FlightEndpoint, FlightError, FlightInfo, FlightService, FlightStreamMetadata, FlightTicket,
};
// Disabled: arrow_flight_sql needs API updates (moved to .bak)
// pub use arrow_flight_sql::{
//     FlightSqlCommand, FlightSqlService, PreparedStatementHandle,
// };
pub use batch::{
    BatchError, BatchJob, BatchManager, BatchOperation, JobManifest, JobReport, JobStatus,
    ManifestObject, ProgressSummary,
};
pub use query_intelligence::{
    DataStatistics, ExecutionStrategy, IndexReason, IndexRecommendation, IndexType, QueryCost,
    QueryIntelligence, QuerySimilarity, QueryStats, QueryStatsSummary,
};
pub use select::{
    CsvInput, CsvOutput, FileHeaderInfo, InputFormat, JsonInput, JsonOutput, JsonType,
    OutputFormat, SelectError, SelectExecutor, SelectRequest,
};
pub use select_cache::{CacheStats as SelectCacheStats, SelectResultCache};
pub use select_optimizer::{OptimizedSelectExecutor, QueryPlanCache};
pub use throttle::{ThrottleConfig, ThrottleManager, ThrottleResult, ThrottleStats};
pub use websocket::{EventBroadcaster, S3Event, S3EventType};
