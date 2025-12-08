use common::error::ProxyError;
use common::now_as_secs;
use common::tunnel::{TunnelMessage, generate_auth_signature};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{debug, error, info, warn};

pub async fn connect(
    client_id: &str,
    server: &str,
    secret: &str,
) -> Result<(mpsc::Sender<TunnelMessage>, mpsc::Receiver<TunnelMessage>), ProxyError> {
    let server_url = format!("{}/tunnel", server);
    info!(url = %server_url, client_id = %client_id, "Connecting to server");

    let (ws_stream, _) = connect_async(server_url)
        .await
        .map_err(|e| ProxyError::Connection(e.to_string()))?;

    let (mut write, mut read) = ws_stream.split();

    // Authenticate
    let timestamp = now_as_secs();
    let signature = generate_auth_signature(client_id, timestamp, secret);

    let auth_msg = TunnelMessage::Auth {
        client_id: client_id.to_string(),
        timestamp,
        signature,
    };

    write
        .send(auth_msg.to_ws_message()?)
        .await
        .map_err(|e| ProxyError::Connection(e.to_string()))?;

    // Wait for auth response
    if let Some(msg) = read.next().await {
        let msg = msg.map_err(|e| ProxyError::Connection(e.to_string()))?;
        let response = TunnelMessage::from_ws_message(msg)?;

        match response {
            TunnelMessage::AuthResponse { success, message } => {
                if !success {
                    return Err(ProxyError::AuthFailed(
                        message.unwrap_or_else(|| "Unknown error".to_string()),
                    ));
                }
                info!("Authentication successful");
            }
            _ => {
                return Err(ProxyError::AuthFailed("Unexpected response".to_string()));
            }
        }
    } else {
        return Err(ProxyError::Connection("No auth response".to_string()));
    }

    // Create channels
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<TunnelMessage>(100);
    let (inbound_tx, inbound_rx) = mpsc::channel::<TunnelMessage>(100);

    // Spawn writer task
    tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            match msg.to_ws_message() {
                Ok(ws_msg) => {
                    if let Err(e) = write.send(ws_msg).await {
                        error!("Failed to send message: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                }
            }
        }
        debug!("Writer task ended");
    });

    // Spawn reader task
    tokio::spawn(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(ws_msg) => {
                    if ws_msg.is_close() {
                        info!("Server closed connection");
                        break;
                    }

                    match TunnelMessage::from_ws_message(ws_msg) {
                        Ok(tunnel_msg) => {
                            if inbound_tx.send(tunnel_msg).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse message: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
        debug!("Reader task ended");
    });

    Ok((outbound_tx, inbound_rx))
}
