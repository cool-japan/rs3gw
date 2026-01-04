//! Object Lambda transformations
//!
//! Provides transformation capabilities for objects retrieved from S3.
//! Allows modifying object data on-the-fly before returning to clients.

use bytes::{Bytes, BytesMut};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Object Lambda transformation errors
#[derive(Debug, Error)]
pub enum TransformError {
    #[error("Transformation failed: {0}")]
    Failed(String),
    #[error("Invalid transformation configuration: {0}")]
    InvalidConfig(String),
    #[error("Transformation not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Transformation context containing metadata about the request
#[derive(Debug, Clone)]
pub struct TransformContext {
    /// Object key
    pub key: String,
    /// Bucket name
    pub bucket: String,
    /// Content type
    pub content_type: String,
    /// Request headers
    pub headers: HashMap<String, String>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

/// Transformation result
#[derive(Debug)]
pub struct TransformResult {
    /// Transformed data
    pub data: Bytes,
    /// New content type (if changed)
    pub content_type: Option<String>,
    /// Additional metadata to add
    pub metadata: HashMap<String, String>,
}

impl TransformResult {
    /// Create a simple result with just data
    pub fn new(data: Bytes) -> Self {
        Self {
            data,
            content_type: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a result with new content type
    pub fn with_content_type(data: Bytes, content_type: String) -> Self {
        Self {
            data,
            content_type: Some(content_type),
            metadata: HashMap::new(),
        }
    }
}

/// Transformation function type
pub type TransformFn =
    Arc<dyn Fn(&Bytes, &TransformContext) -> Result<TransformResult, TransformError> + Send + Sync>;

/// Object Lambda configuration for a specific access point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectLambdaConfig {
    /// Configuration name
    pub name: String,
    /// Supporting access point ARN
    pub supporting_access_point_arn: String,
    /// List of transformation IDs to apply
    pub transformation_ids: Vec<String>,
    /// Whether transformations are enabled
    pub enabled: bool,
}

/// Built-in transformation types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LambdaTransformationType {
    /// Redact sensitive data (PII masking)
    Redact { patterns: Vec<String> },
    /// Convert to uppercase
    ToUpperCase,
    /// Convert to lowercase
    ToLowerCase,
    /// Add prefix to content
    AddPrefix { prefix: String },
    /// Add suffix to content
    AddSuffix { suffix: String },
    /// Replace text
    Replace { from: String, to: String },
    /// Compress data
    Compress { algorithm: String },
    /// Decompress data
    Decompress { algorithm: String },
    /// JSON transformation
    JsonTransform { path: String, operation: String },
}

/// Transformation registry
pub struct TransformRegistry {
    /// Registered transformations
    transformations: HashMap<String, TransformFn>,
    /// Object Lambda configurations
    configs: HashMap<String, ObjectLambdaConfig>,
}

impl TransformRegistry {
    /// Create a new transformation registry
    pub fn new() -> Self {
        let mut registry = Self {
            transformations: HashMap::new(),
            configs: HashMap::new(),
        };

        // Register built-in transformations
        registry.register_builtin_transformations();
        registry
    }

    /// Register a transformation function
    pub fn register<F>(&mut self, id: String, transform: F)
    where
        F: Fn(&Bytes, &TransformContext) -> Result<TransformResult, TransformError>
            + Send
            + Sync
            + 'static,
    {
        self.transformations.insert(id, Arc::new(transform));
    }

