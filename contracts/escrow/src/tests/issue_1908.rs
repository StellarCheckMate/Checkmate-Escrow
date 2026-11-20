//! Regression tests for Issue #1908.
//!
//! `register_bracket` previously stored the bracket players without verifying
//! they are distinct, allowing one address to occupy multiple slots in a
//! tournament bracket. That would let a single player join both sides of a
//! match and collect the payout regardless of the outcome.
//!
//! The fix validates that all players in the supplied list are pairwise
//! distinct before persisting the bracket. Duplicate entries are rejected
//! with [`Error::InvalidPlayers`].

use super::*;

// ── Duplicate player rejection ────────────────────────────────────────────────

/// A bracket with two identical players must be rejected.
/// This is the primary regression guard for Issue #1908.
#[test]
fn test_register_bracket_rejects_duplicate_players() {
    let (env, contract_id, _oracle, player1, player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // player1 appears in both slot 0 and slot 1.
    let players = soroban_sdk::vec![&env, player1.clone(), player1.clone()];
    let result = client.try_register_bracket(&admin, &1, &admin, &players);

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "duplicate player in bracket must be rejected"
    );
}

/// A four-player bracket where the duplicate appears in non-adjacent slots
/// must also be rejected.
#[test]
fn test_register_bracket_rejects_non_adjacent_duplicate() {
    let (env, contract_id, _oracle, player1, player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let player3 = Address::generate(&env);

    // Slots: [player1, player2, player3, player1] — first and last are the same.
    let players = soroban_sdk::vec![
        &env,
        player1.clone(),
        player2.clone(),
        player3.clone(),
        player1.clone()
    ];
    let result = client.try_register_bracket(&admin, &2, &admin, &players);

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "non-adjacent duplicate player must still be rejected"
    );
}

/// A bracket where every address is the same (more than two slots) must be
/// rejected.
#[test]
fn test_register_bracket_rejects_all_same_player() {
    let (env, contract_id, _oracle, player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let players = soroban_sdk::vec![&env, player1.clone(), player1.clone(), player1.clone()];
    let result = client.try_register_bracket(&admin, &3, &admin, &players);

    assert_eq!(
        result,
        Err(Ok(Error::InvalidPlayers)),
        "bracket where all slots are the same player must be rejected"
    );
}

// ── Happy path ────────────────────────────────────────────────────────────────

/// A bracket with all distinct players must be accepted and stored.
#[test]
fn test_register_bracket_accepts_distinct_players() {
    let (env, contract_id, _oracle, player1, player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);

    let players = soroban_sdk::vec![
        &env,
        player1.clone(),
        player2.clone(),
        player3.clone(),
        player4.clone()
    ];
    let result = client.try_register_bracket(&admin, &10, &admin, &players);

    assert_eq!(
        result,
        Ok(()),
        "bracket with all distinct players must be accepted"
    );
}

/// An empty player list (a bracket registered without participants) must
/// succeed because an organizer may register the bracket ID before seeding
/// participants into it through match creation.
#[test]
fn test_register_bracket_accepts_empty_player_list() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let players = soroban_sdk::vec![&env];
    let result = client.try_register_bracket(&admin, &20, &admin, &players);

    assert_eq!(
        result,
        Ok(()),
        "empty player list must be accepted (no duplicates possible)"
    );
}

/// A single-player bracket has no pairs to compare, so it must succeed.
#[test]
fn test_register_bracket_accepts_single_player() {
    let (env, contract_id, _oracle, player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let players = soroban_sdk::vec![&env, player1.clone()];
    let result = client.try_register_bracket(&admin, &21, &admin, &players);

    assert_eq!(
        result,
        Ok(()),
        "single-player bracket has no pairs to compare and must be accepted"
    );
}
