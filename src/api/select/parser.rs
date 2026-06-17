#![cfg(feature = "formats")]
//! S3 Select SQL parser and helper functions
//!
//! Contains SQL parsing, condition evaluation, and helper functions.

use std::collections::HashMap;

use serde_json::Value as JsonValue;
use tracing::debug;

use super::types::{
    AggregateFunction, ColumnRef, CompareOp, Condition, CsvOutput, FieldValue, Operand,
    OrderByClause, ParsedQuery, QuoteFields, Record, SelectColumn, SelectError,
};

/// Parse a simple SQL SELECT statement
pub fn parse_sql(sql: &str) -> Result<ParsedQuery, SelectError> {
    let sql = sql.trim();
    debug!(sql, "Parsing SQL expression");
    let lower = sql.to_lowercase();
    if !lower.starts_with("select ") {
        return Err(SelectError::InvalidSql(
            "Query must start with SELECT".to_string(),
        ));
    }
    let from_pos = lower
        .find(" from ")
        .ok_or_else(|| SelectError::InvalidSql("Missing FROM clause".to_string()))?;
    let columns_str = sql[7..from_pos].trim();
    let columns = parse_select_columns(columns_str)?;
    let after_from = &sql[from_pos + 6..];
    let lower_after = after_from.to_lowercase();
    let where_pos = find_keyword_pos(&lower_after, "where");
    let group_by_pos = find_keyword_pos(&lower_after, "group by");
    let order_by_pos = find_keyword_pos(&lower_after, "order by");
    let limit_pos = find_keyword_pos(&lower_after, "limit");
    let table_end = where_pos
        .or(group_by_pos)
        .or(order_by_pos)
        .or(limit_pos)
        .unwrap_or(after_from.len());
    let table_part = after_from[..table_end].trim();
    let from_alias = {
        let table_lower = table_part.to_lowercase();
        if table_lower == "s3object" {
            None
        } else if table_lower.starts_with("s3object ") {
            let alias_part = table_part[9..].trim();
            if alias_part.to_lowercase().starts_with("as ") {
                Some(alias_part[3..].trim().to_string())
            } else {
                Some(alias_part.to_string())
            }
        } else {
            return Err(SelectError::InvalidSql(
                "FROM clause must reference s3object".to_string(),
            ));
        }
    };
    let where_clause = if let Some(w_pos) = where_pos {
        let where_start = w_pos + 6;
        let where_end = group_by_pos
            .or(order_by_pos)
            .or(limit_pos)
            .unwrap_or(after_from.len());
        let cond_str = after_from[where_start..where_end].trim();
        Some(parse_condition(cond_str)?)
    } else {
        None
    };
    let group_by = if let Some(g_pos) = group_by_pos {
        let group_start = g_pos + 9;
        let group_end = order_by_pos.or(limit_pos).unwrap_or(after_from.len());
        let group_str = after_from[group_start..group_end].trim();
        Some(parse_group_by(group_str)?)
    } else {
        None
    };
    let order_by = if let Some(o_pos) = order_by_pos {
        let order_start = o_pos + 9;
        let order_end = limit_pos.unwrap_or(after_from.len());
        let order_str = after_from[order_start..order_end].trim();
        Some(parse_order_by(order_str)?)
    } else {
        None
    };
    let limit =
        if let Some(l_pos) = limit_pos {
            let limit_start = l_pos + 6;
            let limit_str = after_from[limit_start..].trim();
            Some(limit_str.parse::<usize>().map_err(|_| {
                SelectError::InvalidSql(format!("Invalid LIMIT value: {}", limit_str))
            })?)
        } else {
            None
        };
    Ok(ParsedQuery {
        columns,
        from_alias,
        where_clause,
        group_by,
        order_by,
        limit,
    })
}
/// Find keyword position (must be at word boundary)
fn find_keyword_pos(s: &str, keyword: &str) -> Option<usize> {
    let search = format!(" {} ", keyword);
    if let Some(pos) = s.find(&search) {
        return Some(pos + 1);
    }
    let search_end = format!(" {}", keyword);
    if s.ends_with(&search_end) {
        return Some(s.len() - keyword.len());
    }
    None
}
/// Parse column list (supports aggregation functions)
pub fn parse_select_columns(s: &str) -> Result<Vec<SelectColumn>, SelectError> {
    let s = s.trim();
    if s == "*" {
        return Ok(vec![SelectColumn::Column(ColumnRef::All)]);
    }
    let mut columns = Vec::new();
    for part in s.split(',') {
        let col = part.trim();
        if col.is_empty() {
            continue;
        }
        if let Some(agg_col) = parse_aggregate_function(col)? {
            columns.push(agg_col);
            continue;
        }
        let col = if let Some(dot_pos) = col.find('.') {
            &col[dot_pos + 1..]
        } else {
            col
        };
        if let Some(suffix) = col.strip_prefix('_') {
            if let Ok(idx) = suffix.parse::<usize>() {
                columns.push(SelectColumn::Column(ColumnRef::Indexed(
                    idx.saturating_sub(1),
                )));
                continue;
            }
        }
        columns.push(SelectColumn::Column(ColumnRef::Named(col.to_string())));
    }
    Ok(columns)
}
/// Parse aggregation function (e.g., COUNT(*), SUM(amount), AVG(price))
fn parse_aggregate_function(s: &str) -> Result<Option<SelectColumn>, SelectError> {
    let s = s.trim();
    let s_lower = s.to_lowercase();
    for (func_name, func_type) in [
        ("count", AggregateFunction::Count),
        ("sum", AggregateFunction::Sum),
        ("avg", AggregateFunction::Avg),
        ("min", AggregateFunction::Min),
        ("max", AggregateFunction::Max),
    ] {
        if s_lower.starts_with(func_name) && s_lower.contains('(') {
            if let Some(start) = s.find('(') {
                if let Some(end) = s.rfind(')') {
                    let inner = s[start + 1..end].trim();
                    let column = if inner == "*" {
                        None
                    } else {
                        Some(inner.to_string())
                    };
                    return Ok(Some(SelectColumn::Aggregate {
                        function: func_type,
                        column,
                        alias: None,
                    }));
                }
            }
        }
    }
    Ok(None)
}
/// Parse GROUP BY clause
pub fn parse_group_by(s: &str) -> Result<Vec<String>, SelectError> {
    let mut columns = Vec::new();
    for part in s.split(',') {
        let col = part.trim();
        if col.is_empty() {
            continue;
        }
        columns.push(col.to_string());
    }
    if columns.is_empty() {
        return Err(SelectError::InvalidSql(
            "GROUP BY clause must specify at least one column".to_string(),
        ));
    }
    Ok(columns)
}
/// Parse ORDER BY clause
pub fn parse_order_by(s: &str) -> Result<Vec<OrderByClause>, SelectError> {
    let mut order_by = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        let parts: Vec<&str> = part.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        let column = parts[0].to_string();
        let ascending = if parts.len() > 1 {
            parts[1].to_lowercase() != "desc"
        } else {
            true
        };
        order_by.push(OrderByClause { column, ascending });
    }
    if order_by.is_empty() {
        return Err(SelectError::InvalidSql(
            "ORDER BY clause must specify at least one column".to_string(),
        ));
    }
    Ok(order_by)
}
/// Parse a WHERE condition (simplified)
pub fn parse_condition(s: &str) -> Result<Condition, SelectError> {
    let s = s.trim();
    let lower = s.to_lowercase();
    if let Some(pos) = find_logical_op(&lower, " and ") {
        let left = parse_condition(&s[..pos])?;
        let right = parse_condition(&s[pos + 5..])?;
        return Ok(Condition::And(Box::new(left), Box::new(right)));
    }
    if let Some(pos) = find_logical_op(&lower, " or ") {
        let left = parse_condition(&s[..pos])?;
        let right = parse_condition(&s[pos + 4..])?;
        return Ok(Condition::Or(Box::new(left), Box::new(right)));
    }
    if lower.starts_with("not ") {
        let inner = parse_condition(&s[4..])?;
        return Ok(Condition::Not(Box::new(inner)));
    }
    if lower.ends_with(" is null") {
        let col = &s[..s.len() - 8].trim();
        return Ok(Condition::IsNull(parse_operand(col)?));
    }
    if lower.ends_with(" is not null") {
        let col = &s[..s.len() - 12].trim();
        return Ok(Condition::IsNotNull(parse_operand(col)?));
    }
    if let Some(pos) = lower.find(" like ") {
        let value = parse_operand(s[..pos].trim())?;
        let pattern = s[pos + 6..].trim().trim_matches('\'').to_string();
        return Ok(Condition::Like { value, pattern });
    }
    for (op_str, op) in [
        ("!=", CompareOp::Ne),
        ("<>", CompareOp::Ne),
        ("<=", CompareOp::Le),
        (">=", CompareOp::Ge),
        ("<", CompareOp::Lt),
        (">", CompareOp::Gt),
        ("=", CompareOp::Eq),
    ] {
        if let Some(pos) = s.find(op_str) {
            let left = parse_operand(s[..pos].trim())?;
            let right = parse_operand(s[pos + op_str.len()..].trim())?;
            return Ok(Condition::Comparison { left, op, right });
        }
    }
    Err(SelectError::InvalidSql(format!(
        "Cannot parse condition: {}",
        s
    )))
}
/// Find logical operator position (outside quotes)
fn find_logical_op(s: &str, op: &str) -> Option<usize> {
    let mut in_quote = false;
    let mut i = 0;
    let bytes = s.as_bytes();
    let op_bytes = op.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            in_quote = !in_quote;
        }
        if !in_quote && bytes[i..].starts_with(op_bytes) {
            return Some(i);
        }
        i += 1;
    }
    None
}
/// Parse an operand (column ref or literal)
fn parse_operand(s: &str) -> Result<Operand, SelectError> {
    let s = s.trim();
    if s.starts_with('\'') && s.ends_with('\'') && s.len() >= 2 {
        return Ok(Operand::StringLiteral(s[1..s.len() - 1].to_string()));
    }
    if s.eq_ignore_ascii_case("null") {
        return Ok(Operand::Null);
    }
    if s.eq_ignore_ascii_case("true") {
        return Ok(Operand::BoolLiteral(true));
    }
    if s.eq_ignore_ascii_case("false") {
        return Ok(Operand::BoolLiteral(false));
    }
    if let Ok(n) = s.parse::<f64>() {
        return Ok(Operand::NumberLiteral(n));
    }
    let col = if let Some(dot_pos) = s.find('.') {
        &s[dot_pos + 1..]
    } else {
        s
    };
    if let Some(suffix) = col.strip_prefix('_') {
        if let Ok(idx) = suffix.parse::<usize>() {
            return Ok(Operand::Column(ColumnRef::Indexed(idx.saturating_sub(1))));
        }
    }
    Ok(Operand::Column(ColumnRef::Named(col.to_string())))
}
/// Evaluate a condition against a record
pub fn evaluate_condition_public(
    condition: &Condition,
    record: &Record,
) -> Result<bool, SelectError> {
    evaluate_condition(condition, record)
}
/// Evaluate a condition against a record (internal)
pub(crate) fn evaluate_condition(
    condition: &Condition,
    record: &Record,
) -> Result<bool, SelectError> {
    match condition {
        Condition::Comparison { left, op, right } => {
            let left_val = evaluate_operand(left, record);
            let right_val = evaluate_operand(right, record);
            Ok(compare_values(&left_val, op, &right_val))
        }
        Condition::And(left, right) => {
            Ok(evaluate_condition(left, record)? && evaluate_condition(right, record)?)
        }
        Condition::Or(left, right) => {
            Ok(evaluate_condition(left, record)? || evaluate_condition(right, record)?)
        }
        Condition::Not(inner) => Ok(!evaluate_condition(inner, record)?),
        Condition::IsNull(operand) => {
            let val = evaluate_operand(operand, record);
            Ok(val.is_null())
        }
        Condition::IsNotNull(operand) => {
            let val = evaluate_operand(operand, record);
            Ok(!val.is_null())
        }
        Condition::Like { value, pattern } => {
            let val = evaluate_operand(value, record).as_string();
            Ok(match_like(&val, pattern))
        }
    }
}
/// Evaluate an operand to get its value
fn evaluate_operand(operand: &Operand, record: &Record) -> FieldValue {
    match operand {
        Operand::Column(col) => match col {
            ColumnRef::All => FieldValue::Null,
            ColumnRef::Named(name) => record.get_field(name).unwrap_or(FieldValue::Null),
            ColumnRef::Indexed(idx) => record.get_by_index(*idx).unwrap_or(FieldValue::Null),
        },
        Operand::StringLiteral(s) => FieldValue::String(s.clone()),
        Operand::NumberLiteral(n) => FieldValue::Number(*n),
        Operand::BoolLiteral(b) => FieldValue::Bool(*b),
        Operand::Null => FieldValue::Null,
    }
}
/// Compare two field values
fn compare_values(left: &FieldValue, op: &CompareOp, right: &FieldValue) -> bool {
    if left.is_null() || right.is_null() {
        return false;
    }
    if let (Some(l), Some(r)) = (left.as_f64(), right.as_f64()) {
        return match op {
            CompareOp::Eq => (l - r).abs() < f64::EPSILON,
            CompareOp::Ne => (l - r).abs() >= f64::EPSILON,
            CompareOp::Lt => l < r,
            CompareOp::Le => l <= r,
            CompareOp::Gt => l > r,
            CompareOp::Ge => l >= r,
        };
    }
    let l = left.as_string();
    let r = right.as_string();
    match op {
        CompareOp::Eq => l == r,
        CompareOp::Ne => l != r,
        CompareOp::Lt => l < r,
        CompareOp::Le => l <= r,
        CompareOp::Gt => l > r,
        CompareOp::Ge => l >= r,
    }
}
/// Match LIKE pattern (% = any chars, _ = single char)
fn match_like(s: &str, pattern: &str) -> bool {
    let regex_pattern = pattern.replace('%', ".*").replace('_', ".");
    regex::Regex::new(&format!("^{}$", regex_pattern))
        .map(|re| re.is_match(s))
        .unwrap_or(false)
}
/// Parse a CSV line respecting quotes
pub(crate) fn parse_csv_line(line: &str, delimiter: char, quote: char) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == quote {
            if in_quotes && chars.peek() == Some(&quote) {
                current.push(quote);
                chars.next();
            } else {
                in_quotes = !in_quotes;
            }
        } else if c == delimiter && !in_quotes {
            fields.push(current);
            current = String::new();
        } else {
            current.push(c);
        }
    }
    fields.push(current);
    fields
}
/// Quote a CSV field if needed
pub(crate) fn quote_csv_field(s: &str, config: &CsvOutput) -> String {
    let needs_quote = matches!(config.quote_fields, QuoteFields::Always)
        || s.contains(config.field_delimiter)
        || s.contains(config.quote_character)
        || s.contains('\n')
        || s.contains('\r');
    if needs_quote {
        let escaped = s.replace(
            config.quote_character,
            &format!(
                "{}{}",
                config.quote_escape_character, config.quote_character
            ),
        );
        format!(
            "{}{}{}",
            config.quote_character, escaped, config.quote_character
        )
    } else {
        s.to_string()
    }
}
/// Convert JSON value to Record
pub(crate) fn json_to_record(value: JsonValue) -> Result<Record, SelectError> {
    match value {
        JsonValue::Object(map) => {
            let mut record = HashMap::new();
            for (k, v) in map {
                record.insert(k, json_to_field(v));
            }
            Ok(Record::Map(record))
        }
        JsonValue::Array(arr) => Ok(Record::Array(arr.into_iter().map(json_to_field).collect())),
        _ => Err(SelectError::InvalidFormat(
            "Record must be object or array".to_string(),
        )),
    }
}
/// Convert JSON value to FieldValue
fn json_to_field(value: JsonValue) -> FieldValue {
    match value {
        JsonValue::Null => FieldValue::Null,
        JsonValue::Bool(b) => FieldValue::Bool(b),
        JsonValue::Number(n) => FieldValue::Number(n.as_f64().unwrap_or(0.0)),
        JsonValue::String(s) => FieldValue::String(s),
        JsonValue::Array(_) | JsonValue::Object(_) => FieldValue::String(value.to_string()),
    }
}
/// Compare two field values for sorting
pub(crate) fn compare_field_values(a: &FieldValue, b: &FieldValue) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a, b) {
        (FieldValue::Null, FieldValue::Null) => Ordering::Equal,
        (FieldValue::Null, _) => Ordering::Less,
        (_, FieldValue::Null) => Ordering::Greater,
        (FieldValue::Bool(a), FieldValue::Bool(b)) => a.cmp(b),
        (FieldValue::Number(a), FieldValue::Number(b)) => {
            a.partial_cmp(b).unwrap_or(Ordering::Equal)
        }
        (FieldValue::String(a), FieldValue::String(b)) => a.cmp(b),
        (a, b) => a.as_string().cmp(&b.as_string()),
    }
}