    /// Register built-in transformations
    fn register_builtin_transformations(&mut self) {
        // To uppercase transformation
        self.register("to_uppercase".to_string(), |data, _ctx| {
            let text = String::from_utf8_lossy(data);
            Ok(TransformResult::new(Bytes::from(text.to_uppercase())))
        });

        // To lowercase transformation
        self.register("to_lowercase".to_string(), |data, _ctx| {
            let text = String::from_utf8_lossy(data);
            Ok(TransformResult::new(Bytes::from(text.to_lowercase())))
        });

        // Add prefix transformation
        self.register("add_prefix".to_string(), |data, ctx| {
            let prefix = ctx.metadata.get("prefix").map(|s| s.as_str()).unwrap_or("");
            let mut result = BytesMut::with_capacity(prefix.len() + data.len());
            result.extend_from_slice(prefix.as_bytes());
            result.extend_from_slice(data);
            Ok(TransformResult::new(result.freeze()))
        });

        // Add suffix transformation
        self.register("add_suffix".to_string(), |data, ctx| {
            let suffix = ctx.metadata.get("suffix").map(|s| s.as_str()).unwrap_or("");
            let mut result = BytesMut::with_capacity(data.len() + suffix.len());
            result.extend_from_slice(data);
            result.extend_from_slice(suffix.as_bytes());
            Ok(TransformResult::new(result.freeze()))
        });

        // Redact email addresses
        self.register("redact_emails".to_string(), |data, _ctx| {
            let text = String::from_utf8_lossy(data);
            let email_pattern =
                regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b")
                    .map_err(|e| TransformError::Failed(e.to_string()))?;
            let redacted = email_pattern.replace_all(&text, "[REDACTED-EMAIL]");
            Ok(TransformResult::new(Bytes::from(redacted.to_string())))
        });

        // Redact phone numbers
        self.register("redact_phones".to_string(), |data, _ctx| {
            let text = String::from_utf8_lossy(data);
            let phone_pattern = regex::Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b")
                .map_err(|e| TransformError::Failed(e.to_string()))?;
            let redacted = phone_pattern.replace_all(&text, "[REDACTED-PHONE]");
            Ok(TransformResult::new(Bytes::from(redacted.to_string())))
        });

        // Redact credit card numbers
        self.register("redact_creditcards".to_string(), |data, _ctx| {
            let text = String::from_utf8_lossy(data);
            let cc_pattern = regex::Regex::new(r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b")
                .map_err(|e| TransformError::Failed(e.to_string()))?;
            let redacted = cc_pattern.replace_all(&text, "[REDACTED-CC]");
            Ok(TransformResult::new(Bytes::from(redacted.to_string())))
        });

        // JSON prettify
        self.register("json_prettify".to_string(), |data, _ctx| {
            let value: serde_json::Value = serde_json::from_slice(data)
                .map_err(|e| TransformError::Failed(format!("Invalid JSON: {}", e)))?;
            let pretty = serde_json::to_string_pretty(&value)
                .map_err(|e| TransformError::Failed(e.to_string()))?;
            Ok(TransformResult::with_content_type(
                Bytes::from(pretty),
                "application/json".to_string(),
            ))
        });

        // JSON minify
        self.register("json_minify".to_string(), |data, _ctx| {
            let value: serde_json::Value = serde_json::from_slice(data)
                .map_err(|e| TransformError::Failed(format!("Invalid JSON: {}", e)))?;
            let minified =
                serde_json::to_string(&value).map_err(|e| TransformError::Failed(e.to_string()))?;
            Ok(TransformResult::with_content_type(
                Bytes::from(minified),
                "application/json".to_string(),
            ))
        });
    }

    /// Get a transformation by ID
    pub fn get(&self, id: &str) -> Option<&TransformFn> {
        self.transformations.get(id)
    }

    /// Apply a single transformation
    pub fn apply(
        &self,
        id: &str,
        data: &Bytes,
        context: &TransformContext,
    ) -> Result<TransformResult, TransformError> {
        let transform = self
            .get(id)
            .ok_or_else(|| TransformError::NotFound(id.to_string()))?;
        transform(data, context)
    }

    /// Apply multiple transformations in sequence
    pub fn apply_chain(
        &self,
        ids: &[String],
        mut data: Bytes,
        context: &mut TransformContext,
    ) -> Result<TransformResult, TransformError> {
        let mut final_content_type = None;
        let mut final_metadata = HashMap::new();

        for id in ids {
            let result = self.apply(id, &data, context)?;
            data = result.data;

            if let Some(ct) = result.content_type {
                final_content_type = Some(ct.clone());
                context.content_type = ct;
            }

            final_metadata.extend(result.metadata);
        }

        Ok(TransformResult {
            data,
            content_type: final_content_type,
            metadata: final_metadata,
        })
    }

    /// Register an Object Lambda configuration
    pub fn register_config(&mut self, config: ObjectLambdaConfig) {
        self.configs.insert(config.name.clone(), config);
    }

    /// Get an Object Lambda configuration
    pub fn get_config(&self, name: &str) -> Option<&ObjectLambdaConfig> {
        self.configs.get(name)
    }

    /// Apply transformations based on a configuration
    pub fn apply_config(
        &self,
        config_name: &str,
        data: Bytes,
        context: &mut TransformContext,
    ) -> Result<TransformResult, TransformError> {
        let config = self
            .get_config(config_name)
            .ok_or_else(|| TransformError::NotFound(format!("Config: {}", config_name)))?;

        if !config.enabled {
            return Ok(TransformResult::new(data));
        }

        self.apply_chain(&config.transformation_ids, data, context)
    }

    /// List all registered transformation IDs
    pub fn list_transformations(&self) -> Vec<String> {
        self.transformations.keys().cloned().collect()
    }

    /// List all registered configurations
    pub fn list_configs(&self) -> Vec<String> {
        self.configs.keys().cloned().collect()
    }
}

impl Default for TransformRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context() -> TransformContext {
        TransformContext {
            key: "test.txt".to_string(),
            bucket: "test-bucket".to_string(),
            content_type: "text/plain".to_string(),
            headers: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_uppercase_transformation() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("hello world");
        let ctx = test_context();

        let result = registry
            .apply("to_uppercase", &data, &ctx)
            .expect("Failed to apply uppercase transformation");
        assert_eq!(result.data, Bytes::from("HELLO WORLD"));
    }

    #[test]
    fn test_lowercase_transformation() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("HELLO WORLD");
        let ctx = test_context();

        let result = registry
            .apply("to_lowercase", &data, &ctx)
            .expect("Failed to apply lowercase transformation");
        assert_eq!(result.data, Bytes::from("hello world"));
    }

