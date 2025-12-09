# HA Tunnel

A secure WebSocket tunnel to expose Home Assistant to Amazon Alexa and Google Assistant without requiring port forwarding or a public IP address.

## How It Works

```
Alexa/Google  ──▶  HA Tunnel Server (public)  ◀──WebSocket──▶  HA Tunnel Client  ──▶  Home Assistant
                        (cloud)                                   (local)              (local)
```

The **server** runs on a publicly accessible host and receives requests from Alexa/Google. The **client** runs locally alongside Home Assistant, maintains a persistent WebSocket connection to the server, and proxies requests to your local HA instance.

## Installation

### Home Assistant Add-on (Recommended)

The easiest way to install the client is as a Home Assistant add-on.

1. Add the repository to Home Assistant:

   [![Add Repository](https://my.home-assistant.io/badges/supervisor_add_addon_repository.svg)](https://my.home-assistant.io/redirect/supervisor_add_addon_repository/?repository_url=https%3A%2F%2Fgithub.com%2Ffkrauthan%2Fha-tunnel)

   Or manually add the repository URL: `https://github.com/fkrauthan/ha-tunnel`

2. Install the "HA Tunnel Client" add-on
3. Configure the add-on with your server URL and secret
4. Start the add-on

### Docker

Both server and client are available as Docker images:

```bash
# Server
docker pull ghcr.io/fkrauthan/ha-tunnel-server:latest

# Client
docker pull ghcr.io/fkrauthan/ha-tunnel-client:latest
```

### Build from Source

```bash
# Build both binaries
cargo build --release

# Binaries are located at:
# target/release/ha-tunnel-server
# target/release/ha-tunnel-client
```

## Server Setup

The server must be hosted on a publicly accessible machine (cloud VPS, etc.).

### Quick Start with Docker

```bash
docker run -d \
  -p 3000:3000 \
  -e HA_TUNNEL_SECRET="your-secure-secret" \
  ghcr.io/fkrauthan/ha-tunnel-server:latest
```

### Configuration

Create a `config.toml` file or use environment variables (prefixed with `HA_TUNNEL_`):

```toml
# Server config.toml
secret = "your-secure-secret"   # Required: shared secret for client authentication
host = "0.0.0.0"                # Default: 0.0.0.0
port = 3000                     # Default: 3000
client_timeout = 10             # Seconds to wait for client connection
request_timeout = 30            # Seconds to wait for client response
log_level = "INFO"              # TRACE, DEBUG, INFO, WARN, ERROR
```

## Client Setup

### Home Assistant Add-on Configuration

```yaml
server: "wss://your-server.example.com"  # Server WebSocket URL
secret: "your-secure-secret"              # Must match server secret
ha_server: "http://supervisor/core"       # Default for add-on
ha_external_url: "https://your-ha.duckdns.org"  # Your HA external URL
assistant_alexa: true
assistant_google: true
```

### Docker

```bash
docker run -d \
  -e HA_TUNNEL_SERVER="wss://your-server.example.com" \
  -e HA_TUNNEL_SECRET="your-secure-secret" \
  -e HA_TUNNEL_HA_SERVER="http://homeassistant:8123" \
  ghcr.io/fkrauthan/ha-tunnel-client:latest
```

### Configuration

Create a `config.toml` file or use environment variables (prefixed with `HA_TUNNEL_`):

```toml
# Client config.toml
server = "wss://your-server.example.com"  # Required: server WebSocket URL
secret = "your-secure-secret"              # Required: must match server
ha_server = "http://localhost:8123"        # Required: local Home Assistant URL
ha_external_url = "https://your-ha.duckdns.org"  # External URL for OAuth redirects

# Optional settings
assistant_alexa = true      # Enable Alexa integration
assistant_google = true     # Enable Google Assistant integration
ha_timeout = 10             # Request timeout to HA (seconds)
reconnect_interval = 5      # Reconnection delay (1-300 seconds)
heartbeat_interval = 30     # Heartbeat interval (5-120 seconds)
log_level = "INFO"
```

## Setting Up Alexa/Google Assistant

### Alexa Smart Home

1. Create an Alexa Smart Home skill in the Amazon Developer Console
2. Set the Lambda endpoint to: `https://your-server.example.com/api/alexa/smart_home`
3. Configure account linking with your Home Assistant OAuth settings
4. Enable the skill and link your account

### Google Assistant

1. Create a project in Actions on Google Console
2. Set the fulfillment URL to: `https://your-server.example.com/api/google_assistant`
3. Configure OAuth account linking
4. Test and publish your action

## Security

- Client authentication uses HMAC-SHA256 signatures with a shared secret
- Timestamps are validated within a 60-second window to prevent replay attacks
- All communication should use TLS (wss:// for WebSocket, https:// for HTTP)
- Use a strong, unique secret for production deployments

## Development

```bash
# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy

# Build Docker images locally
docker build -t ha-tunnel-server -f server/Dockerfile .
docker build -t ha-tunnel-client -f client/Dockerfile .
```

## License

MIT License
