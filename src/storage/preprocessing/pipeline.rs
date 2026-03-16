//! Preprocessing pipeline definition and builder.

use super::error::{PreprocessingError, PreprocessingResult};
use super::image::PreprocessingStepType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single preprocessing step in a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessingStep {
    /// Unique identifier for this step
    pub id: String,
    /// Step type
    pub step_type: PreprocessingStepType,
    /// Configuration (JSON-serialized)
    pub config: serde_json::Value,
    /// Whether to cache results of this step
    pub cache_results: bool,
    /// Step description
    pub description: Option<String>,
}

impl PreprocessingStep {
    pub fn new(id: String, step_type: PreprocessingStepType) -> Self {
        Self {
            id,
            step_type,
            config: serde_json::Value::Null,
            cache_results: false,
            description: None,
        }
    }

    pub fn with_config<T: Serialize>(mut self, config: T) -> PreprocessingResult<Self> {
        self.config = serde_json::to_value(config)
            .map_err(|e| PreprocessingError::SerializationError(e.to_string()))?;
        Ok(self)
    }

    pub fn with_cache(mut self, cache: bool) -> Self {
        self.cache_results = cache;
        self
    }

    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }
}

/// Preprocessing pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineDefinition {
    /// Unique pipeline identifier
    pub id: String,
    /// Pipeline name
    pub name: String,
    /// Pipeline version
    pub version: String,
    /// Pipeline description
    pub description: Option<String>,
    /// List of preprocessing steps (executed in order)
    pub steps: Vec<PreprocessingStep>,
    /// Pipeline metadata
    pub metadata: HashMap<String, String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last modified timestamp
    pub modified_at: DateTime<Utc>,
}

impl PipelineDefinition {
    pub fn new(id: String, name: String, version: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            name,
            version,
            description: None,
            steps: Vec::new(),
            metadata: HashMap::new(),
            created_at: now,
            modified_at: now,
        }
    }

    pub fn add_step(&mut self, step: PreprocessingStep) {
        self.steps.push(step);
        self.modified_at = Utc::now();
    }

    pub fn with_description(mut self, desc: String) -> Self {
        self.description = Some(desc);
        self
    }

    pub fn add_metadata(&mut self, key: String, value: String) {
        self.metadata.insert(key, value);
        self.modified_at = Utc::now();
    }
}

/// Builder for preprocessing pipelines
pub struct PipelineBuilder {
    definition: PipelineDefinition,
}

impl PipelineBuilder {
    pub fn new(id: String, name: String) -> Self {
        Self {
            definition: PipelineDefinition::new(id, name, "1.0.0".to_string()),
        }
    }

    pub fn version(mut self, version: String) -> Self {
        self.definition.version = version;
        self
    }

    pub fn description(mut self, desc: String) -> Self {
        self.definition.description = Some(desc);
        self
    }

    pub fn add_step(mut self, step: PreprocessingStep) -> Self {
        self.definition.add_step(step);
        self
    }

    pub fn metadata(mut self, key: String, value: String) -> Self {
        self.definition.add_metadata(key, value);
        self
    }

    pub fn build(self) -> PipelineDefinition {
        self.definition
    }
}
