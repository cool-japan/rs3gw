#![cfg(feature = "formats")]
//! Advanced SQL features for S3 Select
//!
//! Implements JOIN operations, Window Functions, and Common Table Expressions (CTEs)

use super::types::{Condition, SelectColumn, SelectError};
use serde::{Deserialize, Serialize};

/// JOIN type enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

/// JOIN clause specification
#[derive(Debug, Clone)]
pub struct JoinClause {
    /// Type of join operation
    pub join_type: JoinType,
    /// Right table reference (for S3 Select, this would be another object key)
    pub right_table: String,
    /// Optional alias for the right table
    pub right_alias: Option<String>,
    /// JOIN condition (e.g., ON left.id = right.id)
    pub on_condition: Option<JoinCondition>,
}

/// JOIN condition for ON clause
#[derive(Debug, Clone)]
pub struct JoinCondition {
    pub left_column: String,
    pub right_column: String,
    pub operator: JoinOperator,
}

/// Operators allowed in JOIN conditions
#[derive(Debug, Clone, PartialEq)]
pub enum JoinOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

/// Window function type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WindowFunction {
    /// ROW_NUMBER() - Assigns unique sequential integer to rows
    RowNumber,
    /// RANK() - Assigns rank with gaps for ties
    Rank,
    /// DENSE_RANK() - Assigns rank without gaps
    DenseRank,
    /// LEAD(column, offset) - Access subsequent row value
    Lead { column: String, offset: usize },
    /// LAG(column, offset) - Access previous row value
    Lag { column: String, offset: usize },
    /// FIRST_VALUE(column) - First value in window frame
    FirstValue { column: String },
    /// LAST_VALUE(column) - Last value in window frame
    LastValue { column: String },
    /// NTILE(n) - Divides rows into n buckets
    NTile { buckets: usize },
}

/// Window frame specification
#[derive(Debug, Clone, PartialEq)]
pub struct WindowFrame {
    /// Partition columns (PARTITION BY clause)
    pub partition_by: Vec<String>,
    /// Order columns (ORDER BY clause within window)
    pub order_by: Vec<WindowOrderBy>,
    /// Frame specification (ROWS/RANGE between...)
    pub frame_spec: Option<FrameSpecification>,
}

/// Window ORDER BY specification
#[derive(Debug, Clone, PartialEq)]
pub struct WindowOrderBy {
    pub column: String,
    pub ascending: bool,
}

/// Frame specification for window functions
#[derive(Debug, Clone, PartialEq)]
pub struct FrameSpecification {
    pub frame_type: FrameType,
    pub start_bound: FrameBound,
    pub end_bound: FrameBound,
}

/// Frame type (ROWS or RANGE)
#[derive(Debug, Clone, PartialEq)]
pub enum FrameType {
    Rows,
    Range,
}

/// Frame boundary specification
#[derive(Debug, Clone, PartialEq)]
pub enum FrameBound {
    UnboundedPreceding,
    Preceding(usize),
    CurrentRow,
    Following(usize),
    UnboundedFollowing,
}

/// Common Table Expression (CTE) definition
#[derive(Debug, Clone)]
pub struct CteDefinition {
    /// CTE name
    pub name: String,
    /// Column names (optional)
    pub columns: Option<Vec<String>>,
    /// Query defining the CTE
    pub query: String,
}

/// Enhanced ParsedQuery with advanced features
#[derive(Debug, Clone)]
pub struct AdvancedParsedQuery {
    /// Common Table Expressions (WITH clause)
    pub ctes: Vec<CteDefinition>,
    /// Main query columns
    pub columns: Vec<SelectColumn>,
    /// FROM table/alias
    pub from_table: String,
    pub from_alias: Option<String>,
    /// JOIN clauses
    pub joins: Vec<JoinClause>,
    /// WHERE condition
    pub where_clause: Option<Condition>,
    /// GROUP BY columns
    pub group_by: Option<Vec<String>>,
    /// HAVING condition (for grouped queries)
    pub having: Option<Condition>,
    /// ORDER BY clauses
    pub order_by: Option<Vec<super::types::OrderByClause>>,
    /// LIMIT clause
    pub limit: Option<usize>,
    /// OFFSET clause
    pub offset: Option<usize>,
}

