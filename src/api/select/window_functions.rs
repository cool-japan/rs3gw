#![cfg(feature = "formats")]
//! Window function execution for S3 Select
//!
//! Implements ROW_NUMBER, RANK, DENSE_RANK, LEAD, LAG, FIRST_VALUE, LAST_VALUE, NTILE

use std::cmp::Ordering;
use std::collections::HashMap;

use super::advanced_sql::{WindowFrame, WindowFunction};
use super::types::{FieldValue, Record, SelectError};

/// Window function executor
pub struct WindowExecutor {
    /// All input records
    records: Vec<Record>,
    /// Window specification
    window_frame: WindowFrame,
}

impl WindowExecutor {
    pub fn new(records: Vec<Record>, window_frame: WindowFrame) -> Self {
        Self {
            records,
            window_frame,
        }
    }

    /// Execute a window function on all records
    pub fn execute(&self, func: &WindowFunction) -> Result<Vec<FieldValue>, SelectError> {
        match func {
            WindowFunction::RowNumber => self.row_number(),
            WindowFunction::Rank => self.rank(),
            WindowFunction::DenseRank => self.dense_rank(),
            WindowFunction::Lead { column, offset } => self.lead(column, *offset),
            WindowFunction::Lag { column, offset } => self.lag(column, *offset),
            WindowFunction::FirstValue { column } => self.first_value(column),
            WindowFunction::LastValue { column } => self.last_value(column),
            WindowFunction::NTile { buckets } => self.ntile(*buckets),
        }
    }

    /// Partition records according to PARTITION BY clause
    fn partition_records(&self) -> Vec<Vec<usize>> {
        if self.window_frame.partition_by.is_empty() {
            // No partitioning - all records in one partition
            return vec![(0..self.records.len()).collect()];
        }

        let mut partitions: HashMap<Vec<String>, Vec<usize>> = HashMap::new();

        for (idx, record) in self.records.iter().enumerate() {
            // Create partition key from PARTITION BY columns
            let mut key = Vec::new();
            for col in &self.window_frame.partition_by {
                let value = record
                    .get_field(col)
                    .map(|v| format!("{:?}", v))
                    .unwrap_or_default();
                key.push(value);
            }

            partitions.entry(key).or_default().push(idx);
        }

        partitions.into_values().collect()
    }

    /// Sort indices within partition according to ORDER BY clause
    fn sort_partition(&self, indices: &[usize]) -> Vec<usize> {
        if self.window_frame.order_by.is_empty() {
            return indices.to_vec();
        }

        let mut sorted = indices.to_vec();
        sorted.sort_by(|&a, &b| self.compare_records(a, b));
        sorted
    }

    /// Compare two records based on ORDER BY specification
    fn compare_records(&self, a: usize, b: usize) -> Ordering {
        let rec_a = &self.records[a];
        let rec_b = &self.records[b];

        for order_spec in &self.window_frame.order_by {
            let val_a = rec_a.get_field(&order_spec.column);
            let val_b = rec_b.get_field(&order_spec.column);

            let cmp = match (val_a, val_b) {
                (Some(ref a), Some(ref b)) => compare_field_values(a, b),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            };

            if cmp != Ordering::Equal {
                return if order_spec.ascending {
                    cmp
                } else {
                    cmp.reverse()
                };
            }
        }

        Ordering::Equal
    }

