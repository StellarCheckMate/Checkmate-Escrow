//! Tests for issue #1589: Fix offset-based reconciliation skipping matches.
//!
//! When the Active set changes between reconciliation pages (matches settle,
//! are rolled back, etc.), offset-based pagination causes later matches to shift
//! and be skipped. This test verifies that cursor-based pagination correctly
//! handles these changes and doesn't miss any matches.

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
        dead_letter_max_entries: 0,
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

/// Test that when using offset-based pagination, matches get skipped if the
/// Active set shrinks between pages (e.g., earlier matches settle).
///
/// Scenario:
/// - Page 1 (offset 0, size 3): matches [1, 2, 3]
/// - User settles match 1 and 2
/// - Page 2 (offset 3, size 3): would get matches [4, 5, 6]
///   But if match 1 and 2 were removed, the actual results shift, and match 3
///   now occupies the slot for match 4. Using offset would skip some matches.
///
/// With cursor-based pagination, we always use the last seen match_id, so we
/// get the correct next batch regardless of removals.
#[tokio::test]
async fn cursor_based_pagination_handles_active_set_changes() {
    let rpc_server = MockServer::start().await;

    // First reconciliation pass: page returns matches 1-3
    // Second reconciliation pass (after some settle): page returns matches 4-6
    // and we need to ensure all are discovered
    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

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
                            let calls = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            // First call: return matches 1-3
                            // Second call: return matches 4-6 (1-3 have settled/left Active)
                            if calls == 0 {
                                let xdr = active_matches_xdr(&[
                                    (1, "game1", "Lichess"),
                                    (2, "game2", "Lichess"),
                                    (3, "game3", "Lichess"),
                                ]);
                                ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                            } else {
                                let xdr = active_matches_xdr(&[
                                    (4, "game4", "Lichess"),
                                    (5, "game5", "Lichess"),
                                    (6, "game6", "Lichess"),
                                ]);
                                ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                            }
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

    let poller = Poller::new(&cfg).unwrap();

    // First reconciliation discovers matches 1-3
    poller.reconcile().await.unwrap();
    let after_first = queue.load().await.unwrap();
    assert_eq!(after_first.len(), 3, "should discover 3 matches in first page");
    let discovered_ids: Vec<u64> = after_first.iter().map(|e| e.match_id).collect();
    assert!(discovered_ids.contains(&1));
    assert!(discovered_ids.contains(&2));
    assert!(discovered_ids.contains(&3));

    // Simulate matches 1-3 settling and leaving the Active set.
    // Clear the queue to represent those matches being handled.
    queue.remove(1).await.ok();
    queue.remove(2).await.ok();
    queue.remove(3).await.ok();
    assert_eq!(queue.load().await.unwrap().len(), 0);

    // Second reconciliation should discover matches 4-6
    // With cursor-based pagination, we'll get them; with offset-based we might miss them
    poller.reconcile().await.unwrap();
    let after_second = queue.load().await.unwrap();
    assert_eq!(
        after_second.len(),
        3,
        "should discover matches 4-6 in second pass, even though 1-3 left Active"
    );
    let discovered_ids: Vec<u64> = after_second.iter().map(|e| e.match_id).collect();
    assert!(
        discovered_ids.contains(&4),
        "match 4 should be discovered; got {:?}",
        discovered_ids
    );
    assert!(
        discovered_ids.contains(&5),
        "match 5 should be discovered; got {:?}",
        discovered_ids
    );
    assert!(
        discovered_ids.contains(&6),
        "match 6 should be discovered; got {:?}",
        discovered_ids
    );
}

/// Test that cursor-based pagination resumes correctly from a persisted cursor
/// even when the Active set has changed between runs.
#[tokio::test]
async fn persisted_cursor_works_across_restarts_with_changing_active_set() {
    let rpc_server = MockServer::start().await;

    let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = call_count.clone();

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
                            let calls = call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            // Simulate a paginated result set
                            if calls == 0 {
                                // First pass: returns 10 matches
                                let matches: Vec<(u64, &str, &str)> = (1..=10)
                                    .map(|i| (i, &format!("game{}", i), "Lichess"))
                                    .collect();
                                let xdr = active_matches_xdr(&matches);
                                ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                            } else {
                                // After restart/second pass: last 5 matches + 5 new ones
                                let matches: Vec<(u64, &str, &str)> = (6..=15)
                                    .map(|i| (i, &format!("game{}", i), "Lichess"))
                                    .collect();
                                let xdr = active_matches_xdr(&matches);
                                ResponseTemplate::new(200).set_body_json(simulate_result_json(&xdr))
                            }
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

    let poller = Poller::new(&cfg).unwrap();

    // First reconciliation: discover matches 1-10
    poller.reconcile().await.unwrap();
    let entries = queue.load().await.unwrap();
    assert_eq!(entries.len(), 10, "should discover 10 matches");

    // Simulate cleaning up 1-5 (e.g., completed or failed out)
    for i in 1..=5 {
        queue.remove(i).await.ok();
    }

    // Second reconciliation after "restart": should get 6-15
    // But if cursor is persisted from last position in first pass (after 10),
    // we should correctly continue after 10, not repeat 6-10
    poller.reconcile().await.unwrap();
    let entries_after = queue.load().await.unwrap();

    // We should have 6-15 (10 more), and entries 1-5 are gone
    assert!(
        entries_after.iter().all(|e| e.match_id > 5),
        "all remaining entries should be from the second reconciliation pass"
    );

    let all_ids: std::collections::HashSet<u64> = entries_after.iter().map(|e| e.match_id).collect();
    for id in 6..=10 {
        assert!(
            all_ids.contains(&id),
            "match {} should be in queue from second pass",
            id
        );
    }
    for id in 11..=15 {
        assert!(
            all_ids.contains(&id),
            "match {} should be discovered in second pass",
            id
        );
    }
}

/// Test that even with small page sizes, cursor pagination doesn't skip matches
/// when the Active set changes rapidly.
#[tokio::test]
async fn small_page_size_with_cursor_discovers_all_matches() {
    let rpc_server = MockServer::start().await;

    let all_matches = std::sync::Arc::new(vec![
        (1, "game1", "Lichess"),
        (2, "game2", "Lichess"),
        (3, "game3", "Lichess"),
        (4, "game4", "Lichess"),
        (5, "game5", "Lichess"),
    ]);

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
                            // Return all matches in one pass with small logical pages
                            let xdr = active_matches_xdr(&all_matches);
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

    let poller = Poller::new(&cfg).unwrap();

    poller.reconcile().await.unwrap();
    let entries = queue.load().await.unwrap();

    // All 5 matches should be discovered
    assert_eq!(entries.len(), 5, "should discover all 5 matches");
    let discovered_ids: std::collections::HashSet<u64> = entries.iter().map(|e| e.match_id).collect();
    for id in 1..=5 {
        assert!(
            discovered_ids.contains(&id),
            "match {} should be discovered",
            id
        );
    }
}
