use crate::config::{Config, Features};
use common::error::ProxyError;
use common::tunnel::TunnelMessage;
use reqwest::Client;
use tracing::error;

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
    method: String,
    path: String,
    query: Option<String>,
    headers: Vec<(String, String)>,
    body: Option<String>,
    source_ip: Option<String>,
) -> Result<(u16, Vec<(String, String)>, Option<String>), ProxyError> {
    let url = format!(
        "{}{}{}",
        config.ha_server.trim_end_matches('/'),
        path,
        query.map(|s| format!("?{}", s)).unwrap_or("".to_string())
    );
    let mut request = match method.as_str() {
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
    if let Some(ip) = source_ip {
        request = request.header("x-forwarded-for", &ip);
    }

    if let Some(body) = body {
        request = request.body(body.to_string());
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

    let body = response.text().await.ok();

    Ok((status, response_headers, body))
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
            if !validate_request(&config.features, &method, &path) {
                Some(TunnelMessage::HttpResponse {
                    request_id,
                    status: 400,
                    headers: vec![],
                    body: Some("Feature not enabled!".to_string()),
                })
            } else if method == "GET" && path == "/auth/authorize" {
                Some(TunnelMessage::HttpResponse {
                    request_id,
                    status: 307,
                    headers: vec![(
                        "Location".to_string(),
                        format!(
                            "{}?{}",
                            config.ha_external_url,
                            query.unwrap_or("".to_string())
                        ),
                    )],
                    body: None,
                })
            } else {
                Some(
                    match proxy_request(
                        config, client, method, path, query, headers, body, source_ip,
                    )
                    .await
                    {
                        Ok((status, response_headers, response_body)) => {
                            TunnelMessage::HttpResponse {
                                request_id,
                                status,
                                headers: response_headers,
                                body: response_body,
                            }
                        }
                        Err(e) => {
                            error!(request_id = %request_id, error = %e, "Failed to forward request");
                            TunnelMessage::Error {
                                request_id: Some(request_id),
                                code: "upstream_error".to_string(),
                                message: e.to_string(),
                            }
                        }
                    },
                )
            }
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
