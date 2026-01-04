//! WASM plugin transformation implementation

use super::image::Transformer;
use super::types::*;
use async_trait::async_trait;

#[cfg(feature = "wasm-plugins")]
use bytes::Bytes;
#[cfg(feature = "wasm-plugins")]
use std::collections::HashMap;
#[cfg(feature = "wasm-plugins")]
use std::sync::Arc;
#[cfg(feature = "wasm-plugins")]
use tokio::sync::RwLock;
#[cfg(feature = "wasm-plugins")]
use wasmtime::*;

/// WASM plugin transformer
pub struct WasmPluginTransformer {
    #[cfg(feature = "wasm-plugins")]
    engine: Engine,
    #[cfg(feature = "wasm-plugins")]
    plugins: Arc<RwLock<HashMap<String, Module>>>,
}

impl WasmPluginTransformer {
    pub fn new() -> Self {
        #[cfg(feature = "wasm-plugins")]
        {
            Self {
                engine: Engine::default(),
                plugins: Arc::new(RwLock::new(HashMap::new())),
            }
        }

        #[cfg(not(feature = "wasm-plugins"))]
        {
            Self {}
        }
    }

    /// Register a WASM plugin
    #[cfg(feature = "wasm-plugins")]
    pub async fn register_plugin(
        &self,
        name: impl Into<String>,
        wasm_bytes: &[u8],
    ) -> Result<(), TransformationError> {
        let name = name.into();
        let module = Module::new(&self.engine, wasm_bytes).map_err(|e| {
            TransformationError::WasmError(format!("Failed to load WASM module: {}", e))
        })?;

        let mut plugins = self.plugins.write().await;
        plugins.insert(name, module);
        Ok(())
    }

    /// Register a WASM plugin (no-op when feature disabled)
    #[cfg(not(feature = "wasm-plugins"))]
    pub async fn register_plugin(
        &self,
        _name: impl Into<String>,
        _wasm_bytes: &[u8],
    ) -> Result<(), TransformationError> {
        Err(TransformationError::WasmError(
            "WASM plugins not enabled (compile with --features wasm-plugins)".to_string(),
        ))
    }
}

impl Default for WasmPluginTransformer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transformer for WasmPluginTransformer {
    fn supports(&self, transformation: &TransformationType) -> bool {
        matches!(transformation, TransformationType::WasmPlugin { .. })
    }

    async fn transform(
        &self,
        data: &[u8],
        transformation: &TransformationType,
    ) -> Result<TransformationResult, TransformationError> {
        let TransformationType::WasmPlugin {
            plugin_name,
            params,
        } = transformation
        else {
            return Err(TransformationError::UnsupportedFormat(
                "Not a WASM plugin transformation".to_string(),
            ));
        };

        #[cfg(feature = "wasm-plugins")]
        {
            self.execute_plugin(plugin_name, data, params).await
        }

        #[cfg(not(feature = "wasm-plugins"))]
        {
            let _ = (plugin_name, params, data);
            Err(TransformationError::WasmError(
                "WASM plugins not enabled (compile with --features wasm-plugins)".to_string(),
            ))
        }
    }
}

#[cfg(feature = "wasm-plugins")]
impl WasmPluginTransformer {
    async fn execute_plugin(
        &self,
        plugin_name: &str,
        data: &[u8],
        params: &HashMap<String, String>,
    ) -> Result<TransformationResult, TransformationError> {
        let plugins = self.plugins.read().await;
        let module = plugins.get(plugin_name).ok_or_else(|| {
            TransformationError::WasmError(format!("Plugin '{}' not found", plugin_name))
        })?;

        // Create a new store for this execution
        let mut store = Store::new(&self.engine, ());

        // Instantiate the module
        let instance = Instance::new(&mut store, module, &[]).map_err(|e| {
            TransformationError::WasmError(format!("Failed to instantiate plugin: {}", e))
        })?;

        // For now, we'll return the input data unchanged as a placeholder
        // A real implementation would call the WASM function with proper memory management
        let mut result =
            TransformationResult::new(Bytes::from(data.to_vec()), "application/octet-stream");
        result = result
            .with_metadata("plugin", plugin_name)
            .with_metadata("params", format!("{:?}", params))
            .with_metadata("wasm_enabled", "true");

        // Note: Full WASM execution with memory imports/exports would require more complex setup
        // This is a simplified implementation that demonstrates the structure
        let _ = instance; // Use the instance to avoid unused variable warning

        Ok(result)
    }
}
