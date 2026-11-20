//! Tests for issue #1587: Fix reconciliation re-enqueueing dead-lettered matches.
//!
//! When an entry is moved to the dead-letter store after exhausting retries,
//! the next reconciliation cycle should NOT re-enqueue it. Doing so fills the
//! dead-letter store and wastes API quota with infinite cycles.

use std::collections::HashMap;

use tempfile::TempDir;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use zeroize::Zeroizing;

use stellar_xdr::{
    HostFunction, Limits, OperationBody, ReadXdr, ScMap, ScMapEntry, ScString, ScSymbol, ScVal,
    ScVec, Transaction, WriteXdr,
};

use oracle_service::{
    config::OracleConfig,
    dead_letter::DeadLetterStore,
    poller::Poller,
    queue::PendingQueue,
};

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
        chessdotcom_poll_interval_secs: 1,
        max_retries: 1,
        retry_base_delay_secs: 1,
        queue_dir: queue_dir.to_string(),
        reconciliation_interval_secs: 1,
        dead_letter_max_entries: 100,
        max_fee_stroops: 1_000_000,
        http_addr: "0.0.0.0:8000".to_string(),
        metrics_auth_username: None,
        metrics_auth_password: None,
    }
}

fn match_scval(match_id: u64, game_id: &str, platform: &str) -> ScVal {
    let entries = vec![
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol("game_id".try_into().unwrap())),
            val: ScVal::String(ScString(game_id.try_into().unwrap())),
        },
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol("id".try_into().unwrap())),
            val: ScVal::U64(match_id),
        },
        ScMapEntry {
            key: ScVal::Symbol(ScSymbol("platform".try_into().unwrap())),
            val: ScVal::Vec(Some(ScVec(
                vec![ScVal::Symbol(ScSymbol(platform.try_into().unwrap()))]
                    .try_into()
                    .unwrap(),
            ))),
        },
    ];
    ScVal::Map(Some(ScMap(entries.try_into().unwrap())))
}

fn active_matches_xdr(matches: &[(u64, &str, &str)]) -> String {
    let vec_val = ScVal::Vec(Some(ScVec(
        matches
            .iter()
            .map(|(match_id, game_id, platform)| match_scval(*match_id, game_id, platform))
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

fn invoked_function(tx_b64: &str) -> (String, Vec<ScVal>) {
    let tx = Transaction::from_xdr_base64(tx_b64, Limits::none()).expect("valid transaction xdr");
    let op = tx.operations.first().expect("operation present");
    let OperationBody::InvokeHostFunction(inv) = &op.body else {
        panic!("expected InvokeHostFunction operation");
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

/// Test that a dead-lettered match is NOT re-enqueued during reconciliation.
///
/// Scenario:
/// 1. Match is discovered and enqueued
/// 2. All retries are exhausted (e.g., API always returns an error)
/// 3. Entry is moved to dead-letter store
/// 4. Next reconciliation cycle should skip this match
#[tokio::test]
async fn dead_lettered_match_is_not_re_enqueued() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server error"))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;

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
                    let (function_name, _args) = invoked_function(tx_b64);
                    match function_name.as_str() {
                        "get_active_matches_paginated" => {
                            let xdr = active_matches_xdr(&[(200, "game200", "Lichess")]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        other => panic!("unexpected function: {}", other),
                    }
                }
                other => panic!("unexpected RPC method: {}", other),
            }
        })
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // First reconciliation discovers match 200
    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 1);
    assert_eq!(after_reconcile[0].match_id, 200);

    // Tick exhausts retries (game API always fails)
    poller.tick().await.unwrap();
    let after_first_tick = queue.load().await.unwrap();
    // After first failure, should still be in queue but with failed attempt
    assert_eq!(after_first_tick.len(), 1);
    assert_eq!(after_first_tick[0].attempts, 1);

    // Another tick to exhaust max_retries=1
    poller.tick().await.unwrap();
    let after_second_tick = queue.load().await.unwrap();

    // Should now be in dead-letter store, not in queue
    assert!(
        after_second_tick.is_empty(),
        "exhausted entry should be removed from queue"
    );
    let dead_letters = dead_letter.load().await.unwrap();
    assert_eq!(dead_letters.len(), 1, "exhausted entry should be in dead-letter");
    assert_eq!(dead_letters[0].entry.match_id, 200);

    // Second reconciliation cycle: match still in Active but should be skipped
    // because it's in dead-letter store
    poller.reconcile().await.unwrap();
    let after_second_reconcile = queue.load().await.unwrap();

    assert!(
        after_second_reconcile.is_empty(),
        "dead-lettered match should NOT be re-enqueued during reconciliation"
    );

    // Dead-letter store should still have just the one entry
    let dead_letters_after = dead_letter.load().await.unwrap();
    assert_eq!(
        dead_letters_after.len(),
        1,
        "dead-letter store size should not change"
    );
    assert_eq!(dead_letters_after[0].entry.match_id, 200);
}

/// Test that multiple dead-lettered matches are not re-enqueued.
#[tokio::test]
async fn multiple_dead_lettered_matches_are_not_re_enqueued() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500).set_body_string("error"))
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;

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
                    let (function_name, _args) = invoked_function(tx_b64);
                    match function_name.as_str() {
                        "get_active_matches_paginated" => {
                            let xdr = active_matches_xdr(&[
                                (201, "game201", "Lichess"),
                                (202, "game202", "Lichess"),
                                (203, "game203", "Lichess"),
                            ]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        other => panic!("unexpected function: {}", other),
                    }
                }
                other => panic!("unexpected RPC method: {}", other),
            }
        })
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers matches 201, 202, 203
    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 3);

    // Exhaust retries for all three
    for _ in 0..3 {
        poller.tick().await.unwrap();
    }

    let queue_after_ticks = queue.load().await.unwrap();
    assert!(queue_after_ticks.is_empty(), "all entries should be exhausted");

    let dead_letters = dead_letter.load().await.unwrap();
    assert_eq!(dead_letters.len(), 3, "all three should be in dead-letter");

    // Next reconciliation should not re-enqueue any of them
    poller.reconcile().await.unwrap();
    let final_queue = queue.load().await.unwrap();
    assert!(
        final_queue.is_empty(),
        "dead-lettered matches should NOT be re-enqueued"
    );

    let final_dead_letters = dead_letter.load().await.unwrap();
    assert_eq!(
        final_dead_letters.len(),
        3,
        "dead-letter store size should remain at 3"
    );
}

