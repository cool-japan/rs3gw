//! Advanced replication API handlers
//!
//! This module provides HTTP endpoints for configuring and monitoring
//! cross-region replication with advanced features like WAN optimization,
//! selective replication, and comprehensive metrics.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cluster::{
    CrossRegionConfig, DestinationMetrics, Region, RegionLocation, ReplicationFilter,
    ReplicationMetrics,
};
use crate::AppState;

/// Request to configure cross-region replication for a bucket
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SetReplicationConfigRequest {
    /// Source region information
    pub source_region: RegionInfo,

    /// Destination regions
    pub destination_regions: Vec<RegionInfo>,

    /// Replication filter (optional)
    #[serde(default)]
    pub filter: ReplicationFilterInput,

    /// Enable WAN optimization (default: true)
    #[serde(default = "default_true")]
    pub wan_optimization: bool,

    /// Compression level for WAN optimization (1-9, default: 3)
    #[serde(default = "default_compression_level")]
    pub compression_level: i32,

    /// Batch size for WAN optimization (default: 100)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Batch timeout in seconds (default: 10)
    #[serde(default = "default_batch_timeout")]
    pub batch_timeout_secs: u64,

    /// Enable bandwidth throttling (default: false)
    #[serde(default)]
    pub enable_bandwidth_limit: bool,

    /// Bandwidth limit in MB/s (0 = unlimited)
    #[serde(default)]
    pub bandwidth_limit_mbps: u64,

    /// Priority (0-100, default: 50)
    #[serde(default = "default_priority")]
    pub priority: u8,

    /// Replicate delete operations (default: true)
    #[serde(default = "default_true")]
    pub replicate_deletes: bool,

    /// Replicate metadata changes (default: true)
    #[serde(default = "default_true")]
    pub replicate_metadata: bool,
}

fn default_true() -> bool {
    true
}

fn default_compression_level() -> i32 {
    3
}

fn default_batch_size() -> usize {
    100
}

fn default_batch_timeout() -> u64 {
    10
}

fn default_priority() -> u8 {
    50
}

/// Region information for API requests
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RegionInfo {
    /// Region identifier (e.g., "us-east-1", "eu-west-1")
    pub id: String,

    /// Human-readable name
    pub name: String,

    /// Geographic location
    pub location: LocationInfo,

    /// Whether this is the local region
    #[serde(default)]
    pub is_local: bool,
}

/// Location information
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct LocationInfo {
    /// Continent code (e.g., "NA", "EU", "AS")
    pub continent: String,

    /// Country code (ISO 3166-1 alpha-2)
    pub country: String,

    /// City/metro area
    pub city: String,
}

/// Replication filter input
#[derive(Debug, Clone, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct ReplicationFilterInput {
    /// Prefix filters (any match = include)
    #[serde(default)]
    pub prefix_filters: Vec<String>,

    /// Tag filters (all must match)
    #[serde(default)]
    pub tag_filters: HashMap<String, String>,

    /// Minimum object size in bytes
    pub min_size: Option<u64>,

    /// Maximum object size in bytes
    pub max_size: Option<u64>,

    /// Storage class filter
    #[serde(default)]
    pub storage_classes: Vec<String>,

    /// Exclude prefixes
    #[serde(default)]
    pub exclude_prefixes: Vec<String>,
}

/// Response indicating success
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SuccessResponse {
    pub success: bool,
    pub message: String,
}

/// PUT /api/replication/{bucket}/config
/// Configure cross-region replication for a bucket
pub async fn set_replication_config(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    Json(req): Json<SetReplicationConfigRequest>,
) -> Result<Json<SuccessResponse>, StatusCode> {
    let Some(manager) = &state.advanced_replication else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    // Convert API types to internal types
    let source_region = Region {
        id: req.source_region.id,
        name: req.source_region.name,
        location: RegionLocation {
            continent: req.source_region.location.continent,
            country: req.source_region.location.country,
            city: req.source_region.location.city,
        },
        is_local: req.source_region.is_local,
    };

    let destination_regions = req
        .destination_regions
        .into_iter()
        .map(|r| Region {
            id: r.id,
            name: r.name,
            location: RegionLocation {
                continent: r.location.continent,
                country: r.location.country,
                city: r.location.city,
            },
            is_local: r.is_local,
        })
        .collect();

    let filter = ReplicationFilter {
        prefix_filters: req.filter.prefix_filters,
        tag_filters: req.filter.tag_filters,
        min_size: req.filter.min_size,
        max_size: req.filter.max_size,
        storage_classes: req.filter.storage_classes,
        exclude_prefixes: req.filter.exclude_prefixes,
    };

    let config = CrossRegionConfig {
        source_region,
        destination_regions,
        filter,
        wan_optimization: req.wan_optimization,
        compression_level: req.compression_level,
        batch_size: req.batch_size,
        batch_timeout: std::time::Duration::from_secs(req.batch_timeout_secs),
        enable_bandwidth_limit: req.enable_bandwidth_limit,
        bandwidth_limit_mbps: req.bandwidth_limit_mbps,
        priority: req.priority,
        replicate_deletes: req.replicate_deletes,
        replicate_metadata: req.replicate_metadata,
    };

    manager.set_bucket_config(&bucket, config).await;

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Replication configured for bucket: {}", bucket),
    }))
}

/// GET /api/replication/{bucket}/config
/// Get cross-region replication configuration for a bucket
pub async fn get_replication_config(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<CrossRegionConfig>, StatusCode> {
    let Some(manager) = &state.advanced_replication else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    match manager.get_bucket_config(&bucket).await {
        Some(config) => Ok(Json(config)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// DELETE /api/replication/{bucket}/config
/// Remove cross-region replication configuration for a bucket
pub async fn delete_replication_config(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
) -> Result<Json<SuccessResponse>, StatusCode> {
    let Some(manager) = &state.advanced_replication else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    manager.remove_bucket_config(&bucket).await;

    Ok(Json(SuccessResponse {
        success: true,
        message: format!("Replication configuration removed for bucket: {}", bucket),
    }))
}

/// GET /api/replication/metrics
/// Get overall replication metrics
pub async fn get_replication_metrics(
    State(state): State<AppState>,
) -> Result<Json<ReplicationMetrics>, StatusCode> {
    let Some(manager) = &state.advanced_replication else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    let metrics = manager.get_metrics().await;
    Ok(Json(metrics))
}

/// GET /api/replication/metrics/{destination}
/// Get replication metrics for a specific destination
pub async fn get_destination_metrics(
    State(state): State<AppState>,
    Path(destination): Path<String>,
) -> Result<Json<DestinationMetrics>, StatusCode> {
    let Some(manager) = &state.advanced_replication else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    match manager.get_destination_metrics(&destination).await {
        Some(metrics) => Ok(Json(metrics)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

/// POST /api/replication/flush
/// Manually flush pending replication batches
pub async fn flush_replication_batches(
    State(state): State<AppState>,
) -> Result<Json<HashMap<String, usize>>, StatusCode> {
    let Some(manager) = &state.advanced_replication else {
        return Err(StatusCode::NOT_IMPLEMENTED);
    };

    match manager.flush_batches().await {
        Ok(flushed) => Ok(Json(flushed)),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}
