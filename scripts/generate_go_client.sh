#!/bin/bash
# Generate Go gRPC client from proto files

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
PROTO_DIR="$PROJECT_ROOT/proto"
GO_OUT="$PROJECT_ROOT/clients/go"

echo "Generating Go gRPC client..."
echo "Proto dir: $PROTO_DIR"
echo "Output dir: $GO_OUT"

# Create output directory structure
mkdir -p "$GO_OUT/rs3gw"

# Check if protoc is available
if ! command -v protoc &> /dev/null; then
    echo "Error: protoc is not installed"
    echo "Install with: brew install protobuf (macOS) or apt-get install protobuf-compiler (Linux)"
    exit 1
fi

# Check if protoc-gen-go is installed
if ! command -v protoc-gen-go &> /dev/null; then
    echo "protoc-gen-go not found. Installing..."
    go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
fi

# Check if protoc-gen-go-grpc is installed
if ! command -v protoc-gen-go-grpc &> /dev/null; then
    echo "protoc-gen-go-grpc not found. Installing..."
    go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
fi

# Generate Go code from proto files
for proto_file in "$PROTO_DIR"/*.proto; do
    echo "Processing $proto_file..."
    protoc \
        -I"$PROTO_DIR" \
        --go_out="$GO_OUT" \
        --go_opt=paths=source_relative \
        --go_opt=module=github.com/cool-japan/rs3gw/clients/go \
        --go-grpc_out="$GO_OUT" \
        --go-grpc_opt=paths=source_relative \
        --go-grpc_opt=module=github.com/cool-japan/rs3gw/clients/go \
        "$proto_file"
done

echo "Go client generated successfully in $GO_OUT"

# Create go.mod
cat > "$GO_OUT/go.mod" << 'EOF'
module github.com/cool-japan/rs3gw/clients/go

go 1.21

require (
	google.golang.org/grpc v1.60.1
	google.golang.org/protobuf v1.32.0
)
EOF

# Create README
cat > "$GO_OUT/README.md" << 'EOF'
# rs3gw Go gRPC Client

Go client library for rs3gw S3-compatible storage gateway using gRPC.

## Installation

```bash
go get github.com/cool-japan/rs3gw/clients/go
```

## Quick Start

```go
import (
    rs3gw "github.com/cool-japan/rs3gw/clients/go/rs3gw"
    "google.golang.org/grpc"
)

conn, err := grpc.Dial("localhost:9001", grpc.WithInsecure())
if err != nil {
    log.Fatal(err)
}
defer conn.Close()

client := rs3gw.NewS3ServiceClient(conn)
```

See `examples/` directory for more usage examples.

## Requirements

- Go 1.21+
- google.golang.org/grpc >= 1.60.0
- google.golang.org/protobuf >= 1.32.0

## License

MIT OR Apache-2.0
EOF

echo "Setup files created in $GO_OUT"
