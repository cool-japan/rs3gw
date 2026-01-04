//! ML Model Format Detection and Metadata Extraction
//!
//! This module provides detection and metadata extraction for various machine learning model formats:
//! - PyTorch (.pt, .pth)
//! - TensorFlow (SavedModel)
//! - ONNX (.onnx)
//! - Safetensors (.safetensors) - Hugging Face format
//! - Keras (.h5, .keras)
//!
//! # Features
//!
//! - Automatic format detection via magic bytes and file structure
//! - Metadata extraction (model architecture, parameters, framework version)
//! - Tensor shape and dtype information
//! - Model size and complexity metrics
//! - Integration with S3-compatible API via custom headers
//!
//! # Example
//!
//! ```no_run
//! use rs3gw::storage::ml_models::{detect_ml_model_format, extract_ml_metadata};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let data = std::fs::read("model.pt")?;
//! if let Some(format) = detect_ml_model_format(&data).await {
//!     println!("Detected format: {:?}", format);
//!     if let Some(metadata) = extract_ml_metadata(format, &data).await {
//!         println!("Model metadata: {:?}", metadata);
//!     }
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

/// ML model format types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelFormat {
    /// PyTorch model (.pt, .pth)
    PyTorch,
    /// TensorFlow SavedModel
    TensorFlowSavedModel,
    /// ONNX model (.onnx)
    Onnx,
    /// Safetensors format (Hugging Face)
    Safetensors,
    /// Keras model (.h5, .keras)
    Keras,
}

/// Model metadata extracted from ML model files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetadata {
    /// Model format
    pub format: ModelFormat,
    /// Framework name (e.g., "PyTorch", "TensorFlow")
    pub framework: String,
    /// Framework version if available
    pub framework_version: Option<String>,
    /// Model architecture name if available
    pub architecture: Option<String>,
    /// Total parameter count
    pub parameter_count: Option<u64>,
    /// Model size in bytes
    pub model_size: u64,
    /// Tensor information (name -> shape and dtype)
    pub tensors: HashMap<String, TensorInfo>,
    /// Additional custom metadata
    pub custom_metadata: HashMap<String, String>,
}

/// Tensor information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TensorInfo {
    /// Tensor shape (dimensions)
    pub shape: Vec<i64>,
    /// Data type (e.g., "float32", "int64")
    pub dtype: String,
    /// Number of elements
    pub num_elements: u64,
    /// Size in bytes
    pub size_bytes: u64,
}

impl ModelMetadata {
    /// Create new model metadata
    pub fn new(format: ModelFormat, framework: String, model_size: u64) -> Self {
        Self {
            format,
            framework,
            framework_version: None,
            architecture: None,
            parameter_count: None,
            model_size,
            tensors: HashMap::new(),
            custom_metadata: HashMap::new(),
        }
    }

    /// Add tensor information
    pub fn add_tensor(&mut self, name: String, info: TensorInfo) {
        self.tensors.insert(name, info);
    }

    /// Calculate total parameter count from tensors
    pub fn calculate_parameter_count(&mut self) {
        let total: u64 = self.tensors.values().map(|t| t.num_elements).sum();
        self.parameter_count = Some(total);
    }

    /// Convert metadata to HTTP headers (x-amz-meta-*)
    pub fn to_headers(&self) -> HashMap<String, String> {
        let mut headers = HashMap::new();

        headers.insert(
            "x-amz-meta-ml-format".to_string(),
            format!("{:?}", self.format).to_lowercase(),
        );

        headers.insert(
            "x-amz-meta-ml-framework".to_string(),
            self.framework.clone(),
        );

        if let Some(version) = &self.framework_version {
            headers.insert(
                "x-amz-meta-ml-framework-version".to_string(),
                version.clone(),
            );
        }

        if let Some(arch) = &self.architecture {
            headers.insert("x-amz-meta-ml-architecture".to_string(), arch.clone());
        }

        if let Some(params) = self.parameter_count {
            headers.insert(
                "x-amz-meta-ml-parameter-count".to_string(),
                params.to_string(),
            );
        }

        headers.insert(
            "x-amz-meta-ml-model-size".to_string(),
            self.model_size.to_string(),
        );

        headers.insert(
            "x-amz-meta-ml-tensor-count".to_string(),
            self.tensors.len().to_string(),
        );

        // Add custom metadata
        for (key, value) in &self.custom_metadata {
            headers.insert(
                format!("x-amz-meta-ml-{}", key.to_lowercase()),
                value.clone(),
            );
        }

        headers
    }
}

