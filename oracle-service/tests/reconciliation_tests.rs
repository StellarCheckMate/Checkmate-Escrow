//! Adversarial tests for the reconciliation task (`Poller::reconcile`).
//!
//! These prove that a match becomes discoverable — and stays discoverable —
//! without anything external ever calling `Poller::enqueue` directly. That's
//! the gap this task closes: previously nothing in the service ever called
//! `enqueue` for a match's first submission, so the durable
//! queue/retry/dead-letter pipeline sat idle by construction.
//!
//! The mock Soroban RPC server decodes the incoming `simulateTransaction`
//! request's transaction XDR to find which contract function is being
//! invoked (`get_active_matches_paginated` or `has_result`) and returns a
//! correspondingly XDR-encoded response — mirroring how `#[contracttype]`
//! actually encodes structs (`Map` keyed by field-name `Symbol`s) and
//! fieldless enums (`Vec` containing one `Symbol`).

use std::collections::HashMap;

use chrono::Utc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use zeroize::Zeroizing;

use stellar_xdr::{
    HostFunction, Limits, OperationBody, ReadXdr, ScMap, ScMapEntry, ScString, ScSymbol, ScVal,
    ScVec, Transaction, WriteXdr,
};

use oracle_service::{
    config::{OracleConfig, Platform},
    dead_letter::DeadLetterStore,
    poller::Poller,
    queue::{PendingEntry, PendingQueue},
};

// ── Config / fixture helpers ────────────────────────────────────────────────

fn make_config(soroban_rpc_url: &str, queue_dir: &str) -> OracleConfig {
    let seed = [0x24u8; 32];
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
        reconciliation_interval_secs: 1,
    }
}

/// One `Active` match as the mock escrow contract will report it.
struct FixtureMatch {
    match_id: u64,
    game_id: &'static str,
    platform: &'static str,
}

// ── XDR encoding helpers (mirror `#[contracttype]`'s wire format) ──────────

fn match_scval(m: &FixtureMatch) -> ScVal {
    let entries = vec![
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol("game_id".try_into().unwrap())),
            val: ScVal::String(ScString(m.game_id.try_into().unwrap())),
        },
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol("id".try_into().unwrap())),
            val: ScVal::U64(m.match_id),
        },
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol("platform".try_into().unwrap())),
            val: ScVal::Vec(Some(ScVec(
                vec![ScVal::Symbol(ScSymbol(m.platform.try_into().unwrap()))]
                    .try_into()
                    .unwrap(),
            ))),
        },
    ];
    ScVal::Map(Some(ScMap(entries.try_into().unwrap())))
}

fn active_matches_xdr(matches: &[FixtureMatch]) -> String {
    let vec_val = ScVal::Vec(Some(ScVec(
        matches
            .iter()
            .map(match_scval)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    )));
    vec_val.to_xdr_base64(Limits::none()).unwrap()
}

fn bool_xdr(b: bool) -> String {
    ScVal::Bool(b).to_xdr_base64(Limits::none()).unwrap()
}

fn simulate_result_json(xdr: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "result": {
            "minResourceFee": "100",
            "transactionData": "",
            "results": [ { "xdr": xdr } ]
        }
    })
}

/// Extract `(function_name, args)` from a base64-encoded `Transaction`'s sole
/// `InvokeHostFunction` operation.
fn invoked_function(tx_b64: &str) -> (String, Vec<ScVal>) {
    let tx = Transaction::from_xdr_base64(tx_b64, Limits::none()).expect("valid transaction xdr");
    let op = tx.operations.first().expect("operation present");
    let OperationBody::InvokeHostFunction(inv) = &op.body else {
        panic!("expected InvokeHostFunction operation, got {:?}", op.body);
    };
    let HostFunction::InvokeContract(args) = &inv.host_function else {
        panic!("expected InvokeContract host function");
    };
    let function_name = args
        .function_name
        .0
        .to_utf8_string()
        .expect("valid function name");
    (function_name, args.args.to_vec())
}

