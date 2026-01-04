# WASM Plugin Developer Guide

Complete guide for developing custom WebAssembly plugins for rs3gw's transformation system.

## Table of Contents

- [Introduction](#introduction)
- [Plugin Contract](#plugin-contract)
- [Development Setup](#development-setup)
- [Creating Your First Plugin](#creating-your-first-plugin)
- [Advanced Topics](#advanced-topics)
- [Best Practices](#best-practices)
- [Testing and Debugging](#testing-and-debugging)
- [Performance Optimization](#performance-optimization)
- [Security Considerations](#security-considerations)
- [Examples](#examples)

## Introduction

rs3gw's WASM plugin system allows you to extend object transformations without modifying the core codebase. Plugins are:

- **Sandboxed**: Execute in an isolated WebAssembly environment
- **Safe**: Rust's safety guarantees prevent memory corruption
- **Portable**: Write once, run anywhere (any platform that runs rs3gw)
- **Fast**: Near-native performance via JIT compilation
- **Extensible**: Add custom transformations for domain-specific needs

### Use Cases

- **Data Processing**: Custom encryption, compression, or format conversion
- **Image/Video Filters**: Domain-specific image transformations
- **Text Processing**: Natural language processing, tokenization
- **Data Validation**: Custom validation rules for uploaded objects
- **Format Conversion**: Convert between proprietary formats
- **PII Redaction**: Remove sensitive information from documents

## Plugin Contract

All rs3gw WASM plugins must implement the following interface:

### Required Exports

#### 1. Linear Memory

```wasm
(memory (export "memory") ...)
```

WebAssembly linear memory for data transfer. Automatically exported in Rust projects.

#### 2. Memory Allocation

```rust
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32
```

**Purpose**: Allocate memory for input data from rs3gw

**Parameters**:
- `size`: Number of bytes to allocate

**Returns**:
- Pointer to allocated memory
- `0` if allocation failed

**Example**:
```rust
#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    unsafe {
        let ptr = ALLOCATOR.alloc(layout);
        ptr as u32
    }
}
```

#### 3. Transform Function

```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64
```

**Purpose**: Main transformation logic

**Parameters**:
- `ptr`: Pointer to input data in linear memory
- `len`: Length of input data in bytes

**Returns**:
- `u64` with packed output pointer and length:
  - High 32 bits: Output data pointer
  - Low 32 bits: Output data length

**Example**:
```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);

        // Process input...
        let output_ptr = /* allocate output */;
        let output_len = /* output length */;

        // Pack and return
        ((output_ptr as u64) << 32) | (output_len as u64)
    }
}
```

### Optional Exports

#### Plugin Metadata

```rust
// Version number
#[no_mangle]
pub extern "C" fn plugin_version() -> u32 {
    1
}

// Plugin name (packed ptr:len)
#[no_mangle]
pub extern "C" fn plugin_name() -> u64 {
    const NAME: &[u8] = b"my-plugin";
    let ptr = NAME.as_ptr() as u32;
    let len = NAME.len() as u32;
    ((ptr as u64) << 32) | (len as u64)
}

// Plugin description
#[no_mangle]
pub extern "C" fn plugin_description() -> u64 {
    // Similar to plugin_name
}
```

## Development Setup

### Prerequisites

1. **Rust Toolchain** (1.85+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **WASM Target**
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

3. **WASM Tools** (optional but recommended)
   ```bash
   # For optimization
   cargo install wasm-opt

   # For testing
   cargo install wasmtime-cli

   # For inspection
   cargo install twiggy
   ```

### Project Structure

```
my-wasm-plugin/
├── Cargo.toml
├── src/
│   └── lib.rs
├── build.sh
├── test.sh
└── README.md
```

### Cargo.toml Template

```toml
[package]
name = "my-wasm-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Keep dependencies minimal for smaller WASM size

[profile.release]
opt-level = "z"      # Optimize for size
lto = true           # Link-time optimization
strip = true         # Strip symbols
panic = "abort"      # Smaller binary
codegen-units = 1    # Better optimization
```

## Creating Your First Plugin

### Step 1: Initialize Project

```bash
cargo new --lib my-wasm-plugin
cd my-wasm-plugin
```

### Step 2: Configure Cargo.toml

Update `Cargo.toml` with the template above.

### Step 3: Implement Plugin

```rust
// src/lib.rs
use core::slice;
use core::alloc::{GlobalAlloc, Layout};
use core::panic::PanicInfo;

// Simple bump allocator
struct BumpAllocator;
static mut HEAP: [u8; 64 * 1024] = [0; 64 * 1024];
static mut HEAP_POS: usize = 0;

unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let pos = (HEAP_POS + align - 1) & !(align - 1);

        if pos + size > HEAP.len() {
            return core::ptr::null_mut();
        }

        HEAP_POS = pos + size;
        HEAP.as_mut_ptr().add(pos)
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    unsafe {
        let ptr = ALLOCATOR.alloc(layout);
        if ptr.is_null() { 0 } else { ptr as u32 }
    }
}

#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);
        let output_ptr = alloc(len);
        if output_ptr == 0 { return 0; }

        let output = slice::from_raw_parts_mut(output_ptr as *mut u8, len as usize);

        // Your transformation logic here
        output.copy_from_slice(input);

        ((output_ptr as u64) << 32) | (len as u64)
    }
}
```

### Step 4: Build

```bash
cargo build --target wasm32-unknown-unknown --release
```

The output will be at:
`target/wasm32-unknown-unknown/release/my_wasm_plugin.wasm`

### Step 5: Test with rs3gw

```rust
// Register plugin
let wasm_binary = std::fs::read("my_wasm_plugin.wasm")?;
let transformer = WasmPluginTransformer::new();
transformer.register_plugin("my-plugin".to_string(), wasm_binary).await?;

// Use plugin
let result = transformer.transform(
    b"input data",
    &TransformationType::WasmPlugin {
        plugin_name: "my-plugin".to_string(),
        params: HashMap::new(),
    }
).await?;
```

## Advanced Topics

### Custom Allocators

For production plugins, consider using specialized allocators:

#### wee_alloc (Size-optimized)

```toml
[dependencies]
wee_alloc = "0.4"
```

```rust
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
```

#### dlmalloc (Performance-optimized)

```toml
[dependencies]
dlmalloc = "0.2"
```

```rust
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;
```

### Parameter Passing

Parameters can be passed via the `params` HashMap and encoded in input data:

```rust
// Example: JSON header with parameters
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);

        // Parse JSON header (simplified)
        // Format: 4-byte length + JSON params + data
        let param_len = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
        let params_bytes = &input[4..4 + param_len as usize];
        let data = &input[4 + param_len as usize..];

        // Process based on parameters...
    }
}
```

### Error Handling

Return error codes or use special sentinel values:

```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    // Return 0 on error
    if ptr == 0 || len == 0 {
        return 0;
    }

    // ... transformation logic ...

    // Return packed result
    result
}
```

### WASI Support

For file I/O and system interactions:

```toml
[dependencies]
wasi = "0.11"
```

```rust
use wasi::*;

#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    // Can now use file I/O
    // Note: rs3gw must enable WASI in wasmtime configuration
}
```

## Best Practices

### 1. Memory Management

- **Keep heap size reasonable**: 64KB for simple plugins, up to 1MB for complex ones
- **Avoid allocations in hot paths**: Reuse buffers when possible
- **Clean up resources**: Even with bump allocator, be mindful of memory
- **Check allocation failures**: Always handle null pointers

### 2. Performance

- **Minimize copies**: Transform data in-place when possible
- **Use SIMD**: Leverage WASM SIMD instructions for parallel operations
- **Optimize build settings**: Use `opt-level = "z"` and LTO
- **Profile your code**: Use `twiggy` to analyze binary size

### 3. Security

- **Validate inputs**: Check bounds and data integrity
- **Avoid panics**: Use Result types and proper error handling
- **Limit resource usage**: Set reasonable heap limits
- **No unsafe network access**: WASM sandbox prevents this by default

### 4. Portability

- **No platform-specific code**: Keep code cross-platform
- **Document dependencies**: List any required WASI capabilities
- **Version your plugins**: Use `plugin_version()` for compatibility

## Testing and Debugging

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform() {
        // Initialize allocator
        unsafe { HEAP_POS = 0; }

        let input = b"test";
        let ptr = alloc(input.len() as u32);
        unsafe {
            let slice = slice::from_raw_parts_mut(ptr as *mut u8, input.len());
            slice.copy_from_slice(input);
        }

        let result = transform(ptr, input.len() as u32);
        assert_ne!(result, 0);
    }
}
```

### Integration Testing with wasmtime

```bash
# Run with wasmtime
wasmtime run --invoke transform my_plugin.wasm 0 10
```

### Debugging

```rust
// Add debug prints (requires WASI)
#[no_mangle]
pub extern "C" fn debug_log(ptr: u32, len: u32) {
    unsafe {
        let msg = slice::from_raw_parts(ptr as *const u8, len as usize);
        eprintln!("DEBUG: {}", String::from_utf8_lossy(msg));
    }
}
```

## Performance Optimization

### Build Optimization

```bash
# Standard release build
cargo build --target wasm32-unknown-unknown --release

# With wasm-opt
wasm-opt -Oz input.wasm -o output.wasm

# With binaryen
wasm-opt -O4 input.wasm -o output.wasm
```

### Runtime Optimization

1. **Ahead-of-Time Compilation**: Pre-compile WASM modules
2. **Module Caching**: Cache compiled modules between requests
3. **Memory Reuse**: Reuse WASM instances when possible

### Profiling

```bash
# Analyze binary size
twiggy top my_plugin.wasm

# Find code bloat
twiggy garbage my_plugin.wasm

# Inspect functions
twiggy paths my_plugin.wasm transform
```

## Security Considerations

### Sandboxing

WASM provides strong isolation:
- No access to host file system (without WASI)
- No network access (without explicit permissions)
- Memory limited to allocated linear memory
- No access to other processes

### Resource Limits

rs3gw enforces limits on WASM execution:
- Maximum memory allocation
- Maximum execution time
- Stack size limits

### Input Validation

Always validate plugin inputs:

```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    // Check for valid pointer and length
    if ptr == 0 || len == 0 || len > MAX_INPUT_SIZE {
        return 0;
    }

    // Validate data format
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);
        if !is_valid_input(input) {
            return 0;
        }

        // ... safe to process ...
    }
}
```

## Examples

### 1. Base64 Encoder

```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);
        let output_len = ((len + 2) / 3) * 4;
        let output_ptr = alloc(output_len);

        if output_ptr == 0 { return 0; }

        let output = slice::from_raw_parts_mut(output_ptr as *mut u8, output_len as usize);
        base64_encode(input, output);

        ((output_ptr as u64) << 32) | (output_len as u64)
    }
}
```

### 2. JSON Prettifier

```rust
#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);

        // Parse and prettify JSON
        let formatted = match prettify_json(input) {
            Some(data) => data,
            None => return 0,
        };

        let output_ptr = alloc(formatted.len() as u32);
        if output_ptr == 0 { return 0; }

        let output = slice::from_raw_parts_mut(output_ptr as *mut u8, formatted.len());
        output.copy_from_slice(&formatted);

        ((output_ptr as u64) << 32) | (formatted.len() as u64)
    }
}
```

### 3. Image Watermark

```rust
// Note: This is a conceptual example
// In practice, use image processing libraries compatible with WASM

#[no_mangle]
pub extern "C" fn transform(ptr: u32, len: u32) -> u64 {
    unsafe {
        let input = slice::from_raw_parts(ptr as *const u8, len as usize);

        // Load image, add watermark, encode back
        let watermarked = add_watermark(input, "© Company 2025");

        let output_ptr = alloc(watermarked.len() as u32);
        if output_ptr == 0 { return 0; }

        let output = slice::from_raw_parts_mut(output_ptr as *mut u8, watermarked.len());
        output.copy_from_slice(&watermarked);

        ((output_ptr as u64) << 32) | (watermarked.len() as u64)
    }
}
```

## Troubleshooting

### Common Issues

**Problem**: "Memory access out of bounds"
- **Solution**: Check heap size, ensure allocations fit in linear memory

**Problem**: Plugin returns 0
- **Solution**: Add error logging, check allocation failures

**Problem**: Slow performance
- **Solution**: Profile with twiggy, optimize hot paths, reduce allocations

**Problem**: Large binary size
- **Solution**: Use `opt-level = "z"`, wasm-opt, minimize dependencies

## Resources

- [WebAssembly Specification](https://webassembly.github.io/spec/)
- [Rust and WebAssembly Book](https://rustwasm.github.io/docs/book/)
- [wasmtime Documentation](https://docs.wasmtime.dev/)
- [WASI Documentation](https://wasi.dev/)
- [rs3gw GitHub Repository](https://github.com/cool-japan/rs3gw)

## Support

For questions and issues:
- GitHub Issues: https://github.com/cool-japan/rs3gw/issues
- Documentation: https://github.com/cool-japan/rs3gw/docs
- Examples: https://github.com/cool-japan/rs3gw/examples
