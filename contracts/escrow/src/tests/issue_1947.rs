//! Tests for #1947 — propose_admin rejects proposing the zero address as the next admin.
//!
//! In Soroban's execution environment there is no literal "zero address" primitive.
//! The closest sentinels are:
//!   - The contract's own address (self-proposal would lock admin access).
//!   - The existing admin address (a no-op proposal that would immediately
//!     resolve to the current admin).
//!
//! Both are guarded in `propose_admin` with `Error::InvalidAddress`.
//! These tests verify those guards are in place and that a genuinely valid
//! (non-sentinel) address is accepted.

#![cfg(test)]

use super::*;

/// `propose_admin` must reject the contract's own address as the next admin.
///
/// Proposing the contract address would lock admin access since the contract
/// cannot sign transactions on its own behalf.  This is the on-chain equivalent
/// of proposing the zero address: an address that can never accept the proposal.
#[test]
fn test_propose_admin_rejects_contract_address_as_next_admin() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_propose_admin(&contract_id);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "propose_admin must reject the contract address (zero-equivalent sentinel)"
    );
}

/// `propose_admin` must reject the existing admin as the next admin.
///
/// Proposing the current admin is a no-op: accepting it would leave the admin
/// unchanged and waste ledger budget.  The guard rejects this case to prevent
/// accidental or misleading proposals.
#[test]
fn test_propose_admin_rejects_existing_admin_as_next_admin() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_propose_admin(&admin);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAddress)),
        "propose_admin must reject the existing admin address (no-op sentinel)"
    );
}

/// `propose_admin` must accept a valid (non-sentinel) address.
///
/// Any address that is neither the contract itself nor the current admin
/// should be stored as the pending admin without error.
#[test]
fn test_propose_admin_accepts_valid_address() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    let result = client.try_propose_admin(&new_admin);
    assert!(
        result.is_ok(),
        "propose_admin must succeed for a valid non-sentinel address"
    );

    // The pending proposal is stored; current admin is unchanged.
    assert_eq!(
        client.get_admin(),
        admin,
        "current admin must not change until accept_admin is called"
    );
}
