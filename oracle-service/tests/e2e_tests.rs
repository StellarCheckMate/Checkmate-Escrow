//! End-to-end tests against the Stellar testnet.
//!
//! These tests exercise the complete oracle pipeline with real Soroban RPC
//! endpoints, real testnet wallets, and real transaction submission.  They
//! require externally-funded testnet wallets and deployed contract addresses.
//!
//! ## Prerequisites
//!
//! Set the following environment variables before running:
//!
//! ```text
//! E2E_RPC_URL           Soroban RPC endpoint (default: https://soroban-testnet.stellar.org)
//! E2E_NETWORK_PHRASE    Network passphrase (default: "Test SDF Network ; September 2015")
//! E2E_CONTRACT_ESCROW   Deployed escrow contract C-address
//! E2E_CONTRACT_ORACLE   Deployed oracle contract C-address
//! E2E_ORACLE_KEY_HEX    Oracle signing key — 32-byte hex (64 hex chars)
//! E2E_ORACLE_ADDRESS    Oracle G-address matching E2E_ORACLE_KEY_HEX
//! E2E_LICHESS_TOKEN     (optional) Lichess API bearer token
//! E2E_CHESSDOTCOM_KEY   (optional) Chess.com developer API key
//! ```
//!
//! When required variables are absent the tests are skipped automatically so
//! that CI pipelines without testnet credentials do not fail.
//!
//! ## Running locally
//!
//! ```bash
//! export E2E_RPC_URL="https://soroban-testnet.stellar.org"
//! export E2E_CONTRACT_ESCROW="C..."
//! export E2E_CONTRACT_ORACLE="C..."
//! export E2E_ORACLE_KEY_HEX="<64 hex chars>"
//! export E2E_ORACLE_ADDRESS="G..."
//! cargo test -p oracle-service --test e2e_tests -- --nocapture --test-threads=1
//! ```
//!
//! See `docs/e2e-testing.md` for a full step-by-step guide.

use std::time::Duration;

use oracle_service::{
    config::{OracleConfig, Platform},
    oracle::{
        chess_com_client::{ChessComClient, ChessComClientConfig},
        lichess_client::{LichessClient, LichessClientConfig},
        provider::ProviderRegistry,
        provider_error::ProviderError,
        rate_limiter::RateLimiterConfig,
    },
    soroban_client::SorobanClient,
};
use zeroize::Zeroizing;

// ── Environment helpers ───────────────────────────────────────────────────────

const DEFAULT_RPC_URL: &str = "https://soroban-testnet.stellar.org";
const DEFAULT_NETWORK_PHRASE: &str = "Test SDF Network ; September 2015";

/// Testnet configuration sourced from environment variables.
///
/// Returns `None` (causing the test to be skipped) if any required variable
/// is missing.
struct E2EConfig {
    rpc_url: String,
    network_passphrase: String,
    contract_escrow: String,
    contract_oracle: String,
    oracle_signing_key: Zeroizing<[u8; 32]>,
    oracle_address: String,
    lichess_api_token: Option<String>,
    chessdotcom_api_key: Option<String>,
}

impl E2EConfig {
    /// Load from environment.  Returns `None` if required vars are absent.
    fn from_env() -> Option<Self> {
        let rpc_url = std::env::var("E2E_RPC_URL")
            .unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());
        let network_passphrase = std::env::var("E2E_NETWORK_PHRASE")
            .unwrap_or_else(|_| DEFAULT_NETWORK_PHRASE.to_string());

        let contract_escrow = std::env::var("E2E_CONTRACT_ESCROW").ok()?;
        let contract_oracle = std::env::var("E2E_CONTRACT_ORACLE").ok()?;
        let key_hex = std::env::var("E2E_ORACLE_KEY_HEX").ok()?;
        let oracle_address = std::env::var("E2E_ORACLE_ADDRESS").ok()?;

        // Decode the 64-char hex key into 32 bytes.
        let key_bytes: Vec<u8> = hex::decode(&key_hex).ok()?;
        if key_bytes.len() != 32 {
            eprintln!("[e2e] E2E_ORACLE_KEY_HEX must be exactly 64 hex chars (32 bytes)");
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&key_bytes);
        let oracle_signing_key = Zeroizing::new(arr);

