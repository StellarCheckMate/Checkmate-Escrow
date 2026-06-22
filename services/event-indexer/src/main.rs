mod models;
mod db;
mod rpc;
mod api;
mod cache;
mod config;

use anyhow::Result;
use config::Config;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("event_indexer=info".parse()?),
        )
        .init();

    let config = Config::from_env()?;
    info!("Event Indexer starting with config: {:?}", config);

    let db = Arc::new(db::Database::new(&config.db_path)?);
    let cache = Arc::new(RwLock::new(cache::EventCache::new(config.cache_size)));

    db.init_schema()?;
    info!("Database initialized");

    let rpc_client = Arc::new(rpc::SorobanRpcClient::new(&config.rpc_url)?);

    let api_handle = {
        let db = db.clone();
        let cache = cache.clone();
        let rpc = rpc_client.clone();
        tokio::spawn(async move {
            if let Err(e) = api::start_server(
                &config.bind_addr,
                config.bind_port,
                db,
                cache,
                rpc,
            )
            .await
            {
                error!("API server error: {}", e);
            }
        })
    };

    let poller_handle = {
        let db = db.clone();
        let cache = cache.clone();
        let rpc = rpc_client.clone();
        let config = config.clone();
        tokio::spawn(async move {
            if let Err(e) = rpc::event_poller(
                rpc,
                db,
                cache,
                &config.contract_escrow,
                config.poll_interval_secs,
            )
            .await
            {
                error!("Event poller error: {}", e);
            }
        })
    };

    info!("Event Indexer running on {}:{}", config.bind_addr, config.bind_port);

    tokio::select! {
        _ = api_handle => {
            error!("API server stopped unexpectedly");
        }
        _ = poller_handle => {
            error!("Event poller stopped unexpectedly");
        }
    }

    Ok(())
}
