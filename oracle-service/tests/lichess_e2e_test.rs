//! Integration test: Lichess oracle completes a match end-to-end.
//!
//! Sequence: the (mocked) Lichess API returns a decisive game result, the
//! oracle's poller picks it up, signs and submits `submit_result` to the
//! (mocked) Soroban RPC, and the transaction lifecycle reports success.
//!
//! No real network access is used — both the Lichess API and the Soroban RPC
//! are `wiremock` servers on localhost.
//!
//! Run with:
//!   cargo test -p oracle-service --test lichess_e2e_test

use oracle_service::{
    config::{OracleConfig, Platform},
    dead_letter::DeadLetterStore,
    poller::Poller,
    queue::{PendingEntry, PendingQueue},
};

use chrono::Utc;
use stellar_xdr::{HostFunction, Limits, OperationBody, ReadXdr, ScSymbol, ScVal, TransactionEnvelope};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

const MATCH_ID: u64 = 4242;
const GAME_ID: &str = "e2e_lichess_game";

/// Build a minimal OracleConfig pointing at the mock Soroban RPC.
fn make_config(soroban_rpc_url: &str, queue_dir: &str) -> OracleConfig {
    // Fixed 32-byte key so the test is deterministic.
    let seed = [0x7Au8; 32];
    let signing_key = Zeroizing::new(seed);

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
        max_retries: 3,
        retry_base_delay_secs: 1,
        queue_dir: queue_dir.to_string(),
    }
}

/// Mounts the full getAccount → simulateTransaction → sendTransaction →
/// getTransaction lifecycle. A single catch-all response covers all four RPC
/// methods since the client only reads whichever fields it needs per call.
async fn mount_soroban_success(server: &MockServer) {
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

async fn enqueue_due(queue: &PendingQueue, match_id: u64, game_id: &str, platform: Platform) {
    let mut entry = PendingEntry::new(match_id, game_id.to_string(), platform);
    entry.next_attempt_at = Utc::now() - chrono::Duration::seconds(1);
    queue.save(&[entry]).await.unwrap();
}

/// Full happy path: Lichess reports a decisive win, the oracle signs and
/// submits `submit_result(match_id, winner)` for the correct match, and the
/// Soroban RPC confirms the transaction.
///
/// Asserting the escrow contract's on-chain state directly would require a
/// live Soroban network, which is out of scope for this service-level test
/// (see `contracts/escrow/tests/match_lifecycle_test.rs` for that, run
/// against a real in-process Soroban `Env`). Here we assert on the boundary
/// this service owns: it must submit exactly one `submit_result` invocation
/// carrying the correct match id and winner, and it only clears the entry
/// from its pending queue once the RPC lifecycle reports the transaction
/// succeeded — i.e. once the ledger has recorded the match's completion and
/// payout.
#[tokio::test]
async fn lichess_oracle_completes_match_end_to_end() {
    // ── Mock Lichess API: white wins ───────────────────────────────────────
    let lichess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/game/export/{}", GAME_ID)))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
        .mount(&lichess_server)
        .await;

    // ── Mock Soroban RPC ─────────────────────────────────────────────────────
    let rpc_server = MockServer::start().await;
    mount_soroban_success(&rpc_server).await;

    // ── Setup ────────────────────────────────────────────────────────────────
    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);

    let poller = Poller::new_with_lichess_base(&cfg, lichess_server.uri()).unwrap();
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str);

    enqueue_due(&queue, MATCH_ID, GAME_ID, Platform::Lichess).await;

    // ── Poll once: fetch the result and submit it on-chain ──────────────────
    poller.tick().await.unwrap();

    // The queue entry is only removed once submit_result is confirmed, so an
    // empty queue is proof the oracle drove the match to completion.
    assert!(
        queue.load().await.unwrap().is_empty(),
        "match should be completed and removed from the pending queue"
    );
    assert!(
        dead_letter.load().await.unwrap().is_empty(),
        "a successful submission must not be dead-lettered"
    );

    // ── Verify the exact call the oracle made to the contract ──────────────
    let requests = rpc_server.received_requests().await.unwrap();
    let send_tx_body = requests
        .iter()
        .find_map(|req| {
            let body: serde_json::Value = serde_json::from_slice(&req.body).ok()?;
            if body.get("method")?.as_str()? == "sendTransaction" {
                Some(body)
            } else {
                None
            }
        })
        .expect("oracle must call sendTransaction to submit the result");

    let tx_xdr = send_tx_body["params"]["transaction"]
        .as_str()
        .expect("sendTransaction must carry a transaction XDR");
    let envelope = TransactionEnvelope::from_xdr_base64(tx_xdr, Limits::none())
        .expect("submitted transaction must be well-formed XDR");

    let TransactionEnvelope::Tx(v1) = envelope else {
        panic!("expected a TransactionV1Envelope");
    };
    let op = v1.tx.operations.get(0).expect("transaction must have one operation");
    let OperationBody::InvokeHostFunction(invoke_op) = &op.body else {
        panic!("expected an InvokeHostFunction operation");
    };
    let HostFunction::InvokeContract(invoke_args) = &invoke_op.host_function else {
        panic!("expected an InvokeContract host function");
    };

    let expected_fn_name = ScSymbol("submit_result".try_into().unwrap());
    assert_eq!(invoke_args.function_name, expected_fn_name);

    assert_eq!(invoke_args.args.get(0), Some(&ScVal::U64(MATCH_ID)));

    let expected_winner = ScVal::Symbol(ScSymbol("Player1".try_into().unwrap()));
    assert_eq!(invoke_args.args.get(1), Some(&expected_winner));
}
