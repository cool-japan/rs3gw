//! Preprocessing error types.

use thiserror::Error;

#[derive(Error, Debug)]
pub enum PreprocessingError {
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Preprocessing step failed: {0}")]
    StepFailed(String),

    #[error("Pipeline not found: {0}")]
    PipelineNotFound(String),

    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type PreprocessingResult<T> = Result<T, PreprocessingError>;
