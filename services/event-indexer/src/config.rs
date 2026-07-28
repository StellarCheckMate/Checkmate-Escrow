use anyhow::{anyhow, Result};
use std::env;

/// Top-level configuration for the event-indexer service.
///
/// All fields are populated from environment variables so that the binary is
/// twelve-factor compliant and can run identically in Docker, Kubernetes, or
/// bare-metal environments.
#[derive(Clone, Debug)]
pub struct Config {
    // ── Soroban RPC ────────────────────────────────────────────────────────
    pub rpc_url: String,
    pub contract_escrow: String,

    // ── PostgreSQL (write + read pools) ───────────────────────────────────
    /// Primary (read-write) database DSN.
    /// e.g. `postgres://user:pass@host:5432/dbname`
    pub database_url: String,
    /// Optional read-replica DSN for query endpoints.
    /// Falls back to `database_url` when not set.
    pub database_read_url: String,
    /// Max connections in the write pool.
    pub db_pool_size: usize,
    /// Max connections in the read-replica pool.
    pub db_read_pool_size: usize,

    // ── Leader election ───────────────────────────────────────────────────
    /// A unique identifier for this indexer instance.
    /// Defaults to the hostname; override with `EVENT_INDEXER_INSTANCE_ID`.
    pub instance_id: String,
    /// Seconds the leader lease is valid for before it must be renewed.
    pub leader_ttl_secs: u64,
    /// Seconds between heartbeat renewals (must be < `leader_ttl_secs`).
    pub leader_heartbeat_secs: u64,

    // ── API server ────────────────────────────────────────────────────────
    pub bind_addr: String,
    pub bind_port: u16,

    // ── Cache ─────────────────────────────────────────────────────────────
    pub cache_size: usize,
    /// Optional Redis DSN for the shared API response cache
    /// (e.g. `redis://localhost:6379`).
    ///
    /// When unset the service falls back to a process-local response cache with
    /// the same TTLs, so a single-node deployment needs no Redis.  Multi-replica
    /// deployments should set it so that one replica's invalidation is seen by
    /// all of them.
    pub redis_url: Option<String>,

    // ── Polling ───────────────────────────────────────────────────────────
    pub poll_interval_secs: u64,

