# Uppercase WASM Plugin (Rust)

A simple WASM plugin that converts text to uppercase.

## Building

```bash
# Install Rust WASM target if not already installed
rustup target add wasm32-unknown-unknown

# Build the plugin
cargo build --target wasm32-unknown-unknown --release

# The output WASM file will be at:
# target/wasm32-unknown-unknown/release/wasm_uppercase.wasm
```

## Optimizing Size

To further optimize the WASM binary size:

```bash
# Install wasm-opt (part of binaryen)
# macOS: brew install binaryen
# Linux: apt install binaryen

# Optimize the WASM file
wasm-opt -Oz -o uppercase_optimized.wasm \
  target/wasm32-unknown-unknown/release/wasm_uppercase.wasm
```

## Usage with rs3gw

```bash
# Upload the WASM plugin to rs3gw (requires wasm-plugins feature)
curl -X PUT http://localhost:9000/my-bucket/transforms/uppercase.wasm \
  --data-binary @target/wasm32-unknown-unknown/release/wasm_uppercase.wasm

# Use the plugin to transform an object
# (This would be integrated via the S3 API with custom transformation headers)
```

## Plugin Interface

The plugin exports these functions:

- `transform(input_ptr, input_len) -> output_ptr` - Basic transformation
- `transform_with_length(input_ptr, input_len) -> packed_u64` - Returns length+pointer
- `allocate(size) -> ptr` - Allocate memory for host
- `deallocate(ptr, size)` - Free allocated memory

## Example

Input:
```
hello world
```

Output:
```
HELLO WORLD
```
