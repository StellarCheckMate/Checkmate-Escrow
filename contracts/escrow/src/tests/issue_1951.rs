//! Tests for issue #1951: create_match rejects a players vector with duplicate
//! addresses.
//!
//! The self-match guard in `create_match` returns `Error::InvalidPlayers` when
//! `player1 == player2`.  These tests assert both rejection and acceptance.
#![cfg(test)]

use super::*;

// ── duplicate players ─────────────────────────────────────────────────────────

/// `create_match` must return `Error::InvalidPlayers` when both player slots
/// contain the same address.
#[test]
fn test_create_match_rejects_duplicate_player_addresses() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player1, // duplicate: same address used for both sides
        &100,
        &token,
        &String::from_str(&env, "aabbccdd"),
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match must reject when player1 and player2 are the same address"
    );
}

// ── distinct players ──────────────────────────────────────────────────────────

/// `create_match` must succeed when player1 and player2 are different addresses.
#[test]
fn test_create_match_accepts_distinct_player_addresses() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "11223344"),
        &Platform::Lichess,
    );

    assert!(
        result.is_ok(),
        "create_match must accept a match with two distinct player addresses"
    );
}
