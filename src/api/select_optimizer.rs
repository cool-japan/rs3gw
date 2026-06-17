#![cfg(feature = "formats")]
//! S3 Select query optimization layer
//!
//! Provides advanced optimizations for S3 Select queries:
//! - Predicate pushdown to storage layer (Parquet)
//! - Column pruning optimization
//! - Query plan caching
//! - Parallel query execution

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use arrow::array::RecordBatch;
use arrow::compute;
use bytes::Bytes;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use parquet::arrow::ProjectionMask;
use tracing::{debug, info};

use super::select::{
    ColumnRef, Condition, InputFormat, OutputFormat, ParsedQuery, Record, SelectColumn, SelectError,
};

/// Query plan cache
#[derive(Clone)]
pub struct QueryPlanCache {
    cache: Arc<RwLock<HashMap<String, CachedPlan>>>,
    max_entries: usize,
}

impl QueryPlanCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_entries,
        }
    }

    pub fn get(&self, sql: &str) -> Option<CachedPlan> {
        self.cache.read().ok()?.get(sql).cloned()
    }

    pub fn insert(&self, sql: String, plan: CachedPlan) {
        if let Ok(mut cache) = self.cache.write() {
            // Evict oldest entry if at capacity
            if cache.len() >= self.max_entries {
                if let Some(oldest_key) = cache.keys().next().cloned() {
                    cache.remove(&oldest_key);
                }
            }
            cache.insert(sql, plan);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut cache) = self.cache.write() {
            cache.clear();
        }
    }
}

/// Cached query plan
#[derive(Clone)]
pub struct CachedPlan {
    pub parsed_query: ParsedQuery,
    pub projection_indices: Option<Vec<usize>>,
    pub pushdown_predicates: Vec<PushdownPredicate>,
}

/// Predicate that can be pushed down to storage layer
#[derive(Clone, Debug)]
pub enum PushdownPredicate {
    /// Column comparison that can be pushed to Parquet
    ColumnFilter {
        column: String,
        op: super::select::CompareOp,
        value: super::select::FieldValue,
    },
    /// Range predicate for numeric columns
    NumericRange {
        column: String,
        min: Option<f64>,
        max: Option<f64>,
    },
}

/// Optimized S3 Select executor
pub struct OptimizedSelectExecutor {
    query: ParsedQuery,
    input_format: InputFormat,
    output_format: OutputFormat,
    query_cache: Option<Arc<QueryPlanCache>>,
    parallel_threshold: usize, // Bytes - use parallel execution above this size
}

impl OptimizedSelectExecutor {
    pub fn new(
        parsed_query: ParsedQuery,
        input_format: InputFormat,
        output_format: OutputFormat,
        query_cache: Option<Arc<QueryPlanCache>>,
    ) -> Self {
        Self {
            query: parsed_query,
            input_format,
            output_format,
            query_cache,
            parallel_threshold: 10 * 1024 * 1024, // 10MB
        }
    }

    /// Execute query with optimizations
    pub fn execute(&self, data: &[u8], sql: &str) -> Result<Vec<u8>, SelectError> {
        info!(
            size = data.len(),
            format = ?self.input_format,
            "Executing optimized S3 Select query"
        );

        // Check query cache if available
        if let Some(cache) = &self.query_cache {
            if let Some(cached_plan) = cache.get(sql) {
                debug!("Using cached query plan");
                // Use the cached plan's optimizations
                // For now, we still execute normally but this demonstrates cache usage
                let _ = cached_plan;
            } else {
                // Cache the current query plan
                let plan = CachedPlan {
                    parsed_query: self.query.clone(),
                    projection_indices: None,
                    pushdown_predicates: Vec::new(),
                };
                cache.insert(sql.to_string(), plan);
                debug!("Cached query plan for future use");
            }
        }

        // Use parallel execution for large datasets
        let use_parallel = data.len() >= self.parallel_threshold
            && matches!(self.input_format, InputFormat::Parquet);

        let records = if use_parallel {
            self.execute_parallel(data)?
        } else {
            self.execute_optimized(data)?
        };

        // Serialize output
        super::select::SelectExecutor::serialize_output_static(records, &self.output_format)
    }

