use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProxyError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Filter denied request: {0}")]
    FilterDenied(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Tunnel error: {0}")]
    Tunnel(String),

    #[error("Upstream error: {0}")]
    Upstream(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
}

impl From<ProxyError> for axum::response::Response {
    fn from(err: ProxyError) -> Self {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;

        let (status, message) = match &err {
            ProxyError::FilterDenied(_) => (StatusCode::FORBIDDEN, err.to_string()),
            ProxyError::AuthFailed(_) => (StatusCode::UNAUTHORIZED, err.to_string()),
            ProxyError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, err.to_string()),
            ProxyError::InvalidRequest(_) => (StatusCode::BAD_REQUEST, err.to_string()),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal error".to_string(),
            ),
        };

        (status, message).into_response()
    }
}
