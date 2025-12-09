# =====================================================
# Home Assistant Tunnel Client - Addon Dockerfile
# Uses pre-built client image for the binary
# =====================================================
ARG BUILD_FROM
ARG CLIENT_IMAGE=ghcr.io/fkrauthan/ha-tunnel-client:latest

# Get binary from pre-built client image
FROM ${CLIENT_IMAGE} AS client

# Runtime stage - Home Assistant base image
FROM ${BUILD_FROM}

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    libgcc \
    jq

# Copy binary from pre-built client image
COPY --from=client /usr/local/bin/ha-tunnel-client /usr/local/bin/

# Copy run script
COPY run.sh /run.sh
RUN chmod a+x /run.sh

CMD ["/run.sh"]
