//! Tests for S3 Select functionality

use super::parser::parse_sql;
use super::types::{
    ColumnRef, CsvInput, CsvOutput, FileHeaderInfo, InputFormat, JsonInput, JsonOutput, JsonType,
    OutputFormat, ParsedQuery, Record, SelectColumn, SelectExecutor,
};

#[cfg(test)]
mod select_tests {
    use super::*;
    #[test]
    fn test_parse_sql_basic() {
        let query = parse_sql("SELECT * FROM s3object").expect("Failed to parse SQL");
        assert!(matches!(
            query.columns[0],
            SelectColumn::Column(ColumnRef::All)
        ));
        assert!(query.from_alias.is_none());
        assert!(query.where_clause.is_none());
    }
    #[test]
    fn test_parse_sql_with_columns() {
        let query = parse_sql("SELECT name, age FROM s3object").expect("Failed to parse SQL");
        assert_eq!(query.columns.len(), 2);
        assert!(
            matches!(& query.columns[0], SelectColumn::Column(ColumnRef::Named(n)) if n
            == "name")
        );
        assert!(
            matches!(& query.columns[1], SelectColumn::Column(ColumnRef::Named(n)) if n
            == "age")
        );
    }
    #[test]
    fn test_parse_sql_with_where() {
        let query =
            parse_sql("SELECT * FROM s3object WHERE age > 21").expect("Failed to parse SQL");
        assert!(query.where_clause.is_some());
    }
    #[test]
    fn test_parse_sql_with_limit() {
        let query = parse_sql("SELECT * FROM s3object LIMIT 10").expect("Failed to parse SQL");
        assert_eq!(query.limit, Some(10));
    }
    #[test]
    fn test_csv_parsing() {
        let config = CsvInput::new();
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Csv(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = b"a,b,c\n1,2,3\n4,5,6";
        let records = executor
            .parse_input(data)
            .expect("Failed to parse CSV input");
        assert_eq!(records.len(), 3);
    }
    #[test]
    fn test_csv_with_header() {
        let config = CsvInput {
            file_header_info: FileHeaderInfo::Use,
            ..CsvInput::new()
        };
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Csv(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = b"name,age\nAlice,30\nBob,25";
        let records = executor
            .parse_input(data)
            .expect("Failed to parse CSV with header");
        assert_eq!(records.len(), 2);
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("age"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_json_parsing() {
        let config = JsonInput {
            json_type: JsonType::Document,
        };
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"[{"name":"Alice","age":30},{"name":"Bob","age":25}]"#;
        let records = executor
            .parse_input(data)
            .expect("Failed to parse JSON input");
        assert_eq!(records.len(), 2);
    }
    #[test]
    fn test_json_lines_parsing() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}"#;
        let records = executor
            .parse_input(data)
            .expect("Failed to parse JSON lines input");
        assert_eq!(records.len(), 2);
    }
    #[test]
    fn test_where_filter() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query =
            parse_sql("SELECT * FROM s3object WHERE age > 25").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Alice"));
        assert!(!output.contains("Bob"));
    }
    #[test]
    fn test_column_projection() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT name FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("name"));
        assert!(!output.contains("age"));
    }
    #[test]
    fn test_like_operator() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query =
            parse_sql("SELECT * FROM s3object WHERE name LIKE 'A%'").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice"}
{"name":"Bob"}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Alice"));
        assert!(!output.contains("Bob"));
    }
    #[test]
    fn test_csv_output() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT name, age FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Csv(CsvOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Alice") || output.contains("30"));
    }
    #[test]
    fn test_limit() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT * FROM s3object LIMIT 1").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"n":1}
{"n":2}
{"n":3}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        let lines: Vec<_> = output.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 1);
    }
    #[test]
    fn test_parquet_basic() {
        use arrow::array::{Int32Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use parquet::arrow::arrow_writer::ArrowWriter;
        use std::sync::Arc;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, false),
        ]));
        let id_array = Int32Array::from(vec![1, 2, 3]);
        let name_array = StringArray::from(vec!["Alice", "Bob", "Charlie"]);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(id_array), Arc::new(name_array)],
        )
        .expect("Failed to create record batch");
        let mut buffer = Vec::new();
        {
            let mut writer = ArrowWriter::try_new(&mut buffer, schema.clone(), None)
                .expect("Failed to create Arrow writer");
            writer.write(&batch).expect("Failed to write batch");
            writer.close().expect("Failed to close writer");
        }
        let query = parse_sql("SELECT * FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Parquet,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let result = executor.execute(&buffer).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
        assert!(output.contains("Charlie"));
    }
    #[test]
    fn test_parquet_with_where() {
        use arrow::array::{Int32Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use parquet::arrow::arrow_writer::ArrowWriter;
        use std::sync::Arc;
        let schema = Arc::new(Schema::new(vec![
            Field::new("age", DataType::Int32, false),
            Field::new("name", DataType::Utf8, false),
        ]));
        let age_array = Int32Array::from(vec![25, 30, 35]);
        let name_array = StringArray::from(vec!["Alice", "Bob", "Charlie"]);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(age_array), Arc::new(name_array)],
        )
        .expect("Failed to create record batch");
        let mut buffer = Vec::new();
        {
            let mut writer = ArrowWriter::try_new(&mut buffer, schema.clone(), None)
                .expect("Failed to create Arrow writer");
            writer.write(&batch).expect("Failed to write batch");
            writer.close().expect("Failed to close writer");
        }
        let query =
            parse_sql("SELECT name FROM s3object WHERE age > 28").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Parquet,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let result = executor.execute(&buffer).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(!output.contains("Alice"));
        assert!(output.contains("Bob"));
        assert!(output.contains("Charlie"));
    }
    #[test]
    fn test_parquet_column_projection() {
        use arrow::array::{Float64Array, Int32Array, StringArray};
        use arrow::datatypes::{DataType, Field, Schema};
        use arrow::record_batch::RecordBatch;
        use parquet::arrow::arrow_writer::ArrowWriter;
        use std::sync::Arc;
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("score", DataType::Float64, false),
        ]));
        let id_array = Int32Array::from(vec![1, 2]);
        let name_array = StringArray::from(vec!["Alice", "Bob"]);
        let score_array = Float64Array::from(vec![95.5, 87.3]);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(id_array),
                Arc::new(name_array),
                Arc::new(score_array),
            ],
        )
        .expect("Failed to create record batch");
        let mut buffer = Vec::new();
        {
            let mut writer = ArrowWriter::try_new(&mut buffer, schema.clone(), None)
                .expect("Failed to create Arrow writer");
            writer.write(&batch).expect("Failed to write batch");
            writer.close().expect("Failed to close writer");
        }
        let query = parse_sql("SELECT name FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Parquet,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let result = executor.execute(&buffer).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("name"));
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
        assert!(!output.contains("95.5"));
        assert!(!output.contains("87.3"));
    }
    #[test]
    fn test_aggregate_count_all() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT COUNT(*) FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("\"Count\":3") || output.contains("\"Count\": 3"));
    }
    #[test]
    fn test_aggregate_sum() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT SUM(age) FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("90"));
    }
    #[test]
    fn test_aggregate_avg() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT AVG(age) FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("30"));
    }
    #[test]
    fn test_aggregate_min_max() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query =
            parse_sql("SELECT MIN(age), MAX(age) FROM s3object").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(
            output.contains("25") || output.contains("25.0"),
            "Expected MIN value 25, got: {}",
            output
        );
        assert!(
            output.contains("35") || output.contains("35.0"),
            "Expected MAX value 35, got: {}",
            output
        );
    }
    #[test]
    fn test_group_by_simple() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT department, COUNT(*) FROM s3object GROUP BY department")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","department":"Engineering"}
{"name":"Bob","department":"Sales"}
{"name":"Charlie","department":"Engineering"}
{"name":"David","department":"Sales"}
{"name":"Eve","department":"Engineering"}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Engineering"));
        assert!(output.contains("Sales"));
        let lines: Vec<_> = output.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(lines.len(), 2);
    }
    #[test]
    fn test_group_by_with_aggregates() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT department, AVG(salary) FROM s3object GROUP BY department")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","department":"Engineering","salary":100000}
{"name":"Bob","department":"Sales","salary":80000}
{"name":"Charlie","department":"Engineering","salary":120000}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Engineering"));
        assert!(output.contains("Sales"));
        assert!(output.contains("110000"));
        assert!(output.contains("80000"));
    }
    #[test]
    fn test_order_by_ascending() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT name, age FROM s3object ORDER BY age ASC")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        let output_lower = output.to_lowercase();
        let bob_pos = output_lower.find("bob").expect("Bob not found");
        let charlie_pos = output_lower.find("charlie").expect("Charlie not found");
        assert!(bob_pos < charlie_pos, "Bob should appear before Charlie");
    }
    #[test]
    fn test_order_by_descending() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT name, age FROM s3object ORDER BY age DESC")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        let output_lower = output.to_lowercase();
        let bob_pos = output_lower.find("bob").expect("Bob not found");
        let charlie_pos = output_lower.find("charlie").expect("Charlie not found");
        assert!(charlie_pos < bob_pos, "Charlie should appear before Bob");
    }
    #[test]
    fn test_group_by_with_order_by() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql(
            "SELECT department, COUNT(*) FROM s3object GROUP BY department ORDER BY department ASC",
        )
        .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","department":"Sales"}
{"name":"Bob","department":"Engineering"}
{"name":"Charlie","department":"Sales"}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        let output_lower = output.to_lowercase();
        let eng_pos = output_lower
            .find("engineering")
            .expect("Engineering not found");
        let sales_pos = output_lower.find("sales").expect("Sales not found");
        assert!(
            eng_pos < sales_pos,
            "Engineering should appear before Sales"
        );
    }
    #[test]
    fn test_aggregate_with_where() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query =
            parse_sql("SELECT AVG(age) FROM s3object WHERE age > 25").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Alice","age":30}
{"name":"Bob","age":25}
{"name":"Charlie","age":35}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("32.5"));
    }
    #[test]
    fn test_limit_with_order_by() {
        let config = JsonInput {
            json_type: JsonType::Lines,
        };
        let query = parse_sql("SELECT name FROM s3object ORDER BY name ASC LIMIT 2")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Json(config),
            output_format: OutputFormat::Json(JsonOutput::default()),
        };
        let data = br#"{"name":"Charlie"}
{"name":"Alice"}
{"name":"Bob"}
{"name":"David"}"#;
        let result = executor.execute(data).expect("Failed to execute query");
        let output = String::from_utf8(result).expect("Failed to convert to UTF8");
        assert!(output.contains("Alice"), "Expected Alice in output");
        assert!(output.contains("Bob"), "Expected Bob in output");
        assert!(!output.contains("David"), "David should not be in output");
        let lines: Vec<_> = output.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines.len(),
            2,
            "Expected 2 lines, got {}: {}",
            lines.len(),
            output
        );
    }
    #[test]
    fn test_avro_basic_parsing() {
        use apache_avro::{types::Value as AvroValue, Schema, Writer};

        // Create Avro schema
        let schema_str = r#"{
            "type": "record",
            "name": "User",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int"}
            ]
        }"#;
        let schema = Schema::parse_str(schema_str).expect("Failed to parse schema");

        // Create Avro data
        let mut writer = Writer::new(&schema, Vec::new());
        let record1 = AvroValue::Record(vec![
            ("name".to_string(), AvroValue::String("Alice".to_string())),
            ("age".to_string(), AvroValue::Int(30)),
        ]);
        let record2 = AvroValue::Record(vec![
            ("name".to_string(), AvroValue::String("Bob".to_string())),
            ("age".to_string(), AvroValue::Int(25)),
        ]);
        writer.append(record1).expect("Failed to append record");
        writer.append(record2).expect("Failed to append record");
        let avro_data = writer.into_inner().expect("Failed to get Avro data");

        // Parse with SelectExecutor
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Avro,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(&avro_data)
            .expect("Failed to parse Avro input");
        assert_eq!(records.len(), 2);

        // Verify first record
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("age"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_avro_with_query() {
        use apache_avro::{types::Value as AvroValue, Schema, Writer};

        // Create Avro schema
        let schema_str = r#"{
            "type": "record",
            "name": "User",
            "fields": [
                {"name": "name", "type": "string"},
                {"name": "age", "type": "int"},
                {"name": "score", "type": "double"}
            ]
        }"#;
        let schema = Schema::parse_str(schema_str).expect("Failed to parse schema");

        // Create Avro data
        let mut writer = Writer::new(&schema, Vec::new());
        writer
            .append(AvroValue::Record(vec![
                ("name".to_string(), AvroValue::String("Alice".to_string())),
                ("age".to_string(), AvroValue::Int(30)),
                ("score".to_string(), AvroValue::Double(95.5)),
            ]))
            .expect("Failed to append");
        writer
            .append(AvroValue::Record(vec![
                ("name".to_string(), AvroValue::String("Bob".to_string())),
                ("age".to_string(), AvroValue::Int(25)),
                ("score".to_string(), AvroValue::Double(88.0)),
            ]))
            .expect("Failed to append");
        writer
            .append(AvroValue::Record(vec![
                ("name".to_string(), AvroValue::String("Charlie".to_string())),
                ("age".to_string(), AvroValue::Int(35)),
                ("score".to_string(), AvroValue::Double(92.3)),
            ]))
            .expect("Failed to append");
        let avro_data = writer.into_inner().expect("Failed to get Avro data");

        // Query: SELECT name, score FROM s3object WHERE age > 25
        let query = parse_sql("SELECT name, score FROM s3object WHERE age > 25")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Avro,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor.execute(&avro_data).expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return 2 records (Alice and Charlie with age > 25)
        assert_eq!(lines.len(), 2);
        assert!(result_str.contains("Alice"));
        assert!(result_str.contains("Charlie"));
        assert!(!result_str.contains("Bob")); // Bob's age is 25, not > 25
    }
    #[test]
    fn test_avro_with_different_types() {
        use apache_avro::{types::Value as AvroValue, Schema, Writer};

        // Create Avro schema with various types
        let schema_str = r#"{
            "type": "record",
            "name": "TestRecord",
            "fields": [
                {"name": "str_field", "type": "string"},
                {"name": "int_field", "type": "int"},
                {"name": "long_field", "type": "long"},
                {"name": "float_field", "type": "float"},
                {"name": "double_field", "type": "double"},
                {"name": "bool_field", "type": "boolean"},
                {"name": "null_field", "type": ["null", "string"]}
            ]
        }"#;
        let schema = Schema::parse_str(schema_str).expect("Failed to parse schema");

        // Create Avro data with various types
        let mut writer = Writer::new(&schema, Vec::new());
        writer
            .append(AvroValue::Record(vec![
                (
                    "str_field".to_string(),
                    AvroValue::String("test".to_string()),
                ),
                ("int_field".to_string(), AvroValue::Int(42)),
                ("long_field".to_string(), AvroValue::Long(1234567890)),
                ("float_field".to_string(), AvroValue::Float(3.15)),
                ("double_field".to_string(), AvroValue::Double(2.5)),
                ("bool_field".to_string(), AvroValue::Boolean(true)),
                (
                    "null_field".to_string(),
                    AvroValue::Union(0, Box::new(AvroValue::Null)),
                ),
            ]))
            .expect("Failed to append");
        let avro_data = writer.into_inner().expect("Failed to get Avro data");

        // Parse and execute
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Avro,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(&avro_data)
            .expect("Failed to parse Avro input");
        assert_eq!(records.len(), 1);

        // Verify all fields are present
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("str_field"));
            assert!(map.contains_key("int_field"));
            assert!(map.contains_key("long_field"));
            assert!(map.contains_key("float_field"));
            assert!(map.contains_key("double_field"));
            assert!(map.contains_key("bool_field"));
            assert!(map.contains_key("null_field"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_avro_with_limit() {
        use apache_avro::{types::Value as AvroValue, Schema, Writer};

        // Create Avro schema
        let schema_str = r#"{
            "type": "record",
            "name": "Item",
            "fields": [
                {"name": "id", "type": "int"},
                {"name": "value", "type": "string"}
            ]
        }"#;
        let schema = Schema::parse_str(schema_str).expect("Failed to parse schema");

        // Create Avro data with 5 records
        let mut writer = Writer::new(&schema, Vec::new());
        for i in 1..=5 {
            writer
                .append(AvroValue::Record(vec![
                    ("id".to_string(), AvroValue::Int(i)),
                    (
                        "value".to_string(),
                        AvroValue::String(format!("item_{}", i)),
                    ),
                ]))
                .expect("Failed to append");
        }
        let avro_data = writer.into_inner().expect("Failed to get Avro data");

        // Query with LIMIT 3
        let query = parse_sql("SELECT * FROM s3object LIMIT 3").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Avro,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor.execute(&avro_data).expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return only 3 records due to LIMIT
        assert_eq!(lines.len(), 3);
    }
    #[test]
    fn test_orc_basic_parsing() {
        use arrow::array::{Int32Array, RecordBatch, StringArray};
        use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
        use orc_rust::ArrowWriterBuilder;
        use std::sync::Arc;

        // Create Arrow schema
        let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int32, false),
        ]));

        // Create Arrow record batch
        let names = StringArray::from(vec!["Alice", "Bob"]);
        let ages = Int32Array::from(vec![30, 25]);
        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(names), Arc::new(ages)])
            .expect("Failed to create record batch");

        // Write to ORC format
        let mut orc_buffer = Vec::new();
        let mut writer = ArrowWriterBuilder::new(&mut orc_buffer, schema)
            .try_build()
            .expect("Failed to build ORC writer");
        writer.write(&batch).expect("Failed to write batch");
        writer.close().expect("Failed to close writer");

        // Parse with SelectExecutor
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Orc,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(&orc_buffer)
            .expect("Failed to parse ORC input");
        assert_eq!(records.len(), 2);

        // Verify first record
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("age"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_orc_with_query() {
        use arrow::array::{Float64Array, Int32Array, RecordBatch, StringArray};
        use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
        use orc_rust::ArrowWriterBuilder;
        use std::sync::Arc;

        // Create Arrow schema
        let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("name", DataType::Utf8, false),
            Field::new("age", DataType::Int32, false),
            Field::new("score", DataType::Float64, false),
        ]));

        // Create Arrow record batch
        let names = StringArray::from(vec!["Alice", "Bob", "Charlie"]);
        let ages = Int32Array::from(vec![30, 25, 35]);
        let scores = Float64Array::from(vec![95.5, 88.0, 92.3]);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(names), Arc::new(ages), Arc::new(scores)],
        )
        .expect("Failed to create record batch");

        // Write to ORC format
        let mut orc_buffer = Vec::new();
        let mut writer = ArrowWriterBuilder::new(&mut orc_buffer, schema)
            .try_build()
            .expect("Failed to build ORC writer");
        writer.write(&batch).expect("Failed to write batch");
        writer.close().expect("Failed to close writer");

        // Query: SELECT name, score FROM s3object WHERE age > 25
        let query = parse_sql("SELECT name, score FROM s3object WHERE age > 25")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Orc,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor.execute(&orc_buffer).expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return 2 records (Alice and Charlie with age > 25)
        assert_eq!(lines.len(), 2);
        assert!(result_str.contains("Alice"));
        assert!(result_str.contains("Charlie"));
        assert!(!result_str.contains("Bob")); // Bob's age is 25, not > 25
    }
    #[test]
    fn test_orc_with_different_types() {
        use arrow::array::{BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray};
        use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
        use orc_rust::ArrowWriterBuilder;
        use std::sync::Arc;

        // Create Arrow schema with various types
        let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("str_field", DataType::Utf8, false),
            Field::new("int_field", DataType::Int64, false),
            Field::new("float_field", DataType::Float64, false),
            Field::new("bool_field", DataType::Boolean, false),
        ]));

        // Create Arrow record batch
        let strings = StringArray::from(vec!["test"]);
        let ints = Int64Array::from(vec![42]);
        let floats = Float64Array::from(vec![3.15]);
        let bools = BooleanArray::from(vec![true]);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(strings),
                Arc::new(ints),
                Arc::new(floats),
                Arc::new(bools),
            ],
        )
        .expect("Failed to create record batch");

        // Write to ORC format
        let mut orc_buffer = Vec::new();
        let mut writer = ArrowWriterBuilder::new(&mut orc_buffer, schema)
            .try_build()
            .expect("Failed to build ORC writer");
        writer.write(&batch).expect("Failed to write batch");
        writer.close().expect("Failed to close writer");

        // Parse and execute
        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Orc,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(&orc_buffer)
            .expect("Failed to parse ORC input");
        assert_eq!(records.len(), 1);

        // Verify all fields are present
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("str_field"));
            assert!(map.contains_key("int_field"));
            assert!(map.contains_key("float_field"));
            assert!(map.contains_key("bool_field"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_orc_with_limit() {
        use arrow::array::{Int32Array, RecordBatch, StringArray};
        use arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
        use orc_rust::ArrowWriterBuilder;
        use std::sync::Arc;

        // Create Arrow schema
        let schema = Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Int32, false),
            Field::new("value", DataType::Utf8, false),
        ]));

        // Create Arrow record batch with 5 records
        let ids = Int32Array::from(vec![1, 2, 3, 4, 5]);
        let values = StringArray::from(vec!["item_1", "item_2", "item_3", "item_4", "item_5"]);
        let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(ids), Arc::new(values)])
            .expect("Failed to create record batch");

        // Write to ORC format
        let mut orc_buffer = Vec::new();
        let mut writer = ArrowWriterBuilder::new(&mut orc_buffer, schema)
            .try_build()
            .expect("Failed to build ORC writer");
        writer.write(&batch).expect("Failed to write batch");
        writer.close().expect("Failed to close writer");

        // Query with LIMIT 3
        let query = parse_sql("SELECT * FROM s3object LIMIT 3").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Orc,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor.execute(&orc_buffer).expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return only 3 records due to LIMIT
        assert_eq!(lines.len(), 3);
    }
    #[test]
    fn test_protobuf_json_basic_parsing() {
        // Protobuf JSON format (newline-delimited)
        let protobuf_json_data = r#"{"name": "Alice", "age": 30}
{"name": "Bob", "age": 25}"#;

        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Protobuf,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(protobuf_json_data.as_bytes())
            .expect("Failed to parse Protobuf JSON input");
        assert_eq!(records.len(), 2);

        // Verify first record
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("age"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_protobuf_json_with_query() {
        // Protobuf JSON format with multiple records
        let protobuf_json_data = r#"{"name": "Alice", "age": 30, "score": 95.5}
{"name": "Bob", "age": 25, "score": 88.0}
{"name": "Charlie", "age": 35, "score": 92.3}"#;

        // Query: SELECT name, score FROM s3object WHERE age > 25
        let query = parse_sql("SELECT name, score FROM s3object WHERE age > 25")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Protobuf,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor
            .execute(protobuf_json_data.as_bytes())
            .expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return 2 records (Alice and Charlie with age > 25)
        assert_eq!(lines.len(), 2);
        assert!(result_str.contains("Alice"));
        assert!(result_str.contains("Charlie"));
        assert!(!result_str.contains("Bob")); // Bob's age is 25, not > 25
    }
    #[test]
    fn test_protobuf_json_with_nested_data() {
        // Protobuf JSON with nested structures
        let protobuf_json_data = r#"{"user": {"name": "Alice", "age": 30}, "active": true}
{"user": {"name": "Bob", "age": 25}, "active": false}"#;

        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::Protobuf,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(protobuf_json_data.as_bytes())
            .expect("Failed to parse Protobuf JSON input");
        assert_eq!(records.len(), 2);

        // Verify fields are present
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("user"));
            assert!(map.contains_key("active"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_protobuf_json_with_limit() {
        // Protobuf JSON with 5 records
        let protobuf_json_data = r#"{"id": 1, "value": "item_1"}
{"id": 2, "value": "item_2"}
{"id": 3, "value": "item_3"}
{"id": 4, "value": "item_4"}
{"id": 5, "value": "item_5"}"#;

        // Query with LIMIT 3
        let query = parse_sql("SELECT * FROM s3object LIMIT 3").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::Protobuf,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor
            .execute(protobuf_json_data.as_bytes())
            .expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return only 3 records due to LIMIT
        assert_eq!(lines.len(), 3);
    }
    #[test]
    fn test_messagepack_basic_parsing() {
        use rmp_serde::Serializer;
        use serde::Serialize;

        // Create MessagePack data with 2 records
        let mut msgpack_data = Vec::new();

        // Serialize first record
        let record1 = serde_json::json!({"name": "Alice", "age": 30});
        record1
            .serialize(&mut Serializer::new(&mut msgpack_data))
            .expect("Failed to serialize to MessagePack");

        // Serialize second record
        let record2 = serde_json::json!({"name": "Bob", "age": 25});
        record2
            .serialize(&mut Serializer::new(&mut msgpack_data))
            .expect("Failed to serialize to MessagePack");

        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::MessagePack,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(&msgpack_data)
            .expect("Failed to parse MessagePack input");
        assert_eq!(records.len(), 2);

        // Verify first record
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("name"));
            assert!(map.contains_key("age"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_messagepack_with_query() {
        use rmp_serde::Serializer;
        use serde::Serialize;

        // Create MessagePack data with 3 records
        let mut msgpack_data = Vec::new();

        let records_data = vec![
            serde_json::json!({"name": "Alice", "age": 30, "score": 95.5}),
            serde_json::json!({"name": "Bob", "age": 25, "score": 88.0}),
            serde_json::json!({"name": "Charlie", "age": 35, "score": 92.3}),
        ];

        for record in &records_data {
            record
                .serialize(&mut Serializer::new(&mut msgpack_data))
                .expect("Failed to serialize to MessagePack");
        }

        // Query: SELECT name, score FROM s3object WHERE age > 25
        let query = parse_sql("SELECT name, score FROM s3object WHERE age > 25")
            .expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::MessagePack,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor.execute(&msgpack_data).expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return 2 records (Alice and Charlie with age > 25)
        assert_eq!(lines.len(), 2);
        assert!(result_str.contains("Alice"));
        assert!(result_str.contains("Charlie"));
        assert!(!result_str.contains("Bob")); // Bob's age is 25, not > 25
    }
    #[test]
    fn test_messagepack_with_nested_data() {
        use rmp_serde::Serializer;
        use serde::Serialize;

        // Create MessagePack data with nested structures
        let mut msgpack_data = Vec::new();

        let records_data = vec![
            serde_json::json!({"user": {"name": "Alice", "age": 30}, "active": true}),
            serde_json::json!({"user": {"name": "Bob", "age": 25}, "active": false}),
        ];

        for record in &records_data {
            record
                .serialize(&mut Serializer::new(&mut msgpack_data))
                .expect("Failed to serialize to MessagePack");
        }

        let executor = SelectExecutor {
            query: ParsedQuery {
                columns: vec![SelectColumn::Column(ColumnRef::All)],
                from_alias: None,
                where_clause: None,
                group_by: None,
                order_by: None,
                limit: None,
            },
            input_format: InputFormat::MessagePack,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let records = executor
            .parse_input(&msgpack_data)
            .expect("Failed to parse MessagePack input");
        assert_eq!(records.len(), 2);

        // Verify fields are present
        if let Record::Map(map) = &records[0] {
            assert!(map.contains_key("user"));
            assert!(map.contains_key("active"));
        } else {
            panic!("Expected Map record");
        }
    }
    #[test]
    fn test_messagepack_with_limit() {
        use rmp_serde::Serializer;
        use serde::Serialize;

        // Create MessagePack data with 5 records
        let mut msgpack_data = Vec::new();

        for i in 1..=5 {
            let record = serde_json::json!({"id": i, "value": format!("item_{}", i)});
            record
                .serialize(&mut Serializer::new(&mut msgpack_data))
                .expect("Failed to serialize to MessagePack");
        }

        // Query with LIMIT 3
        let query = parse_sql("SELECT * FROM s3object LIMIT 3").expect("Failed to parse SQL");
        let executor = SelectExecutor {
            query,
            input_format: InputFormat::MessagePack,
            output_format: OutputFormat::Json(JsonOutput::default()),
        };

        let result = executor.execute(&msgpack_data).expect("Failed to execute");
        let result_str = String::from_utf8(result).expect("Failed to convert to string");
        let lines: Vec<&str> = result_str.trim().split('\n').collect();

        // Should return only 3 records due to LIMIT
        assert_eq!(lines.len(), 3);
    }
}