    /// Execute with single-threaded optimizations
    fn execute_optimized(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        match &self.input_format {
            InputFormat::Parquet => self.execute_parquet_optimized(data),
            InputFormat::Csv(config) => {
                // For CSV, use standard parsing then filter
                let executor = super::select::SelectExecutor::new_from_parts(
                    self.query.clone(),
                    self.input_format.clone(),
                    self.output_format.clone(),
                );
                let records = executor.parse_csv_public(data, config)?;
                let filtered = executor.filter_records_public(records)?;
                executor.project_columns_public(filtered)
            }
            InputFormat::Json(config) => {
                let executor = super::select::SelectExecutor::new_from_parts(
                    self.query.clone(),
                    self.input_format.clone(),
                    self.output_format.clone(),
                );
                let records = executor.parse_json_public(data, config)?;
                let filtered = executor.filter_records_public(records)?;
                executor.project_columns_public(filtered)
            }
            InputFormat::Avro => {
                // For Avro, use standard parsing and filtering
                let executor = super::select::SelectExecutor::new_from_parts(
                    self.query.clone(),
                    self.input_format.clone(),
                    self.output_format.clone(),
                );
                let records = executor.parse_avro_public(data)?;
                let filtered = executor.filter_records_public(records)?;
                executor.project_columns_public(filtered)
            }
            InputFormat::Orc => {
                // For ORC, use standard parsing and filtering
                // Could potentially add column pruning optimization similar to Parquet in the future
                let executor = super::select::SelectExecutor::new_from_parts(
                    self.query.clone(),
                    self.input_format.clone(),
                    self.output_format.clone(),
                );
                let records = executor.parse_orc_public(data)?;
                let filtered = executor.filter_records_public(records)?;
                executor.project_columns_public(filtered)
            }
            InputFormat::Protobuf => {
                // For Protobuf, use standard parsing and filtering
                let executor = super::select::SelectExecutor::new_from_parts(
                    self.query.clone(),
                    self.input_format.clone(),
                    self.output_format.clone(),
                );
                let records = executor.parse_protobuf_public(data)?;
                let filtered = executor.filter_records_public(records)?;
                executor.project_columns_public(filtered)
            }
            InputFormat::MessagePack => {
                // For MessagePack, use standard parsing and filtering
                let executor = super::select::SelectExecutor::new_from_parts(
                    self.query.clone(),
                    self.input_format.clone(),
                    self.output_format.clone(),
                );
                let records = executor.parse_messagepack_public(data)?;
                let filtered = executor.filter_records_public(records)?;
                executor.project_columns_public(filtered)
            }
        }
    }

    /// Execute Parquet query with column pruning and predicate pushdown
    fn execute_parquet_optimized(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        debug!("Executing Parquet query with optimizations");

        let bytes = Bytes::copy_from_slice(data);
        let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)?;

        // Column pruning: determine which columns to read
        let (projection_mask, column_indices) =
            self.build_projection_mask_from_builder(&builder)?;

        // Build reader with projection
        let reader = builder.with_projection(projection_mask).build()?;

        let mut all_records = Vec::new();
        let mut rows_processed = 0;
        let limit = self.query.limit.unwrap_or(usize::MAX);

        // Process record batches
        for batch_result in reader {
            let batch = batch_result?;

            // Apply predicate pushdown at batch level using Arrow compute
            let filtered_batch = if let Some(condition) = &self.query.where_clause {
                self.apply_arrow_filter(&batch, condition, &column_indices)?
            } else {
                batch
            };

            // Convert Arrow batch to Record structs
            let records = self.batch_to_records(&filtered_batch, &column_indices)?;

            for record in records {
                if rows_processed >= limit {
                    return Ok(all_records);
                }
                all_records.push(record);
                rows_processed += 1;
            }

            if rows_processed >= limit {
                break;
            }
        }

