//! Auto-generated module
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use crate::api::select::ParsedQuery;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;

// Import Default trait implementations
#[allow(unused_imports)]
use super::columnusagestats_traits::*;
#[allow(unused_imports)]
use super::complexitycomponents_traits::*;
#[allow(unused_imports)]
use super::costbreakdown_traits::*;
#[allow(unused_imports)]
use super::costmodelweights_traits::*;
#[allow(unused_imports)]
use super::datastatistics_traits::*;
#[allow(unused_imports)]
use super::queryintelligence_traits::*;

/// Query complexity classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplexityClass {
    /// Trivial queries (simple SELECT with basic filter)
    Trivial,
    /// Simple queries (single table, basic operations)
    Simple,
    /// Moderate queries (multiple operations, some aggregations)
    Moderate,
    /// Complex queries (joins, window functions, CTEs)
    Complex,
    /// Very complex queries (multiple joins, subqueries, advanced features)
    VeryComplex,
}
impl ComplexityClass {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ComplexityClass::Trivial => "trivial",
            ComplexityClass::Simple => "simple",
            ComplexityClass::Moderate => "moderate",
            ComplexityClass::Complex => "complex",
            ComplexityClass::VeryComplex => "very_complex",
        }
    }
    /// Get color code for visualization (for dashboards)
    pub fn color_code(&self) -> &'static str {
        match self {
            ComplexityClass::Trivial => "#00ff00",
            ComplexityClass::Simple => "#90ee90",
            ComplexityClass::Moderate => "#ffff00",
            ComplexityClass::Complex => "#ffa500",
            ComplexityClass::VeryComplex => "#ff0000",
        }
    }
}
/// Query similarity score
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySimilarity {
    /// Similar query SQL
    pub query: String,
    /// Similarity score (0.0 - 1.0)
    pub similarity: f64,
    /// Cached result key (if available)
    pub cache_key: Option<String>,
}
/// Type of index to create
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IndexType {
    /// B-Tree index (good for range queries, sorting)
    BTree,
    /// Hash index (good for equality lookups)
    Hash,
    /// Full-text search index
    FullText,
    /// Bitmap index (good for low-cardinality columns)
    Bitmap,
}
/// Index recommendation based on query patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexRecommendation {
    /// Column name to index
    pub column_name: String,
    /// Recommended index type
    pub index_type: IndexType,
    /// Reason for recommendation
    pub reason: IndexReason,
    /// Impact score (0.0 - 1.0, higher = more beneficial)
    pub impact_score: f64,
    /// Estimated speedup factor (e.g., 5.0 = 5x faster)
    pub estimated_speedup: f64,
    /// Number of queries that would benefit
    pub query_count: usize,
    /// Average selectivity of queries using this column
    pub avg_selectivity: f64,
}
/// Reason for index recommendation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum IndexReason {
    /// Frequently used in WHERE clauses
    FilterColumn,
    /// Frequently used in ORDER BY
    SortColumn,
    /// Used in JOIN conditions
    JoinColumn,
    /// Used in GROUP BY
    GroupByColumn,
    /// High scan cost without index
    HighScanCost,
}
/// Complexity components breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityComponents {
    /// Projection complexity (number and type of columns)
    pub projection_score: f64,
    /// Filter/WHERE clause complexity
    pub filter_score: f64,
    /// Aggregation complexity (GROUP BY, aggregates)
    pub aggregation_score: f64,
    /// Join complexity (number and type of joins)
    pub join_score: f64,
    /// Sorting complexity (ORDER BY)
    pub sort_score: f64,
    /// Subquery/CTE complexity
    pub subquery_score: f64,
    /// Window function complexity
    pub window_score: f64,
    /// Overall SQL feature usage score
    pub feature_score: f64,
}
/// Query statistics summary
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryStatsSummary {
    /// Total number of queries executed
    pub total_queries: usize,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Cache hit rate (0.0 - 1.0)
    pub cache_hit_rate: f64,
    /// Parallel execution rate (0.0 - 1.0)
    pub parallel_execution_rate: f64,
    /// Average selectivity (rows returned / rows scanned)
    pub avg_selectivity: f64,
    /// Total rows scanned across all queries
    pub total_rows_scanned: u64,
    /// Total rows returned across all queries
    pub total_rows_returned: u64,
    /// Slowest queries (top 5)
    pub slowest_queries: Vec<String>,
}
/// Query execution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStats {
    /// SQL query text
    pub sql: String,
    /// Normalized query fingerprint
    pub fingerprint: String,
    /// Actual execution time in milliseconds
    pub execution_time_ms: f64,
    /// Actual memory usage in bytes
    pub memory_bytes: u64,
    /// Number of rows scanned
    pub rows_scanned: u64,
    /// Number of rows returned
    pub rows_returned: u64,
    /// Object size in bytes
    pub object_size_bytes: u64,
    /// Timestamp of execution
    pub timestamp: SystemTime,
    /// Whether query used parallel execution
    pub parallel_execution: bool,
    /// Whether query hit cache
    pub cache_hit: bool,
}
/// Query complexity estimation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryComplexity {
    /// Complexity score (0.0 - 100.0)
    pub score: f64,
    /// Complexity classification
    pub classification: ComplexityClass,
    /// Individual component scores
    pub components: ComplexityComponents,
    /// Estimated resource requirements
    pub resource_estimate: ResourceEstimate,
    /// Optimization suggestions
    pub suggestions: Vec<String>,
}
/// Query intelligence engine
pub struct QueryIntelligence {
    /// Historical query statistics
    stats: Arc<RwLock<Vec<QueryStats>>>,
    /// Query fingerprint to stats mapping
    fingerprint_index: Arc<RwLock<HashMap<String, Vec<usize>>>>,
    /// Maximum history size
    max_history: usize,
    /// Cost model weights (learned from execution history)
    cost_weights: Arc<RwLock<CostModelWeights>>,
}
impl QueryIntelligence {
    /// Create a new query intelligence engine
    pub fn new() -> Self {
        Self::with_capacity(10000)
    }
    /// Create with custom history capacity
    pub fn with_capacity(max_history: usize) -> Self {
        Self {
            stats: Arc::new(RwLock::new(Vec::with_capacity(max_history))),
            fingerprint_index: Arc::new(RwLock::new(HashMap::new())),
            max_history,
            cost_weights: Arc::new(RwLock::new(CostModelWeights::default())),
        }
    }
    /// Record query execution statistics
    pub async fn record_execution(&self, stats: QueryStats) {
        let mut history = self.stats.write().await;
        let mut index = self.fingerprint_index.write().await;
        let idx = history.len();
        history.push(stats.clone());
        index
            .entry(stats.fingerprint.clone())
            .or_insert_with(Vec::new)
            .push(idx);
        if history.len() > self.max_history {
            history.remove(0);
            index.clear();
            for (i, s) in history.iter().enumerate() {
                index
                    .entry(s.fingerprint.clone())
                    .or_insert_with(Vec::new)
                    .push(i);
            }
        }
        drop(history);
        drop(index);
        self.update_cost_model().await;
    }
    /// Predict query execution cost
    pub async fn predict_cost(
        &self,
        query: &ParsedQuery,
        data_stats: &DataStatistics,
    ) -> QueryCost {
        let weights = self.cost_weights.read().await;
        let fingerprint = Self::compute_fingerprint(query);
        let history = self.stats.read().await;
        let index = self.fingerprint_index.read().await;
        let similar_stats: Vec<&QueryStats> = index
            .get(&fingerprint)
            .map(|indices| indices.iter().filter_map(|&i| history.get(i)).collect())
            .unwrap_or_default();
        let mut breakdown = CostBreakdown::default();
        let estimated_rows = if similar_stats.is_empty() {
            data_stats.total_rows
        } else {
            let avg_scanned: u64 = similar_stats.iter().map(|s| s.rows_scanned).sum::<u64>()
                / similar_stats.len() as u64;
            avg_scanned
        };
        breakdown.scan_cost = estimated_rows as f64 * weights.scan_per_row;
        let filter_count = query
            .where_clause
            .as_ref()
            .map(|_| Self::count_conditions(query))
            .unwrap_or(0);
        breakdown.filter_cost =
            estimated_rows as f64 * filter_count as f64 * weights.filter_per_row;
        let column_count = query.columns.len();
        breakdown.projection_cost =
            estimated_rows as f64 * column_count as f64 * weights.projection_per_column;
        let has_aggregates = query
            .columns
            .iter()
            .any(|col| matches!(col, crate::api::select::SelectColumn::Aggregate { .. }));
        if has_aggregates {
            let group_count = if let Some(ref group_by) = query.group_by {
                if group_by.is_empty() {
                    1
                } else {
                    group_by
                        .iter()
                        .filter_map(|col| data_stats.column_cardinality.get(col))
                        .product::<u64>()
                        .max(1)
                }
            } else {
                1
            };
            breakdown.aggregation_cost = group_count as f64 * weights.aggregation_per_group;
        }
        if let Some(ref order_by) = query.order_by {
            if !order_by.is_empty() {
                let comparisons = if estimated_rows > 0 {
                    estimated_rows as f64 * (estimated_rows as f64).log2()
                } else {
                    0.0
                };
                breakdown.sort_cost = comparisons * weights.sort_per_comparison;
            }
        }
        let execution_time_ms = breakdown.scan_cost
            + breakdown.filter_cost
            + breakdown.projection_cost
            + breakdown.aggregation_cost
            + breakdown.sort_cost
            + breakdown.join_cost;
        let bytes_per_row = data_stats.avg_row_size;
        let memory_bytes = (estimated_rows as f64 * bytes_per_row) as u64;
        let chunk_size = 8192;
        let io_operations = estimated_rows.div_ceil(chunk_size);
        let confidence = if similar_stats.is_empty() {
            0.3
        } else {
            let sample_factor = (similar_stats.len() as f64 / 10.0).min(1.0);
            0.5 + 0.5 * sample_factor
        };
        QueryCost {
            execution_time_ms,
            memory_bytes,
            io_operations,
            confidence,
            breakdown,
        }
    }
    /// Get recommended execution strategy
    pub async fn get_execution_strategy(
        &self,
        query: &ParsedQuery,
        data_stats: &DataStatistics,
    ) -> ExecutionStrategy {
        let cost = self.predict_cost(query, data_stats).await;
        const PARALLEL_THRESHOLD_BYTES: u64 = 10 * 1024 * 1024;
        const STREAMING_MEMORY_LIMIT: u64 = 100 * 1024 * 1024;
        const LARGE_DATASET_ROWS: u64 = 1_000_000;
        let selectivity = self.estimate_selectivity(query, data_stats);
        if selectivity < 0.01 && data_stats.total_rows > 100_000 {
            return ExecutionStrategy::IndexScan;
        }
        if cost.memory_bytes > STREAMING_MEMORY_LIMIT {
            let chunk_size = (STREAMING_MEMORY_LIMIT / data_stats.avg_row_size as u64) as usize;
            return ExecutionStrategy::Streaming { chunk_size };
        }
        if data_stats.total_bytes > PARALLEL_THRESHOLD_BYTES
            || data_stats.total_rows > LARGE_DATASET_ROWS
        {
            let num_threads = num_cpus::get().min(8);
            return ExecutionStrategy::Parallel { num_threads };
        }
        if selectivity > 0.5 {
            ExecutionStrategy::FullScan
        } else {
            ExecutionStrategy::Sequential
        }
    }
    /// Find similar cached queries
    pub async fn find_similar_queries(
        &self,
        query: &ParsedQuery,
        threshold: f64,
    ) -> Vec<QuerySimilarity> {
        let fingerprint = Self::compute_fingerprint(query);
        let history = self.stats.read().await;
        let index = self.fingerprint_index.read().await;
        let mut similarities = Vec::new();
        if let Some(indices) = index.get(&fingerprint) {
            for &idx in indices {
                if let Some(stats) = history.get(idx) {
                    similarities.push(QuerySimilarity {
                        query: stats.sql.clone(),
                        similarity: 1.0,
                        cache_key: Some(format!("{}:{}", stats.sql, stats.fingerprint)),
                    });
                }
            }
        }
        if similarities.is_empty() {
            let query_sql = Self::normalize_query(query);
            for stats in history.iter().rev().take(100) {
                let similarity = Self::compute_similarity(&query_sql, &stats.sql);
                if similarity >= threshold {
                    similarities.push(QuerySimilarity {
                        query: stats.sql.clone(),
                        similarity,
                        cache_key: Some(format!("{}:{}", stats.sql, stats.fingerprint)),
                    });
                }
            }
        }
        similarities.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        similarities.truncate(10);
        similarities
    }
    /// Get query execution statistics
    pub async fn get_statistics(&self) -> Vec<QueryStats> {
        self.stats.read().await.clone()
    }
    /// Get query statistics summary
    pub async fn get_summary(&self) -> QueryStatsSummary {
        let history = self.stats.read().await;
        if history.is_empty() {
            return QueryStatsSummary::default();
        }
        let total_queries = history.len();
        let total_execution_time: f64 = history.iter().map(|s| s.execution_time_ms).sum();
        let avg_execution_time = total_execution_time / total_queries as f64;
        let cache_hits = history.iter().filter(|s| s.cache_hit).count();
        let cache_hit_rate = cache_hits as f64 / total_queries as f64;
        let parallel_executions = history.iter().filter(|s| s.parallel_execution).count();
        let parallel_rate = parallel_executions as f64 / total_queries as f64;
        let total_rows_scanned: u64 = history.iter().map(|s| s.rows_scanned).sum();
        let total_rows_returned: u64 = history.iter().map(|s| s.rows_returned).sum();
        let avg_selectivity = if total_rows_scanned > 0 {
            total_rows_returned as f64 / total_rows_scanned as f64
        } else {
            0.0
        };
        let mut sorted_by_time = history.clone();
        sorted_by_time.sort_by(|a, b| {
            b.execution_time_ms
                .partial_cmp(&a.execution_time_ms)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let slowest_queries: Vec<String> = sorted_by_time
            .iter()
            .take(5)
            .map(|s| s.sql.clone())
            .collect();
        QueryStatsSummary {
            total_queries,
            avg_execution_time_ms: avg_execution_time,
            cache_hit_rate,
            parallel_execution_rate: parallel_rate,
            avg_selectivity,
            total_rows_scanned,
            total_rows_returned,
            slowest_queries,
        }
    }
    /// Get automatic index recommendations based on query patterns
    ///
    /// Analyzes historical query patterns to recommend which columns/fields
    /// should be indexed for optimal performance.
    pub async fn get_index_recommendations(&self) -> Vec<IndexRecommendation> {
        let history = self.stats.read().await;
        if history.len() < 5 {
            return Vec::new();
        }
        let filter_columns: HashMap<String, ColumnUsageStats> = HashMap::new();
        let sort_columns: HashMap<String, ColumnUsageStats> = HashMap::new();
        let join_columns: HashMap<String, ColumnUsageStats> = HashMap::new();
        for _ in &*history {}
        let mut recommendations = Vec::new();
        for (column, usage) in filter_columns.iter() {
            if usage.frequency > history.len() / 10 {
                let impact_score = Self::calculate_index_impact(usage, history.len());
                recommendations.push(IndexRecommendation {
                    column_name: column.clone(),
                    index_type: IndexType::BTree,
                    reason: IndexReason::FilterColumn,
                    impact_score,
                    estimated_speedup: Self::estimate_speedup(usage),
                    query_count: usage.frequency,
                    avg_selectivity: usage.avg_selectivity,
                });
            }
        }
        for (column, usage) in sort_columns.iter() {
            if usage.frequency > history.len() / 20 {
                let impact_score = Self::calculate_index_impact(usage, history.len());
                recommendations.push(IndexRecommendation {
                    column_name: column.clone(),
                    index_type: IndexType::BTree,
                    reason: IndexReason::SortColumn,
                    impact_score,
                    estimated_speedup: Self::estimate_speedup(usage),
                    query_count: usage.frequency,
                    avg_selectivity: usage.avg_selectivity,
                });
            }
        }
        for (column, usage) in join_columns.iter() {
            if usage.frequency > 0 {
                let impact_score = Self::calculate_index_impact(usage, history.len()) * 1.5;
                recommendations.push(IndexRecommendation {
                    column_name: column.clone(),
                    index_type: IndexType::Hash,
                    reason: IndexReason::JoinColumn,
                    impact_score,
                    estimated_speedup: Self::estimate_speedup(usage) * 2.0,
                    query_count: usage.frequency,
                    avg_selectivity: usage.avg_selectivity,
                });
            }
        }
        recommendations.sort_by(|a, b| {
            b.impact_score
                .partial_cmp(&a.impact_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        recommendations.truncate(10);
        recommendations
    }
    /// Calculate the potential impact of creating an index
    fn calculate_index_impact(usage: &ColumnUsageStats, total_queries: usize) -> f64 {
        let frequency_score = usage.frequency as f64 / total_queries as f64;
        let selectivity_score = 1.0 - usage.avg_selectivity;
        let time_score = usage.avg_query_time / 1000.0;
        frequency_score * 0.4 + selectivity_score * 0.3 + time_score.min(1.0) * 0.3
    }
    /// Estimate speedup from adding an index
    fn estimate_speedup(usage: &ColumnUsageStats) -> f64 {
        if usage.avg_selectivity < 0.01 {
            10.0
        } else if usage.avg_selectivity < 0.1 {
            5.0
        } else if usage.avg_selectivity < 0.5 {
            2.0
        } else {
            1.2
        }
    }
    /// Update cost model weights using linear regression
    async fn update_cost_model(&self) {
        let history = self.stats.read().await;
        if history.len() < 10 {
            return;
        }
        let recent: Vec<&QueryStats> = history.iter().rev().take(100).collect();
        let mut scan_samples: Vec<f64> = Vec::new();
        for stats in recent {
            if stats.rows_scanned > 0 {
                let scan_rate = stats.execution_time_ms / stats.rows_scanned as f64;
                scan_samples.push(scan_rate);
            }
        }
        let mut weights = self.cost_weights.write().await;
        let alpha = 0.1;
        if !scan_samples.is_empty() {
            let avg_scan_rate: f64 = scan_samples.iter().sum::<f64>() / scan_samples.len() as f64;
            weights.scan_per_row = weights.scan_per_row * (1.0 - alpha) + avg_scan_rate * alpha;
        }
        weights.sample_count += history.len();
    }
    /// Compute query fingerprint for caching
    pub fn compute_fingerprint(query: &ParsedQuery) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let normalized = Self::normalize_query(query);
        let mut hasher = DefaultHasher::new();
        normalized.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
    /// Normalize query for comparison
    pub fn normalize_query(query: &ParsedQuery) -> String {
        let mut parts = Vec::new();
        if let Some(ref alias) = query.from_alias {
            parts.push(alias.clone());
        } else {
            parts.push("s3object".to_string());
        }
        let mut cols: Vec<String> = query.columns.iter().map(|c| format!("{:?}", c)).collect();
        cols.sort();
        parts.push(cols.join(","));
        if let Some(ref group_by) = query.group_by {
            if !group_by.is_empty() {
                let mut groups = group_by.clone();
                groups.sort();
                parts.push(groups.join(","));
            }
        }
        if let Some(ref order_by) = query.order_by {
            if !order_by.is_empty() {
                let orders: Vec<String> = order_by
                    .iter()
                    .map(|o| format!("{}:{}", o.column, if o.ascending { "ASC" } else { "DESC" }))
                    .collect();
                parts.push(orders.join(","));
            }
        }
        parts.join("|")
    }
    /// Count conditions in WHERE clause
    fn count_conditions(query: &ParsedQuery) -> usize {
        query.where_clause.as_ref().map(|_| 1).unwrap_or(0)
    }
    /// Estimate query selectivity
    fn estimate_selectivity(&self, query: &ParsedQuery, _data_stats: &DataStatistics) -> f64 {
        if query.where_clause.is_none() {
            return 1.0;
        }
        0.1
    }
    /// Compute similarity between two query strings
    pub fn compute_similarity(query1: &str, query2: &str) -> f64 {
        let distance = Self::levenshtein_distance(query1, query2);
        let max_len = query1.len().max(query2.len());
        if max_len == 0 {
            return 1.0;
        }
        1.0 - (distance as f64 / max_len as f64)
    }
    /// Compute Levenshtein edit distance
    pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
        let len1 = s1.chars().count();
        let len2 = s2.chars().count();
        if len1 == 0 {
            return len2;
        }
        if len2 == 0 {
            return len1;
        }
        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];
        #[allow(clippy::needless_range_loop)]
        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        #[allow(clippy::needless_range_loop)]
        for j in 0..=len2 {
            matrix[0][j] = j;
        }
        let chars1: Vec<char> = s1.chars().collect();
        let chars2: Vec<char> = s2.chars().collect();
        for i in 1..=len1 {
            for j in 1..=len2 {
                let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };
                matrix[i][j] = (matrix[i - 1][j] + 1)
                    .min(matrix[i][j - 1] + 1)
                    .min(matrix[i - 1][j - 1] + cost);
            }
        }
        matrix[len1][len2]
    }
}
impl QueryIntelligence {
    /// Calculate query complexity
    ///
    /// Analyzes a parsed query and returns a comprehensive complexity assessment
    /// including a numerical score, classification, and optimization suggestions.
    ///
    /// # Arguments
    ///
    /// * `query` - Parsed SQL query to analyze
    /// * `object_size_bytes` - Size of the object being queried (for scaling estimates)
    ///
    /// # Returns
    ///
    /// QueryComplexity with score, classification, and recommendations
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let complexity = intelligence.calculate_complexity(&parsed_query, 10_000_000).await;
    /// println!("Complexity: {} (score: {})", complexity.classification.as_str(), complexity.score);
    /// ```
    pub async fn calculate_complexity(
        &self,
        query: &ParsedQuery,
        object_size_bytes: u64,
    ) -> QueryComplexity {
        let mut components = ComplexityComponents::default();
        let mut suggestions = Vec::new();
        let projection_count = query.columns.len();
        components.projection_score = match projection_count {
            0..=3 => 2.0,
            4..=10 => 5.0,
            11..=20 => 10.0,
            _ => 15.0,
        };
        if projection_count > 10 {
            suggestions.push("Consider reducing the number of projected columns".to_string());
        }
        if query.where_clause.is_some() {
            components.filter_score = 5.0;
        }
        let has_aggregates = query
            .columns
            .iter()
            .any(|col| matches!(col, crate::api::select::SelectColumn::Aggregate { .. }));
        if has_aggregates {
            components.aggregation_score = 5.0;
            if query.group_by.is_some() {
                components.aggregation_score += 5.0;
                if let Some(ref group_by) = query.group_by {
                    components.aggregation_score += (group_by.len().saturating_sub(1)) as f64 * 2.0;
                }
            }
            components.aggregation_score = components.aggregation_score.min(15.0);
            if query.group_by.as_ref().map_or(0, |g| g.len()) > 3 {
                suggestions.push("High-cardinality GROUP BY may impact performance".to_string());
            }
        }
        components.join_score = 0.0;
        if let Some(ref order_by) = query.order_by {
            components.sort_score = 5.0 + (order_by.len().saturating_sub(1)) as f64 * 2.0;
            components.sort_score = components.sort_score.min(10.0);
            if order_by.len() > 2 {
                suggestions.push("Multi-column sorting can be expensive".to_string());
            }
        }
        components.subquery_score = 0.0;
        components.window_score = 0.0;
        if query.limit.is_some() {
            components.feature_score -= 2.0;
        }
        let total_score = components.projection_score
            + components.filter_score
            + components.aggregation_score
            + components.join_score
            + components.sort_score
            + components.subquery_score
            + components.window_score
            + components.feature_score;
        let classification = match total_score {
            s if s < 10.0 => ComplexityClass::Trivial,
            s if s < 25.0 => ComplexityClass::Simple,
            s if s < 45.0 => ComplexityClass::Moderate,
            s if s < 65.0 => ComplexityClass::Complex,
            _ => ComplexityClass::VeryComplex,
        };
        let resource_estimate = self
            .estimate_resources(total_score, object_size_bytes, query)
            .await;
        QueryComplexity {
            score: total_score,
            classification,
            components,
            resource_estimate,
            suggestions,
        }
    }
    /// Estimate resource requirements based on complexity
    async fn estimate_resources(
        &self,
        complexity_score: f64,
        object_size_bytes: u64,
        query: &ParsedQuery,
    ) -> ResourceEstimate {
        let cpu_intensity = match complexity_score {
            s if s < 10.0 => 1,
            s if s < 25.0 => 2,
            s if s < 45.0 => 4,
            s if s < 65.0 => 7,
            _ => 10,
        };
        let has_aggregates = query
            .columns
            .iter()
            .any(|col| matches!(col, crate::api::select::SelectColumn::Aggregate { .. }));
        let has_sorting = query.order_by.is_some();
        let memory_intensity = match (has_aggregates, has_sorting) {
            (false, false) => 1,
            (true, false) => 3,
            (false, true) => 4,
            (true, true) => 6,
        };
        let io_intensity = if object_size_bytes < 1_000_000 {
            1
        } else if object_size_bytes < 10_000_000 {
            3
        } else if object_size_bytes < 100_000_000 {
            5
        } else if object_size_bytes < 1_000_000_000 {
            7
        } else {
            10
        };
        let recommended_parallelism = if object_size_bytes > 10_000_000 && complexity_score > 20.0 {
            4
        } else if object_size_bytes > 100_000_000 {
            8
        } else {
            1
        };
        let cacheable = complexity_score < 30.0 && object_size_bytes < 10_000_000;
        let execution_tier = match (complexity_score, object_size_bytes) {
            (s, size) if s < 10.0 && size < 1_000_000 => ExecutionTier::VeryFast,
            (s, size) if s < 25.0 && size < 10_000_000 => ExecutionTier::Fast,
            (s, size) if s < 45.0 && size < 100_000_000 => ExecutionTier::Medium,
            (s, size) if s < 65.0 || size < 1_000_000_000 => ExecutionTier::Slow,
            _ => ExecutionTier::VerySlow,
        };
        ResourceEstimate {
            cpu_intensity,
            memory_intensity,
            io_intensity,
            recommended_parallelism,
            cacheable,
            execution_tier,
        }
    }
    /// Get complexity distribution of historical queries
    ///
    /// Returns a summary of complexity classification across all recorded queries
    pub async fn get_complexity_distribution(&self) -> HashMap<String, usize> {
        let stats = self.stats.read().await;
        let mut distribution = HashMap::new();
        distribution.insert("trivial".to_string(), 0);
        distribution.insert("simple".to_string(), 0);
        distribution.insert("moderate".to_string(), 0);
        distribution.insert("complex".to_string(), 0);
        distribution.insert("very_complex".to_string(), 0);
        for stat in stats.iter() {
            let class = if stat.execution_time_ms < 10.0 {
                "trivial"
            } else if stat.execution_time_ms < 100.0 {
                "simple"
            } else if stat.execution_time_ms < 500.0 {
                "moderate"
            } else if stat.execution_time_ms < 2000.0 {
                "complex"
            } else {
                "very_complex"
            };
            *distribution.entry(class.to_string()).or_insert(0) += 1;
        }
        distribution
    }
}
/// Cost model weights learned from execution history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModelWeights {
    /// Base cost per row scanned (ms)
    pub scan_per_row: f64,
    /// Cost per filter operation (ms)
    pub filter_per_row: f64,
    /// Cost per projection (ms)
    pub projection_per_column: f64,
    /// Cost per aggregation (ms)
    pub aggregation_per_group: f64,
    /// Cost per sort comparison (ms)
    pub sort_per_comparison: f64,
    /// Cost per join operation (ms)
    pub join_per_row: f64,
    /// Memory cost per byte (relative)
    pub memory_per_byte: f64,
    /// Number of samples used for learning
    pub sample_count: usize,
}
/// Query execution cost prediction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryCost {
    /// Predicted execution time in milliseconds
    pub execution_time_ms: f64,
    /// Predicted memory usage in bytes
    pub memory_bytes: u64,
    /// Predicted I/O operations
    pub io_operations: u64,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// Cost breakdown by operation type
    pub breakdown: CostBreakdown,
}
/// Data distribution statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataStatistics {
    /// Total number of rows
    pub total_rows: u64,
    /// Total data size in bytes
    pub total_bytes: u64,
    /// Average row size in bytes
    pub avg_row_size: f64,
    /// Data format (CSV, JSON, Parquet, etc.)
    pub format: String,
    /// Compression ratio (if compressed)
    pub compression_ratio: Option<f64>,
    /// Column cardinality (unique values per column)
    pub column_cardinality: HashMap<String, u64>,
    /// Null percentages per column
    pub null_percentages: HashMap<String, f64>,
    /// Whether data is sorted
    pub is_sorted: bool,
    /// Data skew factor (coefficient of variation)
    pub skew_factor: f64,
}
/// Execution strategy recommendation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    /// Sequential execution for small datasets
    Sequential,
    /// Parallel execution for large datasets
    Parallel { num_threads: usize },
    /// Streaming execution for memory-constrained queries
    Streaming { chunk_size: usize },
    /// Index scan for highly selective queries
    IndexScan,
    /// Full table scan for low selectivity
    FullScan,
}
/// Estimated resource requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEstimate {
    /// Estimated CPU usage (relative scale 1-10)
    pub cpu_intensity: u8,
    /// Estimated memory usage (relative scale 1-10)
    pub memory_intensity: u8,
    /// Estimated I/O intensity (relative scale 1-10)
    pub io_intensity: u8,
    /// Recommended parallelization level
    pub recommended_parallelism: usize,
    /// Whether query benefits from caching
    pub cacheable: bool,
    /// Estimated execution time tier
    pub execution_tier: ExecutionTier,
}
/// Cost breakdown by operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    /// Cost of scanning data
    pub scan_cost: f64,
    /// Cost of filtering/predicates
    pub filter_cost: f64,
    /// Cost of projections
    pub projection_cost: f64,
    /// Cost of aggregations
    pub aggregation_cost: f64,
    /// Cost of sorting
    pub sort_cost: f64,
    /// Cost of joins
    pub join_cost: f64,
}
/// Execution time tiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionTier {
    /// < 10ms
    VeryFast,
    /// 10-100ms
    Fast,
    /// 100ms-1s
    Medium,
    /// 1-10s
    Slow,
    /// > 10s
    VerySlow,
}
/// Statistics about column usage patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnUsageStats {
    /// Number of queries using this column
    pub frequency: usize,
    /// Average selectivity (rows_returned / rows_scanned)
    pub avg_selectivity: f64,
    /// Average query execution time (ms)
    pub avg_query_time: f64,
    /// Total rows scanned across all queries
    pub total_rows_scanned: u64,
}
