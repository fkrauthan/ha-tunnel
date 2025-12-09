# =====================================================
# Home Assistant Tunnel Client - Dockerfile
# =====================================================
# Build from project root: docker build -t ha-tunnel-client -f client/Dockerfile .

# Build stage
FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev

WORKDIR /build

# Copy workspace manifests first for better caching
COPY Cargo.toml Cargo.loc[k] ./
COPY common/Cargo.toml ./common/
COPY client/Cargo.toml ./client/
COPY server/Cargo.toml ./server/

# Create dummy sources to build dependencies (layer caching)
RUN mkdir -p common/src client/src server/src && \
    echo "pub fn dummy() {}" > common/src/lib.rs && \
    echo "fn main() {}" > client/src/main.rs && \
    echo "fn main() {}" > server/src/main.rs && \
    cargo build --release --bin ha-tunnel-client 2>/dev/null || true && \
    rm -rf common/src client/src

# Copy actual source code
COPY common/src ./common/src
COPY client/src ./client/src

# Build the real binary
RUN touch common/src/lib.rs client/src/main.rs && \
    cargo build --release --package ha-tunnel-client && \
    strip /build/target/release/ha-tunnel-client

# =====================================================
# Runtime stage - minimal image
# =====================================================
FROM alpine:3.23

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    tini

# Create non-root user
RUN addgroup -g 1000 hatunnel && \
    adduser -u 1000 -G hatunnel -s /bin/sh -D hatunnel

# Copy binary
COPY --from=builder /build/target/release/ha-tunnel-client /usr/local/bin/

# Create config directory
RUN mkdir -p /config && \
    chown -R hatunnel:hatunnel /config

WORKDIR /config

# Switch to non-root user
USER hatunnel

# Use tini as init system
ENTRYPOINT ["/sbin/tini", "--"]

# Default command
CMD ["ha-tunnel-client", "-c", "/config/config.toml"]