impl TensorInfo {
    /// Create new tensor info
    pub fn new(shape: Vec<i64>, dtype: String) -> Self {
        let num_elements = shape.iter().map(|&d| d as u64).product();
        let dtype_size = estimate_dtype_size(&dtype);
        let size_bytes = num_elements * dtype_size;

        Self {
            shape,
            dtype,
            num_elements,
            size_bytes,
        }
    }
}

/// Detect ML model format from file data
pub async fn detect_ml_model_format(data: &[u8]) -> Option<ModelFormat> {
    if data.len() < 16 {
        return None;
    }

    // PyTorch magic bytes (ZIP archive with specific structure)
    // PyTorch models are saved as ZIP files with pickle data
    if data.starts_with(b"PK\x03\x04") {
        // Check for PyTorch-specific markers
        if contains_pytorch_markers(data) {
            debug!("Detected PyTorch model format");
            return Some(ModelFormat::PyTorch);
        }

        // Check for Keras .keras format (also ZIP)
        if contains_keras_markers(data) {
            debug!("Detected Keras model format");
            return Some(ModelFormat::Keras);
        }
    }

    // ONNX magic bytes (Protocol Buffers)
    if data.starts_with(b"\x08") || contains_onnx_signature(data) {
        debug!("Detected ONNX model format");
        return Some(ModelFormat::Onnx);
    }

    // Safetensors magic bytes (custom binary format)
    if is_safetensors_format(data) {
        debug!("Detected Safetensors model format");
        return Some(ModelFormat::Safetensors);
    }

    // HDF5 format (used by Keras .h5 files)
    if data.starts_with(b"\x89HDF\r\n\x1a\n") {
        debug!("Detected Keras HDF5 model format");
        return Some(ModelFormat::Keras);
    }

    // TensorFlow SavedModel (directory structure with saved_model.pb)
    // This is harder to detect from a single file, but we can check for protobuf signature
    if is_tensorflow_savedmodel(data) {
        debug!("Detected TensorFlow SavedModel format");
        return Some(ModelFormat::TensorFlowSavedModel);
    }

    None
}

/// Extract metadata from ML model file
pub async fn extract_ml_metadata(format: ModelFormat, data: &[u8]) -> Option<ModelMetadata> {
    match format {
        ModelFormat::PyTorch => extract_pytorch_metadata(data).await,
        ModelFormat::TensorFlowSavedModel => extract_tensorflow_metadata(data).await,
        ModelFormat::Onnx => extract_onnx_metadata(data).await,
        ModelFormat::Safetensors => extract_safetensors_metadata(data).await,
        ModelFormat::Keras => extract_keras_metadata(data).await,
    }
}

/// Extract PyTorch model metadata
async fn extract_pytorch_metadata(data: &[u8]) -> Option<ModelMetadata> {
    let mut metadata = ModelMetadata::new(
        ModelFormat::PyTorch,
        "PyTorch".to_string(),
        data.len() as u64,
    );

    // PyTorch models are ZIP archives containing pickled tensors
    // For now, we provide basic detection and size information
    // Full extraction would require ZIP parsing and pickle deserialization

    metadata
        .custom_metadata
        .insert("storage_format".to_string(), "torch_zip".to_string());

    // Estimate based on common patterns
    // Real implementation would parse the ZIP and pickle data
    debug!("Extracted PyTorch metadata (basic)");

    Some(metadata)
}

/// Extract TensorFlow SavedModel metadata
async fn extract_tensorflow_metadata(data: &[u8]) -> Option<ModelMetadata> {
    let mut metadata = ModelMetadata::new(
        ModelFormat::TensorFlowSavedModel,
        "TensorFlow".to_string(),
        data.len() as u64,
    );

    // TensorFlow SavedModel uses Protocol Buffers
    // Would require protobuf parsing for full metadata extraction

    metadata
        .custom_metadata
        .insert("storage_format".to_string(), "saved_model_pb".to_string());

    debug!("Extracted TensorFlow metadata (basic)");

    Some(metadata)
}

