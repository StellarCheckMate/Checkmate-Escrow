//! Batched Lichess polling and rate-limit back-pressure (#1725).
//!
//! - `LichessClient::fetch_results_batch` resolves many games per
//!   `POST /api/games/export/_ids` request (up to 300 IDs each).
//! - `Poller::tick` sends all due Lichess games in one batch, falls back to
//!   per-game requests when the batch endpoint is unavailable, and stops
//!   querying a platform that answered 429 until `Retry-After` has elapsed
//!   without charging the deferred entries a retry attempt.
//!
//! The poller scenarios avoid a successful fetch so no Soroban submission is
//! attempted; the RPC mock server is never called.

use chrono::Utc;
use contracts_oracle::types::Winner;
use oracle_service::{
    config::{OracleConfig, Platform},
    oracle::{LichessClient, LichessError, LICHESS_BATCH_MAX_IDS},
    poller::Poller,
    queue::{PendingEntry, PendingQueue},
};
use tempfile::TempDir;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

const BATCH_PATH: &str = "/api/games/export/_ids";

fn ndjson(games: &[serde_json::Value]) -> ResponseTemplate {
    let lines: Vec<String> = games.iter().map(|g| g.to_string()).collect();
    ResponseTemplate::new(200)
        .insert_header("Content-Type", "application/x-ndjson")
        .set_body_string(lines.join("\n"))
}

fn client(server: &MockServer) -> LichessClient {
    LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
        .unwrap()
}

fn make_config(rpc_url: &str, queue_dir: &str) -> OracleConfig {
    OracleConfig {
        rpc_url: rpc_url.to_string(),
        network_passphrase: "Test SDF Network ; September 2015".to_string(),
        contract_escrow: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
        contract_oracle: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
        oracle_signing_key: Zeroizing::new([0x42u8; 32]),
        oracle_address: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF".to_string(),
        lichess_api_token: None,
        chessdotcom_api_key: None,
        poll_interval_secs: 1,
        chessdotcom_poll_interval_secs: 1,
        max_retries: 5,
        retry_base_delay_secs: 1,
        queue_dir: queue_dir.to_string(),
        reconciliation_interval_secs: 1,
        dead_letter_max_entries: 0,
        max_fee_stroops: 1_000_000,
        http_addr: "127.0.0.1:0".to_string(),
    }
}

async fn enqueue_due(queue: &PendingQueue, match_id: u64, game_id: &str, platform: Platform) {
    let mut entry = PendingEntry::new(match_id, game_id.to_string(), platform);
    entry.next_attempt_at = Utc::now() - chrono::Duration::seconds(1);
    let mut entries = queue.load().await.unwrap();
    entries.push(entry);
    queue.save(&entries).await.unwrap();
}

// ── Client ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_resolves_every_game_in_one_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BATCH_PATH))
        .respond_with(ndjson(&[
            serde_json::json!({"id": "white001", "status": "mate", "winner": "white"}),
            serde_json::json!({"id": "black001", "status": "resign", "winner": "black"}),
            serde_json::json!({"id": "draw0001", "status": "draw"}),
            serde_json::json!({"id": "going001", "status": "started"}),
        ]))
        .expect(1)
        .mount(&server)
        .await;

    let ids: Vec<String> = ["white001", "black001", "draw0001", "going001", "missing1", "bad!id"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let results = client(&server).fetch_results_batch(&ids).await.unwrap();

    assert_eq!(results["white001"].as_ref().unwrap().winner, Winner::Player1);
    assert_eq!(results["black001"].as_ref().unwrap().winner, Winner::Player2);
    assert_eq!(results["draw0001"].as_ref().unwrap().winner, Winner::Draw);
    assert!(matches!(results["going001"], Err(LichessError::GameNotFinished)));
    assert!(matches!(results["missing1"], Err(LichessError::GameNotFound)));
    assert!(matches!(results["bad!id"], Err(LichessError::InvalidGameId)));

    // Invalid IDs are never sent to Lichess.
    let requests = server.received_requests().await.unwrap();
    let body = String::from_utf8(requests[0].body.clone()).unwrap();
    assert_eq!(body, "white001,black001,draw0001,going001,missing1");
}

#[tokio::test]
async fn batch_splits_requests_at_lichess_id_limit() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BATCH_PATH))
        .respond_with(ndjson(&[]))
        .expect(2)
        .mount(&server)
        .await;

    let ids: Vec<String> = (0..=LICHESS_BATCH_MAX_IDS).map(|i| format!("g{i:07}")).collect();
    let results = client(&server).fetch_results_batch(&ids).await.unwrap();

    assert_eq!(results.len(), LICHESS_BATCH_MAX_IDS + 1);
    let sizes: Vec<usize> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| String::from_utf8(r.body.clone()).unwrap().split(',').count())
        .collect();
    assert_eq!(sizes, vec![LICHESS_BATCH_MAX_IDS, 1]);
}

