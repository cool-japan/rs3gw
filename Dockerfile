# rs3gw - High-Performance S3 Gateway
# Multi-stage build for minimal image size
# Supports linux/amd64 and linux/arm64 architectures

# Build stage
FROM rust:1.85-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies by building them first
COPY Cargo.toml Cargo.lock* ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true
RUN rm -rf src

# Build the actual application
COPY src ./src
COPY tests ./tests
RUN touch src/main.rs && cargo build --release

# Runtime stage - minimal image
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies (curl for health check)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false rs3gw

# Create data directory
RUN mkdir -p /data && chown rs3gw:rs3gw /data

# Copy binary
COPY --from=builder /build/target/release/rs3gw /usr/local/bin/rs3gw

# Set ownership
RUN chown rs3gw:rs3gw /usr/local/bin/rs3gw

USER rs3gw

# Environment variables
ENV RS3GW_BIND_ADDR=0.0.0.0:9000
ENV RS3GW_STORAGE_ROOT=/data
ENV RS3GW_DEFAULT_BUCKET=default
ENV RUST_LOG=info

# Expose S3 port
EXPOSE 9000

# Health check using /health endpoint
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -sf http://localhost:9000/health || exit 1

# Data volume
VOLUME /data

ENTRYPOINT ["/usr/local/bin/rs3gw"]
