use anyhow::Result;
use config::Config as ConfigParser;
use std::net::IpAddr;
use std::path::PathBuf;
use tracing::Level;

#[derive(Debug, Clone, Default)]
pub enum ProxyMode {
    /// No proxy - use direct connection IP (default)
    #[default]
    None,
    /// Trust X-Forwarded-For header (Traefik, nginx, most proxies)
    XForwardedFor,
    /// Trust X-Real-IP header (nginx)
    XRealIp,
    /// Trust CF-Connecting-IP header (Cloudflare)
    Cloudflare,
    /// Trust True-Client-IP header (Akamai, Cloudflare Enterprise)
    TrueClientIp,
    /// Trust Forwarded header (RFC 7239 standard)
    Forwarded,
    /// Custom header name
    Custom(String),
}

impl ProxyMode {
    pub fn header_name(&self) -> Option<&str> {
        match self {
            ProxyMode::None => None,
            ProxyMode::XForwardedFor => Some("x-forwarded-for"),
            ProxyMode::XRealIp => Some("x-real-ip"),
            ProxyMode::Cloudflare => Some("cf-connecting-ip"),
            ProxyMode::TrueClientIp => Some("true-client-ip"),
            ProxyMode::Forwarded => Some("forwarded"),
            ProxyMode::Custom(name) => Some(name),
        }
    }
}

pub struct Config {
    pub log_level: Level,

    pub host: String,
    pub port: u16,

    pub secret: String,

    pub client_timeout: u64,
    pub request_timeout: u64,

    /// Proxy mode for extracting real client IP
    pub proxy_mode: ProxyMode,
    /// List of trusted proxy IPs/networks. If empty, all proxies are trusted.
    pub trusted_proxies: Vec<IpAddr>,
}

pub fn parse_config(config_file: PathBuf) -> Result<Config> {
    let settings = ConfigParser::builder()
        .set_default("log_level", "INFO")?
        .set_default("host", "0.0.0.0")?
        .set_default("port", 3000)?
        .set_default("client_timeout", 10)?
        .set_default("request_timeout", 30)?
        .set_default("proxy_mode", "none")?
        .set_default::<&str, Vec<String>>("trusted_proxies", vec![])?
        .add_source(config::File::with_name(config_file.to_str().unwrap()).required(false))
        .add_source(config::Environment::with_prefix("HA_TUNNEL"))
        .build()?;

    let log_level = settings.get_string("log_level")?.parse()?;
    let host = settings.get_string("host")?;
    let port = settings.get_int("port")?.try_into()?;

    let secret = settings.get_string("secret")?;

    let client_timeout = settings.get_int("client_timeout")?.try_into()?;
    let request_timeout = settings.get_int("request_timeout")?.try_into()?;

    let proxy_mode = parse_proxy_mode(&settings.get_string("proxy_mode")?);
    let trusted_proxies = settings
        .get_array("trusted_proxies")
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.into_string().ok()?.parse::<IpAddr>().ok())
        .collect();

    Ok(Config {
        log_level,

        host,
        port,

        secret,

        client_timeout,
        request_timeout,

        proxy_mode,
        trusted_proxies,
    })
}

fn parse_proxy_mode(mode: &str) -> ProxyMode {
    match mode.to_lowercase().as_str() {
        "none" | "" => ProxyMode::None,
        "x-forwarded-for" | "xforwardedfor" | "xff" => ProxyMode::XForwardedFor,
        "x-real-ip" | "xrealip" => ProxyMode::XRealIp,
        "cloudflare" | "cf-connecting-ip" => ProxyMode::Cloudflare,
        "true-client-ip" | "trueclientip" | "akamai" => ProxyMode::TrueClientIp,
        "forwarded" | "rfc7239" => ProxyMode::Forwarded,
        other => ProxyMode::Custom(other.to_string()),
    }
}
