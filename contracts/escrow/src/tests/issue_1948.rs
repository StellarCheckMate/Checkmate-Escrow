//! Tests for #1948 — cancel_admin_proposal clears the pending nominee so a stale
//! nominee cannot accept.
//!
//! Verifies that:
//! - `accept_admin` fails after `cancel_admin_proposal` (stale nominee is blocked).
//! - A fresh `propose_admin` succeeds after cancellation, restoring the two-step
//!   transfer flow.

#![cfg(test)]

use super::*;

/// `accept_admin` must fail after `cancel_admin_proposal` has been called.
///
/// Once a proposal is cancelled the pending nominee is cleared.  If the
/// previously-proposed address tries to call `accept_admin` it must receive
/// `Error::Unauthorized` rather than completing the transfer.
#[test]
fn test_accept_admin_fails_after_cancel_admin_proposal() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let pending_admin = Address::generate(&env);

    // Propose, then cancel.
    client.propose_admin(&pending_admin);
    client.cancel_admin_proposal();

    // The pending nominee must not be able to accept.
    env.mock_auths(&[MockAuth {
        address: &pending_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_accept_admin();
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "stale nominee must not be able to accept after cancellation"
    );

    // Admin remains unchanged.
    assert_eq!(
        client.get_admin(),
        admin,
        "admin must remain the original admin after failed accept"
    );
}

/// A fresh `propose_admin` works after a prior proposal has been cancelled.
///
/// Cancellation must not leave the contract in a broken state: the admin must
/// be able to propose a new (or the same) next-admin immediately after cancelling.
#[test]
fn test_fresh_propose_admin_works_after_cancellation() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let first_candidate = Address::generate(&env);
    let second_candidate = Address::generate(&env);

    // First proposal, then cancel.
    client.propose_admin(&first_candidate);
    client.cancel_admin_proposal();

    // A fresh proposal for a different address must succeed.
    let result = client.try_propose_admin(&second_candidate);
    assert!(
        result.is_ok(),
        "propose_admin must succeed after prior cancellation"
    );

    // The second candidate can now accept.
    env.mock_auths(&[MockAuth {
        address: &second_candidate,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.accept_admin();
    assert_eq!(
        client.get_admin(),
        second_candidate,
        "second candidate must become admin after accepting fresh proposal"
    );
}
