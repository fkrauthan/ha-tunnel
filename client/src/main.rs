use crate::config::parse_config;
use crate::tunnel_client::connect;
use anyhow::Result;
use clap::Parser;
use common::now_as_secs;
use common::tunnel::TunnelMessage;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn};
use uuid::Uuid;

mod config;
mod tunnel_client;

#[derive(Parser, Debug)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = parse_config(args.config)?;

    tracing_subscriber::fmt()
        .with_max_level(config.log_level)
        .with_target(false)
        .init();

    info!("Starting Home Assistant Tunnel Client");

    let reconnect_interval = Duration::from_secs(config.reconnect_interval);
    let heartbeat_interval = Duration::from_secs(config.heartbeat_interval);
    let client_id = Uuid::new_v4().to_string();

    loop {
        info!(url = %config.server, "Connecting to server...");

        match connect(&client_id, &config.server, &config.secret).await {
            Ok((tx, mut rx)) => {
                info!("Connected to server");

                // Spawn heartbeat task
                let heartbeat_tx = tx.clone();
                let heartbeat_handle = tokio::spawn(async move {
                    let mut interval = tokio::time::interval(heartbeat_interval);
                    loop {
                        interval.tick().await;
                        let ping = TunnelMessage::Ping {
                            timestamp: now_as_secs(),
                        };
                        if heartbeat_tx.send(ping).await.is_err() {
                            break;
                        }
                    }
                });

                // Process incoming requests
                while let Some(msg) = rx.recv().await {
                    // TODO needs implementation
                    // let response = proxy.handle_request(msg).await;
                    //
                    // if tx.send(response).await.is_err() {
                    //     error!("Failed to send response, connection may be closed");
                    //     break;
                    // }
                }

                heartbeat_handle.abort();
                warn!("Connection to server lost");
            }
            Err(e) => {
                error!("Failed to connect to server: {}", e);
            }
        }

        info!(
            "Reconnecting in {} seconds...",
            reconnect_interval.as_secs()
        );
        sleep(reconnect_interval).await;
    }
}
