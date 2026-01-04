# JSON Filter WASM Plugin (Rust)

A WASM plugin that filters JSON objects to include only specified fields.

## Features

- Filters JSON objects to include only specified fields
- Supports both single objects and arrays of objects
- Useful for data minimization and privacy compliance (GDPR, HIPAA)
- No-std compatible for minimal binary size

## Building

```bash
# Install Rust WASM target
rustup target add wasm32-unknown-unknown

# Build the plugin
cargo build --target wasm32-unknown-unknown --release

# Optimize (optional)
wasm-opt -Oz -o json_filter_optimized.wasm \
  target/wasm32-unknown-unknown/release/wasm_json_filter.wasm
```

## Usage

### Example 1: Filter single object

Input JSON:
```json
{
  "name": "John Doe",
  "email": "john@example.com",
  "password": "secret123",
  "ssn": "123-45-6789",
  "age": 30
}
```

Parameters: `"name,email,age"`

Output:
```json
{
  "name": "John Doe",
  "email": "john@example.com",
  "age": 30
}
```

### Example 2: Filter array of objects

Input JSON:
```json
[
  {"id": 1, "name": "Alice", "salary": 50000, "dept": "Engineering"},
  {"id": 2, "name": "Bob", "salary": 60000, "dept": "Marketing"}
]
```

Parameters: `"id,name,dept"`

Output:
```json
[
  {"id": 1, "name": "Alice", "dept": "Engineering"},
  {"id": 2, "name": "Bob", "dept": "Marketing"}
]
```

## Plugin Interface

- `transform_with_params(input_ptr, input_len, params_ptr, params_len) -> u64`
  - Filters JSON based on comma-separated field names
  - Returns packed result (length in high 32 bits, pointer in low 32 bits)

- `allocate(size) -> ptr` - Allocate memory for host
- `deallocate(ptr, size)` - Free allocated memory

## Use Cases

1. **PII Redaction**: Remove sensitive fields before logging or analytics
2. **API Response Filtering**: Reduce bandwidth by including only needed fields
3. **Data Compliance**: Ensure only authorized fields are transmitted
4. **Cost Optimization**: Minimize data transfer costs
