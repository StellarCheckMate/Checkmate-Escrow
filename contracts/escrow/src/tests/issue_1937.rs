//! Tests for Issue #1937 — Enhancement: add a get_frozen_players_count view.
//!
//! Counting frozen players previously required pulling the full paginated list.
//! `get_frozen_players_count` returns the count directly without the read cost
//! of materialising every frozen-player address.

#![cfg(test)]
extern crate std;

use super::*;

// ── get_frozen_players_count default ─────────────────────────────────────────

#[test]
fn test_get_frozen_players_count_zero_when_none_frozen() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(
        client.get_frozen_players_count(),
        0,
        "count must be zero when no players are frozen"
    );
}

// ── count matches paginated list length ──────────────────────────────────────

#[test]
fn test_get_frozen_players_count_matches_paginated_list_length() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let reason = String::from_str(&env, "cheating");

    client.admin_freeze_player(&player1, &reason);
    client.admin_freeze_player(&player2, &reason);

    let count = client.get_frozen_players_count();
    // Fetch the full list with a limit large enough to include every entry.
    let list = client.get_frozen_players(&0, &100);

    assert_eq!(
        count,
        list.len(),
        "get_frozen_players_count must equal the length of the paginated list"
    );
}

#[test]
fn test_get_frozen_players_count_increments_on_each_freeze() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let reason = String::from_str(&env, "stalling");

    assert_eq!(client.get_frozen_players_count(), 0);

    client.admin_freeze_player(&player1, &reason);
    assert_eq!(client.get_frozen_players_count(), 1);

    client.admin_freeze_player(&player2, &reason);
    assert_eq!(client.get_frozen_players_count(), 2);
}

#[test]
fn test_get_frozen_players_count_decrements_on_unfreeze() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let reason = String::from_str(&env, "investigation");

    client.admin_freeze_player(&player1, &reason);
    assert_eq!(client.get_frozen_players_count(), 1);

    client.admin_unfreeze_player(&player1);
    assert_eq!(
        client.get_frozen_players_count(),
        0,
        "count must drop to zero after the only frozen player is unfrozen"
    );
}

#[test]
fn test_get_frozen_players_count_matches_list_after_mixed_operations() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let third = Address::generate(&env);
    let reason = String::from_str(&env, "repeated violations");

    client.admin_freeze_player(&player1, &reason);
    client.admin_freeze_player(&player2, &reason);
    client.admin_freeze_player(&third, &reason);
    client.admin_unfreeze_player(&player2);

    let count = client.get_frozen_players_count();
    let list = client.get_frozen_players(&0, &100);

    assert_eq!(
        count,
        list.len(),
        "count must match paginated list length after freeze/unfreeze operations"
    );
    assert_eq!(count, 2);
}
