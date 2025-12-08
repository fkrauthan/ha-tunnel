use anyhow::Result;
use config::Config as ConfigParser;
use std::path::PathBuf;
use tracing::Level;

pub struct Config {
    pub log_level: Level,

    pub server: String,
    pub reconnect_interval: u64,
    pub heartbeat_interval: u64,

    pub secret: String,
}

pub fn parse_config(config_file: PathBuf) -> Result<Config> {
    let settings = ConfigParser::builder()
        .set_default("log_level", "INFO")?
        .set_default("reconnect_interval", 5)?
        .set_default("heartbeat_interval", 30)?
        .add_source(config::File::with_name(config_file.to_str().unwrap()).required(false))
        .add_source(config::Environment::with_prefix("HA_TUNNEL"))
        .build()?;

    let log_level = settings.get_string("log_level")?.parse()?;

    let server = settings.get_string("server")?;
    let reconnect_interval = settings.get_int("reconnect_interval")?.try_into()?;
    let heartbeat_interval = settings.get_int("heartbeat_interval")?.try_into()?;

    let secret = settings.get_string("secret")?;

    Ok(Config {
        log_level,

        server,
        reconnect_interval,
        heartbeat_interval,

        secret,
    })
}
