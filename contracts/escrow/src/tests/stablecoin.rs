//! Tests for issue #961: Stablecoin-Only Mode for Regulatory Compliance
//!
//! Covers:
//! - `add_stablecoin_issuer` (admin-only, idempotent)
//! - `remove_stablecoin_issuer`
//! - `is_stablecoin`
//! - `stablecoin_only_mode` flag in `ProtocolConfig`
//! - `create_match` rejecting non-stablecoin tokens when mode is enabled
//! - `create_match` accepting stablecoin tokens when mode is enabled
//! - Mode disabled by default (no rejection)

use super::*;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Enable stablecoin-only mode on the contract.
fn enable_stablecoin_mode(client: &EscrowContractClient, admin: &Address) {
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: true,
        minimum_stake: 1,
    });
}

// ── add_stablecoin_issuer ─────────────────────────────────────────────────────

#[test]
fn test_add_stablecoin_issuer_marks_token_as_stablecoin() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Before registration the token is not a stablecoin.
    assert!(!client.is_stablecoin(&token));

    client.add_stablecoin_issuer(&token);

    assert!(client.is_stablecoin(&token));
}

#[test]
fn test_add_stablecoin_issuer_is_idempotent() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_stablecoin_issuer(&token);
    // Calling again must not panic or corrupt the count.
    client.add_stablecoin_issuer(&token);

    assert!(client.is_stablecoin(&token));
}

#[test]
fn test_add_stablecoin_issuer_requires_admin() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let attacker = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "add_stablecoin_issuer",
            args: (token.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_add_stablecoin_issuer(&token);
    assert!(
        matches!(result, Err(Err(_)) | Err(Ok(Error::Unauthorized))),
        "expected auth failure for non-admin caller"
    );
}

// ── remove_stablecoin_issuer ──────────────────────────────────────────────────

#[test]
fn test_remove_stablecoin_issuer_unmarks_token() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_stablecoin_issuer(&token);
    assert!(client.is_stablecoin(&token));

    client.remove_stablecoin_issuer(&token);
    assert!(!client.is_stablecoin(&token));
}

#[test]
fn test_remove_stablecoin_issuer_requires_admin() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let attacker = Address::generate(&env);

    client.add_stablecoin_issuer(&token);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "remove_stablecoin_issuer",
            args: (token.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_remove_stablecoin_issuer(&token);
    assert!(
        matches!(result, Err(Err(_)) | Err(Ok(Error::Unauthorized))),
        "expected auth failure for non-admin caller"
    );
}

// ── stablecoin_only_mode in create_match ──────────────────────────────────────

#[test]
fn test_stablecoin_mode_disabled_by_default_allows_any_token() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Mode off by default — any token should work.
    let config = client.get_protocol_config();
    assert!(!config.stablecoin_only_mode, "stablecoin_only_mode must be false by default");

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "sc_default_game"),
        &Platform::Lichess,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_stablecoin_mode_rejects_non_stablecoin_token() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    enable_stablecoin_mode(&client, &admin);

    // `token` is NOT registered as a stablecoin issuer.
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "sc_reject_game"),
        &Platform::Lichess,
    );
    assert!(
        matches!(result, Err(Ok(Error::NotStablecoin))),
        "expected NotStablecoin error, got: {:?}",
        result
    );
}

#[test]
fn test_stablecoin_mode_accepts_registered_stablecoin_token() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Register the token as a stablecoin issuer, then enable mode.
    client.add_stablecoin_issuer(&token);
    enable_stablecoin_mode(&client, &admin);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "sc_accept_game"),
        &Platform::Lichess,
    );
    assert_eq!(id, 0, "create_match should succeed for a registered stablecoin token");
}

#[test]
fn test_stablecoin_mode_can_be_toggled_off() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    enable_stablecoin_mode(&client, &admin);

    // Token not registered — must fail.
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "sc_toggle_game1"),
        &Platform::Lichess,
    );
    assert!(result.is_err(), "should fail in stablecoin-only mode");

    // Disable mode.
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        minimum_stake: 1,
    });

    // Same token should now succeed.
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "sc_toggle_game2"),
        &Platform::Lichess,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_multiple_stablecoin_issuers_can_coexist() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token2_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token2_addr = token2_id.address();
    let asset_client2 = StellarAssetClient::new(&env, &token2_addr);
    asset_client2.mint(&player1, &1000);
    asset_client2.mint(&player2, &1000);

    client.add_stablecoin_issuer(&token);
    client.add_stablecoin_issuer(&token2_addr);
    enable_stablecoin_mode(&client, &admin);

    let id1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "sc_multi_game1"),
        &Platform::Lichess,
    );
    assert_eq!(id1, 0);

    let id2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token2_addr,
        &String::from_str(&env, "sc_multi_game2"),
        &Platform::Lichess,
    );
    assert_eq!(id2, 1);

    // A third unregistered token must still be rejected.
    let unknown_token = Address::generate(&env);
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &unknown_token,
        &String::from_str(&env, "sc_multi_game3"),
        &Platform::Lichess,
    );
    assert!(result.is_err(), "unregistered token must be rejected");
}

#[test]
fn test_is_stablecoin_returns_false_for_unknown_token() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    assert!(!client.is_stablecoin(&unknown));
}