/// Parse advanced SQL with JOINs, CTEs, and window functions
pub fn parse_advanced_sql(sql: &str) -> Result<AdvancedParsedQuery, SelectError> {
    let sql = sql.trim();
    let lower = sql.to_lowercase();

    // Parse CTEs (WITH clause)
    let (ctes, main_query) = if lower.starts_with("with ") {
        parse_ctes(sql)?
    } else {
        (Vec::new(), sql.to_string())
    };

    // Parse main SELECT query
    let main_lower = main_query.to_lowercase();
    if !main_lower.starts_with("select ") {
        return Err(SelectError::InvalidSql(
            "Query must start with SELECT or WITH".to_string(),
        ));
    }

    // Find FROM position
    let from_pos = main_lower
        .find(" from ")
        .ok_or_else(|| SelectError::InvalidSql("Missing FROM clause".to_string()))?;

    // Parse SELECT columns
    let columns_str = main_query[7..from_pos].trim();
    let columns = super::parser::parse_select_columns(columns_str)?;

    // Parse FROM and JOIN clauses
    let after_from = &main_query[from_pos + 6..];
    let (from_table, from_alias, joins, remainder) = parse_from_and_joins(after_from)?;

    // Parse WHERE, GROUP BY, HAVING, ORDER BY, LIMIT, OFFSET
    let where_clause = extract_where_clause(&remainder)?;
    let group_by = extract_group_by(&remainder)?;
    let having = extract_having_clause(&remainder)?;
    let order_by = extract_order_by(&remainder)?;
    let limit = extract_limit(&remainder)?;
    let offset = extract_offset(&remainder)?;

    Ok(AdvancedParsedQuery {
        ctes,
        columns,
        from_table,
        from_alias,
        joins,
        where_clause,
        group_by,
        having,
        order_by,
        limit,
        offset,
    })
}

/// Parse WITH clause CTEs
fn parse_ctes(sql: &str) -> Result<(Vec<CteDefinition>, String), SelectError> {
    // Find the main SELECT after WITH clause
    // Format: WITH cte1 AS (query1), cte2 AS (query2) SELECT ...

    // Simple implementation: find "select" that's not inside parentheses
    let mut paren_depth = 0;
    let mut select_pos = None;
    let chars: Vec<char> = sql.chars().collect();

    for i in 0..chars.len() {
        match chars[i] {
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            _ => {}
        }

        if paren_depth == 0 && i + 6 < chars.len() {
            let word: String = chars[i..i + 6].iter().collect();
            if word.to_lowercase() == "select" {
                select_pos = Some(i);
                break;
            }
        }
    }

    let select_pos = select_pos.ok_or_else(|| {
        SelectError::InvalidSql("No main SELECT found after WITH clause".to_string())
    })?;

    let cte_section = &sql[5..select_pos].trim(); // Skip "WITH "
    let main_query = &sql[select_pos..];

    // Parse individual CTEs (simplified - full implementation would handle nested CTEs)
    let ctes = parse_cte_definitions(cte_section)?;

    Ok((ctes, main_query.to_string()))
}

/// Parse CTE definitions from WITH clause
fn parse_cte_definitions(cte_str: &str) -> Result<Vec<CteDefinition>, SelectError> {
    // Split by top-level commas (not inside parentheses)
    let cte_parts = split_by_top_level_comma(cte_str);

    let mut ctes = Vec::new();
    for part in cte_parts {
        let cte = parse_single_cte(part.trim())?;
        ctes.push(cte);
    }

    Ok(ctes)
}

/// Parse a single CTE definition
fn parse_single_cte(cte_str: &str) -> Result<CteDefinition, SelectError> {
    // Format: cte_name [(col1, col2)] AS (query)
    let as_pos = cte_str
        .to_lowercase()
        .find(" as ")
        .ok_or_else(|| SelectError::InvalidSql("CTE missing AS keyword".to_string()))?;

    let name_part = cte_str[..as_pos].trim();
    let query_part = cte_str[as_pos + 4..].trim();

    // Extract name and optional column list
    let (name, columns) = if name_part.contains('(') {
        let paren_pos = name_part
            .find('(')
            .ok_or_else(|| SelectError::InvalidSql("Invalid CTE column list".to_string()))?;
        let name = name_part[..paren_pos].trim().to_string();
        let cols_str = name_part[paren_pos + 1..].trim_end_matches(')').trim();
        let columns: Vec<String> = cols_str.split(',').map(|s| s.trim().to_string()).collect();
        (name, Some(columns))
    } else {
        (name_part.to_string(), None)
    };

    // Extract query (remove surrounding parentheses)
    let query = query_part
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim()
        .to_string();

    Ok(CteDefinition {
        name,
        columns,
        query,
    })
}

