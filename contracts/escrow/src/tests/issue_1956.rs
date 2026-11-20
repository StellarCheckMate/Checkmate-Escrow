//! Tests for Issue #1956 — is_stablecoin returns false for a token whose
//! issuer is not in the stablecoin set
//!
//! The non-stablecoin rejection path of `is_stablecoin` must correctly return
//! `false` for unregistered tokens and `true` for registered issuers.

use super::*;

// ── Returns false for unregistered (non-stablecoin) tokens ───────────────────

/// `is_stablecoin` must return `false` for a freshly generated address that
/// has never been registered as a stablecoin issuer.
#[test]
fn test_is_stablecoin_returns_false_for_unregistered_token() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    assert!(
        !client.is_stablecoin(&unknown),
        "is_stablecoin must return false for a token that has never been registered"
    );
}

/// `is_stablecoin` must return `false` for a valid token contract address
/// that was not explicitly added via `add_stablecoin_issuer`, even if other
/// tokens are registered.
#[test]
fn test_is_stablecoin_returns_false_when_other_tokens_registered() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Register a different token.
    let other_token_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let other_token = other_token_id.address();
    client.add_stablecoin_issuer(&other_token);

    // The original token is still not registered.
    assert!(
        !client.is_stablecoin(&token),
        "is_stablecoin must return false for a token not in the stablecoin set, \
         even when other tokens are registered"
    );
}

/// `is_stablecoin` must return `false` for a token that was added and then
/// subsequently removed from the stablecoin set.
#[test]
fn test_is_stablecoin_returns_false_after_removal() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_stablecoin_issuer(&token);
    assert!(client.is_stablecoin(&token), "pre-condition: token is registered");

    client.remove_stablecoin_issuer(&token);
    assert!(
        !client.is_stablecoin(&token),
        "is_stablecoin must return false after the issuer is removed"
    );
}

// ── Returns true for registered stablecoin issuers ────────────────────────────

/// `is_stablecoin` must return `true` for a token registered via
/// `add_stablecoin_issuer`.
#[test]
fn test_is_stablecoin_returns_true_for_registered_issuer() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_stablecoin_issuer(&token);
    assert!(
        client.is_stablecoin(&token),
        "is_stablecoin must return true for a token registered as a stablecoin issuer"
    );
}

/// `is_stablecoin` must return `true` for each issuer in a multi-issuer
/// stablecoin set, and `false` for an address not in the set.
#[test]
fn test_is_stablecoin_returns_true_for_each_registered_issuer() {
    let (env, contract_id, _oracle, _p1, _p2, token1, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token2_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token2 = token2_id.address();

    client.add_stablecoin_issuer(&token1);
    client.add_stablecoin_issuer(&token2);

    assert!(
        client.is_stablecoin(&token1),
        "is_stablecoin must return true for the first registered issuer"
    );
    assert!(
        client.is_stablecoin(&token2),
        "is_stablecoin must return true for the second registered issuer"
    );

    let unknown = Address::generate(&env);
    assert!(
        !client.is_stablecoin(&unknown),
        "is_stablecoin must return false for an address not in the stablecoin set"
    );
}

/// Re-registering an already-registered issuer must remain idempotent — the
/// issuer must still be recognised as a stablecoin.
#[test]
fn test_is_stablecoin_returns_true_after_idempotent_re_registration() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_stablecoin_issuer(&token);
    client.add_stablecoin_issuer(&token); // duplicate add — must be idempotent

    assert!(
        client.is_stablecoin(&token),
        "is_stablecoin must return true after an idempotent double-registration"
    );
}
