use crate::ServerState;
use crate::auth::verify_auth_signature;
use axum::Router;
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use common::now_as_secs;
use common::tunnel::TunnelMessage;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub client_id: String,
    pub connected_at: u64,
    pub last_ping: u64,
    pub sender: mpsc::Sender<TunnelMessage>,
}

pub fn create_router(state: Arc<ServerState>) -> Router {
    Router::new()
        // Tunnel endpoint (WebSocket)
        .route("/tunnel", get(handle_tunnel_connection))
        // API endpoints
        // .route("/api/*path", any(handle_api_request)) TODO implement
        // .route("/api", any(handle_api_request))
        // Health check at root
        .route("/health", get(health_check))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone())
}

async fn health_check(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    let client_count = state.clients.len();
    let status = if client_count > 0 { "ok" } else { "no_clients" };

    axum::Json(serde_json::json!({
        "status": status,
        "clients": client_count
    }))
}

async fn handle_tunnel_connection(
    ws: WebSocketUpgrade,
    State(state): State<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    info!(addr = %addr, "New tunnel connection");

    ws.on_upgrade(move |socket| handle_tunnel_socket(socket, state))
}

async fn handle_tunnel_socket(socket: axum::extract::ws::WebSocket, state: Arc<ServerState>) {
    use axum::extract::ws::Message;

    let (mut ws_tx, mut ws_rx) = socket.split();

    // Wait for authentication
    let auth_timeout = Duration::from_secs(10);
    let auth_result = tokio::time::timeout(auth_timeout, ws_rx.next()).await;

    let client_id = match auth_result {
        Ok(Some(Ok(Message::Text(text)))) => {
            info!(
                "Client ID: {:?}",
                serde_json::from_str::<TunnelMessage>(&text)
            );
            match serde_json::from_str::<TunnelMessage>(&text) {
                Ok(TunnelMessage::Auth {
                    client_id,
                    timestamp,
                    signature,
                }) => {
                    if verify_auth_signature(
                        &client_id,
                        timestamp,
                        &signature,
                        &state.config.secret,
                    ) {
                        info!(client_id = %client_id, "Client authenticated");

                        // Send success response
                        let response = TunnelMessage::AuthResponse {
                            success: true,
                            message: None,
                        };
                        let msg = serde_json::to_string(&response).unwrap();
                        if ws_tx.send(Message::text(msg)).await.is_err() {
                            return;
                        }

                        client_id
                    } else {
                        warn!(client_id = %client_id, "Authentication failed");
                        let response = TunnelMessage::AuthResponse {
                            success: false,
                            message: Some("Invalid signature".to_string()),
                        };
                        let msg = serde_json::to_string(&response).unwrap();
                        let _ = ws_tx.send(Message::text(msg)).await;
                        return;
                    }
                }
                _ => {
                    warn!("Invalid auth message");
                    return;
                }
            }
        }
        _ => {
            warn!("Auth timeout or error");
            return;
        }
    };

    // Create channel for sending messages to this client
    let (tx, mut rx) = mpsc::channel::<TunnelMessage>(100);

    // Register client
    state.clients.insert(
        client_id.clone(),
        ClientConnection {
            client_id: client_id.clone(),
            connected_at: now_as_secs(),
            last_ping: now_as_secs(),
            sender: tx,
        },
    );

    // Notify waiters that a client connected
    let client_count = state.clients.len();
    let _ = state.client_connected_tx.send(client_count);

    info!(client_id = %client_id, client_count = client_count, "Client connected");

    // Spawn task to forward outbound messages
    let outbound_client_id = client_id.clone();
    let outbound_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = match serde_json::to_string(&msg) {
                Ok(t) => t,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    continue;
                }
            };
            if ws_tx.send(Message::text(text)).await.is_err() {
                break;
            }
        }
        debug!(client_id = %outbound_client_id, "Outbound task ended");
    });

    // Process incoming messages
    while let Some(msg) = ws_rx.next().await {
        match msg {
            Ok(Message::Text(text)) => match serde_json::from_str::<TunnelMessage>(&text) {
                Ok(tunnel_msg) => {
                    handle_client_message(&state, &client_id, tunnel_msg).await;
                }
                Err(e) => {
                    warn!("Failed to parse message: {}", e);
                }
            },
            Ok(Message::Close(_)) => {
                info!(client_id = %client_id, "Client disconnected");
                break;
            }
            Err(e) => {
                error!(client_id = %client_id, error = %e, "WebSocket error");
                break;
            }
            _ => {}
        }
    }

    // Cleanup
    state.clients.remove(&client_id);
    outbound_task.abort();

    info!(client_id = %client_id, "Client removed");
}

async fn handle_client_message(state: &Arc<ServerState>, client_id: &str, msg: TunnelMessage) {
    match msg {
        TunnelMessage::HttpResponse { ref request_id, .. } => {
            // Find pending request and send response
            if let Some((_, sender)) = state.pending_requests.remove(request_id) {
                let _ = sender.send(msg);
            } else {
                warn!(request_id = %request_id, "No pending request found");
            }
        }
        TunnelMessage::Error { ref request_id, .. } => {
            if let Some(request_id) = &request_id
                && let Some((_, sender)) = state.pending_requests.remove(request_id) {
                    let _ = sender.send(msg);
                }
        }
        TunnelMessage::Ping { timestamp } => {
            if let Some(mut client) = state.clients.get_mut(client_id) {
                client.last_ping = now_as_secs();

                let response = TunnelMessage::Pong { timestamp };
                if let Err(e) = client.sender.send(response).await {
                    error!("Failed to send message: {}", e);
                }
            }
            debug!(client_id = %client_id, latency_s = %(now_as_secs() - timestamp), "Ping received");
        }
        _ => {
            warn!(client_id = %client_id, "Unexpected message type");
        }
    }
}
