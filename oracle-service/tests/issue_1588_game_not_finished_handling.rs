//! Tests for issue #1588: Fix GameNotFinished handling.
//!
//! GameNotFinished errors should be handled separately from regular transient
//! errors. They should not count toward max_retries, allowing long games to be
//! retried indefinitely without being dead-lettered.
//!
//! Only time-based limits (e.g. match timeout) should apply to GameNotFinished.

use tempfile::TempDir;
use wiremock::matchers::method;
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

fn make_config(soroban_rpc_url: &str, queue_dir: &str, max_retries: u32) -> OracleConfig {
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
        max_retries,
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

/// Test that GameNotFinished errors do NOT increment the retry counter.
///
/// Classic/Daily games can run for days or weeks. If GameNotFinished counts
/// as a transient error towards max_retries, a long game would exhaust all
/// retries and be dead-lettered even though nothing is wrong.
///
/// GameNotFinished should reschedule without counting attempts.
#[tokio::test]
async fn game_not_finished_does_not_count_towards_retries() {
    let chess_server = MockServer::start().await;

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    Mock::given(method("GET"))
        .respond_with(move |_req: &Request| {
            let calls = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if calls < 5 {
                // First 5 calls: game not finished
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status": "playing"
                }))
            } else {
                // After 5 calls: game finished
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
                            let xdr = active_matches_xdr(&[(300, "game300", "Lichess")]);
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
    let cfg = make_config(&rpc_server.uri(), dir_str, 2);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    // Reconciliation discovers the match
    poller.reconcile().await.unwrap();
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // Multiple ticks: first 5 get GameNotFinished, then game finishes
    for i in 0..5 {
        poller.tick().await.unwrap();
        let entries = queue.load().await.unwrap();
        // With max_retries=2, if GameNotFinished counted towards retries,
        // we'd be dead-lettered after 2-3 ticks. But we should stay in queue.
        assert!(
            !entries.is_empty(),
            "after tick {}: entry should still be in queue (GameNotFinished should not count)",
            i
        );
        assert_eq!(
            entries[0].attempts, 0,
            "GameNotFinished should not increment attempts"
        );
    }

    // Now the game is finished, next tick should submit successfully
    poller.tick().await.unwrap();

    let final_queue = queue.load().await.unwrap();
    assert!(
        final_queue.is_empty(),
        "entry should be removed after successful submission"
    );

    let dead_letters = dead_letter.load().await.unwrap();
    assert!(
        dead_letters.is_empty(),
        "entry should never be dead-lettered for GameNotFinished"
    );
}

/// Test that even with a very low max_retries, GameNotFinished doesn't cause
/// dead-lettering as long as the game eventually finishes.
#[tokio::test]
async fn long_game_with_low_max_retries_succeeds_when_finished() {
    let chess_server = MockServer::start().await;

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    Mock::given(method("GET"))
        .respond_with(move |_req: &Request| {
            let calls = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if calls < 10 {
                // Game takes 10 retries to finish
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status": "playing"
                }))
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "winner": "black"
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
                            let xdr = active_matches_xdr(&[(301, "game301", "Lichess")]);
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
    // Very low max_retries: 1
    let cfg = make_config(&rpc_server.uri(), dir_str, 1);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    poller.reconcile().await.unwrap();
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // Run 12 ticks: first 10 get GameNotFinished, 11th should get the result and submit
    for i in 0..12 {
        poller.tick().await.unwrap();

        let queue_state = queue.load().await.unwrap();
        if queue_state.is_empty() {
            // Successfully submitted
            assert!(
                i >= 10,
                "should submit after game finishes (tick {})",
                i
            );
            break;
        } else {
            // Still waiting for game to finish
            assert!(
                i < 11,
                "should have submitted by now (tick {})",
                i
            );
        }
    }

    let final_queue = queue.load().await.unwrap();
    assert!(
        final_queue.is_empty(),
        "entry should be successfully submitted and removed"
    );

    let dead_letters = dead_letter.load().await.unwrap();
    assert!(
        dead_letters.is_empty(),
        "entry should never reach dead-letter despite low max_retries"
    );
}

/// Test that GameNotFinished is distinct from other transient errors.
/// A regular transient error (network timeout) should count towards retries.
#[tokio::test]
async fn regular_transient_errors_still_count_towards_retries() {
    let chess_server = MockServer::start().await;
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

    Mock::given(method("GET"))
        .respond_with(move |_req: &Request| {
            let calls = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if calls < 5 {
                // Network timeout (transient, but counts towards retries)
                ResponseTemplate::new(504)
            } else {
                // Eventually responds with result
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
                            let xdr = active_matches_xdr(&[(302, "game302", "Lichess")]);
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
    let cfg = make_config(&rpc_server.uri(), dir_str, 3);
    let queue = PendingQueue::new(dir_str);
    let dead_letter = DeadLetterStore::new(dir_str, 100);

    let poller = Poller::new_with_lichess_base(&cfg, chess_server.uri()).unwrap();

    poller.reconcile().await.unwrap();
    assert_eq!(queue.load().await.unwrap().len(), 1);

    // Run ticks: first 5 get network timeout, then result
    for i in 0..7 {
        poller.tick().await.unwrap();

        let entries = queue.load().await.unwrap();
        if entries.is_empty() {
            // Successfully submitted
            assert!(
                i >= 5,
                "should submit after retries succeed (tick {})",
                i
            );
            break;
        } else if i < 5 {
            // Network timeout should increment attempts
            assert!(
                entries[0].attempts > 0,
                "regular transient errors should increment attempts"
            );
        }
    }

    let final_queue = queue.load().await.unwrap();
    assert!(
        final_queue.is_empty(),
        "entry should be successfully submitted"
    );

    let dead_letters = dead_letter.load().await.unwrap();
    assert!(
        dead_letters.is_empty(),
        "entry should not be dead-lettered (succeeded before max_retries)"
    );
}
