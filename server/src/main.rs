mod auth;
mod client_ip;
mod config;
mod proxy;

use crate::config::{Config, parse_config};
use crate::proxy::{ClientConnection, create_router};
use anyhow::Result;
use clap::Parser;
use common::tunnel::TunnelMessage;
use dashmap::DashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{oneshot, watch};
use tracing::info;

#[derive(Parser, Debug)]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

struct ServerState {
    config: Config,
    /// Connected clients indexed by client_id
    clients: DashMap<String, ClientConnection>,
    /// Pending requests waiting for responses
    pending_requests: DashMap<String, oneshot::Sender<TunnelMessage>>,
    /// Notifier for when clients connect (sender side)
    client_connected_tx: watch::Sender<usize>,
    /// Notifier for when clients connect (receiver side, clone this to wait)
    client_connected_rx: watch::Receiver<usize>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let config = parse_config(args.config)?;

    tracing_subscriber::fmt()
        .with_max_level(config.log_level)
        .with_target(false)
        .init();

    info!("Starting Home Assistant Tunnel Server");

    let (client_connected_tx, client_connected_rx) = watch::channel(0usize);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;

    info!("Server listening on {}", addr);

    let state = Arc::new(ServerState {
        config,
        clients: DashMap::new(),
        pending_requests: DashMap::new(),
        client_connected_tx,
        client_connected_rx,
    });
    let app = create_router(state.clone());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
