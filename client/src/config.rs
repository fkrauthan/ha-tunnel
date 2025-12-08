use anyhow::Result;
use config::Config as ConfigParser;
use std::path::PathBuf;
use tracing::Level;

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

    pub secret: String,

    pub features: Features,
}

pub fn parse_config(config_file: PathBuf) -> Result<Config> {
    let settings = ConfigParser::builder()
        .set_default("log_level", "INFO")?
        .set_default("reconnect_interval", 5)?
        .set_default("heartbeat_interval", 30)?
        .set_default("ha_timeout", 10)?
        .set_default("assistant_alexa", true)?
        .set_default("assistant_google", true)?
        .add_source(config::File::with_name(config_file.to_str().unwrap()).required(false))
        .add_source(config::Environment::with_prefix("HA_TUNNEL"))
        .build()?;

    let log_level = settings.get_string("log_level")?.parse()?;

    let server = settings.get_string("server")?;
    let reconnect_interval = settings.get_int("reconnect_interval")?.try_into()?;
    let heartbeat_interval = settings.get_int("heartbeat_interval")?.try_into()?;

    let ha_server = settings.get_string("ha_server")?;
    let ha_timeout = settings.get_int("ha_timeout")?.try_into()?;
    let ha_external_url = settings
        .get_string("ha_external_url")
        .unwrap_or_else(|_| ha_server.clone());

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

        secret,

        features: Features {
            assistant_alexa,
            assistant_google,
        },
    })
}