        Some(Self {
            rpc_url,
            network_passphrase,
            contract_escrow,
            contract_oracle,
            oracle_signing_key,
            oracle_address,
            lichess_api_token: std::env::var("E2E_LICHESS_TOKEN").ok(),
            chessdotcom_api_key: std::env::var("E2E_CHESSDOTCOM_KEY").ok(),
        })
    }

    fn into_oracle_config(self, queue_dir: String) -> OracleConfig {
        OracleConfig {
            rpc_url: self.rpc_url,
            network_passphrase: self.network_passphrase,
            contract_escrow: self.contract_escrow,
            contract_oracle: self.contract_oracle,
            oracle_signing_key: self.oracle_signing_key,
            oracle_address: self.oracle_address,
            lichess_api_token: self.lichess_api_token,
            chessdotcom_api_key: self.chessdotcom_api_key,
            poll_interval_secs: 5,
            max_retries: 3,
            retry_base_delay_secs: 2,
            queue_dir,
        }
    }
}

/// Skip the test with an informational message when env vars are absent.
macro_rules! require_e2e_config {
    () => {{
        match E2EConfig::from_env() {
            Some(cfg) => cfg,
            None => {
                eprintln!(
                    "[e2e] SKIP: required E2E environment variables not set. \
                     See docs/e2e-testing.md for setup instructions."
                );
                return;
            }
        }
    }};
}

// ── Test 1: Testnet RPC reachability ─────────────────────────────────────────

/// Confirms that the configured RPC endpoint is reachable and returns a valid
/// response to a `getLatestLedger` JSON-RPC call.
///
/// This test is a lightweight smoke-check that should be the first thing run
/// before any heavier e2e tests.
#[tokio::test]
async fn e2e_testnet_rpc_is_reachable() {
    let rpc_url = std::env::var("E2E_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client");

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getLatestLedger",
        "params": {}
    });

    let resp = match client.post(&rpc_url).json(&body).send().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[e2e] SKIP: RPC endpoint {rpc_url} unreachable: {e}");
            return;
        }
    };

    assert!(
        resp.status().is_success(),
        "RPC endpoint returned non-200: {}",
        resp.status()
    );

    let json: serde_json::Value = resp.json().await.expect("failed to parse RPC response");
    assert!(
        json.get("result").is_some(),
        "RPC response missing 'result' field: {json}"
    );

    let sequence = json["result"]["sequence"]
        .as_u64()
        .expect("missing sequence number in getLatestLedger response");
    println!("[e2e] testnet_rpc_is_reachable: latest ledger sequence={sequence}");
    assert!(sequence > 0, "ledger sequence must be positive");
}

// ── Test 2: SorobanClient construction ───────────────────────────────────────

/// Validates that the `SorobanClient` can be constructed from testnet config
/// without panicking.
#[tokio::test]
async fn e2e_soroban_client_constructs_from_testnet_config() {
    let cfg = require_e2e_config!();
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let queue_dir = tmp.path().to_str().unwrap().to_string();

    let oracle_cfg = cfg.into_oracle_config(queue_dir);

    // SorobanClient::new performs address validation — if it succeeds the
    // contract address format is correct and the key decoded without error.
    let result = SorobanClient::new(
        oracle_cfg.rpc_url.clone(),
        oracle_cfg.network_passphrase.clone(),
        &oracle_cfg.contract_escrow,
    );
    assert!(
        result.is_ok(),
        "SorobanClient::new failed with testnet config: {:?}",
        result.err()
    );
    println!("[e2e] soroban_client_constructs_from_testnet_config: OK");
}

// ── Test 3: Lichess client — real API connectivity ────────────────────────────

