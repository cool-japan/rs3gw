//! Text tokenization configuration.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for text tokenization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenizationConfig {
    /// Tokenizer type
    pub tokenizer_type: String, // "whitespace", "wordpiece", "bpe", "sentencepiece"
    /// Vocabulary file path
    pub vocab_path: Option<PathBuf>,
    /// Maximum sequence length
    pub max_length: Option<usize>,
    /// Padding strategy
    pub padding: bool,
    /// Truncation strategy
    pub truncation: bool,
    /// Whether to lowercase
    pub lowercase: bool,
}

impl Default for TokenizationConfig {
    fn default() -> Self {
        Self {
            tokenizer_type: "whitespace".to_string(),
            vocab_path: None,
            max_length: Some(512),
            padding: true,
            truncation: true,
            lowercase: true,
        }
    }
}