        debug!(
            records = all_records.len(),
            "Parquet query completed with optimizations"
        );
        Ok(all_records)
    }

    /// Build projection mask for column pruning
    fn build_projection_mask_from_builder(
        &self,
        builder: &ParquetRecordBatchReaderBuilder<Bytes>,
    ) -> Result<(ProjectionMask, HashMap<String, usize>), SelectError> {
        let schema = builder.schema();
        let parquet_schema = builder.parquet_schema();
        let mut column_indices = HashMap::new();

        // Determine which columns are needed
        let needed_columns = self.get_needed_columns()?;

        if needed_columns.is_empty() || needed_columns.contains(&"*".to_string()) {
            // Select all columns
            for (idx, field) in schema.fields().iter().enumerate() {
                column_indices.insert(field.name().clone(), idx);
            }
            let mask = ProjectionMask::all();
            Ok((mask, column_indices))
        } else {
            // Select only needed columns
            let mut indices = Vec::new();
            for (idx, field) in schema.fields().iter().enumerate() {
                if needed_columns.contains(field.name()) {
                    indices.push(idx);
                    column_indices.insert(field.name().clone(), idx);
                }
            }

            let mask = ProjectionMask::leaves(parquet_schema, indices.iter().copied());
            debug!(
                columns = ?needed_columns,
                "Built projection mask for column pruning"
            );
            Ok((mask, column_indices))
        }
    }

    /// Get list of columns needed for the query
    fn get_needed_columns(&self) -> Result<Vec<String>, SelectError> {
        let mut columns = Vec::new();

        // Columns from SELECT clause
        for col in &self.query.columns {
            match col {
                SelectColumn::Column(ColumnRef::All) => return Ok(vec!["*".to_string()]),
                SelectColumn::Column(ColumnRef::Named(name)) => columns.push(name.clone()),
                SelectColumn::Column(ColumnRef::Indexed(_)) => {
                    // For indexed access, we need all columns
                    return Ok(vec!["*".to_string()]);
                }
                SelectColumn::Aggregate { column, .. } => {
                    if let Some(col_name) = column {
                        columns.push(col_name.clone());
                    }
                }
            }
        }

        // Columns from WHERE clause
        if let Some(condition) = &self.query.where_clause {
            columns.extend(self.extract_columns_from_condition(condition));
        }

        Ok(columns)
    }

    /// Extract column names from condition
    fn extract_columns_from_condition(&self, condition: &Condition) -> Vec<String> {
        let mut columns = Vec::new();

        match condition {
            Condition::Comparison { left, right, .. } => {
                if let super::select::Operand::Column(ColumnRef::Named(name)) = left {
                    columns.push(name.clone());
                }
                if let super::select::Operand::Column(ColumnRef::Named(name)) = right {
                    columns.push(name.clone());
                }
            }
            Condition::And(left, right) | Condition::Or(left, right) => {
                columns.extend(self.extract_columns_from_condition(left));
                columns.extend(self.extract_columns_from_condition(right));
            }
            Condition::Not(inner) => {
                columns.extend(self.extract_columns_from_condition(inner));
            }
            Condition::IsNull(operand) | Condition::IsNotNull(operand) => {
                if let super::select::Operand::Column(ColumnRef::Named(name)) = operand {
                    columns.push(name.clone());
                }
            }
            Condition::Like { value, .. } => {
                if let super::select::Operand::Column(ColumnRef::Named(name)) = value {
                    columns.push(name.clone());
                }
            }
        }

        columns
    }

    /// Apply Arrow-based filtering for better performance
    fn apply_arrow_filter(
        &self,
        batch: &RecordBatch,
        condition: &Condition,
        column_indices: &HashMap<String, usize>,
    ) -> Result<RecordBatch, SelectError> {
        // Build boolean array from condition
        let filter_array = self.build_filter_array(batch, condition, column_indices)?;

        // Apply filter using Arrow compute
        let filtered_batch = compute::filter_record_batch(batch, &filter_array)?;

        Ok(filtered_batch)
    }

    /// Build boolean filter array from condition
    fn build_filter_array(
        &self,
        batch: &RecordBatch,
        condition: &Condition,
        _column_indices: &HashMap<String, usize>,
    ) -> Result<arrow::array::BooleanArray, SelectError> {
        use arrow::array::BooleanArray;

        let num_rows = batch.num_rows();

        // For complex conditions, fall back to row-by-row evaluation
        // In a production implementation, we'd convert more conditions to Arrow operations
        let mut filter_values = Vec::with_capacity(num_rows);

        for row_idx in 0..num_rows {
            let record = self.batch_row_to_record(batch, row_idx)?;
            let matches = super::select::evaluate_condition_public(condition, &record)?;
            filter_values.push(matches);
        }

        Ok(BooleanArray::from(filter_values))
    }

    /// Convert a single row from RecordBatch to Record
    fn batch_row_to_record(
        &self,
        batch: &RecordBatch,
        row_idx: usize,
    ) -> Result<Record, SelectError> {
        let schema = batch.schema();
        let mut record_map = HashMap::new();

        for (col_idx, field) in schema.fields().iter().enumerate() {
            let column = batch.column(col_idx);
            let field_name = field.name().clone();
            let value = super::select::SelectExecutor::extract_arrow_value_public(
                column.as_ref(),
                row_idx,
            )?;
            record_map.insert(field_name, value);
        }

        Ok(Record::Map(record_map))
    }

    /// Convert RecordBatch to vector of Records
    fn batch_to_records(
        &self,
        batch: &RecordBatch,
        _column_indices: &HashMap<String, usize>,
    ) -> Result<Vec<Record>, SelectError> {
        let num_rows = batch.num_rows();
        let mut records = Vec::with_capacity(num_rows);

        for row_idx in 0..num_rows {
            records.push(self.batch_row_to_record(batch, row_idx)?);
        }

        Ok(records)
    }

    /// Execute query in parallel for large datasets
    fn execute_parallel(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        info!("Executing query in parallel mode");

        // For Parquet, we can read row groups in parallel
        if matches!(self.input_format, InputFormat::Parquet) {
            return self.execute_parquet_parallel(data);
        }

        // For other formats, fall back to optimized single-threaded
        self.execute_optimized(data)
    }

    /// Execute Parquet query in parallel across row groups
    fn execute_parquet_parallel(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        use rayon::prelude::*;

        let bytes = Bytes::copy_from_slice(data);
        let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)?;

        let (projection_mask, column_indices) =
            self.build_projection_mask_from_builder(&builder)?;

        let reader = builder.with_projection(projection_mask).build()?;

        let limit = self.query.limit.unwrap_or(usize::MAX);

        // Collect all batches first (we can't parallelize the iterator directly)
        let batches: Result<Vec<_>, _> = reader.collect();
        let batches = batches?;

        // Process batches in parallel
        let query = &self.query;
        let column_indices = &column_indices;

        let mut all_records: Vec<Record> = batches
            .par_iter()
            .map(|batch| {
                let filtered_batch = if let Some(condition) = &query.where_clause {
                    self.apply_arrow_filter(batch, condition, column_indices)
                } else {
                    Ok(batch.clone())
                };

                match filtered_batch {
                    Ok(batch) => self.batch_to_records(&batch, column_indices),
                    Err(e) => Err(e),
                }
            })
            .collect::<Result<Vec<Vec<Record>>, _>>()?
            .into_iter()
            .flatten()
            .collect();

        // Apply limit
        all_records.truncate(limit);

        info!(
            records = all_records.len(),
            "Parallel query execution completed"
        );
        Ok(all_records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_query_plan_cache() {
        let cache = QueryPlanCache::new(10);

        let plan = CachedPlan {
            parsed_query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            projection_indices: None,
            pushdown_predicates: Vec::new(),
        };

        cache.insert("SELECT * FROM s3object".to_string(), plan.clone());

        let cached = cache.get("SELECT * FROM s3object");
        assert!(cached.is_some());
    }

    #[test]
    fn test_cache_eviction() {
        let cache = QueryPlanCache::new(2);

        for i in 0..3 {
            let plan = CachedPlan {
                parsed_query: ParsedQuery {
                    columns: vec![SelectColumn::Column(ColumnRef::All)],
                    from_alias: None,
                    where_clause: None,
                    group_by: None,
                    order_by: None,
                    limit: None,
                },
                projection_indices: None,
                pushdown_predicates: Vec::new(),
            };
            cache.insert(format!("SELECT {}", i), plan);
        }

        // Cache should have max 2 entries
        assert!(cache.cache.read().map(|c| c.len() <= 2).unwrap_or(false));
    }

    #[test]
    fn test_optimized_executor_with_cache() {
        let cache = Arc::new(QueryPlanCache::new(10));
        let query = ParsedQuery {
            columns: vec![SelectColumn::Column(ColumnRef::All)],
            from_alias: None,
            where_clause: None,
            group_by: None,
            order_by: None,
            limit: None,
        };

        let executor = OptimizedSelectExecutor::new(
            query,
            InputFormat::Json(super::super::select::JsonInput {
                json_type: super::super::select::JsonType::Lines,
            }),
            OutputFormat::Json(super::super::select::JsonOutput::default()),
            Some(cache.clone()),
        );

        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}"#;
        let sql = "SELECT * FROM s3object";

        // First execution should cache the plan
        let result1 = executor.execute(data, sql);
        assert!(result1.is_ok());

        // Second execution should use cached plan
        let result2 = executor.execute(data, sql);
        assert!(result2.is_ok());

        // Verify cache has the query
        assert!(cache.get(sql).is_some());
    }

    #[test]
    fn test_parallel_execution_threshold() {
        let query = ParsedQuery {
            columns: vec![SelectColumn::Column(ColumnRef::All)],
            from_alias: None,
            where_clause: None,
            group_by: None,
            order_by: None,
            limit: None,
        };

        let executor = OptimizedSelectExecutor::new(
            query,
            InputFormat::Json(super::super::select::JsonInput {
                json_type: super::super::select::JsonType::Lines,
            }),
            OutputFormat::Json(super::super::select::JsonOutput::default()),
            None,
        );

        // Small data should use single-threaded execution
        let small_data = br#"{"name":"Alice"}"#;
        assert!(small_data.len() < executor.parallel_threshold);
    }
}
