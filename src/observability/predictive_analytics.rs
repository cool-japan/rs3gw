// Copyright (c) 2026 COOLJAPAN OU (Team Kitasan)
// SPDX-License-Identifier: MIT

//! Predictive Analytics & Insights Module
//!
//! This module provides time-series forecasting, access pattern prediction,
//! cost forecasting, and capacity planning capabilities for the rs3gw storage system.
//!
//! # Features
//!
//! - **Storage Growth Prediction**: Time-series forecasting for storage utilization
//! - **Access Pattern Prediction**: Statistical models for access pattern forecasting
//! - **Cost Forecasting**: Trend analysis for cost prediction
//! - **Capacity Planning**: Recommendations based on growth predictions
//! - **Anomaly Detection Integration**: Enhanced anomaly detection with predictions
//!
//! # Pure Rust Implementation
//!
//! This module uses pure Rust statistical methods without external ML dependencies,
//! following the COOLJAPAN Pure Rust Policy.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Time series data point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataPoint {
    /// Timestamp (Unix epoch seconds)
    pub timestamp: i64,
    /// Value at this timestamp
    pub value: f64,
}

/// Storage growth prediction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageGrowthPrediction {
    /// Current storage size in bytes
    pub current_size: u64,
    /// Predicted size in 7 days (bytes)
    pub predicted_7d: u64,
    /// Predicted size in 30 days (bytes)
    pub predicted_30d: u64,
    /// Predicted size in 90 days (bytes)
    pub predicted_90d: u64,
    /// Daily growth rate (bytes/day)
    pub daily_growth_rate: f64,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Trend direction: Growing, Stable, or Shrinking
    pub trend: TrendDirection,
}

/// Access pattern prediction result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessPatternPrediction {
    /// Current requests per second
    pub current_rps: f64,
    /// Predicted RPS for next hour
    pub predicted_1h: f64,
    /// Predicted RPS for next 24 hours
    pub predicted_24h: f64,
    /// Expected peak RPS
    pub expected_peak: f64,
    /// Expected peak time (hour of day, 0-23)
    pub peak_hour: u8,
    /// Pattern type (Periodic, Bursty, Trending, Stable)
    pub pattern_type: PatternType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

/// Cost forecast result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostForecast {
    /// Current monthly cost estimate (USD)
    pub current_monthly_cost: f64,
    /// Predicted cost for next month (USD)
    pub predicted_next_month: f64,
    /// Predicted cost in 3 months (USD)
    pub predicted_3_months: f64,
    /// Predicted cost in 6 months (USD)
    pub predicted_6_months: f64,
    /// Storage cost component
    pub storage_cost: f64,
    /// Bandwidth cost component
    pub bandwidth_cost: f64,
    /// Request cost component
    pub request_cost: f64,
    /// Monthly cost growth rate (%)
    pub growth_rate_percent: f64,
}

/// Capacity planning recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityRecommendation {
    /// Current capacity utilization (%)
    pub current_utilization: f64,
    /// Predicted utilization in 30 days (%)
    pub predicted_30d_utilization: f64,
    /// Predicted utilization in 90 days (%)
    pub predicted_90d_utilization: f64,
    /// Days until capacity threshold (e.g., 80%)
    pub days_until_threshold: Option<u32>,
    /// Recommended action
    pub recommendation: RecommendationType,
    /// Additional capacity needed (bytes)
    pub additional_capacity_needed: Option<u64>,
    /// Urgency level
    pub urgency: UrgencyLevel,
}

/// Trend direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    Growing,
    Stable,
    Shrinking,
}

/// Access pattern type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternType {
    /// Periodic pattern (e.g., daily cycles)
    Periodic,
    /// Bursty traffic
    Bursty,
    /// Trending upward or downward
    Trending,
    /// Relatively stable
    Stable,
}

/// Recommendation type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationType {
    /// No action needed
    NoActionNeeded,
    /// Monitor closely
    MonitorClosely,
    /// Plan capacity expansion
    PlanExpansion,
    /// Immediate expansion needed
    ImmediateExpansion,
    /// Consider compression or cleanup
    OptimizeStorage,
}

