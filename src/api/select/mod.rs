//! S3 Select API implementation
//!
//! This module provides SQL-like query capability for CSV, JSON, and Parquet objects.
//! Supports SELECT, FROM, WHERE, GROUP BY, ORDER BY, LIMIT, aggregate functions,
//! JOINs, window functions, and Common Table Expressions (CTEs).

#[cfg(feature = "formats")]
pub mod csvoutput_traits;
#[cfg(feature = "formats")]
pub mod fieldvalue_traits;
#[cfg(feature = "formats")]
pub mod outputformat_traits;
#[cfg(feature = "formats")]
pub mod parser;
#[cfg(feature = "formats")]
pub mod types;

// Advanced SQL features
#[cfg(feature = "formats")]
pub mod advanced_sql;
#[cfg(feature = "formats")]
pub mod window_functions;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod refactoring_tests;

// Re-export all public types and functions
#[cfg(feature = "formats")]
pub use advanced_sql::*;
#[cfg(feature = "formats")]
pub use parser::*;
#[cfg(feature = "formats")]
pub use types::*;
#[cfg(feature = "formats")]
pub use window_functions::*;
