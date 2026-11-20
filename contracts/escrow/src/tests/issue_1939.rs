//! Tests for Issue #1939 — Enhancement: add a get_blacklist_reason view.
//!
//! `add_token_to_blacklist` stores a reason string on-chain, but previously
//! there was no view to read it back. `get_blacklist_reason(token)` fills that
//! gap so UIs and monitoring tools can show why a token is blocked.

#![cfg(test)]
extern crate std;

use super::*;

// ── get_blacklist_reason when not blacklisted ─────────────────────────────────

#[test]
fn test_get_blacklist_reason_returns_none_for_unknown_token() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    assert_eq!(
        client.get_blacklist_reason(&unknown),
        None,
        "get_blacklist_reason must return None for a token that was never blacklisted"
    );
}

// ── reason round-trips through add → get ─────────────────────────────────────

#[test]
fn test_get_blacklist_reason_returns_stored_reason() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let reason_text = "rug pull";
    let reason = String::from_str(&env, reason_text);
    client.add_token_to_blacklist(&token, &reason);

    let retrieved = client
        .get_blacklist_reason(&token)
        .expect("reason must be Some for a blacklisted token");
    assert_eq!(
        retrieved,
        reason,
        "get_blacklist_reason must round-trip the stored reason exactly"
    );
}

#[test]
fn test_get_blacklist_reason_round_trips_long_reason() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 255-character reason (one byte below the 256-byte cap).
    let long_reason: std::string::String = "x".repeat(255);
    let reason = String::from_str(&env, &long_reason);
    client.add_token_to_blacklist(&token, &reason);

    let retrieved = client
        .get_blacklist_reason(&token)
        .expect("reason must be Some for a blacklisted token");
    assert_eq!(
        retrieved, reason,
        "get_blacklist_reason must round-trip a near-maximum-length reason"
    );
}

#[test]
fn test_get_blacklist_reason_returns_none_after_removal() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let reason = String::from_str(&env, "fraud");
    client.add_token_to_blacklist(&token, &reason);
    assert!(
        client.get_blacklist_reason(&token).is_some(),
        "reason must be present while token is blacklisted"
    );

    client.remove_token_from_blacklist(&token);
    assert_eq!(
        client.get_blacklist_reason(&token),
        None,
        "get_blacklist_reason must return None after the token is removed from the blacklist"
    );
}

#[test]
fn test_get_blacklist_reason_does_not_affect_is_token_blacklisted() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let reason = String::from_str(&env, "sanctioned asset");
    client.add_token_to_blacklist(&token, &reason);

    // Both views must agree.
    assert!(client.is_token_blacklisted(&token));
    assert!(client.get_blacklist_reason(&token).is_some());
}
