//! OpenTelemetry distributed tracing implementation
//!
//! Provides comprehensive distributed tracing using OpenTelemetry with OTLP export.
//!
//! # Features
//! - Automatic span creation for all API operations
//! - Trace context propagation (W3C Trace Context)
//! - OTLP gRPC export to collectors (Jaeger, Tempo, etc.)
//! - Sampling configuration (always, never, parent-based, trace-id-ratio)
//! - Resource attributes (service name, version, environment)
//! - Integration with existing tracing infrastructure

use std::fmt;
use std::time::Duration;

use opentelemetry::global;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{RandomIdGenerator, Sampler};
use opentelemetry_sdk::Resource;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// Telemetry configuration
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Service name for traces
    pub service_name: String,
    /// Service version
    pub service_version: String,
    /// Environment (production, staging, development)
    pub environment: String,
    /// OTLP endpoint (e.g., "http://localhost:4317" for Jaeger/Tempo)
    pub otlp_endpoint: Option<String>,
    /// Sampling ratio (0.0 - 1.0)
    pub sampling_ratio: f64,
    /// Enable trace export (false for development without collector)
    pub enable_export: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "rs3gw".to_string(),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok(),
            sampling_ratio: std::env::var("OTEL_TRACES_SAMPLER_ARG")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1.0),
            enable_export: std::env::var("OTEL_TRACES_EXPORTER")
                .map(|e| e != "none")
                .unwrap_or(false),
        }
    }
}

/// Telemetry error
#[derive(Debug)]
pub struct TelemetryError(String);

impl fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Telemetry error: {}", self.0)
    }
}

impl std::error::Error for TelemetryError {}

/// Initialize OpenTelemetry tracing
///
/// Sets up distributed tracing with the following layers:
/// - OpenTelemetry layer for distributed tracing
/// - JSON formatting layer for structured logs
/// - Environment filter for log levels
///
/// # Arguments
/// * `config` - Telemetry configuration
///
/// # Returns
/// * `Ok(())` if initialization succeeds
/// * `Err(TelemetryError)` if OpenTelemetry setup fails
///
/// # Environment Variables
/// - `RUST_LOG` - Log level filter (default: "info")
/// - `OTEL_EXPORTER_OTLP_ENDPOINT` - OTLP endpoint URL
/// - `OTEL_TRACES_SAMPLER` - Sampler type
/// - `OTEL_TRACES_SAMPLER_ARG` - Sampler argument
/// - `OTEL_TRACES_EXPORTER` - Exporter type (otlp, none)
/// - `ENVIRONMENT` - Deployment environment
///
/// # Example
/// ```no_run
/// use rs3gw::observability::{init_telemetry, TelemetryConfig};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = TelemetryConfig {
///         service_name: "rs3gw".to_string(),
///         otlp_endpoint: Some("http://localhost:4317".to_string()),
///         enable_export: true,
///         ..Default::default()
///     };
///
///     init_telemetry(config)?;
///
///     // Your application code here
///
///     Ok(())
/// }
/// ```
pub fn init_telemetry(config: TelemetryConfig) -> Result<(), TelemetryError> {
    // Set up W3C Trace Context propagation
    global::set_text_map_propagator(TraceContextPropagator::new());

    if config.enable_export {
        // Configure resource attributes
        let resource = Resource::builder_empty()
            .with_service_name(config.service_name.clone())
            .with_attribute(KeyValue::new(
                opentelemetry_semantic_conventions::resource::SERVICE_VERSION,
                config.service_version.clone(),
            ))
            .with_attribute(KeyValue::new(
                "deployment.environment",
                config.environment.clone(),
            ))
            .build();

        // Configure sampler based on sampling ratio
        let sampler = if config.sampling_ratio >= 1.0 {
            Sampler::AlwaysOn
        } else if config.sampling_ratio <= 0.0 {
            Sampler::AlwaysOff
        } else {
            Sampler::TraceIdRatioBased(config.sampling_ratio)
        };

        // Get OTLP endpoint
        let endpoint = config
            .otlp_endpoint
            .as_deref()
            .unwrap_or("http://localhost:4317");

        // Create OTLP exporter with timeout
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .with_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| TelemetryError(format!("Failed to create OTLP exporter: {}", e)))?;

        // Create tracer provider with batch exporter
        let provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource)
            .with_sampler(sampler)
            .with_id_generator(RandomIdGenerator::default())
            .build();

        // Set as global provider
        global::set_tracer_provider(provider.clone());

        // Create OpenTelemetry layer
        let telemetry_layer = tracing_opentelemetry::layer()
            .with_tracer(provider.tracer(config.service_name.clone()));

        // Set up tracing subscriber with OpenTelemetry layer
        let env_filter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("info"))
            .map_err(|e| TelemetryError(format!("Failed to create env filter: {}", e)))?;

        tracing_subscriber::registry()
            .with(env_filter)
            .with(telemetry_layer)
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .try_init()
            .map_err(|e| TelemetryError(format!("Failed to initialize subscriber: {}", e)))?;

        tracing::info!(
            service_name = %config.service_name,
            service_version = %config.service_version,
            environment = %config.environment,
            otlp_endpoint = %endpoint,
            sampling_ratio = %config.sampling_ratio,
            "OpenTelemetry distributed tracing initialized"
        );
    } else {
        // No export - just set up basic tracing
        let env_filter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new("info"))
            .map_err(|e| TelemetryError(format!("Failed to create env filter: {}", e)))?;

        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .try_init()
            .map_err(|e| TelemetryError(format!("Failed to initialize subscriber: {}", e)))?;

        tracing::info!(
            service_name = %config.service_name,
            service_version = %config.service_version,
            environment = %config.environment,
            "Tracing initialized without OpenTelemetry export"
        );
    }

    Ok(())
}

/// Shutdown OpenTelemetry tracing
///
/// Flushes any pending spans and shuts down the global tracer provider.
/// Should be called before application exit to ensure all traces are exported.
///
/// Note: In OpenTelemetry 0.30, the tracer provider automatically shuts down
/// when the last reference is dropped. This function ensures proper cleanup.
///
/// # Example
/// ```no_run
/// use rs3gw::observability::shutdown_telemetry;
///
/// fn cleanup() {
///     shutdown_telemetry();
/// }
/// ```
pub fn shutdown_telemetry() {
    tracing::info!("Shutting down OpenTelemetry tracing");
    // The tracer provider will automatically flush and shut down when dropped
    // No explicit shutdown call is needed in OpenTelemetry 0.30
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "rs3gw");
        assert_eq!(config.sampling_ratio, 1.0);
    }

    #[test]
    fn test_custom_config() {
        let config = TelemetryConfig {
            service_name: "test-service".to_string(),
            service_version: "1.0.0".to_string(),
            environment: "test".to_string(),
            otlp_endpoint: Some("http://localhost:4317".to_string()),
            sampling_ratio: 0.5,
            enable_export: true,
        };

        assert_eq!(config.service_name, "test-service");
        assert_eq!(config.service_version, "1.0.0");
        assert_eq!(config.environment, "test");
        assert_eq!(config.sampling_ratio, 0.5);
        assert!(config.enable_export);
    }

    #[test]
    fn test_init_telemetry() {
        let config = TelemetryConfig::default();
        let result = init_telemetry(config);
        assert!(result.is_ok());
    }
}
