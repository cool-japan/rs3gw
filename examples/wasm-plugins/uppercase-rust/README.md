# Uppercase WASM Plugin Example

A simple rs3gw WASM plugin that converts text to uppercase.

## Overview

This example demonstrates how to create a WASM plugin for rs3gw's transformation system. The plugin:
- Reads input data from WASM linear memory
- Transforms text to uppercase
- Returns the result via the plugin contract

## Prerequisites

- Rust 1.85+
- `wasm32-unknown-unknown` target installed:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```

## Building

```bash
# From this directory
cargo build --target wasm32-unknown-unknown --release

# The output WASM module will be at:
# target/wasm32-unknown-unknown/release/uppercase_wasm_plugin.wasm
```

## Plugin Contract

rs3gw WASM plugins must export the following functions:

### Required Exports

1. **`memory`**: WebAssembly linear memory
   - Automatically exported by default

2. **`alloc(size: u32) -> ptr: u32`**
   - Allocates memory for input data
   - Parameters:
     - `size`: Number of bytes to allocate
   - Returns: Pointer to allocated memory (0 if failed)

3. **`transform(ptr: u32, len: u32) -> result: u64`**
   - Main transformation function
   - Parameters:
     - `ptr`: Pointer to input data in linear memory
     - `len`: Length of input data
   - Returns: Packed u64 with:
     - High 32 bits: Output data pointer
     - Low 32 bits: Output data length

### Optional Exports

- **`plugin_version() -> u32`**: Returns plugin version number
- **`plugin_name() -> u64`**: Returns plugin name (packed ptr:len)

## Usage with rs3gw

### 1. Register the Plugin

```rust
use rs3gw::storage::transformations::{WasmPluginTransformer, TransformationType};
use std::collections::HashMap;

// Load WASM binary
let wasm_binary = std::fs::read(
    "target/wasm32-unknown-unknown/release/uppercase_wasm_plugin.wasm"
)?;

// Create transformer and register plugin
let transformer = WasmPluginTransformer::new();
transformer.register_plugin("uppercase".to_string(), wasm_binary).await?;
```

### 2. Use the Plugin

```rust
use bytes::Bytes;

// Prepare transformation
let input_data = b"hello world";
let params = TransformationType::WasmPlugin {
    plugin_name: "uppercase".to_string(),
    params: HashMap::new(),
};

// Execute transformation
let result = transformer.transform(input_data, &params).await?;

// Output: b"HELLO WORLD"
println!("Result: {}", String::from_utf8_lossy(&result.data));
```

### 3. Using via S3 API (Future)

```python
import boto3

s3 = boto3.client('s3', endpoint_url='http://localhost:9000')

# Upload object with transformation parameter
s3.put_object(
    Bucket='mybucket',
    Key='text.txt',
    Body=b'hello world',
    Metadata={
        'x-amz-meta-transform': 'wasm-plugin',
        'x-amz-meta-plugin': 'uppercase'
    }
)
```

## Memory Management

This plugin uses a simple bump allocator for demonstration purposes:
- 64KB heap size
- No deallocation support (suitable for short-lived transformations)
- For production plugins, consider using `wee_alloc` or a custom allocator

## Testing

```bash
# Run the test script
./test.sh
```

Or manually test with `wasmtime`:

```bash
# Install wasmtime
cargo install wasmtime-cli

# Run with wasmtime
wasmtime target/wasm32-unknown-unknown/release/uppercase_wasm_plugin.wasm
```

## Size Optimization

The compiled WASM module is optimized for size:
- Release profile with `opt-level = "z"`
- Link-time optimization (LTO)
- Symbol stripping
- Panic abort mode

Typical binary size: **~500 bytes** (for this simple plugin)

## Extending the Plugin

To add custom parameters:

```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    // Parameters can be passed via rs3gw's plugin params HashMap
    // and encoded in the input data (e.g., JSON header)

    // Example: First 4 bytes = parameter, rest = data
    // let param = read_u32(input_slice, 0);
    // let data = &input_slice[4..];

    // ... transformation logic ...
}
```

## Security Considerations

1. **Memory Safety**: Rust's safety guarantees prevent buffer overflows
2. **Resource Limits**: The WASM runtime enforces memory and execution limits
3. **Sandboxing**: WASM provides isolated execution environment
4. **Input Validation**: Always validate input data in production plugins

## Performance

- **Compilation**: JIT compilation via wasmtime's Cranelift backend
- **Execution**: Near-native performance for most operations
- **Overhead**: ~10-20% compared to native Rust (for simple operations)

## Next Steps

- Implement more complex transformations (e.g., image filters, encryption)
- Add error handling and logging
- Use WASI for file I/O capabilities
- Implement parameter parsing from plugin params
- Add comprehensive tests

## References

- [WebAssembly Specification](https://webassembly.github.io/spec/)
- [Rust WASM Book](https://rustwasm.github.io/docs/book/)
- [wasmtime Documentation](https://docs.wasmtime.dev/)
- [rs3gw Transformation API](../../../docs/transformations.md)
