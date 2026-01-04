//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::TieringError;

pub type TieringResult<T> = Result<T, TieringError>;
#[cfg(test)]
mod tests {
    use super::super::types::{IntelligentTieringManager, TieringPolicy, TieringTransition};
    use crate::storage::storage_class::StorageClass;
    use chrono::Utc;
    use tempfile::TempDir;
    async fn create_test_manager() -> (IntelligentTieringManager, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let manager = IntelligentTieringManager::new(temp_dir.path().to_path_buf())
            .await
            .expect("Failed to create manager");
        (manager, temp_dir)
    }
    #[tokio::test]
    async fn test_policy_builder() {
        let policy = TieringPolicy::builder()
            .hot_threshold_days(10)
            .warm_threshold_days(40)
            .cold_threshold_days(100)
            .enable_auto_transition(true)
            .build();
        assert_eq!(policy.hot_threshold_days, 10);
        assert_eq!(policy.warm_threshold_days, 40);
        assert_eq!(policy.cold_threshold_days, 100);
        assert!(policy.enable_auto_transition);
    }
    #[tokio::test]
    async fn test_preset_policies() {
        let cost_opt = TieringPolicy::cost_optimized();
        assert!(cost_opt.cost_priority > 0.8);
        let balanced = TieringPolicy::balanced();
        assert!((balanced.cost_priority - 0.5).abs() < 0.1);
        let perf_opt = TieringPolicy::performance_optimized();
        assert!(perf_opt.performance_priority > 0.8);
    }
    #[tokio::test]
    async fn test_set_and_get_policy() {
        let (manager, _temp) = create_test_manager().await;
        let policy = TieringPolicy::balanced();
        manager
            .set_policy("test-bucket", policy.clone())
            .await
            .expect("Failed to set policy");
        let retrieved = manager.get_policy("test-bucket").await;
        assert!(retrieved.is_some());
        assert_eq!(
            retrieved.expect("Policy should exist").hot_threshold_days,
            policy.hot_threshold_days
        );
    }
    #[tokio::test]
    async fn test_cost_estimation() {
        let (manager, _temp) = create_test_manager().await;
        let savings = manager.estimate_cost_savings(
            StorageClass::Standard,
            StorageClass::Glacier,
            1_073_741_824,
            None,
        );
        assert!(savings > 0.0);
    }
    #[tokio::test]
    async fn test_transition_history() {
        let (manager, _temp) = create_test_manager().await;
        let transition = TieringTransition {
            bucket: "test-bucket".to_string(),
            key: "test-key".to_string(),
            from_class: StorageClass::Standard,
            to_class: StorageClass::Glacier,
            timestamp: Utc::now(),
            reason: "Test transition".to_string(),
            auto_transitioned: true,
        };
        manager
            .transition_history
            .write()
            .await
            .push(transition.clone());
        let history = manager.get_transition_history().await;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].bucket, "test-bucket");
        let bucket_history = manager.get_bucket_transition_history("test-bucket").await;
        assert_eq!(bucket_history.len(), 1);
    }
    #[tokio::test]
    async fn test_ml_based_tiering_with_cache_manager() {
        use crate::storage::ml_cache::{MlCacheConfig, SmartCacheManager};
        use bytes::Bytes;
        use std::sync::Arc;
        use tokio::fs;
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let cache_config = MlCacheConfig {
            max_size_bytes: 1024 * 1024,
            max_objects: 100,
            default_ttl_secs: 300,
            adaptive_sizing: true,
            enable_prefetch: true,
            prefetch_threshold: 3,
            pattern_window_size: 10,
            ema_alpha: 0.3,
            prefetch_min_confidence: 0.7,
            warming_batch_size: 10,
        };
        let cache_manager =
            Arc::new(SmartCacheManager::new(cache_config).expect("Failed to create cache manager"));
        let manager = IntelligentTieringManager::with_cache(
            temp_dir.path().to_path_buf(),
            cache_manager.clone(),
        )
        .await
        .expect("Failed to create manager with cache");
        let bucket_path = temp_dir.path().join("test-bucket");
        fs::create_dir(&bucket_path)
            .await
            .expect("Failed to create bucket");
        let objects_dir = bucket_path.join("objects");
        fs::create_dir(&objects_dir)
            .await
            .expect("Failed to create objects directory");
        let object_path = objects_dir.join("test-object");
        let test_data = vec![0u8; 200_000];
        fs::write(&object_path, &test_data)
            .await
            .expect("Failed to write object");
        let metadata = crate::storage::ObjectMetadata {
            key: "test-object".to_string(),
            size: test_data.len() as u64,
            etag: "test-etag".to_string(),
            last_modified: Utc::now(),
            content_type: "application/octet-stream".to_string(),
            metadata: std::collections::HashMap::new(),
        };
        for _ in 0..5 {
            cache_manager
                .put(
                    "test-bucket",
                    "test-object",
                    metadata.clone(),
                    Bytes::from(test_data.clone()),
                )
                .await
                .expect("Failed to put in cache");
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            cache_manager.get("test-bucket", "test-object").await;
        }
        let policy = TieringPolicy::balanced();
        manager
            .set_policy("test-bucket", policy.clone())
            .await
            .expect("Failed to set policy");
        let analysis = manager
            .analyze_tiering("test-bucket", &policy)
            .await
            .expect("Failed to analyze tiering");
        assert_eq!(analysis.bucket, "test-bucket");
        assert!(analysis.total_objects > 0);
    }
    #[tokio::test]
    async fn test_predictive_tiering_with_analytics() {
        use crate::observability::PredictiveAnalytics;
        use std::sync::Arc;
        use tokio::fs;
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let analytics = Arc::new(PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000_000,
        ));
        let now = chrono::Utc::now().timestamp();
        analytics.record_storage_size(now, 100_000_000).await;
        let cache_config = crate::storage::ml_cache::MlCacheConfig {
            max_size_bytes: 1024 * 1024,
            max_objects: 100,
            default_ttl_secs: 300,
            adaptive_sizing: true,
            enable_prefetch: true,
            prefetch_threshold: 3,
            pattern_window_size: 10,
            ema_alpha: 0.3,
            prefetch_min_confidence: 0.7,
            warming_batch_size: 10,
        };
        let cache_manager = Arc::new(
            crate::storage::ml_cache::SmartCacheManager::new(cache_config)
                .expect("Failed to create cache manager"),
        );
        let manager = IntelligentTieringManager::with_analytics(
            temp_dir.path().to_path_buf(),
            cache_manager,
            analytics.clone(),
        )
        .await
        .expect("Failed to create manager with analytics");
        let bucket_path = temp_dir.path().join("test-bucket");
        fs::create_dir(&bucket_path)
            .await
            .expect("Failed to create bucket");
        let objects_dir = bucket_path.join("objects");
        fs::create_dir(&objects_dir)
            .await
            .expect("Failed to create objects directory");
        let object_path = objects_dir.join("test-object");
        let test_data = vec![0u8; 200_000];
        fs::write(&object_path, &test_data)
            .await
            .expect("Failed to write object");
        let policy = TieringPolicy::balanced();
        manager
            .set_policy("test-bucket", policy.clone())
            .await
            .expect("Failed to set policy");
        let analysis = manager
            .analyze_tiering_predictive("test-bucket", &policy)
            .await
            .expect("Failed to analyze tiering with predictions");
        assert_eq!(analysis.bucket, "test-bucket");
        assert!(analysis.total_objects > 0);
    }
    #[tokio::test]
    async fn test_capacity_aware_recommendations() {
        use crate::observability::PredictiveAnalytics;
        use std::sync::Arc;
        use tokio::fs;
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let analytics = Arc::new(PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000,
        ));
        let now = chrono::Utc::now().timestamp();
        analytics.record_storage_size(now, 900_000_000).await;
        let cache_config = crate::storage::ml_cache::MlCacheConfig {
            max_size_bytes: 1024 * 1024,
            max_objects: 100,
            default_ttl_secs: 300,
            adaptive_sizing: true,
            enable_prefetch: true,
            prefetch_threshold: 3,
            pattern_window_size: 10,
            ema_alpha: 0.3,
            prefetch_min_confidence: 0.7,
            warming_batch_size: 10,
        };
        let cache_manager = Arc::new(
            crate::storage::ml_cache::SmartCacheManager::new(cache_config)
                .expect("Failed to create cache manager"),
        );
        let manager = IntelligentTieringManager::with_analytics(
            temp_dir.path().to_path_buf(),
            cache_manager,
            analytics.clone(),
        )
        .await
        .expect("Failed to create manager");
        let bucket_path = temp_dir.path().join("test-bucket");
        fs::create_dir(&bucket_path)
            .await
            .expect("Failed to create bucket");
        let objects_dir = bucket_path.join("objects");
        fs::create_dir(&objects_dir)
            .await
            .expect("Failed to create objects directory");
        let object_path = objects_dir.join("test-object");
        let test_data = vec![0u8; 200_000];
        fs::write(&object_path, &test_data)
            .await
            .expect("Failed to write object");
        let policy = TieringPolicy::balanced();
        manager
            .set_policy("test-bucket", policy)
            .await
            .expect("Failed to set policy");
        let recommendations = manager
            .get_capacity_aware_recommendations("test-bucket")
            .await
            .expect("Failed to get capacity-aware recommendations");
        let _ = recommendations.len();
    }
    #[tokio::test]
    async fn test_set_predictive_analytics() {
        use crate::observability::PredictiveAnalytics;
        use std::sync::Arc;
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut manager = IntelligentTieringManager::new(temp_dir.path().to_path_buf())
            .await
            .expect("Failed to create manager");
        assert!(manager.predictive_analytics.is_none());
        let analytics = Arc::new(PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000_000,
        ));
        manager.set_predictive_analytics(analytics);
        assert!(manager.predictive_analytics.is_some());
    }
    #[tokio::test]
    async fn test_cost_savings_with_forecast() {
        use crate::observability::PredictiveAnalytics;
        use std::sync::Arc;
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let analytics = Arc::new(PredictiveAnalytics::new(
            10_000,
            0.023,
            0.09,
            0.0004,
            1_000_000_000_000,
        ));
        let now = chrono::Utc::now().timestamp();
        analytics.record_storage_size(now, 1_000_000_000).await;
        let cache_config = crate::storage::ml_cache::MlCacheConfig {
            max_size_bytes: 1024 * 1024,
            max_objects: 100,
            default_ttl_secs: 300,
            adaptive_sizing: true,
            enable_prefetch: true,
            prefetch_threshold: 3,
            pattern_window_size: 10,
            ema_alpha: 0.3,
            prefetch_min_confidence: 0.7,
            warming_batch_size: 10,
        };
        let cache_manager = Arc::new(
            crate::storage::ml_cache::SmartCacheManager::new(cache_config)
                .expect("Failed to create cache manager"),
        );
        let manager = IntelligentTieringManager::with_analytics(
            temp_dir.path().to_path_buf(),
            cache_manager,
            analytics.clone(),
        )
        .await
        .expect("Failed to create manager");
        let cost_forecast = analytics.forecast_costs().await;
        if let Some(forecast) = cost_forecast {
            let savings = manager.estimate_cost_savings_with_forecast(
                StorageClass::Standard,
                StorageClass::Glacier,
                1_000_000_000,
                &forecast,
            );
            assert!(savings >= 0.0);
        }
    }
}
