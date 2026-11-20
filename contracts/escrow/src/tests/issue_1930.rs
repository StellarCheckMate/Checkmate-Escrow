//! #1930 — `tier_from_match_count` does not handle a zero match count
//! consistently across callers.
//!
//! A brand-new player with zero completed matches must always be assigned
//! [`PlayerTier::Bronze`], regardless of which path queries their tier
//! (`tier_from_match_count`, `create_match` tier enforcement, or `deposit`
//! tier recheck).  This test suite pins the zero-match tier and verifies it
//! is stable across the entry points that consume it.

use super::*;

/// `tier_from_match_count` returns `Bronze` for a fresh player with no history.
#[test]
fn test_zero_match_count_returns_bronze() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(
        client.tier_from_match_count(&player1),
        PlayerTier::Bronze,
        "a player with 0 completed matches must be Bronze"
    );
}

/// `tier_from_match_count` returns the same `Bronze` tier for every new
/// address that has never participated in a match, confirming the behaviour
/// is not tied to a specific address.
#[test]
fn test_zero_match_count_is_stable_across_different_players() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Both players start with zero completed matches — both must be Bronze.
    let tier_p1 = client.tier_from_match_count(&player1);
    let tier_p2 = client.tier_from_match_count(&player2);

    assert_eq!(
        tier_p1,
        PlayerTier::Bronze,
        "player1 with 0 matches must be Bronze"
    );
    assert_eq!(
        tier_p2,
        PlayerTier::Bronze,
        "player2 with 0 matches must be Bronze"
    );
    assert_eq!(
        tier_p1, tier_p2,
        "zero-match tier must be identical across all new players"
    );
}

/// The `create_match` tier enforcement and `tier_from_match_count` agree on
/// the Bronze stake bounds for a new player.
///
/// A stake of 100 (the Bronze ceiling) must be accepted by `create_match`
/// for a fresh player, confirming the enforcer uses the same Bronze tier
/// that `tier_from_match_count` returns.
#[test]
fn test_zero_match_tier_is_consistent_with_create_match_enforcement() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Verify the public API says Bronze.
    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Bronze);

    // Bronze ceiling is 100; creating with stake=100 must succeed.
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19300001"),
        &Platform::Lichess,
    );
    let m = client.get_match(&id);
    assert_eq!(m.stake_amount, 100);

    // A stake above the Bronze ceiling (101) must be rejected.
    let result = client.try_create_match(
        &player1,
        &player2,
        &101,
        &token,
        &String::from_str(&env, "19300002"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::TierStakeNotAllowed)),
        "stake above Bronze ceiling must be rejected for a zero-match player"
    );
}

/// `deposit` applies the same tier check as `create_match`.  A fresh player
/// depositing into a Bronze-range match must succeed, confirming the
/// deposit path also uses Bronze for a zero-match player.
#[test]
fn test_zero_match_tier_is_consistent_with_deposit_enforcement() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Verify tier view.
    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Bronze);
    assert_eq!(client.tier_from_match_count(&player2), PlayerTier::Bronze);

    // Create a match at the Bronze ceiling; both players depositing must succeed.
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19300003"),
        &Platform::Lichess,
    );

    // Both deposits must succeed: neither player has completed any match yet,
    // so both are Bronze and the Bronze ceiling (100) allows this stake.
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let m = client.get_match(&id);
    assert!(m.player1_deposited, "player1 must have deposited successfully");
    assert!(m.player2_deposited, "player2 must have deposited successfully");
}
