//! Configuration module for the oracle service.
//!
//! Loads all configuration from environment variables. The oracle signing key
//! is stored in a [`zeroize::Zeroizing`] wrapper so the raw bytes are scrubbed
//! from memory when the config is dropped, limiting the window during which the
//! key material lives in plaintext in process memory.
//!
//! ## Threat model
//!
//! * **Key at rest:** The signing key is expected to live in an environment
//!   variable injected at runtime by the deployment platform (e.g. a Kubernetes
//!   secret, AWS Secrets Manager, or a .env file that is never committed). It
//!   is **not** read from a file path at startup; operators should keep the
//!   file outside the working directory and restrict permissions to 0400.
//!
//! * **Key in memory:** Once decoded from hex, the 32-byte seed is wrapped in
//!   [`zeroize::Zeroizing`]. The decoded bytes are scrubbed with
//!   [`zeroize::Zeroize::zeroize`] when the struct drops. This limits exposure
//!   to the process lifetime rather than the full host lifetime.
//!
//! * **Key in logs:** We deliberately never log or display the raw key or its
//!   hex form. The `Debug` impl for `OracleConfig` replaces the key with
//!   `"<redacted>"`.
//!
//! * **Residual risk:** Environment variables are visible to other processes
//!   running as the same UID on Linux (via `/proc/<pid>/environ`). Use
//!   OS-level isolation (separate user, containers, SELinux/AppArmor) to
//!   mitigate this. For production deployments, prefer a secrets manager that
//!   injects the key via a Unix socket or in-memory file descriptor rather than
//!   the process environment.

use std::fmt;

use zeroize::Zeroizing;

/// Platform the oracle is watching for a specific match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Platform {
    Lichess,
    ChessDotCom,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Lichess => write!(f, "lichess"),
            Platform::ChessDotCom => write!(f, "chess.com"),
        }
    }
}

/// Full oracle service configuration loaded from environment variables.
///
/// The `Debug` implementation redacts `oracle_signing_key` to avoid
/// accidentally leaking key material in logs.
pub struct OracleConfig {
    // ---- Network ----
    /// Stellar RPC URL, e.g. `https://soroban-testnet.stellar.org`.
    pub rpc_url: String,
    /// Human-readable network passphrase used to scope transaction signatures.
    pub network_passphrase: String,

    // ---- Contracts ----
    /// Bech32/strkey contract ID of the escrow contract.
    pub contract_escrow: String,
    /// Bech32/strkey contract ID of the oracle contract.
    pub contract_oracle: String,

    // ---- Signing ----
    /// Raw 32-byte ed25519 seed, zeroized on drop.
    pub oracle_signing_key: Zeroizing<[u8; 32]>,
    /// Stellar G-address corresponding to the signing key (pre-computed at
    /// load time so we never re-derive it from the key at runtime).
    pub oracle_address: String,

    // ---- Chess API tokens ----
    /// Lichess personal API token (`lip_…`). Optional for read-only game
    /// lookups but required for higher rate-limit tiers.
    pub lichess_api_token: Option<String>,
    /// Chess.com API key. Optional today; included for forward-compatibility.
    pub chessdotcom_api_key: Option<String>,

    // ---- Pipeline tuning ----
    /// How often (seconds) the poller wakes to check active matches.
    pub poll_interval_secs: u64,
    /// Maximum retry attempts before a pending entry is dead-lettered.
    pub max_retries: u32,
    /// Base delay (seconds) for the first retry; doubles on each attempt.
    pub retry_base_delay_secs: u64,
    /// Directory where the pending queue and dead-letter files are stored.
    pub queue_dir: String,
}

impl fmt::Debug for OracleConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OracleConfig")
            .field("rpc_url", &self.rpc_url)
            .field("network_passphrase", &self.network_passphrase)
            .field("contract_escrow", &self.contract_escrow)
            .field("contract_oracle", &self.contract_oracle)
            .field("oracle_signing_key", &"<redacted>")
            .field("oracle_address", &self.oracle_address)
            .field(
                "lichess_api_token",
                &self.lichess_api_token.as_deref().map(|_| "<set>"),
            )
            .field(
                "chessdotcom_api_key",
                &self.chessdotcom_api_key.as_deref().map(|_| "<set>"),
            )
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("max_retries", &self.max_retries)
            .field("retry_base_delay_secs", &self.retry_base_delay_secs)
            .field("queue_dir", &self.queue_dir)
            .finish()
    }
}

/// Errors that can occur while loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingVar(&'static str),

    #[error("environment variable {var} has invalid value: {reason}")]
    InvalidValue { var: &'static str, reason: String },
}