/// Extract ONNX model metadata
async fn extract_onnx_metadata(data: &[u8]) -> Option<ModelMetadata> {
    let mut metadata = ModelMetadata::new(ModelFormat::Onnx, "ONNX".to_string(), data.len() as u64);

    // ONNX uses Protocol Buffers with a defined schema
    // Full parsing would extract graph structure, operators, and tensor shapes

    metadata
        .custom_metadata
        .insert("storage_format".to_string(), "onnx_protobuf".to_string());

    debug!("Extracted ONNX metadata (basic)");

    Some(metadata)
}

/// Extract Safetensors metadata
async fn extract_safetensors_metadata(data: &[u8]) -> Option<ModelMetadata> {
    if data.len() < 8 {
        return None;
    }

    let mut metadata = ModelMetadata::new(
        ModelFormat::Safetensors,
        "Safetensors".to_string(),
        data.len() as u64,
    );

    // Safetensors format:
    // - First 8 bytes: u64 (little-endian) - header length
    // - Next N bytes: JSON header with tensor metadata
    // - Remaining: tensor data

    let header_len = u64::from_le_bytes(data[0..8].try_into().ok()?);

    if data.len() < (8 + header_len as usize) {
        warn!("Safetensors file too small for header");
        return Some(metadata);
    }

    let header_bytes = &data[8..(8 + header_len as usize)];
    let header_str = std::str::from_utf8(header_bytes).ok()?;

    // Parse JSON header
    if let Ok(header) = serde_json::from_str::<serde_json::Value>(header_str) {
        if let Some(obj) = header.as_object() {
            for (name, tensor_info) in obj {
                if name == "__metadata__" {
                    // Extract custom metadata
                    if let Some(meta_obj) = tensor_info.as_object() {
                        for (k, v) in meta_obj {
                            if let Some(s) = v.as_str() {
                                metadata.custom_metadata.insert(k.clone(), s.to_string());
                            }
                        }
                    }
                    continue;
                }

                // Extract tensor information
                if let Some(tensor_obj) = tensor_info.as_object() {
                    let dtype = tensor_obj
                        .get("dtype")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();

                    let shape: Vec<i64> = tensor_obj
                        .get("shape")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
                        .unwrap_or_default();

                    let tensor_info = TensorInfo::new(shape, dtype);
                    metadata.add_tensor(name.clone(), tensor_info);
                }
            }
        }
    }

    metadata.calculate_parameter_count();

    debug!(
        "Extracted Safetensors metadata: {} tensors",
        metadata.tensors.len()
    );

    Some(metadata)
}

/// Extract Keras model metadata
async fn extract_keras_metadata(data: &[u8]) -> Option<ModelMetadata> {
    let mut metadata =
        ModelMetadata::new(ModelFormat::Keras, "Keras".to_string(), data.len() as u64);

    // Keras models can be HDF5 (.h5) or ZIP (.keras)
    if data.starts_with(b"\x89HDF\r\n\x1a\n") {
        metadata
            .custom_metadata
            .insert("storage_format".to_string(), "hdf5".to_string());
    } else if data.starts_with(b"PK\x03\x04") {
        metadata
            .custom_metadata
            .insert("storage_format".to_string(), "keras_zip".to_string());
    }

    debug!("Extracted Keras metadata (basic)");

    Some(metadata)
}

// Helper functions for format detection

fn contains_pytorch_markers(data: &[u8]) -> bool {
    // Look for PyTorch-specific markers in ZIP content
    // PyTorch models typically contain files like "data.pkl", "version", etc.
    let markers: &[&[u8]] = &[b"data.pkl", b"version", b"constants.pkl"];

    markers
        .iter()
        .any(|&marker| data.windows(marker.len()).any(|window| window == marker))
}

fn contains_keras_markers(data: &[u8]) -> bool {
    // Keras .keras format contains config.json and weights
    let markers: &[&[u8]] = &[b"config.json", b"keras_version"];

    markers
        .iter()
        .any(|&marker| data.windows(marker.len()).any(|window| window == marker))
}

