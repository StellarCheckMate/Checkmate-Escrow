//! Tests for the oracle-verified player rating registry (issue #1434).
//!
//! `register_player_rating` requires oracle authorization and stores a
//! `PlayerRating` record in persistent storage keyed by `(player, platform)`.
//! `get_player_rating` is a view that returns `None` until a rating is
//! registered and the correct value afterward.

use super::*;
use crate::types::{Platform, PlayerRating};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Convenience wrapper around `register_player_rating`.
fn register(
    client: &EscrowContractClient,
    env: &Env,
    oracle: &Address,
    player: &Address,
    platform: Platform,
    username: &str,
    rating: u32,
) -> Result<(), crate::errors::Error> {
    client.try_register_player_rating(
        oracle,
        player,
        &platform,
        &soroban_sdk::String::from_str(env, username),
        &rating,
    )
    .map_err(|e| e.unwrap())
}

// ── registration tests ────────────────────────────────────────────────────────

#[test]
fn register_and_retrieve_lichess_rating() {
    let (env, contract_id, oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    register(&client, &env, &oracle, &player1, Platform::Lichess, "Magnus", 2850).unwrap();

    let record: PlayerRating = client.get_player_rating(&player1, &Platform::Lichess).unwrap();
    assert_eq!(record.username, soroban_sdk::String::from_str(&env, "Magnus"));
    assert_eq!(record.rating, 2850);
}

#[test]
fn register_and_retrieve_chess_com_rating() {
    let (env, contract_id, oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    register(&client, &env, &oracle, &player1, Platform::ChessDotCom, "Hikaru", 3200).unwrap();

    let record: PlayerRating = client.get_player_rating(&player1, &Platform::ChessDotCom).unwrap();
    assert_eq!(record.username, soroban_sdk::String::from_str(&env, "Hikaru"));
    assert_eq!(record.rating, 3200);
}

#[test]
fn ratings_are_keyed_per_platform() {
    // The same player can have independent ratings for Lichess and Chess.com.
    let (env, contract_id, oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    register(&client, &env, &oracle, &player1, Platform::Lichess, "PlayerX", 1800).unwrap();
    register(&client, &env, &oracle, &player1, Platform::ChessDotCom, "PlayerX_com", 1750).unwrap();

    let lichess = client.get_player_rating(&player1, &Platform::Lichess).unwrap();
    let chess_com = client.get_player_rating(&player1, &Platform::ChessDotCom).unwrap();

    assert_eq!(lichess.rating, 1800);
    assert_eq!(chess_com.rating, 1750);
}

#[test]
fn oracle_can_update_existing_rating() {
    let (env, contract_id, oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    register(&client, &env, &oracle, &player1, Platform::Lichess, "Magnus", 2800).unwrap();
    // Oracle updates the rating after a new verified game.
    register(&client, &env, &oracle, &player1, Platform::Lichess, "Magnus", 2855).unwrap();

    let record = client.get_player_rating(&player1, &Platform::Lichess).unwrap();
    assert_eq!(record.rating, 2855);
}

#[test]
fn get_player_rating_returns_none_when_not_registered() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.get_player_rating(&player1, &Platform::Lichess);
    assert!(result.is_none(), "expected None for unregistered player");
}

// ── authorization tests ───────────────────────────────────────────────────────

#[test]
fn non_oracle_cannot_register_rating() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // player2 pretends to be the oracle — must be rejected.
    let result = client.try_register_player_rating(
        &player2,
        &player1,
        &Platform::Lichess,
        &soroban_sdk::String::from_str(&env, "Magnus"),
        &2850u32,
    );
    assert!(result.is_err(), "non-oracle must not be able to register a rating");
}

#[test]
fn admin_cannot_register_rating() {
    let (env, contract_id, _oracle, player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_register_player_rating(
        &admin,
        &player1,
        &Platform::Lichess,
        &soroban_sdk::String::from_str(&env, "Magnus"),
        &2850u32,
    );
    assert!(result.is_err(), "admin must not be able to register a rating (oracle only)");
}

#[test]
fn rating_records_current_ledger() {
    let (env, contract_id, oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let before = env.ledger().sequence();
    register(&client, &env, &oracle, &player1, Platform::Lichess, "Magnus", 2850).unwrap();

    let record = client.get_player_rating(&player1, &Platform::Lichess).unwrap();
    assert!(record.recorded_ledger >= before, "recorded_ledger must be >= ledger at call time");
}