    /// ROW_NUMBER() - assigns sequential number to each row within partition
    fn row_number(&self) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);
            for (row_num, &idx) in sorted.iter().enumerate() {
                results[idx] = FieldValue::Number((row_num + 1) as f64);
            }
        }

        Ok(results)
    }

    /// RANK() - assigns rank with gaps for ties
    fn rank(&self) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);
            let mut current_rank = 1;
            let mut prev_idx: Option<usize> = None;

            for (row_num, &idx) in sorted.iter().enumerate() {
                if let Some(prev) = prev_idx {
                    // Check if current record is different from previous
                    if self.compare_records(prev, idx) != Ordering::Equal {
                        current_rank = row_num + 1;
                    }
                }

                results[idx] = FieldValue::Number(current_rank as f64);
                prev_idx = Some(idx);
            }
        }

        Ok(results)
    }

    /// DENSE_RANK() - assigns rank without gaps
    fn dense_rank(&self) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);
            let mut current_rank = 1;
            let mut prev_idx: Option<usize> = None;

            for &idx in &sorted {
                if let Some(prev) = prev_idx {
                    // Check if current record is different from previous
                    if self.compare_records(prev, idx) != Ordering::Equal {
                        current_rank += 1;
                    }
                }

                results[idx] = FieldValue::Number(current_rank as f64);
                prev_idx = Some(idx);
            }
        }

        Ok(results)
    }

    /// LEAD(column, offset) - access value from subsequent row
    fn lead(&self, column: &str, offset: usize) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);

            for (pos, &idx) in sorted.iter().enumerate() {
                if pos + offset < sorted.len() {
                    let lead_idx = sorted[pos + offset];
                    if let Some(value) = self.records[lead_idx].get_field(column) {
                        results[idx] = value;
                    }
                }
                // else: remains Null (default)
            }
        }

        Ok(results)
    }

    /// LAG(column, offset) - access value from previous row
    fn lag(&self, column: &str, offset: usize) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);

            for (pos, &idx) in sorted.iter().enumerate() {
                if pos >= offset {
                    let lag_idx = sorted[pos - offset];
                    if let Some(value) = self.records[lag_idx].get_field(column) {
                        results[idx] = value;
                    }
                }
                // else: remains Null (default)
            }
        }

        Ok(results)
    }

    /// FIRST_VALUE(column) - first value in window frame
    fn first_value(&self, column: &str) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);

            if !sorted.is_empty() {
                let first_idx = sorted[0];
                let first_value = self.records[first_idx]
                    .get_field(column)
                    .unwrap_or(FieldValue::Null);

                // All rows in partition get the first value
                for &idx in &sorted {
                    results[idx] = first_value.clone();
                }
            }
        }

        Ok(results)
    }

    /// LAST_VALUE(column) - last value in window frame
    fn last_value(&self, column: &str) -> Result<Vec<FieldValue>, SelectError> {
        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);

            if !sorted.is_empty() {
                let last_idx = sorted[sorted.len() - 1];
                let last_value = self.records[last_idx]
                    .get_field(column)
                    .unwrap_or(FieldValue::Null);

                // All rows in partition get the last value
                for &idx in &sorted {
                    results[idx] = last_value.clone();
                }
            }
        }

        Ok(results)
    }

    /// NTILE(n) - divides rows into n buckets
    fn ntile(&self, buckets: usize) -> Result<Vec<FieldValue>, SelectError> {
        if buckets == 0 {
            return Err(SelectError::InvalidSql(
                "NTILE buckets must be greater than 0".to_string(),
            ));
        }

        let partitions = self.partition_records();
        let mut results = vec![FieldValue::Null; self.records.len()];

        for partition in partitions {
            let sorted = self.sort_partition(&partition);
            let partition_size = sorted.len();

            // Calculate bucket size
            let bucket_size = partition_size / buckets;
            let remainder = partition_size % buckets;

            for (pos, &idx) in sorted.iter().enumerate() {
                // Distribute remainder evenly among first buckets
                let bucket = if pos < remainder * (bucket_size + 1) {
                    pos / (bucket_size + 1) + 1
                } else {
                    (pos - remainder) / bucket_size + 1
                };

                results[idx] = FieldValue::Number(bucket as f64);
            }
        }

        Ok(results)
    }
}