/// Parse FROM clause and JOINs
fn parse_from_and_joins(
    from_str: &str,
) -> Result<(String, Option<String>, Vec<JoinClause>, String), SelectError> {
    // Simple implementation: extract table name, alias, and JOIN clauses
    let lower = from_str.to_lowercase();

    // Find first JOIN keyword or WHERE/GROUP BY/ORDER BY/LIMIT
    let join_keywords = vec![
        "inner join",
        "left join",
        "right join",
        "full join",
        "cross join",
        "join",
    ];
    let end_keywords = vec!["where", "group by", "order by", "limit", "offset"];

    let mut first_join_pos = None;
    for keyword in &join_keywords {
        if let Some(pos) = lower.find(keyword) {
            first_join_pos = match first_join_pos {
                None => Some(pos),
                Some(current) => Some(current.min(pos)),
            };
        }
    }

    let mut first_end_pos = None;
    for keyword in &end_keywords {
        if let Some(pos) = lower.find(keyword) {
            first_end_pos = match first_end_pos {
                None => Some(pos),
                Some(current) => Some(current.min(pos)),
            };
        }
    }

    let table_end = first_join_pos.or(first_end_pos).unwrap_or(from_str.len());
    let table_part = from_str[..table_end].trim();

    // Parse table name and alias
    let parts: Vec<&str> = table_part.split_whitespace().collect();
    let (table, alias) = match parts.len() {
        1 => (parts[0].to_string(), None),
        2 if parts[1].to_lowercase() == "as" => {
            return Err(SelectError::InvalidSql(
                "AS keyword without alias".to_string(),
            ));
        }
        2 => (parts[0].to_string(), Some(parts[1].to_string())),
        3 if parts[1].to_lowercase() == "as" => (parts[0].to_string(), Some(parts[2].to_string())),
        _ => return Err(SelectError::InvalidSql("Invalid FROM clause".to_string())),
    };

    // Parse JOIN clauses
    let (joins, remainder) = if let Some(join_pos) = first_join_pos {
        let join_section = &from_str[join_pos..];
        parse_joins(join_section)?
    } else {
        (Vec::new(), from_str[table_end..].to_string())
    };

    Ok((table, alias, joins, remainder))
}

/// Parse JOIN clauses
fn parse_joins(join_str: &str) -> Result<(Vec<JoinClause>, String), SelectError> {
    // This is a simplified parser - full implementation would handle complex JOINs
    let mut joins = Vec::new();
    let mut remaining = join_str;

    loop {
        let lower = remaining.to_lowercase();

        // Determine JOIN type
        let (join_type, keyword_len) = if lower.starts_with("inner join ") {
            (JoinType::Inner, 11)
        } else if lower.starts_with("left join ") {
            (JoinType::Left, 10)
        } else if lower.starts_with("right join ") {
            (JoinType::Right, 11)
        } else if lower.starts_with("full join ") || lower.starts_with("full outer join ") {
            (
                JoinType::Full,
                if lower.starts_with("full outer join ") {
                    16
                } else {
                    10
                },
            )
        } else if lower.starts_with("cross join ") {
            (JoinType::Cross, 11)
        } else if lower.starts_with("join ") {
            (JoinType::Inner, 5) // Default to INNER
        } else {
            // No more JOINs
            break;
        };

        remaining = &remaining[keyword_len..];

        // Find ON clause or next JOIN/WHERE
        let on_pos = remaining.to_lowercase().find(" on ");
        let next_join = find_next_join_keyword(&remaining.to_lowercase());
        let next_where = remaining.to_lowercase().find(" where ");

        let table_end = on_pos
            .or(next_join)
            .or(next_where)
            .unwrap_or(remaining.len());
        let table_part = remaining[..table_end].trim();

        // Parse right table and alias
        let table_parts: Vec<&str> = table_part.split_whitespace().collect();
        let (right_table, right_alias) = match table_parts.len() {
            1 => (table_parts[0].to_string(), None),
            2 if table_parts[1].to_lowercase() != "as" => {
                (table_parts[0].to_string(), Some(table_parts[1].to_string()))
            }
            3 if table_parts[1].to_lowercase() == "as" => {
                (table_parts[0].to_string(), Some(table_parts[2].to_string()))
            }
            _ => {
                return Err(SelectError::InvalidSql(
                    "Invalid JOIN table specification".to_string(),
                ))
            }
        };

        // Parse ON condition
        let on_condition = if let Some(on_pos) = on_pos {
            remaining = &remaining[on_pos + 4..]; // Skip " ON "
            let cond_end = find_next_join_keyword(&remaining.to_lowercase())
                .or(remaining.to_lowercase().find(" where "))
                .or(remaining.to_lowercase().find(" group by "))
                .or(remaining.to_lowercase().find(" order by "))
                .or(remaining.to_lowercase().find(" limit "))
                .unwrap_or(remaining.len());

            let on_clause = remaining[..cond_end].trim();
            remaining = &remaining[cond_end..];

            Some(parse_join_condition(on_clause)?)
        } else {
            remaining = &remaining[table_end..];
            if join_type != JoinType::Cross {
                return Err(SelectError::InvalidSql(format!(
                    "{:?} JOIN requires ON clause",
                    join_type
                )));
            }
            None
        };

        joins.push(JoinClause {
            join_type,
            right_table,
            right_alias,
            on_condition,
        });

        if next_join.is_none() {
            break;
        }
    }

    Ok((joins, remaining.to_string()))
}

