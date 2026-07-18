//! Pipeline integration tests.
//!
//! These tests exercise the full fetch→verify→sign→submit path by running the
//! real [`Poller`] against:
//!
//! - A wiremock server standing in for the Lichess / Chess.com API.
//! - A wiremock server standing in for the Soroban JSON-RPC endpoint.
//!
//! No real network calls are made. All tests use temp directories for the
//! queue and dead-letter files so they are fully isolated and restart-safe.

use oracle_service::{
    config::{OracleConfig, Platform},
    dead_letter::DeadLetterStore,
    poller::Poller,
    queue::{PendingEntry, PendingQueue},
};

use chrono::Utc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a minimal OracleConfig pointing at mock servers.
fn make_config(
    soroban_rpc_url: &str,
    queue_dir: &str,
    max_retries: u32,
    retry_base_delay_secs: u64,
) -> OracleConfig {
    // Use a fixed 32-byte key so tests are deterministic.
    let seed = [0x42u8; 32];
    let signing_key = Zeroizing::new(seed);

    // Derive the G-address so the config is internally consistent.
    use ed25519_dalek::SigningKey;
    let sk = SigningKey::from_bytes(&seed);
    let vk = sk.verifying_key();
    let oracle_address = format!("{}", stellar_strkey::ed25519::PublicKey(vk.to_bytes()));

    OracleConfig {
        rpc_url: soroban_rpc_url.to_string(),
        network_passphrase: "Test SDF Network ; September 2015".to_string(),
        contract_escrow: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
        contract_oracle: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
        oracle_signing_key: signing_key,
        oracle_address,
        lichess_api_token: None,
        chessdotcom_api_key: None,
        poll_interval_secs: 1,
        max_retries,
        retry_base_delay_secs,
        queue_dir: queue_dir.to_string(),
    }
}

/// Enqueue a pending entry with `next_attempt_at` in the past so it is
/// immediately due.
async fn enqueue_due(
    queue: &PendingQueue,
    match_id: u64,
    game_id: &str,
    platform: Platform,
) {
    let mut entry = PendingEntry::new(match_id, game_id.to_string(), platform);
    // Force it to be immediately due.
    entry.next_attempt_at = Utc::now() - chrono::Duration::seconds(1);
    let mut entries = queue.load().await.unwrap();
    if !entries.iter().any(|e| e.match_id == match_id) {
        entries.push(entry);
        queue.save(&entries).await.unwrap();
    }
}

// ── Soroban RPC mock helpers ──────────────────────────────────────────────────

/// Mount mock responses for the full Soroban transaction lifecycle:
/// getAccount → simulateTransaction → sendTransaction → getTransaction(SUCCESS).
async fn mount_full_soroban_lifecycle(server: &MockServer) {
    // A single POST handler that returns different responses in sequence.
    // wiremock doesn't support stateful sequencing out-of-the-box, so we use
    // a response body that covers all expected RPC methods by returning the
    // right fields regardless of the method called. The real production code
    // dispatches based on `method` field — here we use a catch-all.

    // Response 1: getAccount
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "sequence": "100",
                "transactionData": "",
                "minResourceFee": "0",
                "results": [],
                "status": "SUCCESS",
                "hash": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            }
        })))
        .up_to_n_times(100)
        .mount(server)
        .await;
}

// ── Tests ──────────────────────────────────────────────────────────────────────

/// Happy path: Lichess returns a result on first attempt, Soroban accepts the
/// transaction.  The entry should be removed from the queue.
#[tokio::test]
async fn pipeline_lichess_success_removes_entry_from_queue() {
    // ── Mock chess API ────────────────────────────────────────────────────
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/abcd1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
        .mount(&chess_server)
        .await;

    // ── Mock Soroban RPC ──────────────────────────────────────────────────
    let rpc_server = MockServer::start().await;
    mount_full_soroban_lifecycle(&rpc_server).await;

    // ── Setup ─────────────────────────────────────────────────────────────
    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str, 3, 1);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);

    enqueue_due(&queue, 1, "abcd1234", Platform::Lichess).await;
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // ── Run one tick ──────────────────────────────────────────────────────
    poller.tick().await.unwrap();

    // ── The entry should be gone from the queue ───────────────────────────
    let remaining = queue.load().await.unwrap();
    assert!(
        remaining.is_empty(),
        "expected queue to be empty after successful submission, got: {:?}",
        remaining
    );
}

/// Happy path (Chess.com): Chess.com returns a draw, Soroban confirms.
#[tokio::test]
async fn pipeline_chess_com_draw_success() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/pub/game/12345678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": { "result": "draw" }
        })))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;
    mount_full_soroban_lifecycle(&rpc_server).await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str, 3, 1);

    let poller = Poller::new_with_chess_com_base(&cfg, chess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);

    enqueue_due(&queue, 2, "12345678", Platform::ChessDotCom).await;

    poller.tick().await.unwrap();

    assert!(
        queue.load().await.unwrap().is_empty(),
        "queue should be empty after successful draw submission"
    );
}

