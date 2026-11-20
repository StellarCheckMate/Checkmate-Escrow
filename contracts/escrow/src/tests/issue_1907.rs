//! Regression tests for Issue #1907.
//!
//! `set_protocol_config` previously accepted a `ProtocolConfig` whose
//! `fee_recipient` could be the zero address (or the oracle address), so
//! collected fees could be routed to an unrecoverable or unintended account.
//!
//! The fix adds a guard that rejects `fee_recipient` values that equal:
//! - The contract's own address (pre-existing check, retained).
//! - The oracle address (new guard: routing fees to the oracle is a
//!   misconfiguration that would give the oracle operator unintended custody).

use super::*;

// ── fee_recipient == oracle ───────────────────────────────────────────────────

/// Setting `fee_recipient` to the oracle address must be rejected.
/// The oracle is an operational keypair for game-result submission and must
/// not receive protocol fees.
#[test]
fn test_set_protocol_config_rejects_oracle_as_fee_recipient() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mut config = client.get_protocol_config().unwrap();
    config.fee_recipient = oracle.clone();

    let result = client.try_set_protocol_config(&config);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "fee_recipient == oracle address must be rejected"
    );
}

// ── fee_recipient == contract ─────────────────────────────────────────────────

/// Setting `fee_recipient` to the contract's own address must be rejected
/// (pre-existing guard — retained as a regression test).
#[test]
fn test_set_protocol_config_rejects_contract_as_fee_recipient() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mut config = client.get_protocol_config().unwrap();
    config.fee_recipient = contract_id.clone();

    let result = client.try_set_protocol_config(&config);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "fee_recipient == contract address must be rejected"
    );
}

// ── Happy path ────────────────────────────────────────────────────────────────

/// A `fee_recipient` set to a distinct, non-oracle external address must be
/// accepted.
#[test]
fn test_set_protocol_config_accepts_valid_fee_recipient() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Use a freshly-generated address that is neither the contract nor the oracle.
    let external_recipient = Address::generate(&env);

    let mut config = client.get_protocol_config().unwrap();
    config.fee_recipient = external_recipient.clone();

    let result = client.try_set_protocol_config(&config);
    assert_eq!(
        result,
        Ok(()),
        "a valid external fee_recipient must be accepted"
    );
}
