//! Regression tests for Issue #1906.
//!
//! `set_preferred_payout_token` previously stored any `Address` as the
//! player's preferred payout token without validating the token against the
//! allowlist or blacklist. A player could pin a payout to a token the contract
//! would later refuse to pay out in.
//!
//! The fix validates the token at the time of storage:
//! - Blacklisted tokens are rejected.
//! - Tokens not on the allowlist are rejected when enforcement is active.
//! - Tokens that fail the stablecoin check are rejected in stablecoin-only mode.

use super::*;

// ── Blacklist rejection ───────────────────────────────────────────────────────

/// A blacklisted token must be rejected by `set_preferred_payout_token`.
/// This is the primary regression guard for Issue #1906.
#[test]
fn test_set_preferred_payout_token_rejects_blacklisted_token() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &String::from_str(&env, "sanctioned"));

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert_eq!(
        result,
        Err(Ok(Error::TokenNotAllowed)),
        "blacklisted token must be rejected as a preferred payout token"
    );
}

/// Clearing the preference (passing `None`) must still succeed even when the
/// previously-pinned token has since been blacklisted.
#[test]
fn test_set_preferred_payout_token_clear_always_succeeds() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No token stored yet — clearing is a no-op and must succeed.
    let result = client.try_set_preferred_payout_token(&player1, &None);
    assert_eq!(result, Ok(()), "clearing None must always succeed");
}

// ── Allowlist enforcement ─────────────────────────────────────────────────────

/// When the allowlist is enforced and the token is not listed, the token must
/// be rejected as a preferred payout token.
#[test]
fn test_set_preferred_payout_token_rejects_token_not_on_allowlist() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Add a *different* token to the allowlist so enforcement is activated.
    let other_token = Address::generate(&env);
    client.add_allowed_token(&other_token);

    // The original token is not on the allowlist.
    let unlisted_token = Address::generate(&env);
    let result = client.try_set_preferred_payout_token(&player1, &Some(unlisted_token));
    assert_eq!(
        result,
        Err(Ok(Error::TokenNotAllowed)),
        "token not on allowlist must be rejected when enforcement is active"
    );
}

/// When the allowlist is enforced and the token *is* listed, storing the
/// preference must succeed.
#[test]
fn test_set_preferred_payout_token_accepts_allowlisted_token() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Add the token to the allowlist.
    client.add_allowed_token(&token);

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert_eq!(
        result,
        Ok(()),
        "allowlisted token must be accepted as a preferred payout token"
    );

    let stored = client.get_preferred_payout_token(&player1);
    assert_eq!(stored, Some(token), "stored token must match the one set");
}

// ── Stablecoin-only mode ──────────────────────────────────────────────────────

/// When stablecoin-only mode is active and the token is not a registered
/// stablecoin, the token must be rejected.
#[test]
fn test_set_preferred_payout_token_rejects_non_stablecoin_in_stablecoin_mode() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Enable stablecoin-only mode without registering the token as a stablecoin.
    let mut config = client.get_protocol_config().unwrap();
    config.stablecoin_only_mode = true;
    client.set_protocol_config(&config).unwrap();

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert_eq!(
        result,
        Err(Ok(Error::TokenNotAllowed)),
        "non-stablecoin token must be rejected in stablecoin-only mode"
    );
}

// ── Happy path ────────────────────────────────────────────────────────────────

/// When no restrictions are active, any token address must be accepted and
/// persisted correctly.
#[test]
fn test_set_preferred_payout_token_accepts_valid_token_no_restrictions() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_set_preferred_payout_token(&player1, &Some(token.clone()));
    assert_eq!(result, Ok(()), "valid token must be accepted without restrictions");

    let stored = client.get_preferred_payout_token(&player1);
    assert_eq!(stored, Some(token), "stored token must match the one set");
}