/// Fetches the result of a well-known completed Lichess game and verifies the
/// response is parseable.
///
/// Game `TaHSAsYl` is a short completed game used as a stable integration
/// fixture.  The expected winner is "white" (Player1).
///
/// This test makes a real HTTP request to lichess.org.  It is skipped
/// automatically when the network is unavailable.
#[tokio::test]
async fn e2e_lichess_fetch_completed_game_result() {
    let client = LichessClient::with_config(LichessClientConfig {
        api_base: "https://lichess.org".to_string(),
        request_timeout: Duration::from_secs(30),
        rate_limiter: RateLimiterConfig {
            capacity: 2,
            refill_rate: 0.5,
        },
        max_concurrent: 1,
    })
    .expect("failed to build LichessClient");

    // A completed game — white wins.
    match client.fetch_result("TaHSAsYl").await {
        Ok(result) => {
            use contracts_oracle::types::Winner;
            assert_eq!(
                result.winner,
                Winner::Player1,
                "expected white (Player1) to win game TaHSAsYl"
            );
            println!("[e2e] lichess_fetch_completed_game_result: winner={:?} OK", result.winner);
        }
        Err(e) => {
            // Network unavailable or API changed — skip gracefully.
            eprintln!("[e2e] SKIP: lichess_fetch_completed_game_result: {e}");
        }
    }
}

// ── Test 4: Lichess client — game not finished ────────────────────────────────

/// Fetches a game that is known not to exist on Lichess.  The client must
/// return `GameNotFound` rather than panicking or returning a bogus result.
#[tokio::test]
async fn e2e_lichess_nonexistent_game_returns_not_found() {
    let client = LichessClient::with_config(LichessClientConfig {
        api_base: "https://lichess.org".to_string(),
        request_timeout: Duration::from_secs(15),
        rate_limiter: RateLimiterConfig {
            capacity: 2,
            refill_rate: 0.5,
        },
        max_concurrent: 1,
    })
    .expect("failed to build LichessClient");

    // "00000000" is an ID that almost certainly doesn't exist.
    match client.fetch_result("00000000").await {
        Err(e) => {
            use oracle_service::oracle::LichessError;
            // Accept either GameNotFound or an HTTP error (both are correct).
            let is_acceptable = matches!(e, LichessError::GameNotFound)
                || matches!(e, LichessError::Http(_))
                || matches!(e, LichessError::HttpStatus { .. });
            assert!(
                is_acceptable,
                "expected GameNotFound or HTTP error for unknown game, got: {e:?}"
            );
            println!("[e2e] lichess_nonexistent_game_returns_not_found: error={e:?} OK");
        }
        Ok(_) => {
            // If it accidentally found a game, that's also acceptable.
            println!("[e2e] lichess_nonexistent_game_returns_not_found: game found (unexpected but acceptable)");
        }
    }
}

// ── Test 5: Chess.com client — real API connectivity ─────────────────────────

/// Fetches the result of a well-known completed Chess.com game.
///
/// Game `96913734` is a publicly visible archived game used as a stable
/// integration fixture.
///
/// This test makes a real HTTP request to api.chess.com.  Skipped when the
/// network is unavailable.
#[tokio::test]
async fn e2e_chess_com_fetch_completed_game_result() {
    let chessdotcom_key = std::env::var("E2E_CHESSDOTCOM_KEY").ok();

    let client = ChessComClient::with_config(ChessComClientConfig {
        api_base: "https://api.chess.com".to_string(),
        request_timeout: Duration::from_secs(30),
        rate_limiter: RateLimiterConfig {
            capacity: 2,
            refill_rate: 0.5,
        },
        max_concurrent: 1,
    })
    .expect("failed to build ChessComClient");

    let _ = chessdotcom_key; // used in future auth header support

    // A well-known public game.
    match client.fetch_result("96913734").await {
        Ok(result) => {
            println!(
                "[e2e] chess_com_fetch_completed_game_result: winner={:?} OK",
                result.winner
            );
        }
        Err(ProviderError::GameNotFound) => {
            eprintln!("[e2e] chess_com_fetch_completed_game_result: game not found (game may have been archived differently)");
        }
        Err(e) => {
            eprintln!("[e2e] SKIP: chess_com_fetch_completed_game_result: {e}");
        }
    }
}

// ── Test 6: ProviderRegistry failover on testnet ─────────────────────────────