    #[test]
    fn test_redact_emails() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("Contact us at support@example.com or admin@test.org");
        let ctx = test_context();

        let result = registry
            .apply("redact_emails", &data, &ctx)
            .expect("Failed to apply redact_emails transformation");
        let output = String::from_utf8(result.data.to_vec()).expect("Failed to convert to UTF8");
        assert!(output.contains("[REDACTED-EMAIL]"));
        assert!(!output.contains("support@example.com"));
        assert!(!output.contains("admin@test.org"));
    }

    #[test]
    fn test_redact_phones() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("Call 555-123-4567 or 555.987.6543");
        let ctx = test_context();

        let result = registry
            .apply("redact_phones", &data, &ctx)
            .expect("Failed to apply redact_phones transformation");
        let output = String::from_utf8(result.data.to_vec()).expect("Failed to convert to UTF8");
        assert!(output.contains("[REDACTED-PHONE]"));
        assert!(!output.contains("555-123-4567"));
    }

    #[test]
    fn test_redact_creditcards() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("Card: 1234-5678-9012-3456");
        let ctx = test_context();

        let result = registry
            .apply("redact_creditcards", &data, &ctx)
            .expect("Failed to apply redact_creditcards transformation");
        let output = String::from_utf8(result.data.to_vec()).expect("Failed to convert to UTF8");
        assert!(output.contains("[REDACTED-CC]"));
        assert!(!output.contains("1234-5678-9012-3456"));
    }

    #[test]
    fn test_json_prettify() {
        let registry = TransformRegistry::new();
        let data = Bytes::from(r#"{"name":"test","value":123}"#);
        let ctx = test_context();

        let result = registry
            .apply("json_prettify", &data, &ctx)
            .expect("Failed to apply json_prettify transformation");
        let output = String::from_utf8(result.data.to_vec()).expect("Failed to convert to UTF8");
        assert!(output.contains('\n')); // Should have newlines
        assert!(output.contains("  ")); // Should have indentation
    }

    #[test]
    fn test_transformation_chain() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("hello world");
        let mut ctx = test_context();

        let transformations = vec!["to_uppercase".to_string()];
        let result = registry
            .apply_chain(&transformations, data, &mut ctx)
            .expect("Failed to apply transformation chain");
        assert_eq!(result.data, Bytes::from("HELLO WORLD"));
    }

    #[test]
    fn test_custom_transformation() {
        let mut registry = TransformRegistry::new();

        // Register a custom transformation
        registry.register("reverse".to_string(), |data, _ctx| {
            let text = String::from_utf8_lossy(data);
            let reversed: String = text.chars().rev().collect();
            Ok(TransformResult::new(Bytes::from(reversed)))
        });

        let data = Bytes::from("hello");
        let ctx = test_context();

        let result = registry
            .apply("reverse", &data, &ctx)
            .expect("Failed to apply custom transformation");
        assert_eq!(result.data, Bytes::from("olleh"));
    }

    #[test]
    fn test_object_lambda_config() {
        let mut registry = TransformRegistry::new();

        let config = ObjectLambdaConfig {
            name: "pii-redaction".to_string(),
            supporting_access_point_arn:
                "arn:aws:s3:us-east-1:123456789012:accesspoint/my-access-point".to_string(),
            transformation_ids: vec!["redact_emails".to_string(), "redact_phones".to_string()],
            enabled: true,
        };

        registry.register_config(config);

        let data = Bytes::from("Contact: john@example.com, Phone: 555-123-4567");
        let mut ctx = test_context();

        let result = registry
            .apply_config("pii-redaction", data, &mut ctx)
            .expect("Failed to apply config");
        let output = String::from_utf8(result.data.to_vec()).expect("Failed to convert to UTF8");

        assert!(output.contains("[REDACTED-EMAIL]"));
        assert!(output.contains("[REDACTED-PHONE]"));
    }

    #[test]
    fn test_add_prefix_transformation() {
        let registry = TransformRegistry::new();
        let data = Bytes::from("world");
        let mut ctx = test_context();
        ctx.metadata
            .insert("prefix".to_string(), "Hello, ".to_string());

        let result = registry
            .apply("add_prefix", &data, &ctx)
            .expect("Failed to apply add_prefix transformation");
        assert_eq!(result.data, Bytes::from("Hello, world"));
    }

    #[test]
    fn test_disabled_config() {
        let mut registry = TransformRegistry::new();

        let config = ObjectLambdaConfig {
            name: "disabled-transform".to_string(),
            supporting_access_point_arn:
                "arn:aws:s3:us-east-1:123456789012:accesspoint/my-access-point".to_string(),
            transformation_ids: vec!["to_uppercase".to_string()],
            enabled: false,
        };

        registry.register_config(config);

        let data = Bytes::from("hello");
        let mut ctx = test_context();

        let result = registry
            .apply_config("disabled-transform", data.clone(), &mut ctx)
            .expect("Failed to apply disabled config");
        assert_eq!(result.data, data); // Should return unchanged
    }
}
