//! # CsvOutput - Trait Implementations
//!
//! This module contains trait implementations for `CsvOutput`.
//!
//! ## Implemented Traits
//!
//! - `Default`
//!
//! 🤖 Generated with [SplitRS](https://github.com/cool-japan/splitrs)

use super::types::{CsvOutput, QuoteFields};

impl Default for CsvOutput {
    fn default() -> Self {
        Self {
            field_delimiter: ',',
            record_delimiter: "\n".to_string(),
            quote_character: '"',
            quote_escape_character: '"',
            quote_fields: QuoteFields::AsNeeded,
        }
    }
}
