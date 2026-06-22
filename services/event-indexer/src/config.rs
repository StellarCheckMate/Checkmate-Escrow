use anyhow::{anyhow, Result};
use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub rpc_url: String,
    pub contract_escrow: String,
    pub db_path: String,
    pub bind_addr: String,
    pub bind_port: u16,
    pub cache_size: usize,
    pub poll_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let rpc_url = env::var("STELLAR_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string());

        let contract_escrow = env::var("CONTRACT_ESCROW")
            .map_err(|_| anyhow!("CONTRACT_ESCROW environment variable not set"))?;

        let db_path = env::var("EVENT_INDEXER_DB_PATH")
            .unwrap_or_else(|_| "./events.db".to_string());

        let bind_addr = env::var("EVENT_INDEXER_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        let bind_port = env::var("EVENT_INDEXER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()?;

        let cache_size = env::var("EVENT_INDEXER_CACHE_SIZE")
            .unwrap_or_else(|_| "10000".to_string())
            .parse::<usize>()?;

        let poll_interval_secs = env::var("EVENT_INDEXER_POLL_INTERVAL")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<u64>()?;

        Ok(Config {
            rpc_url,
            contract_escrow,
            db_path,
            bind_addr,
            bind_port,
            cache_size,
            poll_interval_secs,
        })
    }
}
