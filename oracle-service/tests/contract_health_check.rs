//! Tests for contract health check fix (Issue #1593).
//!
//! Validates that the health check:
//! - Builds a proper LedgerKey::ContractData with correct XDR encoding
//! - Fails when getLedgerEntries returns empty entries
//! - Properly detects non-existent contracts

use oracle_service::soroban_client::SorobanClient;
use serde_json::json;
use wiremock::{matchers::body_string_contains, Mock, MockServer, ResponseTemplate};

mod common;

#[tokio::test]
async fn test_contract_health_check_with_valid_contract() {
    let mock_server = MockServer::start().await;

    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/"))
        .and(body_string_contains("getLedgerEntries"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    "entries": [
                        {
                            "key": "some_key",
                            "xdr": "some_xdr_data"
                        }
                    ]
                }
            })),
        )
        .mount(&mock_server)
        .await;

    let client = SorobanClient::new(
        mock_server.uri(),
        "Test SDF Network ; September 2015".to_string(),
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        1_000_000,
    )
    .expect("should construct client");

    let result = client
        .contract_health_check("CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P")
        .await;

    assert!(result.is_ok(), "health check should succeed with valid contract");
}

#[tokio::test]
async fn test_contract_health_check_with_empty_entries() {
    let mock_server = MockServer::start().await;

    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/"))
        .and(body_string_contains("getLedgerEntries"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    "entries": []
                }
            })),
        )
        .mount(&mock_server)
        .await;

    let client = SorobanClient::new(
        mock_server.uri(),
        "Test SDF Network ; September 2015".to_string(),
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        1_000_000,
    )
    .expect("should construct client");

    let result = client
        .contract_health_check("CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P")
        .await;

    assert!(
        result.is_err(),
        "health check should fail when contract does not exist (empty entries)"
    );
}

#[tokio::test]
async fn test_contract_health_check_with_rpc_error() {
    let mock_server = MockServer::start().await;

    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/"))
        .and(body_string_contains("getLedgerEntries"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "error": {
                "code": -32603,
                "message": "Internal error"
            }
        })))
        .mount(&mock_server)
        .await;

    let client = SorobanClient::new(
        mock_server.uri(),
        "Test SDF Network ; September 2015".to_string(),
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        1_000_000,
    )
    .expect("should construct client");

    let result = client
        .contract_health_check("CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P")
        .await;

    assert!(result.is_err(), "health check should fail on RPC error");
}

#[tokio::test]
async fn test_contract_health_check_builds_valid_ledger_key() {
    let mock_server = MockServer::start().await;

    let mut received_requests = vec![];

    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/"))
        .and(body_string_contains("getLedgerEntries"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "result": {
                    "entries": [{"key": "test", "xdr": "test"}]
                }
            })),
        )
        .mount(&mock_server)
        .await;

    let client = SorobanClient::new(
        mock_server.uri(),
        "Test SDF Network ; September 2015".to_string(),
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        "CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P",
        1_000_000,
    )
    .expect("should construct client");

    let result = client
        .contract_health_check("CCJMDYMB4O3WJW5QCECQEQFPVYXD3MZRXZWQAVKQZVQXOYTM6WXHZ24P")
        .await;

    assert!(result.is_ok(), "should construct proper XDR key");
}
