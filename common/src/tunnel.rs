use crate::error::ProxyError;
use log::debug;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TunnelMessage {
    /// Authentication message
    Auth {
        client_id: String,
        timestamp: u64,
        signature: String,
    },

    /// Authentication response
    AuthResponse {
        success: bool,
        message: Option<String>,
    },

    /// HTTP request to forward
    HttpRequest {
        request_id: String,
        method: String,
        path: String,
        query: Option<String>,
        headers: Vec<(String, String)>,
        body: Option<String>,
        source_ip: Option<String>,
    },

    /// HTTP response from upstream
    HttpResponse {
        request_id: String,
        status: u16,
        headers: Vec<(String, String)>,
        body: Option<String>,
    },

    /// Error response
    Error {
        request_id: Option<String>,
        code: String,
        message: String,
    },

    /// Heartbeat
    Ping {
        timestamp: u64,
    },
    Pong {
        timestamp: u64,
    },
}

impl TunnelMessage {
    pub fn to_ws_message(&self) -> Result<Message, ProxyError> {
        let json = serde_json::to_string(self)?;
        Ok(Message::text(json))
    }

    pub fn from_ws_message(msg: Message) -> Result<Self, ProxyError> {
        match msg {
            Message::Text(text) => {
                debug!("TunnelMessage::from_ws_message: {}", text);
                serde_json::from_str(&text).map_err(|e| ProxyError::Tunnel(e.to_string()))
            }
            Message::Binary(data) => {
                serde_json::from_slice(&data).map_err(|e| ProxyError::Tunnel(e.to_string()))
            }
            Message::Ping(_) | Message::Pong(_) => {
                Err(ProxyError::Tunnel("Unexpected ping/pong".to_string()))
            }
            Message::Close(_) => Err(ProxyError::Tunnel("Connection closed".to_string())),
            _ => Err(ProxyError::Tunnel("Unknown message type".to_string())),
        }
    }
}

pub fn generate_auth_signature(client_id: &str, timestamp: u64, secret: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let message = format!("{}:{}", client_id, timestamp);
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(message.as_bytes());

    hex::encode(mac.finalize().into_bytes())
}
