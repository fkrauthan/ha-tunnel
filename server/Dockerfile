# =====================================================
# Home Assistant Tunnel Server - Dockerfile
# =====================================================
# Build from project root: docker build -t ha-tunnel-server -f server/Dockerfile .

# Build stage
FROM rust:1.91-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconfig \
    perl \
    make

WORKDIR /build

# Copy manifests first for better caching
COPY Cargo.toml Cargo.loc[k] ./
COPY common/Cargo.toml ./common/
COPY client/Cargo.toml ./client/
COPY server/Cargo.toml ./server/

# Create dummy src to build dependencies (layer caching)
RUN mkdir -p common/src client/src server/src && \
    echo "pub fn dummy() {}" > common/src/lib.rs && \
    echo "fn main() {}" > client/src/main.rs && \
    echo "fn main() {}" > server/src/main.rs && \
    cargo build --release --bin ha-tunnel-client 2>/dev/null || true && \
    rm -rf common/src server/src

# Copy actual source code
COPY common/src ./common/src
COPY server/src ./server/src

# Build the real binary
ENV OPENSSL_STATIC=1
RUN touch common/src/lib.rs client/src/main.rs && \
    cargo build --release --package ha-tunnel-server && \
    strip /build/target/release/ha-tunnel-server

# =====================================================
# Runtime stage - minimal image
# =====================================================
FROM alpine:3.23

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    tini \
    wget

# Create non-root user
RUN addgroup -g 1000 hatunnel && \
    adduser -u 1000 -G hatunnel -s /bin/sh -D hatunnel

# Copy binary
COPY --from=builder /build/target/release/ha-tunnel-server /usr/local/bin/

# Create config directories
RUN mkdir -p /config && \
    chown -R hatunnel:hatunnel /config

WORKDIR /config

# Switch to non-root user
USER hatunnel

# Expose ports
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1

# Use tini as init system
ENTRYPOINT ["/sbin/tini", "--"]

# Default command
CMD ["ha-tunnel-server", "-c", "/config/config.toml"]
