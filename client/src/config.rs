use anyhow::{Context, Result};
use config::Config as ConfigParser;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{Level, info};

const SUPERVISOR_API_URL: &str = "http://supervisor/core/info";
const HA_SERVER_DETECT: &str = "DETECT";

#[derive(Debug, Deserialize)]
struct SupervisorResponse {
    data: SupervisorCoreInfo,
}

#[derive(Debug, Deserialize)]
struct SupervisorCoreInfo {
    ip_address: String,
    port: u16,
    ssl: bool,
}

pub struct Features {
    pub assistant_alexa: bool,
    pub assistant_google: bool,
}

pub struct Config {
    pub log_level: Level,

    pub server: String,
    pub reconnect_interval: u64,
    pub heartbeat_interval: u64,

    pub ha_server: String,
    pub ha_external_url: String,
    pub ha_timeout: u64,
    pub ha_ignore_ssl: bool,
    pub ha_pass_client_ip: bool,

    pub secret: String,

    pub features: Features,
}

pub async fn parse_config(config_file: PathBuf) -> Result<Config> {
    let settings = ConfigParser::builder()
        .set_default("log_level", "INFO")?
        .set_default("reconnect_interval", 5)?
        .set_default("heartbeat_interval", 30)?
        .set_default("ha_timeout", 10)?
        .set_default("ha_ignore_ssl", false)?
        .set_default("ha_pass_client_ip", false)?
        .set_default("assistant_alexa", true)?
        .set_default("assistant_google", true)?
        .add_source(config::File::with_name(config_file.to_str().unwrap()).required(false))
        .add_source(config::Environment::with_prefix("HA_TUNNEL"))
        .build()?;

    let log_level = settings.get_string("log_level")?.parse()?;

    let server = settings.get_string("server")?;
    let reconnect_interval = settings.get_int("reconnect_interval")?.try_into()?;
    let heartbeat_interval = settings.get_int("heartbeat_interval")?.try_into()?;

    let ha_server_config = settings.get_string("ha_server")?;
    let resolved = resolve_ha_server(&ha_server_config).await?;
    let ha_server = resolved.url;

    let ha_timeout = settings.get_int("ha_timeout")?.try_into()?;
    let ha_external_url = settings
        .get_string("ha_external_url")
        .unwrap_or_else(|_| ha_server.clone());

    let ha_ignore_ssl = if ha_server_config == HA_SERVER_DETECT && resolved.uses_ssl {
        info!("Auto-detected HTTPS server, enabling SSL certificate validation bypass");
        true
    } else {
        settings.get_bool("ha_ignore_ssl")?
    };
    let ha_pass_client_ip = settings.get_bool("ha_pass_client_ip")?;

    let assistant_alexa = settings.get_bool("assistant_alexa")?;
    let assistant_google = settings.get_bool("assistant_google")?;

    let secret = settings.get_string("secret")?;

    Ok(Config {
        log_level,

        server,
        reconnect_interval,
        heartbeat_interval,

        ha_server,
        ha_external_url,
        ha_timeout,
        ha_ignore_ssl,
        ha_pass_client_ip,

        secret,

        features: Features {
            assistant_alexa,
            assistant_google,
        },
    })
}

struct ResolvedHaServer {
    url: String,
    uses_ssl: bool,
}

async fn resolve_ha_server(ha_server_config: &str) -> Result<ResolvedHaServer> {
    if ha_server_config != HA_SERVER_DETECT {
        let uses_ssl = ha_server_config.starts_with("https://");
        return Ok(ResolvedHaServer {
            url: ha_server_config.to_string(),
            uses_ssl,
        });
    }

    let supervisor_token = std::env::var("SUPERVISOR_TOKEN")
        .context("ha_server is set to DETECT but SUPERVISOR_TOKEN environment variable is not set. Are you running as a Home Assistant add-on?")?;

    info!("Detecting Home Assistant server from Supervisor API...");

    let client = reqwest::Client::new();
    let response = client
        .get(SUPERVISOR_API_URL)
        .header("Authorization", format!("Bearer {}", supervisor_token))
        .send()
        .await
        .context("Failed to connect to Supervisor API")?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!(
            "Supervisor API returned error status: {} - {}",
            status.as_u16(),
            response.text().await.unwrap_or_default()
        );
    }

    let supervisor_info: SupervisorResponse = response
        .json()
        .await
        .context("Failed to parse Supervisor API response")?;

    let uses_ssl = supervisor_info.data.ssl;
    let scheme = if uses_ssl { "https" } else { "http" };
    let ha_server = format!(
        "{}://{}:{}",
        scheme, supervisor_info.data.ip_address, supervisor_info.data.port
    );

    info!(ha_server = %ha_server, "Detected Home Assistant server");

    Ok(ResolvedHaServer {
        url: ha_server,
        uses_ssl,
    })
}