/// Helper function to compare field values
fn compare_field_values(a: &FieldValue, b: &FieldValue) -> Ordering {
    use FieldValue::*;
    match (a, b) {
        (Number(x), Number(y)) => {
            if x < y {
                Ordering::Less
            } else if x > y {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
        (String(x), String(y)) => x.cmp(y),
        (Bool(x), Bool(y)) => x.cmp(y),
        (Null, Null) => Ordering::Equal,
        (Null, _) => Ordering::Less,
        (_, Null) => Ordering::Greater,
        (Number(_), _) => Ordering::Greater,
        (_, Number(_)) => Ordering::Less,
        (String(_), Bool(_)) => Ordering::Greater,
        (Bool(_), String(_)) => Ordering::Less,
    }
}

#[cfg(test)]
mod tests {
    use super::super::advanced_sql::WindowOrderBy;
    use super::*;

    fn create_test_records() -> Vec<Record> {
        vec![
            hashmap_record(&[
                ("id", FieldValue::Number(1.0)),
                ("category", FieldValue::String("A".to_string())),
                ("value", FieldValue::Number(100.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(2.0)),
                ("category", FieldValue::String("A".to_string())),
                ("value", FieldValue::Number(200.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(3.0)),
                ("category", FieldValue::String("B".to_string())),
                ("value", FieldValue::Number(150.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(4.0)),
                ("category", FieldValue::String("B".to_string())),
                ("value", FieldValue::Number(250.0)),
            ]),
        ]
    }

    fn hashmap_record(fields: &[(&str, FieldValue)]) -> Record {
        let mut map = HashMap::new();
        for (k, v) in fields {
            map.insert(k.to_string(), v.clone());
        }
        Record::Map(map)
    }

    fn assert_field_eq(a: &FieldValue, b: &FieldValue) {
        match (a, b) {
            (FieldValue::Number(x), FieldValue::Number(y)) => {
                assert!((x - y).abs() < 0.001, "Expected {:?}, got {:?}", b, a);
            }
            (FieldValue::String(x), FieldValue::String(y)) => assert_eq!(x, y),
            (FieldValue::Bool(x), FieldValue::Bool(y)) => assert_eq!(x, y),
            (FieldValue::Null, FieldValue::Null) => {}
            _ => panic!("FieldValue types don't match: {:?} vs {:?}", a, b),
        }
    }

    #[test]
    fn test_row_number_no_partition() {
        let records = create_test_records();
        let window_frame = WindowFrame {
            partition_by: vec![],
            order_by: vec![WindowOrderBy {
                column: "id".to_string(),
                ascending: true,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .row_number()
            .expect("Failed to execute ROW_NUMBER window function");

        assert_eq!(results.len(), 4);
        assert_field_eq(&results[0], &FieldValue::Number(1.0));
        assert_field_eq(&results[1], &FieldValue::Number(2.0));
        assert_field_eq(&results[2], &FieldValue::Number(3.0));
        assert_field_eq(&results[3], &FieldValue::Number(4.0));
    }

    #[test]
    fn test_row_number_with_partition() {
        let records = create_test_records();
        let window_frame = WindowFrame {
            partition_by: vec!["category".to_string()],
            order_by: vec![WindowOrderBy {
                column: "value".to_string(),
                ascending: true,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .row_number()
            .expect("Failed to execute ROW_NUMBER with partition window function");

        // Category A: id=1 (value=100), id=2 (value=200)
        // Category B: id=3 (value=150), id=4 (value=250)
        assert_field_eq(&results[0], &FieldValue::Number(1.0)); // id=1, first in category A
        assert_field_eq(&results[1], &FieldValue::Number(2.0)); // id=2, second in category A
        assert_field_eq(&results[2], &FieldValue::Number(1.0)); // id=3, first in category B
        assert_field_eq(&results[3], &FieldValue::Number(2.0)); // id=4, second in category B
    }

    #[test]
    fn test_rank_with_ties() {
        let records = vec![
            hashmap_record(&[
                ("id", FieldValue::Number(1.0)),
                ("score", FieldValue::Number(100.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(2.0)),
                ("score", FieldValue::Number(100.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(3.0)),
                ("score", FieldValue::Number(90.0)),
            ]),
        ];

        let window_frame = WindowFrame {
            partition_by: vec![],
            order_by: vec![WindowOrderBy {
                column: "score".to_string(),
                ascending: false, // Descending
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .rank()
            .expect("Failed to execute RANK window function");

        // Both score=100 should have rank 1, score=90 should have rank 3 (gap)
        assert_field_eq(&results[0], &FieldValue::Number(1.0));
        assert_field_eq(&results[1], &FieldValue::Number(1.0));
        assert_field_eq(&results[2], &FieldValue::Number(3.0));
    }

    #[test]
    fn test_dense_rank_with_ties() {
        let records = vec![
            hashmap_record(&[
                ("id", FieldValue::Number(1.0)),
                ("score", FieldValue::Number(100.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(2.0)),
                ("score", FieldValue::Number(100.0)),
            ]),
            hashmap_record(&[
                ("id", FieldValue::Number(3.0)),
                ("score", FieldValue::Number(90.0)),
            ]),
        ];

        let window_frame = WindowFrame {
            partition_by: vec![],
            order_by: vec![WindowOrderBy {
                column: "score".to_string(),
                ascending: false,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .dense_rank()
            .expect("Failed to execute DENSE_RANK window function");

        // Both score=100 should have rank 1, score=90 should have rank 2 (no gap)
        assert_field_eq(&results[0], &FieldValue::Number(1.0));
        assert_field_eq(&results[1], &FieldValue::Number(1.0));
        assert_field_eq(&results[2], &FieldValue::Number(2.0));
    }

    #[test]
    fn test_lead() {
        let records = create_test_records();
        let window_frame = WindowFrame {
            partition_by: vec![],
            order_by: vec![WindowOrderBy {
                column: "id".to_string(),
                ascending: true,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .lead("value", 1)
            .expect("Failed to execute LEAD window function");

        assert_field_eq(&results[0], &FieldValue::Number(200.0)); // Next value
        assert_field_eq(&results[1], &FieldValue::Number(150.0));
        assert_field_eq(&results[2], &FieldValue::Number(250.0));
        assert_field_eq(&results[3], &FieldValue::Null); // No next value
    }

    #[test]
    fn test_lag() {
        let records = create_test_records();
        let window_frame = WindowFrame {
            partition_by: vec![],
            order_by: vec![WindowOrderBy {
                column: "id".to_string(),
                ascending: true,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .lag("value", 1)
            .expect("Failed to execute LAG window function");

        assert_field_eq(&results[0], &FieldValue::Null); // No previous value
        assert_field_eq(&results[1], &FieldValue::Number(100.0)); // Previous value
        assert_field_eq(&results[2], &FieldValue::Number(200.0));
        assert_field_eq(&results[3], &FieldValue::Number(150.0));
    }

    #[test]
    fn test_first_value() {
        let records = create_test_records();
        let window_frame = WindowFrame {
            partition_by: vec!["category".to_string()],
            order_by: vec![WindowOrderBy {
                column: "value".to_string(),
                ascending: true,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .first_value("value")
            .expect("Failed to execute FIRST_VALUE window function");

        // Category A: first value is 100
        assert_field_eq(&results[0], &FieldValue::Number(100.0));
        assert_field_eq(&results[1], &FieldValue::Number(100.0));

        // Category B: first value is 150
        assert_field_eq(&results[2], &FieldValue::Number(150.0));
        assert_field_eq(&results[3], &FieldValue::Number(150.0));
    }

    #[test]
    fn test_ntile() {
        let records = create_test_records();
        let window_frame = WindowFrame {
            partition_by: vec![],
            order_by: vec![WindowOrderBy {
                column: "id".to_string(),
                ascending: true,
            }],
            frame_spec: None,
        };

        let executor = WindowExecutor::new(records, window_frame);
        let results = executor
            .ntile(2)
            .expect("Failed to execute NTILE window function");

        // Split into 2 buckets
        assert_field_eq(&results[0], &FieldValue::Number(1.0));
        assert_field_eq(&results[1], &FieldValue::Number(1.0));
        assert_field_eq(&results[2], &FieldValue::Number(2.0));
        assert_field_eq(&results[3], &FieldValue::Number(2.0));
    }
}