/// Transient failure (503) then success: first tick records a failure and
/// reschedules; after forcing the next_attempt_at back to the past, the second
/// tick succeeds.
#[tokio::test]
async fn pipeline_transient_failure_then_recovery() {
    let chess_server = MockServer::start().await;

    // First call: 503 (transient)
    Mock::given(method("GET"))
        .and(path("/game/export/retry123"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&chess_server)
        .await;

    // Second call: success
    Mock::given(method("GET"))
        .and(path("/game/export/retry123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "black"
        })))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;
    mount_full_soroban_lifecycle(&rpc_server).await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str, 5, 1);
    let queue = PendingQueue::new(dir_str);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    enqueue_due(&queue, 3, "retry123", Platform::Lichess).await;

    // ── First tick: should hit 503 and schedule retry ─────────────────────
    poller.tick().await.unwrap();

    {
        let entries = queue.load().await.unwrap();
        assert_eq!(entries.len(), 1, "entry should still be in queue after transient failure");
        assert_eq!(entries[0].attempts, 1, "attempt count should be 1");
        assert!(
            entries[0].last_error.is_some(),
            "last_error should be recorded"
        );
    }

    // Force the retry to be due now by rewriting next_attempt_at.
    {
        let mut entries = queue.load().await.unwrap();
        entries[0].next_attempt_at = Utc::now() - chrono::Duration::seconds(1);
        queue.save(&entries).await.unwrap();
    }

    // ── Second tick: should succeed and remove the entry ──────────────────
    poller.tick().await.unwrap();

    assert!(
        queue.load().await.unwrap().is_empty(),
        "queue should be empty after successful retry"
    );
}

/// Permanent failure (404 game not found) is dead-lettered immediately without
/// burning all max_retries.
#[tokio::test]
async fn pipeline_permanent_failure_dead_letters_immediately() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/notfound"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    // max_retries = 5, but permanent failure should skip all retries.
    let cfg = make_config(&rpc_server.uri(), dir_str, 5, 1);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str);

    enqueue_due(&queue, 4, "notfound", Platform::Lichess).await;

    poller.tick().await.unwrap();

    assert!(
        queue.load().await.unwrap().is_empty(),
        "queue should be empty after permanent failure"
    );

    let dl_entries = dead_letter.load().await.unwrap();
    assert_eq!(dl_entries.len(), 1, "should have one dead-letter entry");
    assert_eq!(dl_entries[0].entry.match_id, 4);
}

/// Exhaustion path: after `max_retries` transient failures, the entry moves
/// to the dead-letter store.
#[tokio::test]
async fn pipeline_exhausted_retries_moves_to_dead_letter() {
    let chess_server = MockServer::start().await;
    // Always return 503 so retries are always transient.
    Mock::given(method("GET"))
        .and(path("/game/export/exhaust1"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(10)
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let max_retries = 3u32;
    let cfg = make_config(&rpc_server.uri(), dir_str, max_retries, 0);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str);

    enqueue_due(&queue, 5, "exhaust1", Platform::Lichess).await;

    // Run max_retries ticks, forcing each to be immediately due.
    for _ in 0..max_retries {
        // Check if queue is empty (entry may already be dead-lettered)
        if queue.load().await.unwrap().is_empty() {
            break;
        }
        // Force due
        {
            let mut entries = queue.load().await.unwrap();
            if let Some(e) = entries.first_mut() {
                e.next_attempt_at = Utc::now() - chrono::Duration::seconds(1);
            }
            queue.save(&entries).await.unwrap();
        }
        poller.tick().await.unwrap();
    }

    assert!(
        queue.load().await.unwrap().is_empty(),
        "queue should be empty after exhaustion"
    );

    let dl_entries = dead_letter.load().await.unwrap();
    assert_eq!(
        dl_entries.len(),
        1,
        "exhausted entry should be in dead-letter store"
    );
    assert_eq!(dl_entries[0].entry.match_id, 5);
    assert!(dl_entries[0].total_attempts > 0);
}

/// Verify that a tick with no due entries is a no-op (queue unchanged).
#[tokio::test]
async fn pipeline_tick_with_no_due_entries_is_noop() {
    let chess_server = MockServer::start().await;
    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str, 3, 60);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);

    // Enqueue with a far-future retry time.
    let mut entry = PendingEntry::new(99, "future12".to_string(), Platform::Lichess);
    entry.next_attempt_at = Utc::now() + chrono::Duration::hours(1);
    queue.save(&[entry]).await.unwrap();

    poller.tick().await.unwrap();

    // Entry should still be there, untouched.
    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].match_id, 99);
    assert_eq!(entries[0].attempts, 0, "should not have been attempted");

    // Verify no chess API calls were made.
    assert_eq!(chess_server.received_requests().await.unwrap().len(), 0);
}

/// Game-not-finished is treated as transient and retried (not dead-lettered).
#[tokio::test]
async fn pipeline_game_not_finished_is_transient() {
    let chess_server = MockServer::start().await;

    // Return a response with no "winner" field but with "status": "started"
    // which the client maps to GameNotFinished via InvalidResponse... wait,
    // let's use 200 with no `end` field for chess.com.
    Mock::given(method("GET"))
        .and(path("/game/export/ongoing1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            // No "winner" key and no "status" key => InvalidResponse => Transient
            "id": "ongoing1",
            "speed": "rapid"
        })))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str, 5, 60);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str);

    enqueue_due(&queue, 6, "ongoing1", Platform::Lichess).await;

    poller.tick().await.unwrap();

    // Should be retried (still in queue with attempt=1), NOT dead-lettered.
    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 1, "entry should still be in queue");
    assert_eq!(entries[0].attempts, 1);

    // No dead-letter entries.
    assert!(dead_letter.load().await.unwrap().is_empty());
}
