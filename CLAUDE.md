# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands

```bash
# Build all crates
cargo build

# Build release binaries
cargo build --release

# Build specific binary
cargo build --bin ha-tunnel-server
cargo build --bin ha-tunnel-client

# Run tests
cargo test

# Run a single test
cargo test <test_name>

# Check without building
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

## Docker Build (server)

```bash
# Build from project root
docker build -t ha-tunnel-server -f server/Dockerfile .
```

## Architecture

This is a Rust workspace project implementing a tunnel system for Home Assistant to expose Alexa/Google Assistant endpoints through a secure WebSocket connection.

### Crate Structure

- **common/** - Shared library with tunnel protocol, error types, and auth utilities
- **server/** - Public-facing server that accepts WebSocket connections from clients and HTTP requests from Alexa/Google
- **client/** - Runs alongside Home Assistant, connects to server via WebSocket, proxies requests to local HA instance

### Request Flow

1. External request (Alexa/Google) → Server HTTP endpoint
2. Server forwards via WebSocket → Connected Client
3. Client proxies to local Home Assistant
4. Response flows back through the same path

### Key Protocol Types (`common/src/tunnel.rs`)

`TunnelMessage` enum handles all WebSocket communication:
- `Auth`/`AuthResponse` - HMAC-SHA256 signature-based authentication
- `HttpRequest`/`HttpResponse` - Proxied HTTP traffic
- `Ping`/`Pong` - Heartbeat mechanism

### Configuration

Both binaries use TOML config files (default: `config.toml`) with environment variable overrides prefixed with `HA_TUNNEL_`.

**Server config:** host, port, secret, client_timeout, request_timeout
**Client config:** server URL, secret, ha_server, ha_external_url, feature flags (assistant_alexa, assistant_google)

### Server Endpoints

- `/tunnel` - WebSocket endpoint for client connections
- `/api/alexa/smart_home` - Alexa Smart Home API
- `/api/google_assistant` - Google Assistant API
- `/auth/authorize`, `/auth/token` - OAuth flow endpoints
- `/health` - Health check

## Technical Notes

- Uses Rust 2024 edition
- Async runtime: Tokio
- HTTP framework: Axum (server), Reqwest (client)
- WebSocket: tokio-tungstenite
- Auth signature validation has 60-second timestamp window
