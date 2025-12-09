use crate::ServerState;
use crate::auth::verify_auth_signature;
use crate::client_ip::extract_client_ip;
use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, State, WebSocketUpgrade};
use axum::http::{HeaderMap, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use common::now_as_secs;
use common::tunnel::TunnelMessage;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tower_http::trace::TraceLayer;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ClientConnection {
    #[allow(dead_code)]
    pub client_id: String,
    #[allow(dead_code)]
    pub connected_at: u64,
    pub last_ping: u64,
    pub sender: mpsc::Sender<TunnelMessage>,
}

pub fn create_router(state: Arc<ServerState>) -> Router {
    Router::new()
        // Tunnel endpoint (WebSocket)
        .route("/tunnel", get(handle_tunnel_connection))
        // API endpoints
        .route("/api/alexa/smart_home", post(handle_api_request))
        .route("/api/google_assistant", post(handle_api_request))
        .route("/auth/authorize", get(handle_api_request))
        .route("/auth/token", post(handle_api_request))
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
                && let Some((_, sender)) = state.pending_requests.remove(request_id)
            {
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

async fn handle_api_request(
    State(state): State<Arc<ServerState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
) -> Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_string();
    let source_ip = extract_client_ip(
        request.headers(),
        addr,
        &state.config.proxy_mode,
        &state.config.trusted_proxies,
    );

    debug!(method = %method, path = %path, source_ip = %source_ip, direct_ip = %addr.ip(), "API request received");

    // Get client wait timeout from config
    let wait_timeout = Duration::from_secs(state.config.client_timeout);

    // Wait for a client to be available
    let client = if state.clients.is_empty() {
        debug!("No clients connected, waiting up to {:?}", wait_timeout);

        let mut rx = state.client_connected_rx.clone();
        let wait_result = tokio::time::timeout(wait_timeout, async {
            loop {
                if !state.clients.is_empty() {
                    return true;
                }
                if rx.changed().await.is_err() {
                    return false;
                }
            }
        })
        .await;

        match wait_result {
            Ok(true) => state.clients.iter().next(),
            _ => {
                warn!("No client connected within timeout");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "No connected clients (timeout waiting for client)",
                )
                    .into_response();
            }
        }
    } else {
        state.clients.iter().next()
    };

    let client = match client {
        Some(c) => c,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "No connected clients").into_response();
        }
    };

    // Extract request details
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            if name.eq("host") {
                return None;
            }
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
        })
        .collect();

    let query = request.uri().query().map(|s| s.to_string());

    // Read body
    let body = match axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024).await {
        Ok(bytes) => {
            if bytes.is_empty() {
                None
            } else {
                String::from_utf8(bytes.to_vec()).ok()
            }
        }
        Err(_) => None,
    };

    // Create request ID and oneshot channel for response
    let request_id = Uuid::new_v4().to_string();
    let (response_tx, response_rx) = oneshot::channel();

    // Store pending request
    state
        .pending_requests
        .insert(request_id.clone(), response_tx);

    // Send request to client
    let tunnel_request = TunnelMessage::HttpRequest {
        request_id: request_id.clone(),
        method,
        path,
        query,
        headers,
        body,
        source_ip: Some(source_ip),
    };

    if client.sender.send(tunnel_request).await.is_err() {
        state.pending_requests.remove(&request_id);
        return (StatusCode::BAD_GATEWAY, "Failed to forward request").into_response();
    }

    // Wait for response with timeout
    let timeout = Duration::from_secs(state.config.request_timeout);
    match tokio::time::timeout(timeout, response_rx).await {
        Ok(Ok(TunnelMessage::HttpResponse {
            status,
            headers,
            body,
            ..
        })) => {
            let body_content = body.unwrap_or_default();
            debug!(
                status = status,
                body_len = body_content.len(),
                "Building response from tunnel"
            );

            let status_code =
                StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            let mut header_map = HeaderMap::new();

            for (name, value) in headers {
                // Skip hop-by-hop headers that shouldn't be forwarded through proxies
                let name_lower = name.to_lowercase();
                if matches!(
                    name_lower.as_str(),
                    "content-length"
                        | "transfer-encoding"
                        | "connection"
                        | "keep-alive"
                        | "te"
                        | "trailers"
                        | "upgrade"
                ) {
                    debug!(header = %name, "Skipping hop-by-hop header");
                    continue;
                }
                if let (Ok(header_name), Ok(header_value)) = (
                    name.parse::<axum::http::header::HeaderName>(),
                    value.parse::<axum::http::header::HeaderValue>(),
                ) {
                    header_map.insert(header_name, header_value);
                }
            }

            (status_code, header_map, body_content).into_response()
        }
        Ok(Ok(TunnelMessage::Error { message, .. })) => {
            (StatusCode::FORBIDDEN, message).into_response()
        }
        Ok(Ok(_)) => (StatusCode::INTERNAL_SERVER_ERROR, "Unexpected response").into_response(),
        Ok(Err(_)) => {
            state.pending_requests.remove(&request_id);
            (StatusCode::INTERNAL_SERVER_ERROR, "Response channel closed").into_response()
        }
        Err(_) => {
            state.pending_requests.remove(&request_id);
            (StatusCode::GATEWAY_TIMEOUT, "Request timeout").into_response()
        }
    }
}
