#!/bin/bash
set -e

echo "Building Uppercase WASM Plugin..."

# Check if wasm32-unknown-unknown target is installed
if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo "Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi

# Build the WASM module
echo "Compiling to WebAssembly..."
cargo build --target wasm32-unknown-unknown --release

# Get the output path
WASM_FILE="target/wasm32-unknown-unknown/release/uppercase_wasm_plugin.wasm"

# Check if the file exists
if [ ! -f "$WASM_FILE" ]; then
    echo "Error: WASM file not found at $WASM_FILE"
    exit 1
fi

# Display file size
SIZE=$(wc -c < "$WASM_FILE")
echo "✅ Build successful!"
echo "📦 Output: $WASM_FILE"
echo "📏 Size: $SIZE bytes"

# Optional: Use wasm-opt for further optimization (if available)
if command -v wasm-opt &> /dev/null; then
    echo "Running wasm-opt for additional optimization..."
    wasm-opt -Oz "$WASM_FILE" -o "${WASM_FILE}.opt"
    OPT_SIZE=$(wc -c < "${WASM_FILE}.opt")
    echo "📏 Optimized size: $OPT_SIZE bytes (saved $((SIZE - OPT_SIZE)) bytes)"
    mv "${WASM_FILE}.opt" "$WASM_FILE"
else
    echo "💡 Tip: Install wasm-opt for further size optimization"
    echo "    cargo install wasm-opt"
fi

echo ""
echo "To use this plugin in rs3gw, copy it to your plugins directory:"
echo "    cp $WASM_FILE /path/to/rs3gw/plugins/"
