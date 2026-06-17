//! S3 API layer
//!
//! This module implements the S3-compatible REST API.

#[cfg(feature = "formats")]
pub mod arrow_flight;
pub mod auth_middleware;
pub mod cors;
pub mod cors_middleware;
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
#[cfg(feature = "formats")]
pub mod query_intelligence;
#[cfg(feature = "formats")]
pub mod query_intelligence_handlers;
pub mod replication_handlers;
pub mod s3_router;
pub mod select;
pub mod select_cache;
pub mod select_cache_handlers;
#[cfg(feature = "formats")]
pub mod select_optimizer;
pub mod sse;
pub mod throttle;
pub mod tiering_handlers;
pub mod training_handlers;
pub mod utils;
pub mod websocket;
pub mod xml_responses;

#[cfg(feature = "formats")]
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
#[cfg(feature = "formats")]
pub use query_intelligence::{
    DataStatistics, ExecutionStrategy, IndexReason, IndexRecommendation, IndexType, QueryCost,
    QueryIntelligence, QuerySimilarity, QueryStats, QueryStatsSummary,
};
#[cfg(feature = "formats")]
pub use select::{
    CsvInput, CsvOutput, FileHeaderInfo, InputFormat, JsonInput, JsonOutput, JsonType,
    OutputFormat, SelectError, SelectExecutor, SelectRequest,
};
pub use select_cache::{CacheStats as SelectCacheStats, SelectResultCache};
#[cfg(feature = "formats")]
pub use select_optimizer::{OptimizedSelectExecutor, QueryPlanCache};
pub use throttle::{ThrottleConfig, ThrottleManager, ThrottleResult, ThrottleStats};
pub use websocket::{EventBroadcaster, S3Event, S3EventType};