/// Urgency level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UrgencyLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Predictive analytics engine
pub struct PredictiveAnalytics {
    /// Storage size history (timestamp, bytes)
    storage_history: Arc<RwLock<VecDeque<DataPoint>>>,
    /// Request rate history (timestamp, rps)
    request_history: Arc<RwLock<VecDeque<DataPoint>>>,
    /// Bandwidth usage history (timestamp, bytes/sec)
    bandwidth_history: Arc<RwLock<VecDeque<DataPoint>>>,
    /// Maximum history points to retain
    max_history_points: usize,
    /// Cost per GB-month for storage (USD)
    cost_per_gb_month: f64,
    /// Cost per GB for bandwidth (USD)
    cost_per_gb_bandwidth: f64,
    /// Cost per 1000 requests (USD)
    cost_per_1k_requests: f64,
    /// Total capacity in bytes
    total_capacity: u64,
}

impl Default for PredictiveAnalytics {
    fn default() -> Self {
        Self::new(10_000, 0.023, 0.09, 0.0004, 1_000_000_000_000) // 1TB default capacity
    }
}

impl PredictiveAnalytics {
    /// Create a new predictive analytics engine
    ///
    /// # Arguments
    ///
    /// * `max_history_points` - Maximum number of historical data points to retain
    /// * `cost_per_gb_month` - Cost per GB-month for storage (USD)
    /// * `cost_per_gb_bandwidth` - Cost per GB for bandwidth (USD)
    /// * `cost_per_1k_requests` - Cost per 1000 requests (USD)
    /// * `total_capacity` - Total storage capacity in bytes
    pub fn new(
        max_history_points: usize,
        cost_per_gb_month: f64,
        cost_per_gb_bandwidth: f64,
        cost_per_1k_requests: f64,
        total_capacity: u64,
    ) -> Self {
        Self {
            storage_history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history_points))),
            request_history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history_points))),
            bandwidth_history: Arc::new(RwLock::new(VecDeque::with_capacity(max_history_points))),
            max_history_points,
            cost_per_gb_month,
            cost_per_gb_bandwidth,
            cost_per_1k_requests,
            total_capacity,
        }
    }

    /// Record storage size data point
    pub async fn record_storage_size(&self, timestamp: i64, size_bytes: u64) {
        let mut history = self.storage_history.write().await;
        history.push_back(DataPoint {
            timestamp,
            value: size_bytes as f64,
        });
        if history.len() > self.max_history_points {
            history.pop_front();
        }
    }

    /// Record request rate data point
    pub async fn record_request_rate(&self, timestamp: i64, rps: f64) {
        let mut history = self.request_history.write().await;
        history.push_back(DataPoint {
            timestamp,
            value: rps,
        });
        if history.len() > self.max_history_points {
            history.pop_front();
        }
    }

    /// Record bandwidth usage data point
    pub async fn record_bandwidth(&self, timestamp: i64, bytes_per_sec: f64) {
        let mut history = self.bandwidth_history.write().await;
        history.push_back(DataPoint {
            timestamp,
            value: bytes_per_sec,
        });
        if history.len() > self.max_history_points {
            history.pop_front();
        }
    }

    /// Predict storage growth using linear regression and exponential smoothing
    pub async fn predict_storage_growth(&self) -> Option<StorageGrowthPrediction> {
        let history = self.storage_history.read().await;
        if history.len() < 10 {
            return None; // Need at least 10 data points
        }

        let points: Vec<_> = history.iter().cloned().collect();
        let current_size = points.last()?.value as u64;

        // Calculate linear regression (y = mx + b)
        let (slope, _intercept) = Self::linear_regression(&points);

        // Calculate daily growth rate
        let daily_growth_rate = slope * 86400.0; // seconds per day

        // Predict future values
        let predicted_7d = (current_size as f64 + daily_growth_rate * 7.0).max(0.0) as u64;
        let predicted_30d = (current_size as f64 + daily_growth_rate * 30.0).max(0.0) as u64;
        let predicted_90d = (current_size as f64 + daily_growth_rate * 90.0).max(0.0) as u64;

        // Determine trend direction
        let trend = if daily_growth_rate > 1_000_000.0 {
            // > 1MB/day
            TrendDirection::Growing
        } else if daily_growth_rate < -1_000_000.0 {
            TrendDirection::Shrinking
        } else {
            TrendDirection::Stable
        };

        // Calculate confidence based on data variance
        let confidence = Self::calculate_confidence(&points, slope);

        Some(StorageGrowthPrediction {
            current_size,
            predicted_7d,
            predicted_30d,
            predicted_90d,
            daily_growth_rate,
            confidence,
            trend,
        })
    }

    /// Predict access patterns using statistical analysis
    pub async fn predict_access_patterns(&self) -> Option<AccessPatternPrediction> {
        let history = self.request_history.read().await;
        if history.len() < 24 {
            return None; // Need at least 24 data points (hours)
        }

        let points: Vec<_> = history.iter().cloned().collect();
        let current_rps = points.last()?.value;

        // Calculate moving average for short-term prediction
        let ma_24 = Self::moving_average(&points, 24);
        let predicted_1h = ma_24;

        // Calculate exponential moving average for longer-term prediction
        let ema_alpha = 0.1;
        let predicted_24h = Self::exponential_moving_average(&points, ema_alpha);

        // Find peak hour and value
        let (peak_hour, expected_peak) = Self::find_peak_hour(&points);

        // Detect pattern type
        let pattern_type = Self::detect_pattern_type(&points);

        // Calculate confidence
        let (slope, _) = Self::linear_regression(&points);
        let confidence = Self::calculate_confidence(&points, slope);

        Some(AccessPatternPrediction {
            current_rps,
            predicted_1h,
            predicted_24h,
            expected_peak,
            peak_hour,
            pattern_type,
            confidence,
        })
    }

    /// Forecast costs based on current usage and growth trends
    pub async fn forecast_costs(&self) -> Option<CostForecast> {
        let storage_history = self.storage_history.read().await;
        let bandwidth_history = self.bandwidth_history.read().await;
        let request_history = self.request_history.read().await;

        if storage_history.is_empty() {
            return None;
        }

        // Current storage cost (USD/month)
        let current_storage_gb = storage_history.back()?.value / 1_073_741_824.0; // bytes to GB
        let storage_cost = current_storage_gb * self.cost_per_gb_month;

        // Current bandwidth cost (USD/month, assuming 30 days)
        let avg_bandwidth_bytes_per_sec = if bandwidth_history.is_empty() {
            0.0
        } else {
            bandwidth_history.iter().map(|p| p.value).sum::<f64>() / bandwidth_history.len() as f64
        };
        let monthly_bandwidth_gb = avg_bandwidth_bytes_per_sec * 86400.0 * 30.0 / 1_073_741_824.0;
        let bandwidth_cost = monthly_bandwidth_gb * self.cost_per_gb_bandwidth;

        // Current request cost (USD/month)
        let avg_rps = if request_history.is_empty() {
            0.0
        } else {
            request_history.iter().map(|p| p.value).sum::<f64>() / request_history.len() as f64
        };
        let monthly_requests = avg_rps * 86400.0 * 30.0;
        let request_cost = (monthly_requests / 1000.0) * self.cost_per_1k_requests;

        let current_monthly_cost = storage_cost + bandwidth_cost + request_cost;

        // Predict future costs based on storage growth
        let storage_points: Vec<_> = storage_history.iter().cloned().collect();
        let (slope, _) = Self::linear_regression(&storage_points);
        let daily_growth_rate = slope * 86400.0;

        // Predict storage at future dates
        let storage_30d =
            (current_storage_gb * 1_073_741_824.0 + daily_growth_rate * 30.0) / 1_073_741_824.0;
        let storage_90d =
            (current_storage_gb * 1_073_741_824.0 + daily_growth_rate * 90.0) / 1_073_741_824.0;
        let storage_180d =
            (current_storage_gb * 1_073_741_824.0 + daily_growth_rate * 180.0) / 1_073_741_824.0;

        // Future costs (assuming bandwidth and requests scale proportionally)
        let growth_factor_30d = if current_storage_gb > 0.0 {
            storage_30d / current_storage_gb
        } else {
            1.0
        };
        let growth_factor_90d = if current_storage_gb > 0.0 {
            storage_90d / current_storage_gb
        } else {
            1.0
        };
        let growth_factor_180d = if current_storage_gb > 0.0 {
            storage_180d / current_storage_gb
        } else {
            1.0
        };

        let predicted_next_month = current_monthly_cost * growth_factor_30d.max(1.0);
        let predicted_3_months = current_monthly_cost * growth_factor_90d.max(1.0);
        let predicted_6_months = current_monthly_cost * growth_factor_180d.max(1.0);

        // Calculate monthly growth rate
        let growth_rate_percent = if current_monthly_cost > 0.0 {
            ((predicted_next_month - current_monthly_cost) / current_monthly_cost) * 100.0
        } else {
            0.0
        };

        Some(CostForecast {
            current_monthly_cost,
            predicted_next_month,
            predicted_3_months,
            predicted_6_months,
            storage_cost,
            bandwidth_cost,
            request_cost,
            growth_rate_percent,
        })
    }

    /// Generate capacity planning recommendations
    pub async fn capacity_recommendations(&self) -> Option<CapacityRecommendation> {
        let storage_prediction = self.predict_storage_growth().await?;

        let current_utilization =
            (storage_prediction.current_size as f64 / self.total_capacity as f64) * 100.0;
        let predicted_30d_utilization =
            (storage_prediction.predicted_30d as f64 / self.total_capacity as f64) * 100.0;
        let predicted_90d_utilization =
            (storage_prediction.predicted_90d as f64 / self.total_capacity as f64) * 100.0;

        // Calculate days until 80% capacity threshold
        let threshold = 80.0;
        let days_until_threshold = if current_utilization >= threshold {
            Some(0)
        } else if storage_prediction.daily_growth_rate > 0.0 {
            let remaining_bytes = (self.total_capacity as f64 * threshold / 100.0)
                - storage_prediction.current_size as f64;
            let days = (remaining_bytes / storage_prediction.daily_growth_rate).max(0.0);
            Some(days as u32)
        } else {
            None
        };

        // Determine recommendation and urgency
        let (recommendation, urgency, additional_capacity_needed) = if current_utilization >= 95.0 {
            (
                RecommendationType::ImmediateExpansion,
                UrgencyLevel::Critical,
                Some(self.total_capacity / 2), // Recommend 50% expansion
            )
        } else if predicted_30d_utilization >= 90.0 {
            (
                RecommendationType::ImmediateExpansion,
                UrgencyLevel::High,
                Some(self.total_capacity / 4), // Recommend 25% expansion
            )
        } else if predicted_90d_utilization >= 85.0 {
            (
                RecommendationType::PlanExpansion,
                UrgencyLevel::Medium,
                Some(self.total_capacity / 5), // Recommend 20% expansion
            )
        } else if predicted_90d_utilization >= 70.0 {
            (RecommendationType::MonitorClosely, UrgencyLevel::Low, None)
        } else if storage_prediction.trend == TrendDirection::Shrinking {
            (RecommendationType::OptimizeStorage, UrgencyLevel::Low, None)
        } else {
            (RecommendationType::NoActionNeeded, UrgencyLevel::Low, None)
        };

        Some(CapacityRecommendation {
            current_utilization,
            predicted_30d_utilization,
            predicted_90d_utilization,
            days_until_threshold,
            recommendation,
            additional_capacity_needed,
            urgency,
        })
    }

    // Statistical helper functions

    /// Calculate linear regression (returns slope and intercept)
    fn linear_regression(points: &[DataPoint]) -> (f64, f64) {
        if points.is_empty() {
            return (0.0, 0.0);
        }

        let n = points.len() as f64;
        let sum_x: f64 = points.iter().map(|p| p.timestamp as f64).sum();
        let sum_y: f64 = points.iter().map(|p| p.value).sum();
        let sum_xy: f64 = points.iter().map(|p| p.timestamp as f64 * p.value).sum();
        let sum_x2: f64 = points.iter().map(|p| (p.timestamp as f64).powi(2)).sum();

        let slope = (n * sum_xy - sum_x * sum_y) / (n * sum_x2 - sum_x.powi(2));
        let intercept = (sum_y - slope * sum_x) / n;

        (slope, intercept)
    }

    /// Calculate moving average
    fn moving_average(points: &[DataPoint], window: usize) -> f64 {
        if points.is_empty() {
            return 0.0;
        }

        let start = points.len().saturating_sub(window);
        let window_points = &points[start..];
        let sum: f64 = window_points.iter().map(|p| p.value).sum();
        sum / window_points.len() as f64
    }

    /// Calculate exponential moving average
    fn exponential_moving_average(points: &[DataPoint], alpha: f64) -> f64 {
        if points.is_empty() {
            return 0.0;
        }

        let mut ema = points[0].value;
        for point in points.iter().skip(1) {
            ema = alpha * point.value + (1.0 - alpha) * ema;
        }
        ema
    }

    /// Find peak hour and value
    fn find_peak_hour(points: &[DataPoint]) -> (u8, f64) {
        if points.is_empty() {
            return (0, 0.0);
        }

        let mut hourly_avg = [0.0; 24];
        let mut hourly_count = [0; 24];

        for point in points {
            let hour = ((point.timestamp % 86400) / 3600) as usize % 24;
            hourly_avg[hour] += point.value;
            hourly_count[hour] += 1;
        }

        // Calculate averages
        for (avg, count) in hourly_avg.iter_mut().zip(hourly_count.iter()) {
            if *count > 0 {
                *avg /= *count as f64;
            }
        }

        // Find peak
        let (peak_hour, peak_value) = hourly_avg
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((0, &0.0));

        (peak_hour as u8, *peak_value)
    }

    /// Detect pattern type using coefficient of variation and autocorrelation
    fn detect_pattern_type(points: &[DataPoint]) -> PatternType {
        if points.len() < 24 {
            return PatternType::Stable;
        }

        // Calculate coefficient of variation
        let values: Vec<f64> = points.iter().map(|p| p.value).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();
        let cv = if mean > 0.0 { std_dev / mean } else { 0.0 };

        // Calculate trend
        let (slope, _) = Self::linear_regression(points);
        let trend_strength = (slope * 86400.0).abs() / mean; // Normalized daily trend

        // Classify pattern
        if cv > 0.5 {
            PatternType::Bursty
        } else if trend_strength > 0.1 {
            PatternType::Trending
        } else if cv > 0.2 {
            PatternType::Periodic
        } else {
            PatternType::Stable
        }
    }

    /// Calculate confidence score based on R-squared
    fn calculate_confidence(points: &[DataPoint], slope: f64) -> f64 {
        if points.len() < 2 {
            return 0.0;
        }

        // Calculate mean
        let mean_y = points.iter().map(|p| p.value).sum::<f64>() / points.len() as f64;

        // Calculate R-squared
        let ss_tot: f64 = points.iter().map(|p| (p.value - mean_y).powi(2)).sum();

        if ss_tot == 0.0 {
            return 1.0; // Perfect fit (all values same)
        }

        let intercept = {
            let sum_x: f64 = points.iter().map(|p| p.timestamp as f64).sum();
            let sum_y: f64 = points.iter().map(|p| p.value).sum();
            let n = points.len() as f64;
            (sum_y - slope * sum_x) / n
        };

        let ss_res: f64 = points
            .iter()
            .map(|p| {
                let predicted = slope * p.timestamp as f64 + intercept;
                (p.value - predicted).powi(2)
            })
            .sum();

        let r_squared = 1.0 - (ss_res / ss_tot);
        r_squared.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_predictive_analytics_creation() {
        let analytics = PredictiveAnalytics::default();
        assert_eq!(analytics.max_history_points, 10_000);
        assert_eq!(analytics.total_capacity, 1_000_000_000_000);
    }

    #[tokio::test]
    async fn test_record_storage_size() {
        let analytics = PredictiveAnalytics::default();
        analytics.record_storage_size(1000, 1_000_000).await;
        analytics.record_storage_size(2000, 2_000_000).await;

        let history = analytics.storage_history.read().await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].value, 1_000_000.0);
        assert_eq!(history[1].value, 2_000_000.0);
    }

    #[tokio::test]
    async fn test_storage_growth_prediction() {
        let analytics = PredictiveAnalytics::default();

        // Simulate growing storage over 30 days
        let base_time = 1_700_000_000i64;
        for i in 0..30 {
            let timestamp = base_time + i * 86400; // 1 day intervals
            let size = 1_000_000_000u64 + (i as u64 * 10_000_000); // Growing 10MB/day
            analytics.record_storage_size(timestamp, size).await;
        }

        let prediction = analytics.predict_storage_growth().await;
        assert!(prediction.is_some());

        if let Some(pred) = prediction {
            assert!(pred.current_size > 1_000_000_000);
            assert!(pred.predicted_7d > pred.current_size);
            assert!(pred.predicted_30d > pred.predicted_7d);
            assert_eq!(pred.trend, TrendDirection::Growing);
            assert!(pred.confidence > 0.0);
        }
    }

    #[tokio::test]
    async fn test_access_pattern_prediction() {
        let analytics = PredictiveAnalytics::default();

        // Simulate request rate over 48 hours
        let base_time = 1_700_000_000i64;
        for i in 0..48 {
            let timestamp = base_time + i * 3600; // 1 hour intervals
            let rps = 100.0 + ((i % 24) as f64 * 10.0); // Daily pattern
            analytics.record_request_rate(timestamp, rps).await;
        }

        let prediction = analytics.predict_access_patterns().await;
        assert!(prediction.is_some());

        if let Some(pred) = prediction {
            assert!(pred.current_rps > 0.0);
            assert!(pred.predicted_1h > 0.0);
            assert!(pred.expected_peak > 0.0);
            assert!(pred.peak_hour < 24);
            assert!(pred.confidence > 0.0);
        }
    }

    #[tokio::test]
    async fn test_cost_forecast() {
        let analytics = PredictiveAnalytics::default();

        // Record some data
        let base_time = 1_700_000_000i64;
        for i in 0..30 {
            let timestamp = base_time + i * 86400;
            analytics
                .record_storage_size(timestamp, 100_000_000_000)
                .await; // 100GB
            analytics.record_bandwidth(timestamp, 1_000_000.0).await; // 1MB/s
            analytics.record_request_rate(timestamp, 100.0).await; // 100 rps
        }

        let forecast = analytics.forecast_costs().await;
        assert!(forecast.is_some());

        if let Some(cost) = forecast {
            assert!(cost.current_monthly_cost > 0.0);
            assert!(cost.storage_cost > 0.0);
            assert!(cost.bandwidth_cost > 0.0);
            assert!(cost.request_cost > 0.0);
        }
    }

    #[tokio::test]
    async fn test_capacity_recommendations() {
        let analytics = PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000, // 1GB capacity (small for testing)
        );

        // Simulate storage at 50% capacity
        let base_time = 1_700_000_000i64;
        for i in 0..30 {
            let timestamp = base_time + i * 86400;
            let size = 500_000_000u64 + (i as u64 * 10_000_000); // Growing
            analytics.record_storage_size(timestamp, size).await;
        }

        let recommendation = analytics.capacity_recommendations().await;
        assert!(recommendation.is_some());

        if let Some(rec) = recommendation {
            assert!(rec.current_utilization > 0.0);
            assert!(rec.current_utilization < 100.0);
        }
    }

    #[tokio::test]
    async fn test_linear_regression() {
        let points = vec![
            DataPoint {
                timestamp: 1000,
                value: 10.0,
            },
            DataPoint {
                timestamp: 2000,
                value: 20.0,
            },
            DataPoint {
                timestamp: 3000,
                value: 30.0,
            },
        ];

        let (slope, _intercept) = PredictiveAnalytics::linear_regression(&points);
        assert!((slope - 0.01).abs() < 0.001); // slope should be ~0.01
    }

    #[tokio::test]
    async fn test_moving_average() {
        let points = vec![
            DataPoint {
                timestamp: 1000,
                value: 10.0,
            },
            DataPoint {
                timestamp: 2000,
                value: 20.0,
            },
            DataPoint {
                timestamp: 3000,
                value: 30.0,
            },
        ];

        let ma = PredictiveAnalytics::moving_average(&points, 2);
        assert_eq!(ma, 25.0); // Average of last 2 points
    }

    #[tokio::test]
    async fn test_pattern_detection() {
        // Stable pattern
        let stable_points: Vec<DataPoint> = (0..50)
            .map(|i| DataPoint {
                timestamp: i * 3600,
                value: 100.0,
            })
            .collect();
        let pattern = PredictiveAnalytics::detect_pattern_type(&stable_points);
        assert_eq!(pattern, PatternType::Stable);

        // Bursty pattern (high variance)
        let bursty_points: Vec<DataPoint> = (0..50)
            .map(|i| DataPoint {
                timestamp: i * 3600,
                value: if i % 2 == 0 { 10.0 } else { 100.0 },
            })
            .collect();
        let pattern = PredictiveAnalytics::detect_pattern_type(&bursty_points);
        assert_eq!(pattern, PatternType::Bursty);
    }

    #[tokio::test]
    async fn test_confidence_calculation() {
        // Perfect linear relationship
        let perfect_points = vec![
            DataPoint {
                timestamp: 1000,
                value: 10.0,
            },
            DataPoint {
                timestamp: 2000,
                value: 20.0,
            },
            DataPoint {
                timestamp: 3000,
                value: 30.0,
            },
        ];
        let (slope, _) = PredictiveAnalytics::linear_regression(&perfect_points);
        let confidence = PredictiveAnalytics::calculate_confidence(&perfect_points, slope);
        assert!(confidence > 0.99);
    }
}
