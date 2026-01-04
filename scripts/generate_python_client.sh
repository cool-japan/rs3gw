#!/bin/bash
# Generate Python gRPC client from proto files

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
PROTO_DIR="$PROJECT_ROOT/proto"
PYTHON_OUT="$PROJECT_ROOT/clients/python"

echo "Generating Python gRPC client..."
echo "Proto dir: $PROTO_DIR"
echo "Output dir: $PYTHON_OUT"

# Create output directory structure
mkdir -p "$PYTHON_OUT/rs3gw_client"
touch "$PYTHON_OUT/rs3gw_client/__init__.py"

# Check if python3 and grpcio-tools are available
if ! command -v python3 &> /dev/null; then
    echo "Error: python3 is not installed"
    exit 1
fi

# Check if grpcio-tools is installed, install if not
if ! python3 -c "import grpc_tools" 2>/dev/null; then
    echo "grpcio-tools not found. Installing..."
    pip3 install grpcio-tools grpcio
fi

# Generate Python code from proto files
python3 -m grpc_tools.protoc \
    -I"$PROTO_DIR" \
    --python_out="$PYTHON_OUT/rs3gw_client" \
    --grpc_python_out="$PYTHON_OUT/rs3gw_client" \
    --pyi_out="$PYTHON_OUT/rs3gw_client" \
    "$PROTO_DIR"/*.proto

echo "Python client generated successfully in $PYTHON_OUT"

# Create setup.py
cat > "$PYTHON_OUT/setup.py" << 'EOF'
from setuptools import setup, find_packages

setup(
    name="rs3gw-client",
    version="0.1.0",
    description="Python gRPC client for rs3gw S3-compatible storage gateway",
    author="rs3gw contributors",
    packages=find_packages(),
    install_requires=[
        "grpcio>=1.60.0",
        "grpcio-tools>=1.60.0",
        "protobuf>=4.25.0",
    ],
    python_requires=">=3.8",
    classifiers=[
        "Development Status :: 4 - Beta",
        "Intended Audience :: Developers",
        "License :: OSI Approved :: MIT License",
        "Programming Language :: Python :: 3",
        "Programming Language :: Python :: 3.8",
        "Programming Language :: Python :: 3.9",
        "Programming Language :: Python :: 3.10",
        "Programming Language :: Python :: 3.11",
        "Programming Language :: Python :: 3.12",
    ],
)
EOF

# Create README
cat > "$PYTHON_OUT/README.md" << 'EOF'
# rs3gw Python gRPC Client

Python client library for rs3gw S3-compatible storage gateway using gRPC.

## Installation

```bash
pip install -e .
```

## Quick Start

See `examples/` directory for usage examples.

## Requirements

- Python 3.8+
- grpcio >= 1.60.0
- protobuf >= 4.25.0

## License

MIT OR Apache-2.0
EOF

echo "Setup files created in $PYTHON_OUT"
