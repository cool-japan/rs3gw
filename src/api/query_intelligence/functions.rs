//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

#[cfg(test)]
mod tests {
    use super::super::types::*;
    use crate::api::select::types::{AggregateFunction, ColumnRef, OrderByClause, SelectColumn};
    use crate::api::select::ParsedQuery;
    use std::collections::HashMap;
    use std::time::SystemTime;

    fn create_test_query() -> ParsedQuery {
        ParsedQuery {
            columns: vec![
                SelectColumn::Column(ColumnRef::Named("name".to_string())),
                SelectColumn::Column(ColumnRef::Named("age".to_string())),
            ],
            from_alias: Some("s3object".to_string()),
            where_clause: None,
            group_by: None,
            order_by: None,
            limit: None,
        }
    }
    fn create_test_data_stats() -> DataStatistics {
        DataStatistics {
            total_rows: 10000,
            total_bytes: 1024 * 1024,
            avg_row_size: 100.0,
            format: "CSV".to_string(),
            compression_ratio: None,
            column_cardinality: HashMap::new(),
            null_percentages: HashMap::new(),
            is_sorted: false,
            skew_factor: 0.1,
        }
    }
    #[tokio::test]
    async fn test_query_intelligence_creation() {
        let intelligence = QueryIntelligence::new();
        let summary = intelligence.get_summary().await;
        assert_eq!(summary.total_queries, 0);
    }
    #[tokio::test]
    async fn test_cost_prediction() {
        let intelligence = QueryIntelligence::new();
        let query = create_test_query();
        let stats = create_test_data_stats();
        let cost = intelligence.predict_cost(&query, &stats).await;
        assert!(cost.execution_time_ms > 0.0);
        assert!(cost.memory_bytes > 0);
        assert!(cost.confidence >= 0.0 && cost.confidence <= 1.0);
    }
    #[tokio::test]
    async fn test_execution_strategy() {
        let intelligence = QueryIntelligence::new();
        let query = create_test_query();
        let stats = create_test_data_stats();
        let strategy = intelligence.get_execution_strategy(&query, &stats).await;
        match strategy {
            ExecutionStrategy::Sequential | ExecutionStrategy::FullScan => {}
            _ => panic!("Expected Sequential or FullScan strategy for small dataset"),
        }
    }
    #[tokio::test]
    async fn test_execution_strategy_large_dataset() {
        let intelligence = QueryIntelligence::new();
        let query = create_test_query();
        let mut stats = create_test_data_stats();
        stats.total_bytes = 20 * 1024 * 1024;
        stats.total_rows = 2_000_000;
        stats.avg_row_size = 10.0;
        let strategy = intelligence.get_execution_strategy(&query, &stats).await;
        match strategy {
            ExecutionStrategy::Parallel { num_threads } => {
                assert!(num_threads > 1);
            }
            ExecutionStrategy::Streaming { chunk_size } => {
                assert!(chunk_size > 0);
            }
            ExecutionStrategy::FullScan => {}
            _ => {
                panic!("Expected parallel, streaming, or full scan strategy for large dataset")
            }
        }
    }
    #[tokio::test]
    async fn test_record_execution() {
        let intelligence = QueryIntelligence::new();
        let stats = QueryStats {
            sql: "SELECT * FROM s3object".to_string(),
            fingerprint: "test123".to_string(),
            execution_time_ms: 10.5,
            memory_bytes: 1024,
            rows_scanned: 100,
            rows_returned: 50,
            object_size_bytes: 10000,
            timestamp: SystemTime::now(),
            parallel_execution: false,
            cache_hit: false,
        };
        intelligence.record_execution(stats).await;
        let summary = intelligence.get_summary().await;
        assert_eq!(summary.total_queries, 1);
        assert_eq!(summary.avg_execution_time_ms, 10.5);
    }
    #[tokio::test]
    async fn test_find_similar_queries() {
        let intelligence = QueryIntelligence::new();
        let stats = QueryStats {
            sql: "SELECT name, age FROM s3object".to_string(),
            fingerprint: QueryIntelligence::compute_fingerprint(&create_test_query()),
            execution_time_ms: 10.0,
            memory_bytes: 1024,
            rows_scanned: 100,
            rows_returned: 50,
            object_size_bytes: 10000,
            timestamp: SystemTime::now(),
            parallel_execution: false,
            cache_hit: false,
        };
        intelligence.record_execution(stats).await;
        let query = create_test_query();
        let similar = intelligence.find_similar_queries(&query, 0.8).await;
        assert!(!similar.is_empty());
        assert_eq!(similar[0].similarity, 1.0);
    }
    #[tokio::test]
    async fn test_fingerprint_computation() {
        let query1 = create_test_query();
        let query2 = create_test_query();
        let fp1 = QueryIntelligence::compute_fingerprint(&query1);
        let fp2 = QueryIntelligence::compute_fingerprint(&query2);
        assert_eq!(fp1, fp2);
    }
    #[tokio::test]
    async fn test_query_normalization() {
        let query = create_test_query();
        let normalized = QueryIntelligence::normalize_query(&query);
        assert!(normalized.contains("s3object"));
    }
    #[tokio::test]
    async fn test_levenshtein_distance() {
        let dist1 = QueryIntelligence::levenshtein_distance("hello", "hello");
        assert_eq!(dist1, 0);
        let dist2 = QueryIntelligence::levenshtein_distance("hello", "hallo");
        assert_eq!(dist2, 1);
        let dist3 = QueryIntelligence::levenshtein_distance("", "hello");
        assert_eq!(dist3, 5);
    }
    #[tokio::test]
    async fn test_similarity_computation() {
        let sim1 = QueryIntelligence::compute_similarity("hello", "hello");
        assert_eq!(sim1, 1.0);
        let sim2 = QueryIntelligence::compute_similarity("hello", "hallo");
        assert!((0.8..1.0).contains(&sim2) || sim2 == 0.8);
        let sim3 = QueryIntelligence::compute_similarity("abc", "xyz");
        assert!(sim3 < 0.5);
    }
    #[tokio::test]
    async fn test_cost_breakdown() {
        let intelligence = QueryIntelligence::new();
        let mut query = create_test_query();
        query.columns.push(SelectColumn::Aggregate {
            function: crate::api::select::AggregateFunction::Count,
            column: None,
            alias: Some("count".to_string()),
        });
        query.group_by = Some(vec!["name".to_string()]);
        query.order_by = Some(vec![OrderByClause {
            column: "age".to_string(),
            ascending: true,
        }]);
        let stats = create_test_data_stats();
        let cost = intelligence.predict_cost(&query, &stats).await;
        assert!(cost.breakdown.scan_cost > 0.0);
        assert!(cost.breakdown.projection_cost > 0.0);
        assert!(cost.breakdown.aggregation_cost > 0.0);
        assert!(cost.breakdown.sort_cost > 0.0);
    }
    #[tokio::test]
    async fn test_cache_hit_tracking() {
        let intelligence = QueryIntelligence::new();
        let stats1 = QueryStats {
            sql: "SELECT * FROM s3object".to_string(),
            fingerprint: "test1".to_string(),
            execution_time_ms: 10.0,
            memory_bytes: 1024,
            rows_scanned: 100,
            rows_returned: 50,
            object_size_bytes: 10000,
            timestamp: SystemTime::now(),
            parallel_execution: false,
            cache_hit: true,
        };
        let stats2 = QueryStats {
            sql: "SELECT age FROM s3object".to_string(),
            fingerprint: "test2".to_string(),
            execution_time_ms: 20.0,
            memory_bytes: 2048,
            rows_scanned: 200,
            rows_returned: 100,
            object_size_bytes: 20000,
            timestamp: SystemTime::now(),
            parallel_execution: false,
            cache_hit: false,
        };
        intelligence.record_execution(stats1).await;
        intelligence.record_execution(stats2).await;
        let summary = intelligence.get_summary().await;
        assert_eq!(summary.total_queries, 2);
        assert_eq!(summary.cache_hit_rate, 0.5);
    }
    #[tokio::test]
    async fn test_index_recommendations_empty() {
        let intelligence = QueryIntelligence::new();
        let recommendations = intelligence.get_index_recommendations().await;
        assert!(recommendations.is_empty());
    }
    #[tokio::test]
    async fn test_index_recommendations_with_data() {
        let intelligence = QueryIntelligence::new();
        for i in 0..10 {
            let stats = QueryStats {
                sql: format!("SELECT * FROM s3object WHERE id = {}", i),
                fingerprint: format!("test{}", i),
                execution_time_ms: 10.0 + i as f64,
                memory_bytes: 1024,
                rows_scanned: 1000,
                rows_returned: 10,
                object_size_bytes: 10000,
                timestamp: SystemTime::now(),
                parallel_execution: false,
                cache_hit: false,
            };
            intelligence.record_execution(stats).await;
        }
        let recommendations = intelligence.get_index_recommendations().await;
        assert!(recommendations.is_empty() || recommendations.len() <= 10);
    }
    #[test]
    fn test_index_type_serialization() {
        let types = vec![
            IndexType::BTree,
            IndexType::Hash,
            IndexType::FullText,
            IndexType::Bitmap,
        ];
        for index_type in types {
            let json = serde_json::to_string(&index_type).expect("Failed to serialize");
            let deserialized: IndexType =
                serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(index_type, deserialized);
        }
    }
    #[test]
    fn test_index_reason_serialization() {
        let reasons = vec![
            IndexReason::FilterColumn,
            IndexReason::SortColumn,
            IndexReason::JoinColumn,
            IndexReason::GroupByColumn,
            IndexReason::HighScanCost,
        ];
        for reason in reasons {
            let json = serde_json::to_string(&reason).expect("Failed to serialize");
            let deserialized: IndexReason =
                serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(reason, deserialized);
        }
    }
    #[tokio::test]
    async fn test_complexity_trivial_query() {
        let intelligence = QueryIntelligence::new();
        let query = ParsedQuery {
            columns: vec![
                SelectColumn::Column(ColumnRef::Named("id".to_string())),
                SelectColumn::Column(ColumnRef::Named("name".to_string())),
            ],
            where_clause: None,
            from_alias: None,
            group_by: None,
            order_by: None,
            limit: Some(10),
        };
        let complexity = intelligence.calculate_complexity(&query, 500_000).await;
        assert_eq!(complexity.classification, ComplexityClass::Trivial);
        assert!(complexity.score < 10.0);
        assert!(complexity.resource_estimate.cacheable);
        assert_eq!(complexity.resource_estimate.cpu_intensity, 1);
    }
    #[tokio::test]
    async fn test_complexity_simple_query() {
        let intelligence = QueryIntelligence::new();
        let query = ParsedQuery {
            columns: vec![
                SelectColumn::Column(ColumnRef::Named("col1".to_string())),
                SelectColumn::Column(ColumnRef::Named("col2".to_string())),
                SelectColumn::Column(ColumnRef::Named("col3".to_string())),
                SelectColumn::Column(ColumnRef::Named("col4".to_string())),
            ],
            where_clause: None,
            from_alias: None,
            group_by: None,
            order_by: None,
            limit: None,
        };
        let complexity = intelligence.calculate_complexity(&query, 2_000_000).await;
        assert_eq!(complexity.classification, ComplexityClass::Trivial);
        assert!(complexity.score < 10.0);
    }
    #[tokio::test]
    async fn test_complexity_moderate_query() {
        let intelligence = QueryIntelligence::new();
        let query = ParsedQuery {
            columns: vec![
                SelectColumn::Column(ColumnRef::Named("category".to_string())),
                SelectColumn::Aggregate {
                    function: AggregateFunction::Count,
                    column: Some("*".to_string()),
                    alias: Some("count".to_string()),
                },
                SelectColumn::Aggregate {
                    function: AggregateFunction::Avg,
                    column: Some("price".to_string()),
                    alias: Some("avg_price".to_string()),
                },
            ],
            where_clause: None,
            from_alias: None,
            group_by: Some(vec!["category".to_string()]),
            order_by: None,
            limit: None,
        };
        let complexity = intelligence.calculate_complexity(&query, 50_000_000).await;
        assert_eq!(complexity.classification, ComplexityClass::Simple);
        assert!(complexity.score >= 10.0 && complexity.score < 25.0);
        assert!(complexity.components.aggregation_score > 0.0);
        assert_eq!(complexity.resource_estimate.memory_intensity, 3);
    }
    #[tokio::test]
    async fn test_complexity_with_sorting() {
        let intelligence = QueryIntelligence::new();
        let query = ParsedQuery {
            columns: vec![
                SelectColumn::Column(ColumnRef::Named("name".to_string())),
                SelectColumn::Column(ColumnRef::Named("age".to_string())),
            ],
            where_clause: None,
            from_alias: None,
            group_by: None,
            order_by: Some(vec![
                OrderByClause {
                    column: "age".to_string(),
                    ascending: false,
                },
                OrderByClause {
                    column: "name".to_string(),
                    ascending: true,
                },
            ]),
            limit: None,
        };
        let complexity = intelligence.calculate_complexity(&query, 10_000_000).await;
        assert!(complexity.components.sort_score > 0.0);
        assert_eq!(complexity.resource_estimate.memory_intensity, 4);
        assert_eq!(complexity.components.sort_score, 7.0);
    }
    #[tokio::test]
    async fn test_complexity_complex_query() {
        let intelligence = QueryIntelligence::new();
        let query = ParsedQuery {
            columns: vec![
                SelectColumn::Column(ColumnRef::Named("category".to_string())),
                SelectColumn::Aggregate {
                    function: AggregateFunction::Count,
                    column: Some("*".to_string()),
                    alias: Some("count".to_string()),
                },
                SelectColumn::Aggregate {
                    function: AggregateFunction::Sum,
                    column: Some("amount".to_string()),
                    alias: Some("total".to_string()),
                },
            ],
            where_clause: None,
            from_alias: None,
            group_by: Some(vec!["category".to_string()]),
            order_by: Some(vec![OrderByClause {
                column: "count".to_string(),
                ascending: false,
            }]),
            limit: None,
        };
        let complexity = intelligence.calculate_complexity(&query, 100_000_000).await;
        assert!(complexity.score >= 10.0);
        assert!(complexity.components.aggregation_score > 0.0);
        assert!(complexity.components.sort_score > 0.0);
        assert_eq!(complexity.resource_estimate.memory_intensity, 6);
    }
    #[tokio::test]
    async fn test_complexity_resource_estimation_small_object() {
        let intelligence = QueryIntelligence::new();
        let query = create_test_query();
        let complexity = intelligence.calculate_complexity(&query, 500_000).await;
        assert_eq!(complexity.resource_estimate.io_intensity, 1);
        assert!(complexity.resource_estimate.cacheable);
        assert_eq!(complexity.resource_estimate.recommended_parallelism, 1);
    }
    #[tokio::test]
    async fn test_complexity_resource_estimation_large_object() {
        let intelligence = QueryIntelligence::new();
        let query = create_test_query();
        let complexity = intelligence.calculate_complexity(&query, 500_000_000).await;
        assert!(complexity.resource_estimate.io_intensity >= 5);
        assert!(complexity.resource_estimate.recommended_parallelism > 1);
    }
    #[tokio::test]
    async fn test_complexity_with_limit() {
        let intelligence = QueryIntelligence::new();
        let mut query = create_test_query();
        query.limit = Some(100);
        let complexity = intelligence.calculate_complexity(&query, 10_000_000).await;
        assert!(complexity.components.feature_score < 0.0);
    }
    #[tokio::test]
    async fn test_complexity_execution_tiers() {
        let intelligence = QueryIntelligence::new();
        let trivial_query = ParsedQuery {
            columns: vec![SelectColumn::Column(ColumnRef::Named("id".to_string()))],
            where_clause: None,
            from_alias: None,
            group_by: None,
            order_by: None,
            limit: Some(1),
        };
        let complexity = intelligence
            .calculate_complexity(&trivial_query, 100_000)
            .await;
        assert_eq!(
            complexity.resource_estimate.execution_tier,
            ExecutionTier::VeryFast
        );
        let mut complex_query = create_test_query();
        complex_query.order_by = Some(vec![OrderByClause {
            column: "name".to_string(),
            ascending: true,
        }]);
        let complexity = intelligence
            .calculate_complexity(&complex_query, 2_000_000_000)
            .await;
        assert!(matches!(
            complexity.resource_estimate.execution_tier,
            ExecutionTier::Slow | ExecutionTier::VerySlow
        ));
    }
    #[tokio::test]
    async fn test_complexity_distribution() {
        let intelligence = QueryIntelligence::new();
        let stats = vec![
            QueryStats {
                sql: "SELECT * FROM t1".to_string(),
                fingerprint: "q1".to_string(),
                execution_time_ms: 5.0,
                memory_bytes: 1024,
                rows_scanned: 100,
                rows_returned: 100,
                object_size_bytes: 1000,
                timestamp: SystemTime::now(),
                parallel_execution: false,
                cache_hit: false,
            },
            QueryStats {
                sql: "SELECT * FROM t1".to_string(),
                fingerprint: "q2".to_string(),
                execution_time_ms: 50.0,
                memory_bytes: 2048,
                rows_scanned: 1000,
                rows_returned: 500,
                object_size_bytes: 10000,
                timestamp: SystemTime::now(),
                parallel_execution: false,
                cache_hit: false,
            },
            QueryStats {
                sql: "SELECT * FROM t1".to_string(),
                fingerprint: "q3".to_string(),
                execution_time_ms: 300.0,
                memory_bytes: 4096,
                rows_scanned: 10000,
                rows_returned: 5000,
                object_size_bytes: 100000,
                timestamp: SystemTime::now(),
                parallel_execution: false,
                cache_hit: false,
            },
        ];
        for stat in stats {
            intelligence.record_execution(stat).await;
        }
        let distribution = intelligence.get_complexity_distribution().await;
        assert_eq!(distribution["trivial"], 1);
        assert_eq!(distribution["simple"], 1);
        assert_eq!(distribution["moderate"], 1);
    }
    #[test]
    fn test_complexity_class_string_conversion() {
        assert_eq!(ComplexityClass::Trivial.as_str(), "trivial");
        assert_eq!(ComplexityClass::Simple.as_str(), "simple");
        assert_eq!(ComplexityClass::Moderate.as_str(), "moderate");
        assert_eq!(ComplexityClass::Complex.as_str(), "complex");
        assert_eq!(ComplexityClass::VeryComplex.as_str(), "very_complex");
    }
    #[test]
    fn test_complexity_class_color_codes() {
        assert_eq!(ComplexityClass::Trivial.color_code(), "#00ff00");
        assert_eq!(ComplexityClass::Simple.color_code(), "#90ee90");
        assert_eq!(ComplexityClass::Moderate.color_code(), "#ffff00");
        assert_eq!(ComplexityClass::Complex.color_code(), "#ffa500");
        assert_eq!(ComplexityClass::VeryComplex.color_code(), "#ff0000");
    }
    #[test]
    fn test_complexity_class_serialization() {
        let classes = vec![
            ComplexityClass::Trivial,
            ComplexityClass::Simple,
            ComplexityClass::Moderate,
            ComplexityClass::Complex,
            ComplexityClass::VeryComplex,
        ];
        for class in classes {
            let json = serde_json::to_string(&class).expect("Failed to serialize");
            let deserialized: ComplexityClass =
                serde_json::from_str(&json).expect("Failed to deserialize");
            assert_eq!(class, deserialized);
        }
    }
}
