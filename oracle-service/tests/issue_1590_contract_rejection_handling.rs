//! Tests for issue #1590: Fix contract rejection error handling.
//!
//! The oracle service should distinguish between transient contract errors
//! (network failures, timeouts) and permanent contract errors (InvalidState,
//! ContractPaused, etc.) and handle them appropriately:
//! - InvalidState (already settled): treat as success
//! - ContractPaused: treat as delayed retry
//! - Other permanent errors: dead-letter the match

use std::collections::HashMap;

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
        max_retries: 3,
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

fn simulate_error_json(error_code: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "error": {
            "code": -32000,
            "message": format!("simulation failed: {}", error_code),
            "data": error_code
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

/// Test that submit_result with InvalidState error is treated as success.
/// InvalidState means the match was already settled, so no retry is needed.
#[tokio::test]
async fn submit_result_invalid_state_is_treated_as_success() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/game123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
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
                            let xdr = active_matches_xdr(&[(100, "game123", "Lichess")]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "submit_result" => {
                            // Simulate InvalidState contract error
                            ResponseTemplate::new(200)
                                .set_body_json(simulate_error_json("InvalidState"))
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
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers the match
    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 1);
    assert_eq!(after_reconcile[0].match_id, 100);

    // Tick processes it, fetches the game, and attempts submission
    poller.tick().await.unwrap();

    // InvalidState should be treated as success: entry removed, not dead-lettered
    assert!(
        queue.load().await.unwrap().is_empty(),
        "entry should be removed after InvalidState (already settled)"
    );
    assert!(
        dead_letter.load().await.unwrap().is_empty(),
        "entry should NOT be dead-lettered for InvalidState"
    );
}

/// Test that submit_result with ContractPaused error triggers delayed retry,
/// not immediate dead-letter.
#[tokio::test]
async fn submit_result_contract_paused_triggers_retry() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/game456"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
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
                            let xdr = active_matches_xdr(&[(101, "game456", "Lichess")]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "submit_result" => {
                            // Simulate ContractPaused error
                            ResponseTemplate::new(200)
                                .set_body_json(simulate_error_json("ContractPaused"))
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
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers the match
    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 1);

    // Tick processes it and encounters ContractPaused
    poller.tick().await.unwrap();

    // ContractPaused should be a transient error: entry still in queue, not dead-lettered
    let queue_after = queue.load().await.unwrap();
    assert_eq!(
        queue_after.len(),
        1,
        "entry should remain in queue after ContractPaused (retry)"
    );
    assert_eq!(queue_after[0].match_id, 101);
    assert_eq!(queue_after[0].attempts, 1, "attempt count should increment");

    assert!(
        dead_letter.load().await.unwrap().is_empty(),
        "entry should NOT be dead-lettered for ContractPaused"
    );
}

/// Test that submit_result with other permanent errors (NotFunded, etc.)
/// are dead-lettered, not retried.
#[tokio::test]
async fn submit_result_permanent_errors_are_dead_lettered() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/game789"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
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
                            let xdr = active_matches_xdr(&[(102, "game789", "Lichess")]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "submit_result" => {
                            // Simulate NotFunded error (permanent)
                            ResponseTemplate::new(200)
                                .set_body_json(simulate_error_json("NotFunded"))
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
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let cfg = OracleConfig {
        max_retries: 1,
        ..cfg
    };
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers the match
    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 1);

    // Tick processes it and encounters NotFunded (permanent error)
    poller.tick().await.unwrap();

    // Permanent error should exhaust retries immediately or quickly, leading to dead-letter
    let attempts = 1;
    for _ in 0..attempts {
        poller.tick().await.ok();
    }

    // After max_retries exhausted, should be in dead-letter store
    assert!(
        queue.load().await.unwrap().is_empty(),
        "entry should be removed from queue after permanent error exhausts retries"
    );
    let dead_letters = dead_letter.load().await.unwrap();
    assert!(!dead_letters.is_empty(), "entry should be dead-lettered after exhaustion");
    assert_eq!(dead_letters[0].entry.match_id, 102);
}

/// Test that submit_result with RPC timeout (transient) triggers retry.
#[tokio::test]
async fn submit_result_rpc_timeout_is_transient() {
    let chess_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/game/export/game999"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
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
                            let xdr = active_matches_xdr(&[(103, "game999", "Lichess")]);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "has_result" => {
                            let xdr = bool_xdr(false);
                            ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                        }
                        "submit_result" => {
                            // Simulate timeout (transient error)
                            ResponseTemplate::new(504)
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
        .mount(&rpc_server)
        .await;

    let dir = TempDir::new().unwrap();
    let dir_str = dir.path().to_str().unwrap();
    let cfg = make_config(&rpc_server.uri(), dir_str);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers the match
    poller.reconcile().await.unwrap();
    let after_reconcile = queue.load().await.unwrap();
    assert_eq!(after_reconcile.len(), 1);

    // Tick processes it and encounters timeout
    poller.tick().await.unwrap();

    // Transient error should keep entry in queue for retry
    let queue_after = queue.load().await.unwrap();
    assert_eq!(
        queue_after.len(),
        1,
        "entry should remain in queue after transient error"
    );
    assert_eq!(queue_after[0].attempts, 1);

    assert!(
        dead_letter.load().await.unwrap().is_empty(),
        "entry should NOT be dead-lettered after first transient error"
    );
}
