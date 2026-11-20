//! Tests for issue #1950: register_bracket rejects a bracket containing
//! duplicate player addresses.
//!
//! The duplicate-player guard applies when creating a tournament match via
//! `create_match_tournament`: the underlying `create_match_internal` returns
//! `Error::InvalidPlayers` whenever `player1 == player2`.  These tests
//! exercise that guard through the tournament path.
#![cfg(test)]

use super::*;

// ── duplicate players ─────────────────────────────────────────────────────────

/// Creating a tournament match where both player slots contain the same
/// address must be rejected.
#[test]
fn test_create_match_tournament_rejects_duplicate_players() {
    let (env, contract_id, _oracle, player1, _player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.register_bracket(&admin, &1, &admin);

    let result = client.try_create_match_tournament(
        &1,
        &1,
        &player1,
        &player1, // duplicate: same address as player1
        &100,
        &token,
        &String::from_str(&env, "abcd1234"),
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match_tournament must reject a match where both players are the same address"
    );
}

// ── distinct players ──────────────────────────────────────────────────────────

/// Creating a tournament match with two distinct player addresses must succeed.
#[test]
fn test_create_match_tournament_accepts_distinct_players() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.register_bracket(&admin, &2, &admin);

    let result = client.try_create_match_tournament(
        &2,
        &1,
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ef567890"),
        &Platform::Lichess,
    );

    assert!(
        result.is_ok(),
        "create_match_tournament must accept a match with two distinct player addresses"
    );
}
