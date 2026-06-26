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
    pub log_level: String,
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

        if poll_interval_secs < 1 || poll_interval_secs > 60 {
            return Err(anyhow!(
                "poll_interval_secs must be between 1 and 60, got {}",
                poll_interval_secs
            ));
        }

        let log_level = env::var("EVENT_INDEXER_LOG_LEVEL")
            .unwrap_or_else(|_| "info".to_string());

        Ok(Config {
            rpc_url,
            contract_escrow,
            db_path,
            bind_addr,
            bind_port,
            cache_size,
            poll_interval_secs,
            log_level,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_env() {
        env::set_var("CONTRACT_ESCROW", "CTEST");
    }

    #[test]
    fn poll_interval_zero_is_rejected() {
        base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "0");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn poll_interval_61_is_rejected() {
        base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "61");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn poll_interval_1_is_accepted() {
        base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "1");
        assert!(Config::from_env().is_ok());
    }

    #[test]
    fn poll_interval_60_is_accepted() {
        base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "60");
        assert!(Config::from_env().is_ok());
    }
}
