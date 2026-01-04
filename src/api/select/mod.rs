//! S3 Select API implementation
//!
//! This module provides SQL-like query capability for CSV, JSON, and Parquet objects.
//! Supports SELECT, FROM, WHERE, GROUP BY, ORDER BY, LIMIT, aggregate functions,
//! JOINs, window functions, and Common Table Expressions (CTEs).

pub mod csvoutput_traits;
pub mod fieldvalue_traits;
pub mod outputformat_traits;
pub mod parser;
pub mod types;

// Advanced SQL features
pub mod advanced_sql;
pub mod window_functions;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod refactoring_tests;

// Re-export all public types and functions
pub use advanced_sql::*;
pub use parser::*;
pub use types::*;
pub use window_functions::*;