    // ── Logging ───────────────────────────────────────────────────────────
    pub log_level: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // ── RPC / contract ────────────────────────────────────────────────
        let rpc_url = env::var("STELLAR_RPC_URL")
            .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string());

        let contract_escrow = env::var("CONTRACT_ESCROW")
            .map_err(|_| anyhow!("CONTRACT_ESCROW environment variable not set"))?;

        if contract_escrow.len() != 56
            || !contract_escrow.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Err(anyhow!(
                "CONTRACT_ESCROW must be a valid 56-character Stellar contract address, got {:?}",
                contract_escrow
            ));
        }

        // ── PostgreSQL ────────────────────────────────────────────────────
        let database_url = env::var("DATABASE_URL").map_err(|_| {
            anyhow!(
                "DATABASE_URL environment variable not set \
                 (e.g. postgres://user:pass@localhost:5432/event_indexer)"
            )
        })?;

        let database_read_url = env::var("DATABASE_READ_URL")
            .unwrap_or_else(|_| database_url.clone());

        let db_pool_size = env::var("EVENT_INDEXER_DB_POOL_SIZE")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<usize>()
            .map_err(|_| anyhow!("EVENT_INDEXER_DB_POOL_SIZE must be a positive integer"))?;

        let db_read_pool_size = env::var("EVENT_INDEXER_DB_READ_POOL_SIZE")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<usize>()
            .map_err(|_| anyhow!("EVENT_INDEXER_DB_READ_POOL_SIZE must be a positive integer"))?;

        // ── Leader election ───────────────────────────────────────────────
        let instance_id = env::var("EVENT_INDEXER_INSTANCE_ID")
            .or_else(|_| {
                std::fs::read_to_string("/etc/hostname")
                    .map(|h| h.trim().to_string())
                    .map_err(|_| std::env::VarError::NotPresent)
            })
            .unwrap_or_else(|_| format!("instance-{}", uuid::Uuid::new_v4()));

        let leader_ttl_secs = env::var("EVENT_INDEXER_LEADER_TTL_SECS")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<u64>()
            .map_err(|_| anyhow!("EVENT_INDEXER_LEADER_TTL_SECS must be a positive integer"))?;

        let leader_heartbeat_secs = env::var("EVENT_INDEXER_LEADER_HEARTBEAT_SECS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u64>()
            .map_err(|_| {
                anyhow!("EVENT_INDEXER_LEADER_HEARTBEAT_SECS must be a positive integer")
            })?;

        if leader_heartbeat_secs >= leader_ttl_secs {
            return Err(anyhow!(
                "EVENT_INDEXER_LEADER_HEARTBEAT_SECS ({}) must be less than \
                 EVENT_INDEXER_LEADER_TTL_SECS ({})",
                leader_heartbeat_secs,
                leader_ttl_secs
            ));
        }

        // ── API server ────────────────────────────────────────────────────
        let bind_addr = env::var("EVENT_INDEXER_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1".to_string());

        let bind_port = env::var("EVENT_INDEXER_PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse::<u16>()?;

        // ── Cache ─────────────────────────────────────────────────────────
        let cache_size = env::var("EVENT_INDEXER_CACHE_SIZE")
            .unwrap_or_else(|_| "10000".to_string())
            .parse::<usize>()?;

        // Blank is treated as unset so `REDIS_URL=` in an env file disables the
        // shared cache instead of failing to connect on every start-up.
        let redis_url = env::var("REDIS_URL")
            .ok()
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty());

        if let Some(ref url) = redis_url {
            if !(url.starts_with("redis://") || url.starts_with("rediss://")) {
                return Err(anyhow!(
                    "REDIS_URL must start with redis:// or rediss://, got {:?}",
                    url
                ));
            }
        }

        // ── Polling ───────────────────────────────────────────────────────
        let poll_interval_secs = env::var("EVENT_INDEXER_POLL_INTERVAL")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<u64>()?;

        if poll_interval_secs < 1 || poll_interval_secs > 60 {
            return Err(anyhow!(
                "poll_interval_secs must be between 1 and 60, got {}",
                poll_interval_secs
            ));
        }

        // ── Logging ───────────────────────────────────────────────────────
        let log_level = env::var("EVENT_INDEXER_LOG_LEVEL")
            .unwrap_or_else(|_| "info".to_string());

        Ok(Config {
            rpc_url,
            contract_escrow,
            database_url,
            database_read_url,
            db_pool_size,
            db_read_pool_size,
            instance_id,
            leader_ttl_secs,
            leader_heartbeat_secs,
            bind_addr,
            bind_port,
            cache_size,
            redis_url,
            poll_interval_secs,
            log_level,
        })
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Serialize env-mutating tests to avoid data races between parallel threads.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    const VALID_ADDR: &str = "CABD7H7QWXSTDZ6YPMPZRJ2FLGDWP5AYWLF5PYQRB5PQV6PDBGFPMTDX";
    const VALID_DSN: &str = "postgres://user:pass@localhost:5432/test";

    fn set_base_env() {
        env::set_var("CONTRACT_ESCROW", VALID_ADDR);
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "5");
        env::set_var("DATABASE_URL", VALID_DSN);
        env::set_var("EVENT_INDEXER_LEADER_TTL_SECS", "30");
        env::set_var("EVENT_INDEXER_LEADER_HEARTBEAT_SECS", "10");
    }

    #[test]
    fn valid_56_char_contract_address_is_accepted() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        assert!(Config::from_env().is_ok());
    }

    #[test]
    fn short_contract_address_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("CONTRACT_ESCROW", "CSHORT");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn empty_contract_address_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("CONTRACT_ESCROW", "");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn poll_interval_zero_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "0");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn poll_interval_61_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "61");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn poll_interval_1_is_accepted() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "1");
        assert!(Config::from_env().is_ok());
    }

    #[test]
    fn poll_interval_60_is_accepted() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("EVENT_INDEXER_POLL_INTERVAL", "60");
        assert!(Config::from_env().is_ok());
    }

    #[test]
    fn missing_database_url_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::remove_var("DATABASE_URL");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn heartbeat_gte_ttl_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("EVENT_INDEXER_LEADER_TTL_SECS", "10");
        env::set_var("EVENT_INDEXER_LEADER_HEARTBEAT_SECS", "10");
        assert!(Config::from_env().is_err());
    }

    #[test]
    fn read_url_falls_back_to_write_url() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::remove_var("DATABASE_READ_URL");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.database_read_url, cfg.database_url);
    }

    #[test]
    fn explicit_read_url_is_used() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("DATABASE_READ_URL", "postgres://ro:pass@replica:5432/test");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.database_read_url, "postgres://ro:pass@replica:5432/test");
        env::remove_var("DATABASE_READ_URL");
    }

    // ── Redis (API response cache) ────────────────────────────────────────

    #[test]
    fn redis_url_is_optional() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::remove_var("REDIS_URL");
        let cfg = Config::from_env().unwrap();
        assert!(cfg.redis_url.is_none(), "no Redis is a supported setup");
    }

    #[test]
    fn valid_redis_url_is_accepted() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("REDIS_URL", "redis://localhost:6379");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.redis_url.as_deref(), Some("redis://localhost:6379"));
        env::remove_var("REDIS_URL");
    }

    #[test]
    fn tls_redis_url_is_accepted() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("REDIS_URL", "rediss://cache.internal:6380");
        assert!(Config::from_env().unwrap().redis_url.is_some());
        env::remove_var("REDIS_URL");
    }

    #[test]
    fn blank_redis_url_is_treated_as_unset() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("REDIS_URL", "   ");
        let cfg = Config::from_env().unwrap();
        assert!(cfg.redis_url.is_none());
        env::remove_var("REDIS_URL");
    }

    #[test]
    fn redis_url_with_wrong_scheme_is_rejected() {
        let _lock = ENV_LOCK.lock().unwrap();
        set_base_env();
        env::set_var("REDIS_URL", "http://localhost:6379");
        assert!(Config::from_env().is_err());
        env::remove_var("REDIS_URL");
    }
}
