use anyhow::Result;
use config::Config as ConfigParser;
use std::path::PathBuf;
use tracing::Level;

pub struct Config {
    pub log_level: Level,

    pub host: String,
    pub port: u16,

    pub secret: String,

    pub timeout: u64,
}

pub fn parse_config(config_file: PathBuf) -> Result<Config> {
    let settings = ConfigParser::builder()
        .set_default("log_level", "INFO")?
        .set_default("host", "0.0.0.0")?
        .set_default("port", 3000)?
        .set_default("timeout", 10)?
        .add_source(config::File::with_name(config_file.to_str().unwrap()).required(false))
        .add_source(config::Environment::with_prefix("HA_TUNNEL"))
        .build()?;

    let log_level = settings.get_string("log_level")?.parse()?;
    let host = settings.get_string("host")?;
    let port = settings.get_int("port")?.try_into()?;

    let secret = settings.get_string("secret")?;

    let timeout = settings.get_int("timeout")?.try_into()?;

    Ok(Config {
        log_level,

        host,
        port,

        secret,

        timeout,
    })
}
