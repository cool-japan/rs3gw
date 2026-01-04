//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::observability::PredictiveAnalytics;
use crate::storage::ml_cache::{AccessPattern, AccessPatternType, SmartCacheManager};
use crate::storage::storage_class::{StorageClass, StorageClassManager};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::functions::TieringResult;

/// Builder for tiering policy
#[derive(Default)]
pub struct TieringPolicyBuilder {
    hot_threshold_days: Option<i64>,
    warm_threshold_days: Option<i64>,
    cold_threshold_days: Option<i64>,
    archive_threshold_days: Option<i64>,
    enable_auto_transition: Option<bool>,
    min_object_size: Option<u64>,
    excluded_prefixes: Option<Vec<String>>,
    cost_priority: Option<f64>,
    performance_priority: Option<f64>,
}
impl TieringPolicyBuilder {
    pub fn hot_threshold_days(mut self, days: i64) -> Self {
        self.hot_threshold_days = Some(days);
        self
    }
    pub fn warm_threshold_days(mut self, days: i64) -> Self {
        self.warm_threshold_days = Some(days);
        self
    }
    pub fn cold_threshold_days(mut self, days: i64) -> Self {
        self.cold_threshold_days = Some(days);
        self
    }
    pub fn archive_threshold_days(mut self, days: i64) -> Self {
        self.archive_threshold_days = Some(days);
        self
    }
    pub fn enable_auto_transition(mut self, enabled: bool) -> Self {
        self.enable_auto_transition = Some(enabled);
        self
    }
    pub fn min_object_size(mut self, size: u64) -> Self {
        self.min_object_size = Some(size);
        self
    }
    pub fn excluded_prefixes(mut self, prefixes: Vec<String>) -> Self {
        self.excluded_prefixes = Some(prefixes);
        self
    }
    pub fn cost_priority(mut self, priority: f64) -> Self {
        self.cost_priority = Some(priority.clamp(0.0, 1.0));
        self
    }
    pub fn performance_priority(mut self, priority: f64) -> Self {
        self.performance_priority = Some(priority.clamp(0.0, 1.0));
        self
    }
    pub fn build(self) -> TieringPolicy {
        TieringPolicy {
            hot_threshold_days: self.hot_threshold_days.unwrap_or(7),
            warm_threshold_days: self.warm_threshold_days.unwrap_or(30),
            cold_threshold_days: self.cold_threshold_days.unwrap_or(90),
            archive_threshold_days: self.archive_threshold_days.unwrap_or(365),
            enable_auto_transition: self.enable_auto_transition.unwrap_or(true),
            min_object_size: self.min_object_size.unwrap_or(128 * 1024),
            excluded_prefixes: self.excluded_prefixes.unwrap_or_default(),
            cost_priority: self.cost_priority.unwrap_or(0.5),
            performance_priority: self.performance_priority.unwrap_or(0.5),
        }
    }
}
/// Error types for intelligent tiering
#[derive(Debug, thiserror::Error)]
pub enum TieringError {
    #[error("Storage class error: {0}")]
    StorageClass(#[from] crate::storage::storage_class::StorageClassError),
    #[error("Cache error: {0}")]
    Cache(#[from] crate::storage::ml_cache::MlCacheError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid policy: {0}")]
    InvalidPolicy(String),
    #[error("Bucket not found: {0}")]
    BucketNotFound(String),
}
/// Record of a tiering transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringTransition {
    pub bucket: String,
    pub key: String,
    pub from_class: StorageClass,
    pub to_class: StorageClass,
    pub timestamp: DateTime<Utc>,
    pub reason: String,
    pub auto_transitioned: bool,
}
/// Tiering recommendation for an object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringRecommendation {
    pub bucket: String,
    pub key: String,
    pub current_class: StorageClass,
    pub recommended_class: StorageClass,
    pub reason: String,
    pub confidence: f64,
    pub estimated_cost_savings: Option<f64>,
    pub access_pattern: Option<AccessPatternType>,
    pub last_accessed: Option<DateTime<Utc>>,
    pub object_size: u64,
}
/// Intelligent tiering manager
pub struct IntelligentTieringManager {
    storage_root: PathBuf,
    storage_class_manager: Arc<RwLock<StorageClassManager>>,
    cache_manager: Option<Arc<SmartCacheManager>>,
    pub(crate) predictive_analytics: Option<Arc<PredictiveAnalytics>>,
    policies: Arc<RwLock<HashMap<String, TieringPolicy>>>,
    pub(crate) transition_history: Arc<RwLock<Vec<TieringTransition>>>,
}
impl IntelligentTieringManager {
    /// Create a new intelligent tiering manager
    pub async fn new(storage_root: PathBuf) -> TieringResult<Self> {
        let storage_class_manager = Arc::new(RwLock::new(StorageClassManager::new()));
        Ok(Self {
            storage_root,
            storage_class_manager,
            cache_manager: None,
            predictive_analytics: None,
            policies: Arc::new(RwLock::new(HashMap::new())),
            transition_history: Arc::new(RwLock::new(Vec::new())),
        })
    }
    /// Create with integrated smart cache manager
    pub async fn with_cache(
        storage_root: PathBuf,
        cache_manager: Arc<SmartCacheManager>,
    ) -> TieringResult<Self> {
        let mut manager = Self::new(storage_root).await?;
        manager.cache_manager = Some(cache_manager);
        Ok(manager)
    }
    /// Create with integrated smart cache manager and predictive analytics
    pub async fn with_analytics(
        storage_root: PathBuf,
        cache_manager: Arc<SmartCacheManager>,
        predictive_analytics: Arc<PredictiveAnalytics>,
    ) -> TieringResult<Self> {
        let mut manager = Self::with_cache(storage_root, cache_manager).await?;
        manager.predictive_analytics = Some(predictive_analytics);
        Ok(manager)
    }
    /// Set predictive analytics engine (can be called after creation)
    pub fn set_predictive_analytics(&mut self, analytics: Arc<PredictiveAnalytics>) {
        self.predictive_analytics = Some(analytics);
    }
    /// Set tiering policy for a bucket
    pub async fn set_policy(&self, bucket: &str, policy: TieringPolicy) -> TieringResult<()> {
        let mut policies = self.policies.write().await;
        policies.insert(bucket.to_string(), policy);
        info!(bucket = bucket, "Tiering policy configured");
        Ok(())
    }
    /// Get tiering policy for a bucket
    pub async fn get_policy(&self, bucket: &str) -> Option<TieringPolicy> {
        let policies = self.policies.read().await;
        policies.get(bucket).cloned()
    }
    /// Analyze bucket and generate tiering recommendations
    pub async fn analyze_tiering(
        &self,
        bucket: &str,
        policy: &TieringPolicy,
    ) -> TieringResult<TieringAnalysis> {
        info!(bucket = bucket, "Starting intelligent tiering analysis");
        let bucket_path = self.storage_root.join(bucket);
        if !bucket_path.exists() {
            return Err(TieringError::BucketNotFound(bucket.to_string()));
        }
        let mut recommendations = Vec::new();
        let mut objects_by_tier: HashMap<StorageClass, usize> = HashMap::new();
        let mut total_objects = 0;
        let objects_path = bucket_path.join("objects");
        if !objects_path.exists() {
            return Ok(TieringAnalysis {
                bucket: bucket.to_string(),
                analyzed_at: Utc::now(),
                total_objects: 0,
                recommendations: vec![],
                potential_cost_savings: 0.0,
                objects_by_tier: HashMap::new(),
            });
        }
        let entries = tokio::fs::read_dir(&objects_path).await?;
        let mut entries = entries;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                total_objects += 1;
                let key = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                if policy
                    .excluded_prefixes
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
                {
                    debug!(key = key.as_str(), "Skipping excluded object");
                    continue;
                }
                let current_class = self
                    .storage_class_manager
                    .read()
                    .await
                    .get_storage_class(bucket, &key);
                let metadata = tokio::fs::metadata(&path).await?;
                let object_size = metadata.len();
                if object_size < policy.min_object_size {
                    continue;
                }
                let access_pattern = if let Some(cache_mgr) = &self.cache_manager {
                    cache_mgr.get_access_pattern(bucket, &key).await
                } else {
                    None
                };
                let last_accessed = metadata.modified().ok().and_then(|t| {
                    DateTime::from_timestamp(
                        t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                        0,
                    )
                });
                if let Some(recommendation) = self.generate_recommendation(
                    bucket,
                    &key,
                    current_class,
                    object_size,
                    access_pattern.as_ref(),
                    last_accessed,
                    policy,
                ) {
                    recommendations.push(recommendation);
                }
                *objects_by_tier.entry(current_class).or_insert(0) += 1;
            }
        }
        let potential_cost_savings = self.calculate_cost_savings(&recommendations);
        info!(
            bucket = bucket,
            total_objects = total_objects,
            recommendations = recommendations.len(),
            potential_savings = potential_cost_savings,
            "Tiering analysis complete"
        );
        Ok(TieringAnalysis {
            bucket: bucket.to_string(),
            analyzed_at: Utc::now(),
            total_objects,
            recommendations,
            potential_cost_savings,
            objects_by_tier,
        })
    }
    /// Generate recommendation for a single object
    fn generate_recommendation(
        &self,
        bucket: &str,
        key: &str,
        current_class: StorageClass,
        object_size: u64,
        access_pattern: Option<&AccessPattern>,
        last_accessed: Option<DateTime<Utc>>,
        policy: &TieringPolicy,
    ) -> Option<TieringRecommendation> {
        let now = Utc::now();
        let (recommended_class, reason, confidence) = if let Some(pattern) = access_pattern {
            self.recommend_from_pattern(pattern, current_class, policy)
        } else if let Some(last_access) = last_accessed {
            self.recommend_from_age(last_access, now, current_class, policy)
        } else {
            return None;
        };
        if recommended_class == current_class {
            return None;
        }
        let estimated_cost_savings = self.estimate_cost_savings(
            current_class,
            recommended_class,
            object_size,
            access_pattern,
        );
        Some(TieringRecommendation {
            bucket: bucket.to_string(),
            key: key.to_string(),
            current_class,
            recommended_class,
            reason,
            confidence,
            estimated_cost_savings: Some(estimated_cost_savings),
            access_pattern: access_pattern.map(|p| p.pattern_type),
            last_accessed,
            object_size,
        })
    }
    /// Recommend storage class based on access pattern
    fn recommend_from_pattern(
        &self,
        pattern: &AccessPattern,
        current: StorageClass,
        _policy: &TieringPolicy,
    ) -> (StorageClass, String, f64) {
        match pattern.pattern_type {
            AccessPatternType::Periodic => {
                if pattern.ema_frequency > 10.0 {
                    (
                        StorageClass::Standard,
                        "High frequency periodic access".to_string(),
                        0.8,
                    )
                } else {
                    (
                        StorageClass::IntelligentTiering,
                        "Low frequency periodic access".to_string(),
                        0.9,
                    )
                }
            }
            AccessPatternType::Bursty => (
                StorageClass::IntelligentTiering,
                "Bursty access pattern detected".to_string(),
                0.85,
            ),
            AccessPatternType::Trending => {
                if pattern.ema_frequency > 5.0 {
                    (
                        StorageClass::Standard,
                        "Increasing access trend detected".to_string(),
                        0.8,
                    )
                } else {
                    (
                        current,
                        "Trending access but still low frequency".to_string(),
                        0.6,
                    )
                }
            }
            AccessPatternType::Declining => {
                if pattern.ema_frequency < 5.0 {
                    (
                        StorageClass::StandardIa,
                        "Declining infrequent access".to_string(),
                        0.9,
                    )
                } else {
                    (
                        StorageClass::IntelligentTiering,
                        "Declining but still somewhat frequent".to_string(),
                        0.75,
                    )
                }
            }
            AccessPatternType::Random | AccessPatternType::Unknown => (
                StorageClass::IntelligentTiering,
                "Random or unknown access pattern".to_string(),
                0.7,
            ),
        }
    }
    /// Recommend storage class based on age
    fn recommend_from_age(
        &self,
        last_access: DateTime<Utc>,
        now: DateTime<Utc>,
        current: StorageClass,
        policy: &TieringPolicy,
    ) -> (StorageClass, String, f64) {
        let days_since_access = (now - last_access).num_days();
        if days_since_access >= policy.archive_threshold_days {
            (
                StorageClass::DeepArchive,
                format!("Not accessed for {} days", days_since_access),
                0.95,
            )
        } else if days_since_access >= policy.cold_threshold_days {
            (
                StorageClass::Glacier,
                format!("Not accessed for {} days", days_since_access),
                0.9,
            )
        } else if days_since_access >= policy.warm_threshold_days {
            (
                StorageClass::IntelligentTiering,
                format!("Not accessed for {} days", days_since_access),
                0.85,
            )
        } else if days_since_access >= policy.hot_threshold_days {
            (
                StorageClass::StandardIa,
                format!("Not accessed for {} days", days_since_access),
                0.8,
            )
        } else {
            (current, "Recently accessed".to_string(), 0.5)
        }
    }
    /// Estimate cost savings from transition
    pub(crate) fn estimate_cost_savings(
        &self,
        from: StorageClass,
        to: StorageClass,
        size_bytes: u64,
        _access_pattern: Option<&AccessPattern>,
    ) -> f64 {
        let storage_cost = |class: StorageClass| match class {
            StorageClass::Standard => 0.023,
            StorageClass::StandardIa => 0.0125,
            StorageClass::IntelligentTiering => 0.023,
            StorageClass::Glacier => 0.004,
            StorageClass::DeepArchive => 0.00099,
            _ => 0.023,
        };
        let from_cost = storage_cost(from);
        let to_cost = storage_cost(to);
        let size_gb = size_bytes as f64 / 1_073_741_824.0;
        (from_cost - to_cost) * size_gb
    }
    /// Calculate total potential cost savings
    fn calculate_cost_savings(&self, recommendations: &[TieringRecommendation]) -> f64 {
        recommendations
            .iter()
            .filter_map(|r| r.estimated_cost_savings)
            .sum()
    }
    /// Apply tiering recommendations
    pub async fn apply_recommendations(
        &self,
        analysis: &TieringAnalysis,
    ) -> TieringResult<Vec<TieringTransition>> {
        let mut transitions = Vec::new();
        for recommendation in &analysis.recommendations {
            match self.storage_class_manager.write().await.transition_object(
                &recommendation.bucket,
                &recommendation.key,
                recommendation.recommended_class,
            ) {
                Ok(_) => {
                    let transition = TieringTransition {
                        bucket: recommendation.bucket.clone(),
                        key: recommendation.key.clone(),
                        from_class: recommendation.current_class,
                        to_class: recommendation.recommended_class,
                        timestamp: Utc::now(),
                        reason: recommendation.reason.clone(),
                        auto_transitioned: true,
                    };
                    info!(
                        bucket = recommendation.bucket.as_str(), key = recommendation.key
                        .as_str(), from = ? recommendation.current_class, to = ?
                        recommendation.recommended_class, "Storage class transitioned"
                    );
                    transitions.push(transition.clone());
                    let mut history = self.transition_history.write().await;
                    history.push(transition);
                }
                Err(e) => {
                    warn!(
                        bucket = recommendation.bucket.as_str(), key = recommendation.key
                        .as_str(), error = ? e, "Failed to transition storage class"
                    );
                }
            }
        }
        Ok(transitions)
    }
    /// Get transition history
    pub async fn get_transition_history(&self) -> Vec<TieringTransition> {
        self.transition_history.read().await.clone()
    }
    /// Get transition history for a specific bucket
    pub async fn get_bucket_transition_history(&self, bucket: &str) -> Vec<TieringTransition> {
        self.transition_history
            .read()
            .await
            .iter()
            .filter(|t| t.bucket == bucket)
            .cloned()
            .collect()
    }
    /// Analyze tiering with predictive analytics integration
    ///
    /// This enhanced analysis uses both historical access patterns and future predictions
    /// to make more intelligent tiering decisions.
    pub async fn analyze_tiering_predictive(
        &self,
        bucket: &str,
        policy: &TieringPolicy,
    ) -> TieringResult<TieringAnalysis> {
        info!(bucket = bucket, "Starting predictive tiering analysis");
        let bucket_path = self.storage_root.join(bucket);
        if !bucket_path.exists() {
            return Err(TieringError::BucketNotFound(bucket.to_string()));
        }
        let mut recommendations = Vec::new();
        let mut objects_by_tier: HashMap<StorageClass, usize> = HashMap::new();
        let mut total_objects = 0;
        let access_prediction = if let Some(analytics) = &self.predictive_analytics {
            analytics.predict_access_patterns().await
        } else {
            None
        };
        let cost_forecast = if let Some(analytics) = &self.predictive_analytics {
            analytics.forecast_costs().await
        } else {
            None
        };
        let objects_path = bucket_path.join("objects");
        if !objects_path.exists() {
            return Ok(TieringAnalysis {
                bucket: bucket.to_string(),
                analyzed_at: Utc::now(),
                total_objects: 0,
                recommendations: vec![],
                potential_cost_savings: 0.0,
                objects_by_tier: HashMap::new(),
            });
        }
        let entries = tokio::fs::read_dir(&objects_path).await?;
        let mut entries = entries;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                total_objects += 1;
                let key = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                if policy
                    .excluded_prefixes
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
                {
                    debug!(key = key.as_str(), "Skipping excluded object");
                    continue;
                }
                let current_class = self
                    .storage_class_manager
                    .read()
                    .await
                    .get_storage_class(bucket, &key);
                let metadata = tokio::fs::metadata(&path).await?;
                let object_size = metadata.len();
                if object_size < policy.min_object_size {
                    continue;
                }
                let access_pattern = if let Some(cache_mgr) = &self.cache_manager {
                    cache_mgr.get_access_pattern(bucket, &key).await
                } else {
                    None
                };
                let last_accessed = metadata.modified().ok().and_then(|t| {
                    DateTime::from_timestamp(
                        t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64,
                        0,
                    )
                });
                if let Some(recommendation) = self.generate_predictive_recommendation(
                    bucket,
                    &key,
                    current_class,
                    object_size,
                    access_pattern.as_ref(),
                    last_accessed,
                    policy,
                    access_prediction.as_ref(),
                    cost_forecast.as_ref(),
                ) {
                    recommendations.push(recommendation);
                }
                *objects_by_tier.entry(current_class).or_insert(0) += 1;
            }
        }
        let potential_cost_savings = if let Some(forecast) = &cost_forecast {
            self.calculate_predictive_cost_savings(&recommendations, forecast)
        } else {
            self.calculate_cost_savings(&recommendations)
        };
        info!(
            bucket = bucket,
            total_objects = total_objects,
            recommendations = recommendations.len(),
            potential_savings = potential_cost_savings,
            predictive_enabled = self.predictive_analytics.is_some(),
            "Predictive tiering analysis complete"
        );
        Ok(TieringAnalysis {
            bucket: bucket.to_string(),
            analyzed_at: Utc::now(),
            total_objects,
            recommendations,
            potential_cost_savings,
            objects_by_tier,
        })
    }
    /// Generate recommendation using predictive analytics
    #[allow(clippy::too_many_arguments)]
    fn generate_predictive_recommendation(
        &self,
        bucket: &str,
        key: &str,
        current_class: StorageClass,
        object_size: u64,
        access_pattern: Option<&AccessPattern>,
        last_accessed: Option<DateTime<Utc>>,
        policy: &TieringPolicy,
        access_prediction: Option<&crate::observability::AccessPatternPrediction>,
        cost_forecast: Option<&crate::observability::CostForecast>,
    ) -> Option<TieringRecommendation> {
        let now = Utc::now();
        let (recommended_class, reason, confidence) = if let Some(prediction) = access_prediction {
            self.recommend_from_prediction(prediction, current_class, policy)
        } else if let Some(pattern) = access_pattern {
            self.recommend_from_pattern(pattern, current_class, policy)
        } else if let Some(last_access) = last_accessed {
            self.recommend_from_age(last_access, now, current_class, policy)
        } else {
            return None;
        };
        if recommended_class == current_class {
            return None;
        }
        let estimated_cost_savings = if let Some(forecast) = cost_forecast {
            self.estimate_cost_savings_with_forecast(
                current_class,
                recommended_class,
                object_size,
                forecast,
            )
        } else {
            self.estimate_cost_savings(
                current_class,
                recommended_class,
                object_size,
                access_pattern,
            )
        };
        Some(TieringRecommendation {
            bucket: bucket.to_string(),
            key: key.to_string(),
            current_class,
            recommended_class,
            reason,
            confidence,
            estimated_cost_savings: Some(estimated_cost_savings),
            access_pattern: access_pattern.map(|p| p.pattern_type),
            last_accessed,
            object_size,
        })
    }
    /// Recommend storage class based on access pattern prediction
    fn recommend_from_prediction(
        &self,
        prediction: &crate::observability::AccessPatternPrediction,
        _current: StorageClass,
        _policy: &TieringPolicy,
    ) -> (StorageClass, String, f64) {
        use crate::observability::PatternType;
        let predicted_hourly_accesses = prediction.predicted_1h * 3600.0;
        let predicted_daily_accesses = prediction.predicted_24h * 86400.0;
        if predicted_hourly_accesses > 10.0 {
            (
                StorageClass::Standard,
                format!(
                    "High predicted access rate ({:.1} RPS, {:.1} accesses/hour)",
                    prediction.predicted_1h, predicted_hourly_accesses
                ),
                prediction.confidence,
            )
        } else if predicted_hourly_accesses > 1.0 || predicted_daily_accesses > 5.0 {
            (
                StorageClass::IntelligentTiering,
                format!(
                    "Medium predicted access rate ({:.1} RPS, {:.0} accesses/day)",
                    prediction.predicted_24h, predicted_daily_accesses
                ),
                prediction.confidence,
            )
        } else {
            match prediction.pattern_type {
                PatternType::Periodic => (
                    StorageClass::StandardIa,
                    "Periodic low-frequency access predicted".to_string(),
                    prediction.confidence * 0.9,
                ),
                PatternType::Bursty => (
                    StorageClass::IntelligentTiering,
                    "Bursty access pattern predicted - use intelligent tiering".to_string(),
                    prediction.confidence * 0.85,
                ),
                PatternType::Trending => {
                    if prediction.predicted_24h > prediction.current_rps * 1.1 {
                        (
                            StorageClass::Standard,
                            "Growing access trend predicted - keep in Standard".to_string(),
                            prediction.confidence * 0.9,
                        )
                    } else {
                        (
                            StorageClass::Glacier,
                            "Declining access trend predicted - archive to Glacier".to_string(),
                            prediction.confidence * 0.85,
                        )
                    }
                }
                PatternType::Stable => {
                    if predicted_daily_accesses < 0.5 {
                        (
                            StorageClass::Glacier,
                            "Stable low access predicted - archive to Glacier".to_string(),
                            prediction.confidence * 0.95,
                        )
                    } else {
                        (
                            StorageClass::StandardIa,
                            "Stable medium access predicted - use Standard-IA".to_string(),
                            prediction.confidence * 0.9,
                        )
                    }
                }
            }
        }
    }
    /// Calculate cost savings using cost forecast
    pub(crate) fn estimate_cost_savings_with_forecast(
        &self,
        from: StorageClass,
        to: StorageClass,
        size_bytes: u64,
        forecast: &crate::observability::CostForecast,
    ) -> f64 {
        let size_gb = size_bytes as f64 / 1_073_741_824.0;
        let forecasted_cost_per_gb = forecast.storage_cost / 1000.0;
        let class_multiplier = |class: StorageClass| match class {
            StorageClass::Standard => 1.0,
            StorageClass::StandardIa => 0.543,
            StorageClass::IntelligentTiering => 1.0,
            StorageClass::Glacier => 0.174,
            StorageClass::DeepArchive => 0.043,
            _ => 1.0,
        };
        let from_cost = forecasted_cost_per_gb * class_multiplier(from);
        let to_cost = forecasted_cost_per_gb * class_multiplier(to);
        (from_cost - to_cost) * size_gb * 30.0
    }
    /// Calculate total cost savings with forecasting
    fn calculate_predictive_cost_savings(
        &self,
        recommendations: &[TieringRecommendation],
        forecast: &crate::observability::CostForecast,
    ) -> f64 {
        let confidence = if forecast.growth_rate_percent.abs() < 10.0 {
            0.95
        } else if forecast.growth_rate_percent.abs() < 30.0 {
            0.85
        } else {
            0.70
        };
        recommendations
            .iter()
            .filter_map(|r| r.estimated_cost_savings)
            .sum::<f64>()
            * confidence
    }
    /// Get capacity-aware tiering recommendations
    ///
    /// Uses capacity planning recommendations to proactively tier data
    /// before capacity issues arise.
    pub async fn get_capacity_aware_recommendations(
        &self,
        bucket: &str,
    ) -> TieringResult<Vec<TieringRecommendation>> {
        if let Some(analytics) = &self.predictive_analytics {
            if let Some(capacity_rec) = analytics.capacity_recommendations().await {
                match capacity_rec.urgency {
                    crate::observability::UrgencyLevel::Critical
                    | crate::observability::UrgencyLevel::High => {
                        info!(
                            urgency = ? capacity_rec.urgency,
                            "Capacity pressure detected - recommending aggressive tiering"
                        );
                        let aggressive_policy = TieringPolicy {
                            hot_threshold_days: 3,
                            warm_threshold_days: 14,
                            cold_threshold_days: 30,
                            archive_threshold_days: 90,
                            enable_auto_transition: true,
                            min_object_size: 64 * 1024,
                            excluded_prefixes: vec![],
                            cost_priority: 0.95,
                            performance_priority: 0.05,
                        };
                        let analysis = self.analyze_tiering(bucket, &aggressive_policy).await?;
                        Ok(analysis.recommendations)
                    }
                    _ => {
                        if let Some(policy) = self.get_policy(bucket).await {
                            let analysis = self.analyze_tiering(bucket, &policy).await?;
                            Ok(analysis.recommendations)
                        } else {
                            Ok(vec![])
                        }
                    }
                }
            } else {
                Ok(vec![])
            }
        } else {
            Ok(vec![])
        }
    }
}
/// Tiering analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringAnalysis {
    pub bucket: String,
    pub analyzed_at: DateTime<Utc>,
    pub total_objects: usize,
    pub recommendations: Vec<TieringRecommendation>,
    pub potential_cost_savings: f64,
    pub objects_by_tier: HashMap<StorageClass, usize>,
}
/// Tiering policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TieringPolicy {
    /// Days before transitioning to STANDARD_IA
    pub hot_threshold_days: i64,
    /// Days before transitioning to INTELLIGENT_TIERING
    pub warm_threshold_days: i64,
    /// Days before transitioning to GLACIER
    pub cold_threshold_days: i64,
    /// Days before transitioning to DEEP_ARCHIVE
    pub archive_threshold_days: i64,
    /// Enable automatic transitions
    pub enable_auto_transition: bool,
    /// Minimum object size for tiering (bytes)
    pub min_object_size: u64,
    /// Exclude prefixes from tiering
    pub excluded_prefixes: Vec<String>,
    /// Cost optimization priority (0.0-1.0, higher = more aggressive)
    pub cost_priority: f64,
    /// Performance priority (0.0-1.0, higher = prefer faster access)
    pub performance_priority: f64,
}
impl TieringPolicy {
    /// Create a new policy builder
    pub fn builder() -> TieringPolicyBuilder {
        TieringPolicyBuilder::default()
    }
    /// Default policy optimized for cost
    pub fn cost_optimized() -> Self {
        Self {
            hot_threshold_days: 7,
            warm_threshold_days: 30,
            cold_threshold_days: 90,
            archive_threshold_days: 365,
            enable_auto_transition: true,
            min_object_size: 128 * 1024,
            excluded_prefixes: vec![],
            cost_priority: 0.9,
            performance_priority: 0.1,
        }
    }
    /// Default policy balanced between cost and performance
    pub fn balanced() -> Self {
        Self {
            hot_threshold_days: 14,
            warm_threshold_days: 60,
            cold_threshold_days: 180,
            archive_threshold_days: 730,
            enable_auto_transition: true,
            min_object_size: 128 * 1024,
            excluded_prefixes: vec![],
            cost_priority: 0.5,
            performance_priority: 0.5,
        }
    }
    /// Default policy optimized for performance
    pub fn performance_optimized() -> Self {
        Self {
            hot_threshold_days: 30,
            warm_threshold_days: 90,
            cold_threshold_days: 365,
            archive_threshold_days: 1095,
            enable_auto_transition: true,
            min_object_size: 128 * 1024,
            excluded_prefixes: vec![],
            cost_priority: 0.1,
            performance_priority: 0.9,
        }
    }
}