fn contains_onnx_signature(data: &[u8]) -> bool {
    // ONNX files typically start with protobuf field tags
    // Look for common ONNX model structure indicators
    if data.len() < 100 {
        return false;
    }

    // Check for "ir_version", "graph", "opset_import" which are common in ONNX
    let markers: &[&[u8]] = &[b"ir_version", b"graph", b"opset_import"];

    markers.iter().any(|&marker| {
        data.windows(marker.len())
            .take(500) // Check first 500 bytes
            .any(|window| window == marker)
    })
}

fn is_safetensors_format(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }

    // Check if first 8 bytes form a reasonable header length
    let header_len = u64::from_le_bytes(match data[0..8].try_into() {
        Ok(bytes) => bytes,
        Err(_) => return false,
    });

    // Header should be reasonable size (< 1MB for metadata)
    if header_len == 0 || header_len > 1_000_000 {
        return false;
    }

    // Check if we have enough data for the header
    if data.len() < (8 + header_len as usize) {
        return false;
    }

    // Try to parse header as JSON
    let header_bytes = &data[8..(8 + header_len as usize)];
    if let Ok(header_str) = std::str::from_utf8(header_bytes) {
        serde_json::from_str::<serde_json::Value>(header_str).is_ok()
    } else {
        false
    }
}

fn is_tensorflow_savedmodel(data: &[u8]) -> bool {
    // TensorFlow SavedModel typically contains protobuf with specific signatures
    // Look for "tensorflow", "saved_model_schema_version", or "MetaGraphDef"
    if data.len() < 100 {
        return false;
    }

    let markers: &[&[u8]] = &[b"tensorflow", b"saved_model", b"MetaGraphDef"];

    markers.iter().any(|&marker| {
        data.windows(marker.len())
            .take(1000) // Check first 1000 bytes
            .any(|window| window == marker)
    })
}

