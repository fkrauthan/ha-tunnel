#!/usr/bin/with-contenv bashio
# =====================================================
# Home Assistant Tunnel Client - Addon Run Script
# =====================================================
set -e

CONFIG_PATH=/data/options.json

# Read configuration from Home Assistant addon options using jq
export HA_TUNNEL_SERVER="$(bashio::config 'server')"
export HA_TUNNEL_SECRET="$(bashio::config 'secret')"
export HA_TUNNEL_HA_SERVER="$(bashio::config 'ha_server')"
export HA_TUNNEL_ASSISTANT_ALEXA="$(bashio::config 'assistant_alexa')"
export HA_TUNNEL_ASSISTANT_GOOGLE="$(bashio::config 'assistant_google')"
export HA_TUNNEL_RECONNECT_INTERVAL="$(bashio::config 'reconnect_interval')"
export HA_TUNNEL_HEARTBEAT_INTERVAL="$(bashio::config 'heartbeat_interval')"
export HA_TUNNEL_HA_TIMEOUT="$(bashio::config 'ha_timeout')"
export HA_TUNNEL_LOG_LEVEL="$(bashio::config 'log_level')"

# Optional: ha_external_url (only set if not empty)
HA_EXTERNAL_URL=$(bashio::config 'ha_external_url')
if [ -n "$HA_EXTERNAL_URL" ]; then
    export HA_TUNNEL_HA_EXTERNAL_URL="$HA_EXTERNAL_URL"
fi

echo "Starting HA Tunnel Client..."
echo "Server: ${HA_TUNNEL_SERVER}"
echo "HA Server: ${HA_TUNNEL_HA_SERVER}"
echo "Alexa: ${HA_TUNNEL_ASSISTANT_ALEXA}, Google: ${HA_TUNNEL_ASSISTANT_GOOGLE}"

# Run the tunnel client
exec ha-tunnel-client
