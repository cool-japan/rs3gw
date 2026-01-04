# Text Replace WASM Plugin (TinyGo)

A WASM plugin written in Go (using TinyGo) that performs simple text find-and-replace operations.

## Prerequisites

Install TinyGo:
```bash
# macOS
brew install tinygo

# Linux (see https://tinygo.org/getting-started/install/)
wget https://github.com/tinygo-org/tinygo/releases/download/v0.30.0/tinygo_0.30.0_amd64.deb
sudo dpkg -i tinygo_0.30.0_amd64.deb
```

## Building

```bash
# Build the WASM module
tinygo build -o text_replace.wasm -target wasi main.go

# Optimize (optional, requires wasm-opt from binaryen)
wasm-opt -Oz -o text_replace_optimized.wasm text_replace.wasm
```

## Usage

### Example 1: Replace sensitive data

Input:
```
User john@example.com logged in from 192.168.1.100
```

Parameters: `john@example.com|[REDACTED]`

Output:
```
User [REDACTED] logged in from 192.168.1.100
```

### Example 2: Update URLs

Input:
```
Visit http://old-domain.com for more info
```

Parameters: `http://old-domain.com|https://new-domain.com`

Output:
```
Visit https://new-domain.com for more info
```

### Example 3: Remove profanity

Input:
```
This is a badword in the text
```

Parameters: `badword|[censored]`

Output:
```
This is a [censored] in the text
```

## Plugin Interface

- `transform_with_params(input_ptr, input_len, params_ptr, params_len) -> u64`
  - Parameters format: `find|replace`
  - Returns packed result (length in high 32 bits, pointer in low 32 bits)

- `allocate(size) -> ptr` - Allocate memory for host
- `deallocate(ptr, size)` - Free allocated memory

## Use Cases

1. **Data Sanitization**: Remove or replace sensitive information
2. **Content Moderation**: Filter inappropriate content
3. **URL Rewriting**: Update links in bulk
4. **Format Conversion**: Replace line endings, delimiters, etc.
5. **Compliance**: Mask PII before storage or transmission

## Performance

TinyGo produces compact WASM binaries (typically 10-50KB after optimization) with good performance for string operations. The binary size and execution speed make it ideal for text processing tasks.

## Integration with rs3gw

```bash
# Upload the plugin (with wasm-plugins feature enabled)
aws s3api put-object \
  --bucket my-bucket \
  --key transforms/text-replace.wasm \
  --body text_replace.wasm

# Apply transformation via custom headers or S3 Object Lambda
```
