//! Regression tests for issue #1724:
//! `create_match` (and all variants) must reject a match where `player1 == player2`.
//!
//! Without this guard a single account could create a match against itself,
//! deposit both sides, and then collect the full pot as the "winner" — a
//! complete loss of escrow integrity.
//!
//! The guard lives in `create_match_internal` and returns `Error::InvalidPlayers`
//! whenever `player1 == player2`.

use super::*;

// ─────────────────────────────────────────────────────────────────────────────
// Helper
// ─────────────────────────────────────────────────────────────────────────────

fn game_id(env: &Env, s: &str) -> soroban_sdk::String {
    soroban_sdk::String::from_str(env, s)
}

// ─────────────────────────────────────────────────────────────────────────────
// create_match
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_1724_create_match_rejects_same_player() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player1,
        &100,
        &token,
        &game_id(&env, "1724a001"),
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match must return InvalidPlayers when player1 == player2"
    );
}

#[test]
fn test_1724_create_match_accepts_distinct_players() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Distinct players — must succeed.
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &game_id(&env, "1724a002"),
        &Platform::Lichess,
    );

    assert!(
        result.is_ok(),
        "create_match must succeed when player1 != player2"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// create_match_tournament
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_1724_create_match_tournament_rejects_same_player() {
    let (env, contract_id, _oracle, player1, _player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Register a bracket so the tournament call does not fail on bracket auth.
    client.register_bracket(&admin, &1, &admin, &soroban_sdk::vec![&env]);

    let result = client.try_create_match_tournament(
        &player1,
        &player1,
        &100,
        &token,
        &1u64,
        &game_id(&env, "1724b001"),
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match_tournament must return InvalidPlayers when player1 == player2"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// create_match_with_referrer
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_1724_create_match_with_referrer_rejects_same_player() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let referrer = Address::generate(&env);

    let result = client.try_create_match_with_referrer(
        &player1,
        &player1,
        &100,
        &token,
        &game_id(&env, "1724c001"),
        &Platform::Lichess,
        &referrer,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match_with_referrer must return InvalidPlayers when player1 == player2"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge: player1 == contract address
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_1724_create_match_rejects_contract_as_player2() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Passing the contract itself as player2 is another invalid-players case.
    let result = client.try_create_match(
        &player1,
        &contract_id,
        &100,
        &token,
        &game_id(&env, "1724d001"),
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "create_match must return InvalidPlayers when player2 is the contract address"
    );
}
