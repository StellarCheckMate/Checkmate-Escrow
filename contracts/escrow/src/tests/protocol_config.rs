/// Tests for `get_protocol_config` / `set_protocol_config`.
///
/// Verifies that the full `ProtocolConfig` struct is correctly stored and
/// returned, and that updates via `set_protocol_config` are reflected
/// immediately in subsequent `get_protocol_config` calls.
use super::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build a non-default `ProtocolConfig` for assertion purposes.
fn custom_config(treasury: &Address) -> ProtocolConfig {
    ProtocolConfig {
        vesting_duration_seconds: 86_400,
        cancellation_fee_basis_points: 50,
        treasury: treasury.clone(),
        stablecoin_only_mode: false,
        maximum_stake: Some(5_000),
        match_timeout_seconds: 604_800, // 7 days
        protocol_fee_bps: 100,
        fee_recipient: treasury.clone(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// After `initialize` + `set_protocol_config` (via `setup()`), calling
/// `get_protocol_config` must return the exact config that was supplied to
/// `set_protocol_config`.
#[test]
fn test_get_protocol_config_returns_config_set_during_initialize() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // The `setup()` fixture stores this exact config via `set_protocol_config`.
    let expected = ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
    };

    let actual = client.get_protocol_config();
    assert_eq!(actual, expected);
}

/// `get_protocol_config` must reflect updates made via a subsequent
/// `set_protocol_config` call (e.g. changing `protocol_fee_bps` and
/// `match_timeout_seconds`).
#[test]
fn test_get_protocol_config_reflects_update_after_set() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let updated = ProtocolConfig {
        vesting_duration_seconds: 3_600,
        cancellation_fee_basis_points: 25,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: Some(10_000),
        match_timeout_seconds: 172_800, // 2 days
        protocol_fee_bps: 200,
        fee_recipient: admin.clone(),
    };

    client.set_protocol_config(&updated);

    let actual = client.get_protocol_config();
    assert_eq!(actual, updated);
}

/// `get_protocol_config` must still return the previous config after
/// `update_oracle` — the oracle address is stored separately from
/// `ProtocolConfig` and must not clobber it.
#[test]
fn test_get_protocol_config_unaffected_by_update_oracle() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let cfg_before = client.get_protocol_config();

    let new_oracle = Address::generate(&env);
    client.update_oracle(&new_oracle);

    let cfg_after = client.get_protocol_config();
    assert_eq!(cfg_before, cfg_after, "update_oracle must not mutate ProtocolConfig");
}

/// `get_protocol_config` must still return the previous config after
/// `transfer_admin` — the admin address is stored separately and must not
/// clobber `ProtocolConfig`.
#[test]
fn test_get_protocol_config_unaffected_by_transfer_admin() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Store a non-default config so we have something meaningful to check.
    let cfg = custom_config(&admin);
    client.set_protocol_config(&cfg);

    let cfg_before = client.get_protocol_config();
    assert_eq!(cfg_before, cfg);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin);

    // Re-read as the new admin.
    let cfg_after = client.get_protocol_config();
    assert_eq!(cfg_after, cfg, "transfer_admin must not mutate ProtocolConfig");
}

/// `set_protocol_config` must be rejected when called by a non-admin.
#[test]
fn test_set_protocol_config_rejected_for_non_admin() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Construct a valid config; the call itself must fail due to auth.
    let cfg = custom_config(&admin);

    // Clear all mocked auths so that the admin signature is absent.
    env.set_auths(&[]);

    let result = client.try_set_protocol_config(&cfg);
    assert!(result.is_err(), "non-admin must not be able to set ProtocolConfig");
}