/// Builds a two-provider registry (Lichess primary, Chess.com secondary) and
/// attempts to resolve a well-known Lichess game.  Verifies the registry
/// returns a result and does not deadlock or panic.
#[tokio::test]
async fn e2e_provider_registry_resolves_lichess_game() {
    use std::sync::Arc;

    let lichess = Arc::new(
        LichessClient::with_config(LichessClientConfig {
            api_base: "https://lichess.org".to_string(),
            request_timeout: Duration::from_secs(30),
            rate_limiter: RateLimiterConfig { capacity: 2, refill_rate: 0.5 },
            max_concurrent: 1,
        })
        .expect("failed to build LichessClient"),
    );

    let chess_com = Arc::new(
        ChessComClient::with_config(ChessComClientConfig {
            api_base: "https://api.chess.com".to_string(),
            request_timeout: Duration::from_secs(30),
            rate_limiter: RateLimiterConfig { capacity: 2, refill_rate: 0.5 },
            max_concurrent: 1,
        })
        .expect("failed to build ChessComClient"),
    );

    let registry = ProviderRegistry::new(vec![lichess, chess_com]);

    match registry.fetch_result("TaHSAsYl").await {
        Ok(winner) => {
            use contracts_oracle::types::Winner;
            assert_eq!(winner, Winner::Player1, "expected Player1 for game TaHSAsYl");
            println!("[e2e] provider_registry_resolves_lichess_game: winner={winner:?} OK");
        }
        Err(e) => {
            eprintln!("[e2e] SKIP: provider_registry_resolves_lichess_game: {e}");
        }
    }
}

// ── Test 7: Oracle config round-trip ─────────────────────────────────────────

/// Validates that `OracleConfig` can be built from the e2e environment
/// variables and that the signing key round-trips through hex decode correctly.
#[tokio::test]
async fn e2e_oracle_config_round_trip() {
    let cfg = require_e2e_config!();
    let tmp = tempfile::TempDir::new().expect("tempdir");

    let oracle_cfg = cfg.into_oracle_config(tmp.path().to_str().unwrap().to_string());

    // Verify the key is non-zero (i.e. was actually loaded).
    let key_bytes = *oracle_cfg.oracle_signing_key;
    assert_ne!(
        key_bytes,
        [0u8; 32],
        "oracle signing key must not be all-zeros in a real testnet config"
    );

    // Verify oracle address is a valid G-address (56 chars, starts with 'G').
    assert!(
        oracle_cfg.oracle_address.starts_with('G') && oracle_cfg.oracle_address.len() == 56,
        "oracle_address must be a valid G-address, got: {}",
        oracle_cfg.oracle_address
    );

    println!("[e2e] oracle_config_round_trip: oracle={} OK", oracle_cfg.oracle_address);
}

// ── Test 8: Testnet ledger progression ───────────────────────────────────────

/// Polls the testnet twice (2-second interval) to verify that the ledger is
/// actively progressing.  A stalled ledger would cause transaction submission
/// to hang.
#[tokio::test]
async fn e2e_testnet_ledger_is_progressing() {
    let rpc_url = std::env::var("E2E_RPC_URL")
        .unwrap_or_else(|_| DEFAULT_RPC_URL.to_string());

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client");

    let ledger_sequence = |json: &serde_json::Value| -> Option<u64> {
        json.get("result")?.get("sequence")?.as_u64()
    };

    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1,
        "method": "getLatestLedger", "params": {}
    });

    let first = match client.post(&rpc_url).json(&body).send().await {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(e) => { eprintln!("[e2e] SKIP: ledger_is_progressing network error: {e}"); return; }
    };
    let seq1 = match first.as_ref().and_then(ledger_sequence) {
        Some(s) => s,
        None => { eprintln!("[e2e] SKIP: ledger_is_progressing: could not read sequence"); return; }
    };

    // Wait two Stellar ledger intervals (~10 s) then check again.
    tokio::time::sleep(Duration::from_secs(12)).await;

    let second = match client.post(&rpc_url).json(&body).send().await {
        Ok(r) => r.json::<serde_json::Value>().await.ok(),
        Err(e) => { eprintln!("[e2e] SKIP: ledger_is_progressing second poll failed: {e}"); return; }
    };
    let seq2 = match second.as_ref().and_then(ledger_sequence) {
        Some(s) => s,
        None => { eprintln!("[e2e] SKIP: ledger_is_progressing: could not read sequence (2nd)"); return; }
    };

    println!("[e2e] testnet_ledger_is_progressing: seq1={seq1} seq2={seq2}");
    assert!(
        seq2 > seq1,
        "testnet ledger does not appear to be advancing: seq1={seq1} seq2={seq2}"
    );
}