fn estimate_dtype_size(dtype: &str) -> u64 {
    match dtype.to_lowercase().as_str() {
        "float16" | "half" | "fp16" | "f16" => 2,
        "float32" | "float" | "fp32" | "f32" => 4,
        "float64" | "double" | "fp64" | "f64" => 8,
        "int8" | "i8" | "uint8" | "u8" | "bool" => 1,
        "int16" | "i16" | "uint16" | "u16" => 2,
        "int32" | "i32" | "uint32" | "u32" => 4,
        "int64" | "i64" | "uint64" | "u64" => 8,
        "bfloat16" | "bf16" => 2,
        _ => 4, // Default to 4 bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_safetensors_detection() {
        // Create a minimal Safetensors file
        let header = r#"{"weight": {"dtype": "F32", "shape": [2, 3], "data_offsets": [0, 24]}}"#;
        let header_bytes = header.as_bytes();
        let header_len = header_bytes.len() as u64;

        let mut data = Vec::new();
        data.extend_from_slice(&header_len.to_le_bytes());
        data.extend_from_slice(header_bytes);
        data.extend_from_slice(&[0u8; 24]); // Dummy tensor data

        let format = detect_ml_model_format(&data).await;
        assert_eq!(format, Some(ModelFormat::Safetensors));
    }

    #[tokio::test]
    async fn test_safetensors_metadata_extraction() {
        // Create a Safetensors file with metadata
        let header = r#"{
            "__metadata__": {"framework": "transformers", "model_type": "bert"},
            "embeddings.weight": {"dtype": "F32", "shape": [30522, 768], "data_offsets": [0, 93863424]},
            "encoder.layer.0.attention.self.query.weight": {"dtype": "F32", "shape": [768, 768], "data_offsets": [93863424, 96227328]}
        }"#;
        let header_bytes = header.as_bytes();
        let header_len = header_bytes.len() as u64;

        let mut data = Vec::new();
        data.extend_from_slice(&header_len.to_le_bytes());
        data.extend_from_slice(header_bytes);
        data.extend_from_slice(&[0u8; 1000]); // Dummy tensor data

        let metadata = extract_safetensors_metadata(&data)
            .await
            .expect("Failed to extract metadata");

        assert_eq!(metadata.format, ModelFormat::Safetensors);
        assert_eq!(metadata.framework, "Safetensors");
        assert_eq!(metadata.tensors.len(), 2);
        assert!(metadata.custom_metadata.contains_key("framework"));
        assert_eq!(
            metadata.custom_metadata.get("framework"),
            Some(&"transformers".to_string())
        );
        assert!(metadata.parameter_count.is_some());
    }

    #[test]
    fn test_tensor_info_creation() {
        let tensor = TensorInfo::new(vec![768, 768], "F32".to_string());
        assert_eq!(tensor.num_elements, 768 * 768);
        assert_eq!(tensor.size_bytes, 768 * 768 * 4); // 4 bytes for F32
        assert_eq!(tensor.dtype, "F32");
    }

    #[test]
    fn test_metadata_to_headers() {
        let mut metadata = ModelMetadata::new(ModelFormat::PyTorch, "PyTorch".to_string(), 1024000);
        metadata.framework_version = Some("2.0.0".to_string());
        metadata.architecture = Some("ResNet50".to_string());
        metadata.parameter_count = Some(25_000_000);

        let headers = metadata.to_headers();

        assert_eq!(
            headers.get("x-amz-meta-ml-format"),
            Some(&"pytorch".to_string())
        );
        assert_eq!(
            headers.get("x-amz-meta-ml-framework"),
            Some(&"PyTorch".to_string())
        );
        assert_eq!(
            headers.get("x-amz-meta-ml-framework-version"),
            Some(&"2.0.0".to_string())
        );
        assert_eq!(
            headers.get("x-amz-meta-ml-architecture"),
            Some(&"ResNet50".to_string())
        );
        assert_eq!(
            headers.get("x-amz-meta-ml-parameter-count"),
            Some(&"25000000".to_string())
        );
    }

    #[test]
    fn test_dtype_size_estimation() {
        assert_eq!(estimate_dtype_size("float32"), 4);
        assert_eq!(estimate_dtype_size("float64"), 8);
        assert_eq!(estimate_dtype_size("int8"), 1);
        assert_eq!(estimate_dtype_size("int64"), 8);
        assert_eq!(estimate_dtype_size("float16"), 2);
        assert_eq!(estimate_dtype_size("bfloat16"), 2);
    }

    #[tokio::test]
    async fn test_pytorch_detection() {
        // PyTorch models are ZIP files with specific markers
        let mut data = Vec::new();
        data.extend_from_slice(b"PK\x03\x04"); // ZIP signature
        data.extend_from_slice(&[0u8; 100]);
        data.extend_from_slice(b"data.pkl"); // PyTorch marker
        data.extend_from_slice(&[0u8; 100]);

        let format = detect_ml_model_format(&data).await;
        assert_eq!(format, Some(ModelFormat::PyTorch));
    }

    #[tokio::test]
    async fn test_keras_hdf5_detection() {
        let mut data = Vec::new();
        data.extend_from_slice(b"\x89HDF\r\n\x1a\n"); // HDF5 signature
        data.extend_from_slice(&[0u8; 100]);

        let format = detect_ml_model_format(&data).await;
        assert_eq!(format, Some(ModelFormat::Keras));
    }

    #[tokio::test]
    async fn test_onnx_detection() {
        let mut data = Vec::new();
        data.extend_from_slice(&[0x08]); // Protobuf start
        data.extend_from_slice(&[0u8; 50]);
        data.extend_from_slice(b"ir_version"); // ONNX marker
        data.extend_from_slice(&[0u8; 100]);

        let format = detect_ml_model_format(&data).await;
        assert_eq!(format, Some(ModelFormat::Onnx));
    }

    #[tokio::test]
    async fn test_parameter_count_calculation() {
        let mut metadata =
            ModelMetadata::new(ModelFormat::Safetensors, "Safetensors".to_string(), 1000);

        metadata.add_tensor(
            "layer1".to_string(),
            TensorInfo::new(vec![100, 50], "F32".to_string()),
        );
        metadata.add_tensor(
            "layer2".to_string(),
            TensorInfo::new(vec![50, 25], "F32".to_string()),
        );

        metadata.calculate_parameter_count();

        assert_eq!(metadata.parameter_count, Some(100 * 50 + 50 * 25));
    }
}