#[tokio::test]
async fn batch_rate_limit_surfaces_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BATCH_PATH))
        .respond_with(ResponseTemplate::new(429).append_header("Retry-After", "42"))
        .mount(&server)
        .await;

    let err = client(&server)
        .fetch_results_batch(&["abcd1234".to_string()])
        .await
        .unwrap_err();

    assert!(
        matches!(err, LichessError::RateLimited { retry_after } if retry_after.as_secs() == 42),
        "unexpected error: {err:?}"
    );
}

#[tokio::test]
async fn single_fetch_reports_in_progress_game_as_not_finished() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/going001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "going001", "status": "started"
        })))
        .mount(&server)
        .await;

    let err = client(&server).fetch_result("going001").await.unwrap_err();
    assert!(matches!(err, LichessError::GameNotFinished));
}

// ── Poller ────────────────────────────────────────────────────────────────────

/// All due Lichess games go out in a single batch request.
#[tokio::test]
async fn tick_batches_due_lichess_games_into_one_request() {
    let chess_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BATCH_PATH))
        .respond_with(ndjson(&[
            serde_json::json!({"id": "batch001", "status": "started"}),
            serde_json::json!({"id": "batch002", "status": "started"}),
            serde_json::json!({"id": "batch003", "status": "started"}),
        ]))
        .expect(1)
        .mount(&chess_server)
        .await;
    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let poller =
        Poller::new_with_lichess_base(&make_config(&rpc_server.uri(), dir_str), chess_server.uri())
            .unwrap();
    let queue = PendingQueue::new(dir_str);
    for (match_id, game_id) in [(11, "batch001"), (12, "batch002"), (13, "batch003")] {
        enqueue_due(&queue, match_id, game_id, Platform::Lichess).await;
    }

    poller.tick().await.unwrap();

    let requests = chess_server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1, "expected a single batch request");
    let mut ids: Vec<String> = String::from_utf8(requests[0].body.clone())
        .unwrap()
        .split(',')
        .map(str::to_string)
        .collect();
    ids.sort();
    assert_eq!(ids, ["batch001", "batch002", "batch003"]);

    // In-progress games are retried later, not settled.
    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries.iter().all(|e| e.attempts == 1));
}

/// A 429 on the batch request pauses Lichess polling until Retry-After has
/// elapsed and does not charge the entries a retry attempt.
#[tokio::test]
async fn tick_backs_off_lichess_after_rate_limit_without_charging_attempts() {
    let chess_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BATCH_PATH))
        .respond_with(ResponseTemplate::new(429).append_header("Retry-After", "60"))
        .mount(&chess_server)
        .await;
    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let poller =
        Poller::new_with_lichess_base(&make_config(&rpc_server.uri(), dir_str), chess_server.uri())
            .unwrap();
    let queue = PendingQueue::new(dir_str);
    enqueue_due(&queue, 21, "limit001", Platform::Lichess).await;
    enqueue_due(&queue, 22, "limit002", Platform::Lichess).await;

    poller.tick().await.unwrap();
    poller.tick().await.unwrap();

    assert_eq!(
        chess_server.received_requests().await.unwrap().len(),
        1,
        "second tick must not query lichess while backing off"
    );
    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.attempts == 0), "rate limiting must not burn retries");
}

/// When the batch endpoint is unavailable the poller falls back to the
/// per-game export endpoint.
#[tokio::test]
async fn tick_falls_back_to_per_game_requests_when_batch_unavailable() {
    let chess_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(BATCH_PATH))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&chess_server)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/game/export/fall000[12]$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "started"
        })))
        .expect(2)
        .mount(&chess_server)
        .await;
    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let poller =
        Poller::new_with_lichess_base(&make_config(&rpc_server.uri(), dir_str), chess_server.uri())
            .unwrap();
    let queue = PendingQueue::new(dir_str);
    enqueue_due(&queue, 31, "fall0001", Platform::Lichess).await;
    enqueue_due(&queue, 32, "fall0002", Platform::Lichess).await;

    poller.tick().await.unwrap();

    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.attempts == 1));
}

/// Chess.com has no batch endpoint: once it rate-limits one game, the
/// remaining due Chess.com games wait for the back-off instead of being sent.
#[tokio::test]
async fn tick_defers_remaining_chess_com_games_after_rate_limit() {
    let chess_server = MockServer::start().await;
    // Retry-After 0 keeps the client's own 429 retries instant.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429).append_header("Retry-After", "0"))
        .mount(&chess_server)
        .await;
    let rpc_server = MockServer::start().await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let poller = Poller::new_with_chess_com_base(
        &make_config(&rpc_server.uri(), dir_str),
        chess_server.uri(),
    )
    .unwrap();
    let queue = PendingQueue::new(dir_str);
    enqueue_due(&queue, 41, "11111111", Platform::ChessDotCom).await;
    enqueue_due(&queue, 42, "22222222", Platform::ChessDotCom).await;

    poller.tick().await.unwrap();

    let requests = chess_server.received_requests().await.unwrap();
    assert!(!requests.is_empty());
    assert!(
        requests.iter().all(|r| r.url.path().ends_with("/11111111")),
        "second game must not be requested while chess.com is backing off"
    );
    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.attempts == 0));
}