/// Find next JOIN keyword position
fn find_next_join_keyword(s: &str) -> Option<usize> {
    let keywords = vec![
        "inner join",
        "left join",
        "right join",
        "full join",
        "cross join",
        " join",
    ];
    let mut min_pos = None;
    for kw in keywords {
        if let Some(pos) = s.find(kw) {
            min_pos = match min_pos {
                None => Some(pos),
                Some(current) => Some(current.min(pos)),
            };
        }
    }
    min_pos
}

/// Parse JOIN ON condition
fn parse_join_condition(cond_str: &str) -> Result<JoinCondition, SelectError> {
    // Simple parser for "table1.col = table2.col"
    let parts: Vec<&str> = cond_str.split_whitespace().collect();
    if parts.len() != 3 {
        return Err(SelectError::InvalidSql(
            "JOIN ON condition must be in format: left_col = right_col".to_string(),
        ));
    }

    let operator = match parts[1] {
        "=" => JoinOperator::Equal,
        "!=" | "<>" => JoinOperator::NotEqual,
        "<" => JoinOperator::LessThan,
        "<=" => JoinOperator::LessThanOrEqual,
        ">" => JoinOperator::GreaterThan,
        ">=" => JoinOperator::GreaterThanOrEqual,
        _ => {
            return Err(SelectError::InvalidSql(format!(
                "Unsupported JOIN operator: {}",
                parts[1]
            )))
        }
    };

    Ok(JoinCondition {
        left_column: parts[0].to_string(),
        right_column: parts[2].to_string(),
        operator,
    })
}

// Helper functions

fn extract_where_clause(sql: &str) -> Result<Option<Condition>, SelectError> {
    let lower = sql.to_lowercase();
    if let Some(where_pos) = lower.find(" where ") {
        let where_end = lower
            .find(" group by ")
            .or(lower.find(" having "))
            .or(lower.find(" order by "))
            .or(lower.find(" limit "))
            .unwrap_or(sql.len());

        let cond_str = sql[where_pos + 7..where_end].trim();
        Ok(Some(super::parser::parse_condition(cond_str)?))
    } else {
        Ok(None)
    }
}

fn extract_group_by(sql: &str) -> Result<Option<Vec<String>>, SelectError> {
    let lower = sql.to_lowercase();
    if let Some(group_pos) = lower.find(" group by ") {
        let group_end = lower
            .find(" having ")
            .or(lower.find(" order by "))
            .or(lower.find(" limit "))
            .unwrap_or(sql.len());

        let group_str = sql[group_pos + 10..group_end].trim();
        Ok(Some(super::parser::parse_group_by(group_str)?))
    } else {
        Ok(None)
    }
}

