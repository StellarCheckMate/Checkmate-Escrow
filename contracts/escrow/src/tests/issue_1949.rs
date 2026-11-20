//! Tests for issue #1949: set_preferred_payout_token rejects a blacklisted or
//! non-allowlisted token.
//!
//! The validation guarding `set_preferred_payout_token` against blacklisted and
//! non-allowlisted tokens was previously uncovered.  These tests assert both
//! rejection cases and the complementary acceptance cases.
#![cfg(test)]

use super::*;

// ── blacklisted token ─────────────────────────────────────────────────────────

/// A player must not be able to set a blacklisted token as their preferred
/// payout token.
#[test]
fn test_set_preferred_payout_token_rejects_blacklisted_token() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &String::from_str(&env, "known scam"));

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert_eq!(
        result,
        Err(Ok(Error::TokenNotAllowed)),
        "set_preferred_payout_token must reject a blacklisted token"
    );
}

/// After a token is removed from the blacklist, a player should be able to
/// set it as their preferred payout token.
#[test]
fn test_set_preferred_payout_token_accepts_token_after_blacklist_removal() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &String::from_str(&env, "temp block"));
    client.remove_token_from_blacklist(&token);

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert!(
        result.is_ok(),
        "set_preferred_payout_token must accept a token after it is removed from the blacklist"
    );
}

// ── non-allowlisted token ─────────────────────────────────────────────────────

/// When allowlist enforcement is active, a player must not be able to set a
/// token that is not on the allowlist as their preferred payout token.
#[test]
fn test_set_preferred_payout_token_rejects_non_allowlisted_token() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Enable allowlist enforcement by adding a *different* token.
    let other_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    client.add_allowed_token(&other_token);

    // The original token is NOT on the allowlist.
    let non_listed = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let result = client.try_set_preferred_payout_token(&player1, &Some(non_listed));
    assert_eq!(
        result,
        Err(Ok(Error::TokenNotAllowed)),
        "set_preferred_payout_token must reject a token not on the allowlist when enforcement is active"
    );
}

/// When allowlist enforcement is active, a player must be able to set an
/// allowlisted token as their preferred payout token.
#[test]
fn test_set_preferred_payout_token_accepts_allowlisted_token() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_allowed_token(&token);

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert!(
        result.is_ok(),
        "set_preferred_payout_token must accept an allowlisted token"
    );
}

/// When no allowlist enforcement is active, any non-blacklisted token should
/// be accepted.
#[test]
fn test_set_preferred_payout_token_accepts_any_token_without_allowlist() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let any_token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    let result = client.try_set_preferred_payout_token(&player1, &Some(any_token));
    assert!(
        result.is_ok(),
        "set_preferred_payout_token must accept any token when no allowlist is enforced"
    );
}
