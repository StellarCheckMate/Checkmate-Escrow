//! Tests for Issue #1909 — Security: create_match does not reject a players
//! vector with duplicate addresses.
//!
//! A match where both player slots name the same address would let a single
//! party "play against themselves", bypass tier checks for one side, and
//! potentially double-collect winnings. The fix requires `player1 != player2`
//! and returns `InvalidPlayers` when the check fails.

use super::*;

/// `create_match` must reject a call where player1 and player2 are the same
/// address, returning `InvalidPlayers`.
#[test]
fn test_create_match_rejects_duplicate_players() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player1, // same address as player1
        &100,
        &token,
        &String::from_str(&env, "1909aa01"),
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match must return InvalidPlayers when player1 == player2"
    );
}

/// Reject duplicate players via `create_match_with_referrer` as well.
/// All public entry points route through `create_match_internal`, so the
/// guard should fire regardless of which variant is used.
#[test]
fn test_create_match_with_referrer_rejects_duplicate_players() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let referrer = Address::generate(&env);

    let result = client.try_create_match_with_referrer(
        &player1,
        &player1, // duplicate
        &100,
        &token,
        &String::from_str(&env, "1909bb01"),
        &Platform::Lichess,
        &referrer,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match_with_referrer must return InvalidPlayers when player1 == player2"
    );
}

/// Sanity-check: a valid match (distinct players) is accepted without error.
#[test]
fn test_create_match_accepts_distinct_players() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "1909cc01"),
        &Platform::Lichess,
    );

    assert!(
        result.is_ok(),
        "create_match must succeed with two distinct player addresses"
    );
}
