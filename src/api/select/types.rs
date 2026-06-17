#![cfg(feature = "formats")]
//! S3 Select types and executor
//!
//! Contains all type definitions and the SelectExecutor implementation.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor};

use arrow::array::*;
use arrow::datatypes::DataType;
use bytes::Bytes;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use tracing::debug;

use super::parser::{
    compare_field_values, evaluate_condition, json_to_record, parse_csv_line, parse_sql,
    quote_csv_field,
};

/// ORDER BY clause
#[derive(Debug, Clone)]
pub struct OrderByClause {
    pub column: String,
    pub ascending: bool,
}
/// Aggregation function
#[derive(Debug, Clone, PartialEq)]
pub enum AggregateFunction {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}
/// Output serialization format
#[derive(Debug, Clone, PartialEq)]
pub enum OutputFormat {
    Csv(CsvOutput),
    Json(JsonOutput),
}
/// Byte range for scanning
#[derive(Debug, Clone)]
pub struct ScanRange {
    pub start: Option<u64>,
    pub end: Option<u64>,
}
/// CSV input configuration
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CsvInput {
    pub file_header_info: FileHeaderInfo,
    pub field_delimiter: char,
    pub record_delimiter: char,
    pub quote_character: char,
    pub quote_escape_character: char,
    pub comments: Option<char>,
    pub allow_quoted_record_delimiter: bool,
}
impl CsvInput {
    pub fn new() -> Self {
        Self {
            file_header_info: FileHeaderInfo::None,
            field_delimiter: ',',
            record_delimiter: '\n',
            quote_character: '"',
            quote_escape_character: '"',
            comments: None,
            allow_quoted_record_delimiter: false,
        }
    }
}
/// How to handle CSV file headers
#[derive(Debug, Clone, PartialEq, Default)]
pub enum FileHeaderInfo {
    Use,
    Ignore,
    #[default]
    None,
}
/// WHERE condition
#[derive(Debug, Clone)]
pub enum Condition {
    Comparison {
        left: Operand,
        op: CompareOp,
        right: Operand,
    },
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
    IsNull(Operand),
    IsNotNull(Operand),
    Like {
        value: Operand,
        pattern: String,
    },
}
/// When to quote fields in CSV output
#[derive(Debug, Clone, PartialEq, Default)]
pub enum QuoteFields {
    Always,
    #[default]
    AsNeeded,
}
/// Operand in a condition
#[derive(Debug, Clone)]
pub enum Operand {
    Column(ColumnRef),
    StringLiteral(String),
    NumberLiteral(f64),
    BoolLiteral(bool),
    Null,
}
/// S3 Select error types
#[derive(Debug, Error)]
pub enum SelectError {
    #[error("Invalid SQL expression: {0}")]
    InvalidSql(String),
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Invalid input format: {0}")]
    InvalidFormat(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("Parquet error: {0}")]
    ParquetError(#[from] parquet::errors::ParquetError),
    #[error("Arrow error: {0}")]
    ArrowError(#[from] arrow::error::ArrowError),
}
/// JSON output configuration
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonOutput {
    pub record_delimiter: String,
}
/// S3 Select request
#[derive(Debug, Clone)]
pub struct SelectRequest {
    pub expression: String,
    pub expression_type: ExpressionType,
    pub input_serialization: InputFormat,
    pub output_serialization: OutputFormat,
    pub scan_range: Option<ScanRange>,
}
/// S3 Select executor
pub struct SelectExecutor {
    pub query: ParsedQuery,
    pub input_format: InputFormat,
    pub output_format: OutputFormat,
}
impl SelectExecutor {
    /// Create a new executor
    pub fn new(request: SelectRequest) -> Result<Self, SelectError> {
        let query = parse_sql(&request.expression)?;
        Ok(Self {
            query,
            input_format: request.input_serialization,
            output_format: request.output_serialization,
        })
    }
    /// Create executor from pre-parsed components (for optimizer)
    pub fn new_from_parts(
        query: ParsedQuery,
        input_format: InputFormat,
        output_format: OutputFormat,
    ) -> Self {
        Self {
            query,
            input_format,
            output_format,
        }
    }
    /// Execute the query on input data
    pub fn execute(&self, data: &[u8]) -> Result<Vec<u8>, SelectError> {
        let records = self.parse_input(data)?;
        let filtered = self.filter_records(records)?;
        let has_aggregates = self
            .query
            .columns
            .iter()
            .any(|c| matches!(c, SelectColumn::Aggregate { .. }));
        let result_records = if has_aggregates || self.query.group_by.is_some() {
            self.apply_group_by_and_aggregates(filtered)?
        } else {
            let mut projected = self.project_columns(filtered)?;
            if self.query.order_by.is_some() {
                self.apply_order_by(&mut projected)?;
            }
            projected
        };
        let limited = if let Some(limit) = self.query.limit {
            result_records.into_iter().take(limit).collect()
        } else {
            result_records
        };
        self.serialize_output(limited)
    }
    /// Parse input data into records
    pub(crate) fn parse_input(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        match &self.input_format {
            InputFormat::Csv(config) => self.parse_csv(data, config),
            InputFormat::Json(config) => self.parse_json(data, config),
            InputFormat::Parquet => self.parse_parquet(data),
            InputFormat::Avro => self.parse_avro(data),
            InputFormat::Orc => self.parse_orc(data),
            InputFormat::Protobuf => self.parse_protobuf(data),
            InputFormat::MessagePack => self.parse_messagepack(data),
        }
    }
    /// Parse CSV data
    fn parse_csv(&self, data: &[u8], config: &CsvInput) -> Result<Vec<Record>, SelectError> {
        let reader = BufReader::new(Cursor::new(data));
        let mut records = Vec::new();
        let mut headers: Option<Vec<String>> = None;
        let mut line_num = 0;
        for line_result in reader.lines() {
            let line = line_result?;
            line_num += 1;
            if let Some(comment_char) = config.comments {
                if line.starts_with(comment_char) {
                    continue;
                }
            }
            if line.trim().is_empty() {
                continue;
            }
            let fields = parse_csv_line(&line, config.field_delimiter, config.quote_character);
            if line_num == 1 && config.file_header_info == FileHeaderInfo::Use {
                headers = Some(fields);
                continue;
            }
            if line_num == 1 && config.file_header_info == FileHeaderInfo::Ignore {
                continue;
            }
            let record = if let Some(ref hdrs) = headers {
                let mut map = HashMap::new();
                for (i, field) in fields.into_iter().enumerate() {
                    let key = hdrs
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("_{}", i + 1));
                    map.insert(key, FieldValue::String(field));
                }
                Record::Map(map)
            } else {
                Record::Array(fields.into_iter().map(FieldValue::String).collect())
            };
            records.push(record);
        }
        Ok(records)
    }
    /// Parse JSON data
    fn parse_json(&self, data: &[u8], config: &JsonInput) -> Result<Vec<Record>, SelectError> {
        let mut records = Vec::new();
        match config.json_type {
            JsonType::Document => {
                let value: JsonValue = serde_json::from_slice(data)?;
                match value {
                    JsonValue::Array(arr) => {
                        for item in arr {
                            records.push(json_to_record(item)?);
                        }
                    }
                    JsonValue::Object(_) => {
                        records.push(json_to_record(value)?);
                    }
                    _ => {
                        return Err(SelectError::InvalidFormat(
                            "JSON document must be object or array".to_string(),
                        ));
                    }
                }
            }
            JsonType::Lines => {
                let reader = BufReader::new(Cursor::new(data));
                for line_result in reader.lines() {
                    let line = line_result?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    let value: JsonValue = serde_json::from_str(&line)?;
                    records.push(json_to_record(value)?);
                }
            }
        }
        Ok(records)
    }
    /// Parse Parquet data
    fn parse_parquet(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        debug!("Parsing Parquet data of {} bytes", data.len());
        let bytes = Bytes::copy_from_slice(data);
        let builder = ParquetRecordBatchReaderBuilder::try_new(bytes)?;
        let reader = builder.build()?;
        let mut records = Vec::new();
        for batch_result in reader {
            let batch = batch_result?;
            let schema = batch.schema();
            let num_rows = batch.num_rows();
            for row_idx in 0..num_rows {
                let mut record_map = HashMap::new();
                for (col_idx, field) in schema.fields().iter().enumerate() {
                    let column = batch.column(col_idx);
                    let field_name = field.name().clone();
                    let value = Self::extract_arrow_value(column.as_ref(), row_idx)?;
                    record_map.insert(field_name, value);
                }
                records.push(Record::Map(record_map));
            }
        }
        debug!("Parsed {} Parquet records", records.len());
        Ok(records)
    }
    /// Parse Apache Avro data
    fn parse_avro(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        use apache_avro::Reader;

        debug!("Parsing Avro data of {} bytes", data.len());
        let cursor = Cursor::new(data);
        let reader = Reader::new(cursor).map_err(|e| {
            SelectError::InvalidFormat(format!("Failed to create Avro reader: {}", e))
        })?;

        let mut records = Vec::new();
        for record_result in reader {
            let value = record_result.map_err(|e| {
                SelectError::InvalidFormat(format!("Failed to read Avro record: {}", e))
            })?;
            let record = Self::avro_value_to_record(value)?;
            records.push(record);
        }

        debug!("Parsed {} Avro records", records.len());
        Ok(records)
    }

    /// Convert Avro Value to Record
    fn avro_value_to_record(value: apache_avro::types::Value) -> Result<Record, SelectError> {
        use apache_avro::types::Value as AvroValue;

        match value {
            AvroValue::Record(fields) => {
                let mut record_map = HashMap::new();
                for (field_name, field_value) in fields {
                    let converted_value = Self::avro_value_to_field_value(field_value)?;
                    record_map.insert(field_name, converted_value);
                }
                Ok(Record::Map(record_map))
            }
            _ => Err(SelectError::InvalidFormat(
                "Avro data must be a record type".to_string(),
            )),
        }
    }

    /// Convert Avro Value to FieldValue
    fn avro_value_to_field_value(
        value: apache_avro::types::Value,
    ) -> Result<FieldValue, SelectError> {
        use apache_avro::types::Value as AvroValue;

        match value {
            AvroValue::Null => Ok(FieldValue::Null),
            AvroValue::Boolean(b) => Ok(FieldValue::Bool(b)),
            AvroValue::Int(i) => Ok(FieldValue::Number(i as f64)),
            AvroValue::Long(l) => Ok(FieldValue::Number(l as f64)),
            AvroValue::Float(f) => Ok(FieldValue::Number(f as f64)),
            AvroValue::Double(d) => Ok(FieldValue::Number(d)),
            AvroValue::Bytes(b) => Ok(FieldValue::String(String::from_utf8_lossy(&b).to_string())),
            AvroValue::String(s) => Ok(FieldValue::String(s)),
            AvroValue::Fixed(_, b) => {
                Ok(FieldValue::String(String::from_utf8_lossy(&b).to_string()))
            }
            AvroValue::Enum(_, s) => Ok(FieldValue::String(s)),
            AvroValue::Union(_, boxed_value) => Self::avro_value_to_field_value(*boxed_value),
            AvroValue::Array(arr) => {
                // Convert array to JSON string representation
                let json_values: Result<Vec<JsonValue>, SelectError> = arr
                    .into_iter()
                    .map(|v| {
                        let field_val = Self::avro_value_to_field_value(v)?;
                        Ok::<JsonValue, SelectError>(field_val.to_json())
                    })
                    .collect();
                let json_array = JsonValue::Array(json_values?);
                Ok(FieldValue::String(json_array.to_string()))
            }
            AvroValue::Map(map) => {
                // Convert map to JSON string representation
                let mut json_map = serde_json::Map::new();
                for (k, v) in map {
                    let field_val = Self::avro_value_to_field_value(v)?;
                    json_map.insert(k, field_val.to_json());
                }
                let json_obj = JsonValue::Object(json_map);
                Ok(FieldValue::String(json_obj.to_string()))
            }
            AvroValue::Record(fields) => {
                // Convert nested record to JSON string representation
                let mut json_map = serde_json::Map::new();
                for (k, v) in fields {
                    let field_val = Self::avro_value_to_field_value(v)?;
                    json_map.insert(k, field_val.to_json());
                }
                let json_obj = JsonValue::Object(json_map);
                Ok(FieldValue::String(json_obj.to_string()))
            }
            AvroValue::Date(d) => Ok(FieldValue::Number(d as f64)),
            AvroValue::Decimal(d) => {
                // Convert decimal to string representation to preserve precision
                Ok(FieldValue::String(format!("{:?}", d)))
            }
            AvroValue::TimeMillis(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::TimeMicros(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::TimestampMillis(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::TimestampMicros(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::TimestampNanos(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::LocalTimestampMillis(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::LocalTimestampMicros(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::LocalTimestampNanos(t) => Ok(FieldValue::Number(t as f64)),
            AvroValue::Duration(d) => {
                // Duration is a fixed-size 12-byte type with months, days, and millis
                // For simplicity, we'll just use the string representation
                Ok(FieldValue::String(format!("{:?}", d)))
            }
            AvroValue::Uuid(u) => Ok(FieldValue::String(u.to_string())),
            AvroValue::BigDecimal(bd) => {
                // Convert BigDecimal to string representation to preserve precision
                Ok(FieldValue::String(bd.to_string()))
            }
        }
    }

    /// Parse Apache ORC data
    fn parse_orc(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        use orc_rust::ArrowReaderBuilder;

        debug!("Parsing ORC data of {} bytes", data.len());

        // ORC reader requires Bytes type which implements ChunkReader
        let bytes = Bytes::copy_from_slice(data);

        // Build ORC reader
        let builder = ArrowReaderBuilder::try_new(bytes).map_err(|e| {
            SelectError::InvalidFormat(format!("Failed to create ORC reader: {}", e))
        })?;

        let reader = builder.build();

        let mut records = Vec::new();

        // Read record batches
        for batch_result in reader {
            let batch = batch_result.map_err(|e| {
                SelectError::InvalidFormat(format!("Failed to read ORC batch: {}", e))
            })?;

            let schema = batch.schema();
            let num_rows = batch.num_rows();

            for row_idx in 0..num_rows {
                let mut record_map = HashMap::new();
                for (col_idx, field) in schema.fields().iter().enumerate() {
                    let column = batch.column(col_idx);
                    let field_name = field.name().clone();
                    let value = Self::extract_arrow_value(column.as_ref(), row_idx)?;
                    record_map.insert(field_name, value);
                }
                records.push(Record::Map(record_map));
            }
        }

        debug!("Parsed {} ORC records", records.len());
        Ok(records)
    }

    /// Parse Protocol Buffers data
    /// Note: This implementation supports protobuf messages in JSON format
    /// For binary protobuf, schema information would be required
    fn parse_protobuf(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        debug!("Parsing Protobuf data of {} bytes", data.len());

        // For S3 Select, we support protobuf-JSON format
        // This is a common format for storing protobuf messages in a queryable way
        // The format is newline-delimited JSON where each line is a protobuf message

        let data_str = std::str::from_utf8(data).map_err(|e| {
            SelectError::InvalidFormat(format!("Invalid UTF-8 in protobuf data: {}", e))
        })?;

        let mut records = Vec::new();

        for line in data_str.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Parse as JSON (protobuf JSON format)
            let json_value: JsonValue = serde_json::from_str(line).map_err(|e| {
                SelectError::InvalidFormat(format!("Failed to parse protobuf JSON: {}", e))
            })?;

            // Convert JSON to Record
            let record = json_to_record(json_value)?;
            records.push(record);
        }

        debug!("Parsed {} Protobuf records", records.len());
        Ok(records)
    }

    /// Parse MessagePack data
    fn parse_messagepack(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        use rmp_serde::Deserializer;

        debug!("Parsing MessagePack data of {} bytes", data.len());

        let mut records = Vec::new();
        let mut cursor = Cursor::new(data);

        // MessagePack data can be a single value or multiple values
        // Try to deserialize as an array first, then fall back to multiple values
        loop {
            if cursor.position() >= data.len() as u64 {
                break;
            }

            // Try to deserialize a single JSON value from MessagePack
            let mut de = Deserializer::new(&mut cursor);
            let value: JsonValue = serde::Deserialize::deserialize(&mut de).map_err(|e| {
                SelectError::InvalidFormat(format!("Failed to parse MessagePack: {}", e))
            })?;

            // Convert JSON to Record
            let record = json_to_record(value)?;
            records.push(record);
        }

        debug!("Parsed {} MessagePack records", records.len());
        Ok(records)
    }

    /// Extract a value from an Arrow array at a specific index
    fn extract_arrow_value(array: &dyn Array, idx: usize) -> Result<FieldValue, SelectError> {
        if array.is_null(idx) {
            return Ok(FieldValue::Null);
        }
        match array.data_type() {
            DataType::Null => Ok(FieldValue::Null),
            DataType::Boolean => {
                let arr = array
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Boolean type mismatch".to_string())
                    })?;
                Ok(FieldValue::Bool(arr.value(idx)))
            }
            DataType::Int8 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Int8Array>()
                    .ok_or_else(|| SelectError::InvalidFormat("Int8 type mismatch".to_string()))?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::Int16 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Int16Array>()
                    .ok_or_else(|| SelectError::InvalidFormat("Int16 type mismatch".to_string()))?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::Int32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Int32Array>()
                    .ok_or_else(|| SelectError::InvalidFormat("Int32 type mismatch".to_string()))?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::Int64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .ok_or_else(|| SelectError::InvalidFormat("Int64 type mismatch".to_string()))?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::UInt8 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt8Array>()
                    .ok_or_else(|| SelectError::InvalidFormat("UInt8 type mismatch".to_string()))?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::UInt16 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt16Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("UInt16 type mismatch".to_string())
                    })?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::UInt32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt32Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("UInt32 type mismatch".to_string())
                    })?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::UInt64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("UInt64 type mismatch".to_string())
                    })?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::Float32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Float32 type mismatch".to_string())
                    })?;
                Ok(FieldValue::Number(arr.value(idx) as f64))
            }
            DataType::Float64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Float64 type mismatch".to_string())
                    })?;
                Ok(FieldValue::Number(arr.value(idx)))
            }
            DataType::Utf8 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| SelectError::InvalidFormat("Utf8 type mismatch".to_string()))?;
                Ok(FieldValue::String(arr.value(idx).to_string()))
            }
            DataType::LargeUtf8 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<LargeStringArray>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("LargeUtf8 type mismatch".to_string())
                    })?;
                Ok(FieldValue::String(arr.value(idx).to_string()))
            }
            DataType::Binary => {
                let arr = array
                    .as_any()
                    .downcast_ref::<BinaryArray>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Binary type mismatch".to_string())
                    })?;
                let bytes = arr.value(idx);
                Ok(FieldValue::String(hex::encode(bytes)))
            }
            DataType::LargeBinary => {
                let arr = array
                    .as_any()
                    .downcast_ref::<LargeBinaryArray>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("LargeBinary type mismatch".to_string())
                    })?;
                let bytes = arr.value(idx);
                Ok(FieldValue::String(hex::encode(bytes)))
            }
            DataType::Date32 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Date32Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Date32 type mismatch".to_string())
                    })?;
                let days = arr.value(idx);
                Ok(FieldValue::String(format!("date:{}", days)))
            }
            DataType::Date64 => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Date64Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Date64 type mismatch".to_string())
                    })?;
                let millis = arr.value(idx);
                Ok(FieldValue::String(format!("date:{}", millis)))
            }
            DataType::Timestamp(unit, _) => {
                let timestamp = match unit {
                    arrow::datatypes::TimeUnit::Second => {
                        let arr = array
                            .as_any()
                            .downcast_ref::<TimestampSecondArray>()
                            .ok_or_else(|| {
                                SelectError::InvalidFormat(
                                    "TimestampSecond type mismatch".to_string(),
                                )
                            })?;
                        arr.value(idx)
                    }
                    arrow::datatypes::TimeUnit::Millisecond => {
                        let arr = array
                            .as_any()
                            .downcast_ref::<TimestampMillisecondArray>()
                            .ok_or_else(|| {
                                SelectError::InvalidFormat(
                                    "TimestampMillisecond type mismatch".to_string(),
                                )
                            })?;
                        arr.value(idx)
                    }
                    arrow::datatypes::TimeUnit::Microsecond => {
                        let arr = array
                            .as_any()
                            .downcast_ref::<TimestampMicrosecondArray>()
                            .ok_or_else(|| {
                                SelectError::InvalidFormat(
                                    "TimestampMicrosecond type mismatch".to_string(),
                                )
                            })?;
                        arr.value(idx)
                    }
                    arrow::datatypes::TimeUnit::Nanosecond => {
                        let arr = array
                            .as_any()
                            .downcast_ref::<TimestampNanosecondArray>()
                            .ok_or_else(|| {
                                SelectError::InvalidFormat(
                                    "TimestampNanosecond type mismatch".to_string(),
                                )
                            })?;
                        arr.value(idx)
                    }
                };
                Ok(FieldValue::String(format!("timestamp:{}", timestamp)))
            }
            DataType::Decimal128(_, scale) => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Decimal128Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Decimal128 type mismatch".to_string())
                    })?;
                let value = arr.value(idx);
                let scale_factor = 10_i128.pow(*scale as u32);
                let float_value = value as f64 / scale_factor as f64;
                Ok(FieldValue::Number(float_value))
            }
            DataType::Decimal256(_, _scale) => {
                let arr = array
                    .as_any()
                    .downcast_ref::<Decimal256Array>()
                    .ok_or_else(|| {
                        SelectError::InvalidFormat("Decimal256 type mismatch".to_string())
                    })?;
                let value = arr.value(idx);
                Ok(FieldValue::String(format!("decimal256:{}", value)))
            }
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
                Ok(FieldValue::String(format!("list[{}]", idx)))
            }
            DataType::Struct(_) => Ok(FieldValue::String(format!("struct[{}]", idx))),
            DataType::Map(_, _) => Ok(FieldValue::String(format!("map[{}]", idx))),
            _ => Ok(FieldValue::String(format!(
                "unsupported:{:?}",
                array.data_type()
            ))),
        }
    }
    /// Filter records based on WHERE clause
    fn filter_records(&self, records: Vec<Record>) -> Result<Vec<Record>, SelectError> {
        let filtered = if let Some(condition) = &self.query.where_clause {
            let mut result = Vec::new();
            for record in records {
                if evaluate_condition(condition, &record)? {
                    result.push(record);
                }
            }
            result
        } else {
            records
        };
        Ok(filtered)
    }
    /// Project selected columns
    fn project_columns(&self, records: Vec<Record>) -> Result<Vec<Record>, SelectError> {
        if self
            .query
            .columns
            .iter()
            .any(|c| matches!(c, SelectColumn::Column(ColumnRef::All)))
        {
            return Ok(records);
        }
        let mut projected = Vec::new();
        for record in records {
            let mut new_record = HashMap::new();
            for col in &self.query.columns {
                match col {
                    SelectColumn::Column(ColumnRef::All) => unreachable!(),
                    SelectColumn::Column(ColumnRef::Named(name)) => {
                        if let Some(value) = record.get_field(name) {
                            new_record.insert(name.clone(), value);
                        }
                    }
                    SelectColumn::Column(ColumnRef::Indexed(idx)) => {
                        let key = format!("_{}", idx + 1);
                        if let Some(value) =
                            record.get_field(&key).or_else(|| record.get_by_index(*idx))
                        {
                            new_record.insert(key, value);
                        }
                    }
                    SelectColumn::Aggregate { .. } => {}
                }
            }
            projected.push(Record::Map(new_record));
        }
        Ok(projected)
    }
    /// Apply GROUP BY and aggregate functions
    fn apply_group_by_and_aggregates(
        &self,
        records: Vec<Record>,
    ) -> Result<Vec<Record>, SelectError> {
        if let Some(ref group_by_cols) = self.query.group_by {
            self.group_and_aggregate(records, group_by_cols)
        } else {
            self.compute_global_aggregates(records)
        }
    }
    /// Compute aggregates without GROUP BY (returns single row)
    fn compute_global_aggregates(&self, records: Vec<Record>) -> Result<Vec<Record>, SelectError> {
        let mut result_row = HashMap::new();
        for col in &self.query.columns {
            match col {
                SelectColumn::Aggregate {
                    function,
                    column,
                    alias,
                } => {
                    let agg_value =
                        self.compute_aggregate(function, column.as_deref(), &records)?;
                    let col_name = if let Some(a) = alias {
                        a.clone()
                    } else if let Some(c) = column {
                        format!("{:?}({})", function, c)
                    } else {
                        format!("{:?}", function)
                    };
                    result_row.insert(col_name, agg_value);
                }
                SelectColumn::Column(col_ref) => {
                    if let Some(first_record) = records.first() {
                        match col_ref {
                            ColumnRef::Named(name) => {
                                if let Some(value) = first_record.get_field(name) {
                                    result_row.insert(name.clone(), value);
                                }
                            }
                            ColumnRef::Indexed(idx) => {
                                let key = format!("_{}", idx + 1);
                                if let Some(value) = first_record.get_by_index(*idx) {
                                    result_row.insert(key, value);
                                }
                            }
                            ColumnRef::All => {}
                        }
                    }
                }
            }
        }
        Ok(vec![Record::Map(result_row)])
    }
    /// Group records and compute aggregates for each group
    fn group_and_aggregate(
        &self,
        records: Vec<Record>,
        group_by_cols: &[String],
    ) -> Result<Vec<Record>, SelectError> {
        let mut groups: HashMap<Vec<String>, Vec<Record>> = HashMap::new();
        for record in records {
            let mut key = Vec::new();
            for col_name in group_by_cols {
                let value = record
                    .get_field(col_name)
                    .map(|v| v.as_string())
                    .unwrap_or_default();
                key.push(value);
            }
            groups.entry(key).or_default().push(record);
        }
        let mut result = Vec::new();
        for (group_key, group_records) in groups {
            let mut result_row = HashMap::new();
            for (i, col_name) in group_by_cols.iter().enumerate() {
                if let Some(key_value) = group_key.get(i) {
                    result_row.insert(col_name.clone(), FieldValue::String(key_value.clone()));
                }
            }
            for col in &self.query.columns {
                if let SelectColumn::Aggregate {
                    function,
                    column,
                    alias,
                } = col
                {
                    let agg_value =
                        self.compute_aggregate(function, column.as_deref(), &group_records)?;
                    let col_name = if let Some(a) = alias {
                        a.clone()
                    } else if let Some(c) = column {
                        format!("{:?}({})", function, c)
                    } else {
                        format!("{:?}", function)
                    };
                    result_row.insert(col_name, agg_value);
                }
            }
            result.push(Record::Map(result_row));
        }
        if self.query.order_by.is_some() {
            self.apply_order_by(&mut result)?;
        }
        Ok(result)
    }
    /// Compute a single aggregate function
    fn compute_aggregate(
        &self,
        function: &AggregateFunction,
        column: Option<&str>,
        records: &[Record],
    ) -> Result<FieldValue, SelectError> {
        match function {
            AggregateFunction::Count => {
                let count = if column.is_some() {
                    records
                        .iter()
                        .filter(|r| {
                            column
                                .and_then(|c| r.get_field(c))
                                .map(|v| !v.is_null())
                                .unwrap_or(false)
                        })
                        .count()
                } else {
                    records.len()
                };
                Ok(FieldValue::Number(count as f64))
            }
            AggregateFunction::Sum => {
                let col = column.ok_or_else(|| {
                    SelectError::InvalidSql("SUM requires a column name".to_string())
                })?;
                let sum: f64 = records
                    .iter()
                    .filter_map(|r| r.get_field(col).and_then(|v| v.as_f64()))
                    .sum();
                Ok(FieldValue::Number(sum))
            }
            AggregateFunction::Avg => {
                let col = column.ok_or_else(|| {
                    SelectError::InvalidSql("AVG requires a column name".to_string())
                })?;
                let values: Vec<f64> = records
                    .iter()
                    .filter_map(|r| r.get_field(col).and_then(|v| v.as_f64()))
                    .collect();
                let avg = if values.is_empty() {
                    0.0
                } else {
                    values.iter().sum::<f64>() / values.len() as f64
                };
                Ok(FieldValue::Number(avg))
            }
            AggregateFunction::Min => {
                let col = column.ok_or_else(|| {
                    SelectError::InvalidSql("MIN requires a column name".to_string())
                })?;
                let min = records
                    .iter()
                    .filter_map(|r| r.get_field(col).and_then(|v| v.as_f64()))
                    .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                Ok(min.map(FieldValue::Number).unwrap_or(FieldValue::Null))
            }
            AggregateFunction::Max => {
                let col = column.ok_or_else(|| {
                    SelectError::InvalidSql("MAX requires a column name".to_string())
                })?;
                let max = records
                    .iter()
                    .filter_map(|r| r.get_field(col).and_then(|v| v.as_f64()))
                    .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                Ok(max.map(FieldValue::Number).unwrap_or(FieldValue::Null))
            }
        }
    }
    /// Apply ORDER BY clause to records
    fn apply_order_by(&self, records: &mut [Record]) -> Result<(), SelectError> {
        if let Some(ref order_by) = self.query.order_by {
            records.sort_by(|a, b| {
                for clause in order_by {
                    let a_val = a.get_field(&clause.column);
                    let b_val = b.get_field(&clause.column);
                    let ordering = match (a_val, b_val) {
                        (Some(av), Some(bv)) => compare_field_values(&av, &bv),
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (None, None) => std::cmp::Ordering::Equal,
                    };
                    let final_ordering = if clause.ascending {
                        ordering
                    } else {
                        ordering.reverse()
                    };
                    if final_ordering != std::cmp::Ordering::Equal {
                        return final_ordering;
                    }
                }
                std::cmp::Ordering::Equal
            });
        }
        Ok(())
    }
    /// Serialize output
    fn serialize_output(&self, records: Vec<Record>) -> Result<Vec<u8>, SelectError> {
        match &self.output_format {
            OutputFormat::Csv(config) => self.serialize_csv(records, config),
            OutputFormat::Json(config) => self.serialize_json(records, config),
        }
    }
    /// Serialize to CSV
    fn serialize_csv(
        &self,
        records: Vec<Record>,
        config: &CsvOutput,
    ) -> Result<Vec<u8>, SelectError> {
        let mut output = String::new();
        for record in records {
            let fields = record.to_fields();
            let line: Vec<String> = fields
                .into_iter()
                .map(|f| quote_csv_field(&f.to_string(), config))
                .collect();
            output.push_str(&line.join(&config.field_delimiter.to_string()));
            output.push_str(&config.record_delimiter);
        }
        Ok(output.into_bytes())
    }
    /// Serialize to JSON
    fn serialize_json(
        &self,
        records: Vec<Record>,
        config: &JsonOutput,
    ) -> Result<Vec<u8>, SelectError> {
        let mut output = String::new();
        let delimiter = if config.record_delimiter.is_empty() {
            "\n"
        } else {
            &config.record_delimiter
        };
        for record in records {
            let json = record.to_json();
            output.push_str(&serde_json::to_string(&json)?);
            output.push_str(delimiter);
        }
        Ok(output.into_bytes())
    }
    pub fn parse_csv_public(
        &self,
        data: &[u8],
        config: &CsvInput,
    ) -> Result<Vec<Record>, SelectError> {
        self.parse_csv(data, config)
    }
    pub fn parse_json_public(
        &self,
        data: &[u8],
        config: &JsonInput,
    ) -> Result<Vec<Record>, SelectError> {
        self.parse_json(data, config)
    }
    /// Public wrapper for parse_avro (for optimizer)
    pub fn parse_avro_public(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        self.parse_avro(data)
    }
    /// Public wrapper for parse_orc (for optimizer)
    pub fn parse_orc_public(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        self.parse_orc(data)
    }
    /// Public wrapper for parse_protobuf (for optimizer)
    pub fn parse_protobuf_public(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        self.parse_protobuf(data)
    }
    /// Public wrapper for parse_messagepack (for optimizer)
    pub fn parse_messagepack_public(&self, data: &[u8]) -> Result<Vec<Record>, SelectError> {
        self.parse_messagepack(data)
    }
    pub fn filter_records_public(&self, records: Vec<Record>) -> Result<Vec<Record>, SelectError> {
        self.filter_records(records)
    }
    pub fn project_columns_public(&self, records: Vec<Record>) -> Result<Vec<Record>, SelectError> {
        self.project_columns(records)
    }
    pub fn serialize_output_static(
        records: Vec<Record>,
        output_format: &OutputFormat,
    ) -> Result<Vec<u8>, SelectError> {
        match output_format {
            OutputFormat::Csv(config) => Self::serialize_csv_static(records, config),
            OutputFormat::Json(config) => Self::serialize_json_static(records, config),
        }
    }
    fn serialize_csv_static(
        records: Vec<Record>,
        config: &CsvOutput,
    ) -> Result<Vec<u8>, SelectError> {
        let mut output = String::new();
        for record in records {
            let fields = record.to_fields();
            let line: Vec<String> = fields
                .into_iter()
                .map(|f| quote_csv_field(&f.to_string(), config))
                .collect();
            output.push_str(&line.join(&config.field_delimiter.to_string()));
            output.push_str(&config.record_delimiter);
        }
        Ok(output.into_bytes())
    }
    fn serialize_json_static(
        records: Vec<Record>,
        config: &JsonOutput,
    ) -> Result<Vec<u8>, SelectError> {
        let mut output = String::new();
        let delimiter = if config.record_delimiter.is_empty() {
            "\n"
        } else {
            &config.record_delimiter
        };
        for record in records {
            let json = record.to_json();
            output.push_str(&serde_json::to_string(&json)?);
            output.push_str(delimiter);
        }
        Ok(output.into_bytes())
    }
    pub fn extract_arrow_value_public(
        array: &dyn arrow::array::Array,
        idx: usize,
    ) -> Result<FieldValue, SelectError> {
        Self::extract_arrow_value(array, idx)
    }
}
/// Field value
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}
impl FieldValue {
    pub fn to_json(&self) -> JsonValue {
        match self {
            FieldValue::Null => JsonValue::Null,
            FieldValue::Bool(b) => JsonValue::Bool(*b),
            FieldValue::Number(n) => serde_json::Number::from_f64(*n)
                .map(JsonValue::Number)
                .unwrap_or(JsonValue::Null),
            FieldValue::String(s) => JsonValue::String(s.clone()),
        }
    }
    pub fn as_string(&self) -> String {
        match self {
            FieldValue::Null => String::new(),
            FieldValue::Bool(b) => b.to_string(),
            FieldValue::Number(n) => n.to_string(),
            FieldValue::String(s) => s.clone(),
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            FieldValue::Number(n) => Some(*n),
            FieldValue::String(s) => s.parse().ok(),
            _ => None,
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, FieldValue::Null)
    }
}
/// JSON input configuration
#[derive(Debug, Clone, PartialEq, Default)]
pub struct JsonInput {
    pub json_type: JsonType,
}
/// JSON document type
#[derive(Debug, Clone, PartialEq, Default)]
pub enum JsonType {
    #[default]
    Document,
    Lines,
}
/// Input serialization format
#[derive(Debug, Clone, PartialEq)]
pub enum InputFormat {
    Csv(CsvInput),
    Json(JsonInput),
    Parquet,
    Avro,
    Orc,
    Protobuf,
    MessagePack,
}
/// CSV output configuration
#[derive(Debug, Clone, PartialEq)]
pub struct CsvOutput {
    pub field_delimiter: char,
    pub record_delimiter: String,
    pub quote_character: char,
    pub quote_escape_character: char,
    pub quote_fields: QuoteFields,
}
/// Parsed SQL query
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    /// Selected columns (* for all, or specific column names, or aggregates)
    pub columns: Vec<SelectColumn>,
    /// FROM clause (should be "s3object" or alias)
    pub from_alias: Option<String>,
    /// WHERE conditions
    pub where_clause: Option<Condition>,
    /// GROUP BY columns
    pub group_by: Option<Vec<String>>,
    /// ORDER BY clauses
    pub order_by: Option<Vec<OrderByClause>>,
    /// LIMIT clause
    pub limit: Option<usize>,
}
/// Expression type (only SQL supported)
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ExpressionType {
    #[default]
    Sql,
}
/// Column reference
#[derive(Debug, Clone)]
pub enum ColumnRef {
    All,
    Named(String),
    Indexed(usize),
}
/// A record (row) of data
#[derive(Debug, Clone)]
pub enum Record {
    Map(HashMap<String, FieldValue>),
    Array(Vec<FieldValue>),
}
impl Record {
    pub fn get_field(&self, name: &str) -> Option<FieldValue> {
        match self {
            Record::Map(map) => map.get(name).cloned(),
            Record::Array(arr) => {
                if let Some(suffix) = name.strip_prefix('_') {
                    if let Ok(idx) = suffix.parse::<usize>() {
                        if idx > 0 && idx <= arr.len() {
                            return Some(arr[idx - 1].clone());
                        }
                    }
                }
                None
            }
        }
    }
    pub fn get_by_index(&self, idx: usize) -> Option<FieldValue> {
        match self {
            Record::Map(_) => None,
            Record::Array(arr) => arr.get(idx).cloned(),
        }
    }
    pub fn to_fields(&self) -> Vec<FieldValue> {
        match self {
            Record::Map(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                keys.into_iter()
                    .filter_map(|k| map.get(k).cloned())
                    .collect()
            }
            Record::Array(arr) => arr.clone(),
        }
    }
    pub fn to_json(&self) -> JsonValue {
        match self {
            Record::Map(map) => {
                let obj: serde_json::Map<String, JsonValue> =
                    map.iter().map(|(k, v)| (k.clone(), v.to_json())).collect();
                JsonValue::Object(obj)
            }
            Record::Array(arr) => JsonValue::Array(arr.iter().map(|v| v.to_json()).collect()),
        }
    }
}
/// Select column (can be a simple column or an aggregation)
#[derive(Debug, Clone)]
pub enum SelectColumn {
    /// Simple column reference
    Column(ColumnRef),
    /// Aggregation function
    Aggregate {
        function: AggregateFunction,
        column: Option<String>,
        alias: Option<String>,
    },
}
/// Comparison operator
#[derive(Debug, Clone, PartialEq)]
pub enum CompareOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
