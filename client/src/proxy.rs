use crate::config::{Config, Features};
use common::error::ProxyError;
use common::tunnel::TunnelMessage;
use reqwest::Client;
use std::time::Instant;
use tracing::{Instrument, debug, debug_span, error};

fn validate_request(features: &Features, method: &str, path: &str) -> bool {
    (features.assistant_alexa && method == "POST" && path == "/api/alexa/smart_home")
        || (features.assistant_google && method == "POST" && path == "/api/google_assistant")
        || ((features.assistant_google || features.assistant_alexa)
            && method == "GET"
            && path == "/auth/authorize")
        || ((features.assistant_google || features.assistant_alexa)
            && method == "POST"
            && path == "/auth/token")
}

#[allow(clippy::too_many_arguments)]
async fn proxy_request(
    config: &Config,
    client: &Client,
    method: &str,
    path: &str,
    query: Option<String>,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    source_ip: Option<String>,
) -> Result<(u16, Vec<(String, String)>, Option<Vec<u8>>), ProxyError> {
    let url = format!(
        "{}{}{}",
        config.ha_server.trim_end_matches('/'),
        path,
        query.map(|s| format!("?{}", s)).unwrap_or("".to_string())
    );
    let mut request = match method {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "DELETE" => client.delete(&url),
        "PATCH" => client.patch(&url),
        _ => {
            return Err(ProxyError::InvalidRequest(format!(
                "Unsupported method: {}",
                method
            )));
        }
    };

    for (name, value) in headers {
        request = request.header(&name, value);
    }
    if let Some(ip) = source_ip
        && config.ha_pass_client_ip
    {
        request = request.header("x-forwarded-for", &ip);
    }

    if let Some(body) = body {
        request = request.body(body);
    }

    let response = request.send().await?;
    let status = response.status().as_u16();
    let response_headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
        })
        .collect();

    let body = response.bytes().await.ok().map(|body| body.to_vec());

    Ok((status, response_headers, body))
}

#[allow(clippy::too_many_arguments)]
async fn handle_http_request(
    config: &Config,
    client: &Client,
    request_id: String,
    method: String,
    path: String,
    query: Option<String>,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    source_ip: Option<String>,
) -> TunnelMessage {
    debug!(method = %method, path = %path, query = ?query, source_ip = ?source_ip, "Received request from server");

    if !validate_request(&config.features, &method, &path) {
        debug!("Request rejected - feature not enabled");
        TunnelMessage::HttpResponse {
            request_id,
            status: 400,
            headers: vec![],
            body: Some("Feature not enabled!".bytes().collect()),
        }
    } else if method == "GET" && path == "/auth/authorize" {
        let redirect_url = format!(
            "{}{}?{}",
            config.ha_external_url.trim_end_matches('/'),
            path,
            query.unwrap_or("".to_string())
        );
        debug!("Redirecting auth request to Home Assistant external URL");
        TunnelMessage::HttpResponse {
            request_id,
            status: 307,
            headers: vec![("Location".to_string(), redirect_url)],
            body: None,
        }
    } else {
        let start = Instant::now();
        match proxy_request(
            config,
            client,
            method.as_str(),
            path.as_str(),
            query,
            headers,
            body,
            source_ip,
        )
        .await
        {
            Ok((status, response_headers, response_body)) => {
                let latency_ms = start.elapsed().as_millis();
                debug!(
                    latency_ms = latency_ms,
                    status = status,
                    "Received response from Home Assistant"
                );
                TunnelMessage::HttpResponse {
                    request_id,
                    status,
                    headers: response_headers,
                    body: response_body,
                }
            }
            Err(e) => {
                let latency_ms = start.elapsed().as_millis();
                error!(latency_ms = latency_ms, error = %e, "Failed to forward request");
                TunnelMessage::Error {
                    request_id: Some(request_id),
                    code: "upstream_error".to_string(),
                    message: e.to_string(),
                }
            }
        }
    }
}

pub async fn handle_request(
    config: &Config,
    client: &Client,
    msg: TunnelMessage,
) -> Option<TunnelMessage> {
    match msg {
        TunnelMessage::HttpRequest {
            request_id,
            method,
            path,
            query,
            headers,
            body,
            source_ip,
        } => {
            let span = debug_span!("request", %request_id);
            Some(
                handle_http_request(
                    config, client, request_id, method, path, query, headers, body, source_ip,
                )
                .instrument(span)
                .await,
            )
        }
        TunnelMessage::Pong { timestamp: _ } => None,
        _ => {
            error!("Unexpected message handled!");
            Some(TunnelMessage::Error {
                request_id: None,
                code: "invalid_message".to_string(),
                message: "Unexpected message type".to_string(),
            })
        }
    }
}