/// Mount a Soroban RPC mock that serves the full `getAccount` →
/// `simulateTransaction` → `sendTransaction` → `getTransaction` lifecycle,
/// dispatching `simulateTransaction` per invoked contract function.
async fn mount_reconciliation_rpc(
    server: &MockServer,
    active_matches: Vec<FixtureMatch>,
    has_result: HashMap<u64, bool>,
) {
    Mock::given(method("POST"))
        .respond_with(move |req: &Request| {
            let body: serde_json::Value = req.body_json().expect("valid JSON-RPC body");
            let rpc_method = body["method"].as_str().unwrap_or("");

            match rpc_method {
                "getAccount" => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": { "sequence": "100" }
                })),
                "simulateTransaction" => {
                    let tx_b64 = body["params"]["transaction"]
                        .as_str()
                        .expect("transaction field present");
                    let (function_name, args) = invoked_function(tx_b64);
                    match function_name.as_str() {
                        "get_active_matches_paginated" => {
                            let xdr = active_matches_xdr(&active_matches);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let ScVal::U64(match_id) = args.first().expect("match_id arg") else {
                                panic!("expected U64 match_id argument");
                            };
                            let result = has_result.get(match_id).copied().unwrap_or(false);
                            let xdr = bool_xdr(result);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "submit_result" => {
                            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": 1,
                                "result": {
                                    "minResourceFee": "100",
                                    "transactionData": "",
                                    "results": []
                                }
                            }))
                        }
                        other => panic!("unexpected simulateTransaction target: {}", other),
                    }
                }
                "sendTransaction" => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "status": "PENDING",
                        "hash": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
                    }
                })),
                "getTransaction" => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": { "status": "SUCCESS" }
                })),
                other => panic!("unexpected RPC method: {}", other),
            }
        })
        .mount(server)
        .await;
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// A match that became `Active` while the service wasn't running at all is
/// still discovered on the next reconciliation cycle after startup, and its
/// result gets submitted — proving the pipeline isn't permanently dead for
/// matches nothing ever called `enqueue` for.
#[tokio::test]
async fn fresh_service_discovers_preexisting_active_match_and_submits_result() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/abcd1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;
    mount_reconciliation_rpc(
        &rpc_server,
        vec![FixtureMatch {
            match_id: 10,
            game_id: "abcd1234",
            platform: "Lichess",
        }],
        HashMap::new(),
    )
    .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);

    // Fresh service: nothing has ever called `enqueue`.
    assert!(queue.load().await.unwrap().is_empty());

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 1, "match should be discovered");
    assert_eq!(after_reconcile[0].match_id, 10);

    poller.tick().await.unwrap();
    assert!(
        queue.load().await.unwrap().is_empty(),
        "discovered match should be verified and submitted, leaving the queue empty"
    );

    let dead_letter = DeadLetterStore::new(dir_str);
    assert!(dead_letter.load().await.unwrap().is_empty());
}

/// Deleting `pending.json` entirely (simulating lost ephemeral storage, e.g.
/// a container rescheduled without a persistent volume) does not permanently
/// drop a still-active match: the next reconciliation cycle re-populates it.
#[tokio::test]
async fn deleted_queue_file_is_repopulated_by_next_reconciliation_cycle() {
    let rpc_server = MockServer::start().await;
    mount_reconciliation_rpc(
        &rpc_server,
        vec![FixtureMatch {
            match_id: 20,
            game_id: "efgh5678",
            platform: "Lichess",
        }],
        HashMap::new(),
    )
    .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);

    let poller = Poller::new(&cfg).unwrap();

    poller.reconcile().await.unwrap();
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // Simulate losing the queue file entirely.
    std::fs::remove_file(dir.path().join("pending.json")).unwrap();
    assert!(
        queue.load().await.unwrap().is_empty(),
        "load() should treat a missing file as an empty queue"
    );

    poller.reconcile().await.unwrap();
    let entries = queue.load().await.unwrap();
    assert_eq!(
        entries.len(),
        1,
        "match should be re-discovered after the queue file was lost"
    );
    assert_eq!(entries[0].match_id, 20);
}

/// A match that already has a result recorded on the oracle contract is not
/// re-enqueued — no duplicate submission attempt.
#[tokio::test]
async fn reconciliation_skips_matches_with_existing_result() {
    let rpc_server = MockServer::start().await;
    let mut has_result = HashMap::new();
    has_result.insert(30, true);
    mount_reconciliation_rpc(
        &rpc_server,
        vec![FixtureMatch {
            match_id: 30,
            game_id: "ijkl9012",
            platform: "Lichess",
        }],
        has_result,
    )
    .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);

    let poller = Poller::new(&cfg).unwrap();
    poller.reconcile().await.unwrap();

    assert!(
        queue.load().await.unwrap().is_empty(),
        "a match with an existing result must not be enqueued"
    );
}

/// A match already queued mid-retry-backoff is left completely untouched by
/// reconciliation — `attempts` and `next_attempt_at` are not reset.
#[tokio::test]
async fn reconciliation_does_not_disturb_inflight_backoff_entry() {
    let rpc_server = MockServer::start().await;
    mount_reconciliation_rpc(
        &rpc_server,
        vec![FixtureMatch {
            match_id: 40,
            game_id: "mnop3456",
            platform: "Lichess",
        }],
        HashMap::new(),
    )
    .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);

    // Pre-seed an entry already mid-backoff from a prior transient failure.
    let mut entry = PendingEntry::new(40, "mnop3456".to_string(), Platform::Lichess);
    entry.attempts = 2;
    let future_retry = Utc::now() + chrono::Duration::minutes(30);
    entry.next_attempt_at = future_retry;
    entry.last_error = Some("previous transient failure".to_string());
    queue.save(&[entry]).await.unwrap();

    let poller = Poller::new(&cfg).unwrap();
    poller.reconcile().await.unwrap();

    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 1, "no duplicate entry should be created");
    assert_eq!(entries[0].attempts, 2, "attempts must not be reset");
    assert_eq!(
        entries[0].next_attempt_at, future_retry,
        "next_attempt_at must not be reset"
    );
    assert_eq!(
        entries[0].last_error.as_deref(),
        Some("previous transient failure")
    );
}