/// Test that once a match is dead-lettered and removed via manual replay,
/// it can be re-enqueued in future reconciliation cycles.
#[tokio::test]
async fn dead_lettered_match_can_be_replayed_and_re_discovered() {
    let chess_server = MockServer::start().await;
    let first_tick = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let first_tick_clone = first_tick.clone();

    Mock::given(method("GET"))
        .respond_with(move |_req: &Request| {
            if first_tick_clone.swap(false, std::sync::atomic::Ordering::SeqCst) {
                // First request: fail
                ResponseTemplate::new(500).set_body_string("error")
            } else {
                // After replay: succeed
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "winner": "white"
                }))
            }
        })
        .mount(&chess_server)
        .await;

    let rpc_server = MockServer::start().await;

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
                    let (function_name, _args) = invoked_function(tx_b64);
                    match function_name.as_str() {
                        "get_active_matches_paginated" => {
                            let xdr = active_matches_xdr(&[(204, "game204", "Lichess")]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "submit_result" => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": 1,
                            "result": {
                                "minResourceFee": "100",
                                "transactionData": "",
                                "results": []
                            }
                        })),
                        other => panic!("unexpected function: {}", other),
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
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers match 204
    poller.reconcile().await.unwrap();
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // Exhaust retries (API fails)
    for _ in 0..3 {
        poller.tick().await.unwrap();
    }

    // Now in dead-letter
    assert!(queue.load().await.unwrap().is_empty());
    assert_eq!(dead_letter.load().await.unwrap().len(), 1);

    // Simulate manual replay: remove from dead-letter and re-enqueue
    dead_letter.remove(204).await.unwrap();
    queue
        .enqueue(204, "game204".into(), oracle_service::config::Platform::Lichess)
        .await
        .ok();

    assert!(dead_letter.load().await.unwrap().is_empty());
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // Now tick should succeed (API now responds with result)
    poller.tick().await.unwrap();

    // Entry should be removed and submitted successfully
    assert!(
        queue.load().await.unwrap().is_empty(),
        "replayed match should be processed and removed"
    );
}
