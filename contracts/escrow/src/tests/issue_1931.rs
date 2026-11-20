//! #1931 — `get_depositor_count` recomputes from storage on every call.
//!
//! The `depositor_count` field is now cached directly on the [`Match`] struct:
//! - Incremented from 0 → 1 on the first deposit, then 1 → 2 on the second.
//! - Reset to 0 when a match is cancelled (via `cancel_match` or `expire_match`).
//!
//! This suite verifies that the counter always agrees with the actual
//! depositor list reflected by `player1_deposited` / `player2_deposited`.

use super::*;

/// Fresh match (no deposits): counter must be 0.
#[test]
fn test_depositor_count_zero_on_fresh_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19310001"),
        &Platform::Lichess,
    );

    assert_eq!(
        client.get_depositor_count(&id),
        0,
        "counter must be 0 before any deposit"
    );
}

/// After player1 deposits, counter must be 1.
#[test]
fn test_depositor_count_one_after_first_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19310002"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    assert_eq!(
        client.get_depositor_count(&id),
        1,
        "counter must be 1 after one deposit"
    );
}

/// After both players deposit, counter must be 2.
#[test]
fn test_depositor_count_two_after_both_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19310003"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    assert_eq!(
        client.get_depositor_count(&id),
        2,
        "counter must be 2 after both players deposit"
    );
}

/// After cancellation the counter must be 0: no funds are held in escrow.
#[test]
fn test_depositor_count_zero_after_cancel() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19310004"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    assert_eq!(client.get_depositor_count(&id), 1);

    client.cancel_match(&id, &player1);

    assert_eq!(
        client.get_depositor_count(&id),
        0,
        "counter must be reset to 0 after cancellation"
    );
}

/// The cached counter must equal the count derived from the boolean flags
/// at every step of the lifecycle (create → deposit → deposit → cancel).
/// This is the invariant that the counter always matches the actual depositor list.
#[test]
fn test_depositor_count_matches_actual_depositor_list_throughout_lifecycle() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19310005"),
        &Platform::Lichess,
    );

    // Helper closure: derive the expected count from the deposit flags.
    let expected_count = |m: &Match| -> u32 {
        (m.player1_deposited as u32) + (m.player2_deposited as u32)
    };

    // Step 0: no deposits.
    let m = client.get_match(&id);
    assert_eq!(client.get_depositor_count(&id), expected_count(&m), "step 0");

    // Step 1: player1 deposits.
    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert_eq!(client.get_depositor_count(&id), expected_count(&m), "step 1");

    // Step 2: player2 deposits.
    client.deposit(&id, &player2);
    let m = client.get_match(&id);
    assert_eq!(client.get_depositor_count(&id), expected_count(&m), "step 2");

    // The match is now Active — both deposited. Counter = 2.
    assert_eq!(client.get_depositor_count(&id), 2);
}

/// Counter stays at 2 after the oracle submits a result (Completed state).
/// The counter tracks *who deposited*, not whether funds are still held.
/// (For the "is money currently held?" query, use `is_currently_escrowed`.)
#[test]
fn test_depositor_count_stays_two_after_payout() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "19310006"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&id, &player1);

    // Both deposit flags are still set after payout; counter mirrors them.
    assert_eq!(
        client.get_depositor_count(&id),
        2,
        "counter must remain 2 after payout: flags are historical"
    );
}