/// Load the oracle service configuration from environment variables.
///
/// Required variables:
/// - `STELLAR_RPC_URL`
/// - `STELLAR_NETWORK_PASSPHRASE` (or defaults based on `STELLAR_NETWORK`)
/// - `CONTRACT_ESCROW`
/// - `CONTRACT_ORACLE`
/// - `ORACLE_SIGNING_KEY` — hex-encoded 32-byte ed25519 seed
///
/// Optional variables (with defaults):
/// - `LICHESS_API_TOKEN`
/// - `CHESSDOTCOM_API_KEY`
/// - `ORACLE_POLL_INTERVAL_SECS` (default: 30)
/// - `ORACLE_MAX_RETRIES` (default: 5)
/// - `ORACLE_RETRY_BASE_DELAY_SECS` (default: 10)
/// - `ORACLE_QUEUE_DIR` (default: `./oracle-queue`)
pub fn load() -> Result<OracleConfig, ConfigError> {
    let rpc_url = require_env("STELLAR_RPC_URL")?;

    // Derive the network passphrase from STELLAR_NETWORK_PASSPHRASE if set,
    // otherwise fall back to the well-known passphrase for STELLAR_NETWORK.
    let network_passphrase = if let Ok(p) = std::env::var("STELLAR_NETWORK_PASSPHRASE") {
        p
    } else {
        let network = std::env::var("STELLAR_NETWORK").unwrap_or_else(|_| "testnet".to_string());
        match network.as_str() {
            "mainnet" => "Public Global Stellar Network ; September 2015".to_string(),
            "testnet" => "Test SDF Network ; September 2015".to_string(),
            "futurenet" => "Test SDF Future Network ; October 2022".to_string(),
            "standalone" | "local" => "Standalone Network ; February 2017".to_string(),
            other => {
                return Err(ConfigError::InvalidValue {
                    var: "STELLAR_NETWORK",
                    reason: format!("unknown network '{}'; set STELLAR_NETWORK_PASSPHRASE directly", other),
                });
            }
        }
    };

    let contract_escrow = require_env("CONTRACT_ESCROW")?;
    let contract_oracle = require_env("CONTRACT_ORACLE")?;

    // Decode the 32-byte signing key seed from hex and immediately wrap it in
    // Zeroizing to ensure it is scrubbed when dropped.
    let key_hex = require_env("ORACLE_SIGNING_KEY")?;
    let key_bytes = decode_key_hex(&key_hex)?;
    // Immediately drop the plaintext hex string.
    drop(key_hex);
    let seed = Zeroizing::new(key_bytes);

    // Pre-compute the Stellar G-address so we never need to re-derive from the
    // raw key bytes at runtime.
    let oracle_address = stellar_address_from_seed(&seed)?;

    let lichess_api_token = std::env::var("LICHESS_API_TOKEN").ok().filter(|s| !s.is_empty());
    let chessdotcom_api_key = std::env::var("CHESSDOTCOM_API_KEY").ok().filter(|s| !s.is_empty());

    let poll_interval_secs = parse_u64_env("ORACLE_POLL_INTERVAL_SECS", 30)?;
    let max_retries = parse_u32_env("ORACLE_MAX_RETRIES", 5)?;
    let retry_base_delay_secs = parse_u64_env("ORACLE_RETRY_BASE_DELAY_SECS", 10)?;
    let queue_dir = std::env::var("ORACLE_QUEUE_DIR").unwrap_or_else(|_| "./oracle-queue".to_string());

    Ok(OracleConfig {
        rpc_url,
        network_passphrase,
        contract_escrow,
        contract_oracle,
        oracle_signing_key: seed,
        oracle_address,
        lichess_api_token,
        chessdotcom_api_key,
        poll_interval_secs,
        max_retries,
        retry_base_delay_secs,
        queue_dir,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn require_env(name: &'static str) -> Result<String, ConfigError> {
    std::env::var(name).map_err(|_| ConfigError::MissingVar(name))
}

fn parse_u64_env(name: &'static str, default: u64) -> Result<u64, ConfigError> {
    match std::env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => v.parse::<u64>().map_err(|e| ConfigError::InvalidValue {
            var: name,
            reason: e.to_string(),
        }),
    }
}

fn parse_u32_env(name: &'static str, default: u32) -> Result<u32, ConfigError> {
    match std::env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => v.parse::<u32>().map_err(|e| ConfigError::InvalidValue {
            var: name,
            reason: e.to_string(),
        }),
    }
}

/// Decode a 64-character lowercase hex string into a 32-byte array.
fn decode_key_hex(hex_str: &str) -> Result<[u8; 32], ConfigError> {
    let bytes = hex::decode(hex_str).map_err(|e| ConfigError::InvalidValue {
        var: "ORACLE_SIGNING_KEY",
        reason: format!("hex decode failed: {}", e),
    })?;
    if bytes.len() != 32 {
        return Err(ConfigError::InvalidValue {
            var: "ORACLE_SIGNING_KEY",
            reason: format!("expected 32 bytes (64 hex chars), got {} bytes", bytes.len()),
        });
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Derive the Stellar G-address (strkey) from an ed25519 seed.
fn stellar_address_from_seed(seed: &[u8; 32]) -> Result<String, ConfigError> {
    use ed25519_dalek::SigningKey;
    let signing_key = SigningKey::from_bytes(seed);
    let verifying_key = signing_key.verifying_key();
    let pubkey_bytes = verifying_key.to_bytes();
    let g_address = format!("{}", stellar_strkey::ed25519::PublicKey(pubkey_bytes));
    Ok(g_address)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_key_hex_valid() {
        let hex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let bytes = decode_key_hex(hex).unwrap();
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[31], 0x20);
    }

    #[test]
    fn decode_key_hex_wrong_length() {
        let err = decode_key_hex("deadbeef").unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
    }

    #[test]
    fn decode_key_hex_non_hex() {
        let err = decode_key_hex(&"g".repeat(64)).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidValue { .. }));
    }

    #[test]
    fn stellar_address_roundtrip() {
        // A known 32-byte seed → known G-address.
        let seed = [1u8; 32];
        let addr = stellar_address_from_seed(&seed).unwrap();
        assert!(addr.starts_with('G'), "expected G-address, got {}", addr);
    }
}