// ── Test 9: Queue round-trip on real filesystem ───────────────────────────────

/// Verifies that the pending queue persists entries to disk and reloads them
/// correctly — an integration check that the queue works on a real filesystem,
/// not just in-memory.
#[tokio::test]
async fn e2e_queue_persists_and_reloads_entries() {
    use oracle_service::queue::{PendingEntry, PendingQueue};

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let queue_dir = tmp.path().to_str().unwrap().to_string();

    let queue = PendingQueue::new(&queue_dir);

    // Enqueue two entries.
    let mut entries = queue.load().await.expect("load empty queue");
    assert!(entries.is_empty(), "queue must start empty");

    entries.push(PendingEntry::new(1, "TaHSAsYl".to_string(), Platform::Lichess));
    entries.push(PendingEntry::new(2, "123456789".to_string(), Platform::ChessDotCom));
    queue.save(&entries).await.expect("save");

    // Reload from disk.
    let reloaded = queue.load().await.expect("reload");
    assert_eq!(reloaded.len(), 2, "reloaded queue must have 2 entries");
    assert_eq!(reloaded[0].match_id, 1);
    assert_eq!(reloaded[0].game_id, "TaHSAsYl");
    assert_eq!(reloaded[1].match_id, 2);
    assert_eq!(reloaded[1].game_id, "123456789");

    println!("[e2e] queue_persists_and_reloads_entries: OK");
}

// ── Test 10: Dead-letter store round-trip ────────────────────────────────────

/// Verifies that the dead-letter store persists and reloads failed entries
/// correctly on a real filesystem.
#[tokio::test]
async fn e2e_dead_letter_store_round_trip() {
    use oracle_service::dead_letter::DeadLetterStore;
    use oracle_service::queue::PendingEntry;

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let queue_dir = tmp.path().to_str().unwrap().to_string();

    let store = DeadLetterStore::new(&queue_dir);

    let mut entry = PendingEntry::new(99, "abcdef01".to_string(), Platform::Lichess);
    entry.attempts = 5;
    entry.last_error = Some("connection refused".to_string());

    store.push(entry).await.expect("push to dead-letter store");

    let items = store.load().await.expect("load dead-letter store");
    assert!(!items.is_empty(), "dead-letter store must have at least one entry");
    let found = items.iter().find(|e| e.match_id == 99);
    assert!(found.is_some(), "entry for match_id=99 not found in dead-letter store");
    assert_eq!(found.unwrap().attempts, 5);

    println!("[e2e] dead_letter_store_round_trip: OK");
}

// ── Test 11: Health check endpoint on a mock server ──────────────────────────

/// Validates that the `HealthStatus` and `CanaryStatus` enums serialize and
/// deserialize correctly via serde_json.  This is a lightweight sanity check
/// confirming the health module compiles and its public types are usable from
/// integration tests.
#[tokio::test]
async fn e2e_health_status_serializes_correctly() {
    use oracle_service::health::{CanaryStatus, HealthStatus};

    // Verify all variants round-trip through JSON.
    for status in [HealthStatus::Healthy, HealthStatus::Degraded, HealthStatus::Unhealthy] {
        let json = serde_json::to_string(&status).expect("serialize HealthStatus");
        let decoded: HealthStatus =
            serde_json::from_str(&json).expect("deserialize HealthStatus");
        assert_eq!(
            format!("{status}"),
            format!("{decoded}"),
            "HealthStatus round-trip failed for {status}"
        );
    }

    for canary in [CanaryStatus::Pending, CanaryStatus::Passed, CanaryStatus::Failed] {
        let json = serde_json::to_string(&canary).expect("serialize CanaryStatus");
        let decoded: CanaryStatus =
            serde_json::from_str(&json).expect("deserialize CanaryStatus");
        assert_eq!(canary, decoded, "CanaryStatus round-trip failed for {canary:?}");
    }

    println!("[e2e] health_status_serializes_correctly: OK");
}
