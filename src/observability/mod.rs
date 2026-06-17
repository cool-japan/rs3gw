//! Observability infrastructure for distributed tracing, metrics, and profiling
//!
//! This module provides comprehensive observability capabilities:
//! - Distributed tracing across all API operations (OpenTelemetry)
//! - Metrics collection and export (Prometheus)
//! - OTLP (OpenTelemetry Protocol) exporter support
//! - Integration with tracing-subscriber for unified logging/tracing
//! - Continuous profiling for CPU, memory, and I/O
//! - Auto-scaling resource management
//! - Custom business metrics and KPI tracking
//! - Performance anomaly detection
//! - Predictive analytics and insights
//! - Real-time metrics tracking for request rates and bandwidth

pub mod anomaly_detection;
pub mod business_metrics;
pub mod metrics_tracker;
pub mod predictive_analytics;
pub mod profiling;
pub mod resource_manager;
#[cfg(feature = "server")]
pub mod tracing;
pub mod usage;

pub use self::anomaly_detection::{
    Anomaly, AnomalyDetectionConfig, AnomalyDetector, AnomalySeverity, AnomalyType, DetectorStats,
};
pub use self::business_metrics::{
    BusinessMetricsCollector, BusinessMetricsSnapshot, CostMetrics, DataTransferMetrics,
    MetricDefinition, MetricType, MetricValue, RequestPatternMetrics, StorageUtilizationMetrics,
    UserActivityMetrics,
};
pub use self::metrics_tracker::{LifetimeStats, MetricsTracker};
pub use self::predictive_analytics::{
    AccessPatternPrediction, CapacityRecommendation, CostForecast, DataPoint, PatternType,
    PredictiveAnalytics, RecommendationType, StorageGrowthPrediction, TrendDirection, UrgencyLevel,
};
pub use self::profiling::{
    CpuStats, IoStats, MemoryStats, ProfileSnapshot, Profiler, ProfilingConfig,
};
pub use self::resource_manager::{LoadMetrics, ResourceConfig, ResourceManager};
#[cfg(feature = "server")]
pub use self::tracing::{init_telemetry, shutdown_telemetry, TelemetryConfig};
pub use self::usage::{
    BucketUsage, CostBreakdown, LoggingUsageHook, PricingConfig, UsageHook, UsageReport,
    UsageTracker,
};