fn extract_having_clause(sql: &str) -> Result<Option<Condition>, SelectError> {
    let lower = sql.to_lowercase();
    if let Some(having_pos) = lower.find(" having ") {
        let having_end = lower
            .find(" order by ")
            .or(lower.find(" limit "))
            .unwrap_or(sql.len());

        let cond_str = sql[having_pos + 8..having_end].trim();
        Ok(Some(super::parser::parse_condition(cond_str)?))
    } else {
        Ok(None)
    }
}

fn extract_order_by(sql: &str) -> Result<Option<Vec<super::types::OrderByClause>>, SelectError> {
    let lower = sql.to_lowercase();
    if let Some(order_pos) = lower.find(" order by ") {
        let order_end = lower
            .find(" limit ")
            .or(lower.find(" offset "))
            .unwrap_or(sql.len());

        let order_str = sql[order_pos + 10..order_end].trim();
        let order_by = super::parser::parse_order_by(order_str)?;
        Ok(Some(order_by))
    } else {
        Ok(None)
    }
}

fn extract_limit(sql: &str) -> Result<Option<usize>, SelectError> {
    let lower = sql.to_lowercase();
    if let Some(limit_pos) = lower.find(" limit ") {
        let limit_end = lower.find(" offset ").unwrap_or(sql.len());
        let limit_str = sql[limit_pos + 7..limit_end].trim();
        Ok(Some(limit_str.parse::<usize>().map_err(|_| {
            SelectError::InvalidSql(format!("Invalid LIMIT value: {}", limit_str))
        })?))
    } else {
        Ok(None)
    }
}

fn extract_offset(sql: &str) -> Result<Option<usize>, SelectError> {
    let lower = sql.to_lowercase();
    if let Some(offset_pos) = lower.find(" offset ") {
        let offset_str = sql[offset_pos + 8..].trim();
        Ok(Some(offset_str.parse::<usize>().map_err(|_| {
            SelectError::InvalidSql(format!("Invalid OFFSET value: {}", offset_str))
        })?))
    } else {
        Ok(None)
    }
}

fn split_by_top_level_comma(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;

    for ch in s.chars() {
        match ch {
            '(' => {
                paren_depth += 1;
                current.push(ch);
            }
            ')' => {
                paren_depth -= 1;
                current.push(ch);
            }
            ',' if paren_depth == 0 => {
                if !current.trim().is_empty() {
                    parts.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }

    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_inner_join() {
        let sql = "SELECT a.id, b.name FROM table1 a INNER JOIN table2 b ON a.id = b.id WHERE a.status = 'active'";
        let result = parse_advanced_sql(sql);
        assert!(result.is_ok());
        let parsed = result.expect("Failed to parse INNER JOIN SQL query");
        assert_eq!(parsed.joins.len(), 1);
        assert_eq!(parsed.joins[0].join_type, JoinType::Inner);
    }

    #[test]
    fn test_parse_multiple_joins() {
        let sql =
            "SELECT * FROM t1 LEFT JOIN t2 ON t1.id = t2.t1_id RIGHT JOIN t3 ON t2.id = t3.t2_id";
        let result = parse_advanced_sql(sql);
        assert!(result.is_ok());
        let parsed = result.expect("Failed to parse multiple JOINs SQL query");
        assert_eq!(parsed.joins.len(), 2);
        assert_eq!(parsed.joins[0].join_type, JoinType::Left);
        assert_eq!(parsed.joins[1].join_type, JoinType::Right);
    }

    #[test]
    fn test_parse_cte() {
        let sql = "WITH temp AS (SELECT id, name FROM users) SELECT * FROM temp WHERE id > 10";
        let result = parse_advanced_sql(sql);
        assert!(result.is_ok());
        let parsed = result.expect("Failed to parse CTE SQL query");
        assert_eq!(parsed.ctes.len(), 1);
        assert_eq!(parsed.ctes[0].name, "temp");
    }

    #[test]
    fn test_parse_multiple_ctes() {
        let sql = "WITH cte1 AS (SELECT * FROM t1), cte2 AS (SELECT * FROM t2) SELECT * FROM cte1 JOIN cte2 ON cte1.id = cte2.id";
        let result = parse_advanced_sql(sql);
        assert!(result.is_ok());
        let parsed = result.expect("Failed to parse multiple CTEs SQL query");
        assert_eq!(parsed.ctes.len(), 2);
    }
}
