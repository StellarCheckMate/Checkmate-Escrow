//! Common test utilities for oracle service tests.

use std::sync::Arc;
use std::time::Duration;

pub use oracle_service::config::{OracleConfig, Platform};
pub use oracle_service::health::{CanaryStatus, CheckResult, CheckStatus, HealthCheckResponse, HealthStatus};
pub use oracle_service::oracle::{ChessComClient, LichessClient};
pub use oracle_service::soroban_client::SorobanClient;

/// Create a minimal test oracle configuration.
pub fn test_config() -> OracleConfig {
    OracleConfig {
        rpc_url: "https://soroban-testnet.stellar.org".to_string(),
        network_passphrase: "Test SDF Network ; September 2015".to_string(),
        contract_escrow: "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P".to_string(),
        contract_oracle: "CCDAKAQSZ6YKQWPVG7FAJSVSB6X2MWKQABVX5PQVHXLX3NK4GQCBKXA".to_string(),
        oracle_signing_key: zeroize::Zeroizing::new([1u8; 32]),
        oracle_address: "GDZST3XVCDTUJ76ZAV2HA72KYYA4JJOBJZNJHXJ6O54BWCQVZQNK7HT".to_string(),
        lichess_api_token: None,
        chessdotcom_api_key: None,
        poll_interval_secs: 30,
        max_retries: 5,
        retry_base_delay_secs: 10,
        queue_dir: "/tmp/test-oracle-queue".to_string(),
    }
}

/// Create a check result for testing.
pub fn test_check_result(status: CheckStatus, latency_ms: u64) -> CheckResult {
    CheckResult {
        status,
        latency_ms,
        last_checked_at: chrono::Utc::now(),
        consecutive_failures: 0,
        details: None,
    }
}

/// Create a check result with an error detail.
pub fn test_check_result_with_detail(
    status: CheckStatus,
    latency_ms: u64,
    details: String,
) -> CheckResult {
    CheckResult {
        status,
        latency_ms,
        last_checked_at: chrono::Utc::now(),
        consecutive_failures: 1,
        details: Some(details),
    }
}
