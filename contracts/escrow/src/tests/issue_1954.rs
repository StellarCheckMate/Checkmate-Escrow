//! Tests for Issue #1954 — finalize_match rejects finalization while the
//! dispute window is still open
//!
//! `finalize_match` must refuse to settle a `PendingResult` match until the
//! dispute window has fully elapsed, and must succeed once the window closes.

use super::*;
use soroban_sdk::testutils::Ledger as _;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Create a match, have both players deposit, then advance the ledger and
/// submit an oracle result, putting the match into `PendingResult`.  Returns
/// `(client, match_id, submit_ledger)`.
fn setup_pending_result(
    env: &Env,
    contract_id: &Address,
    oracle: &Address,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> (EscrowContractClient, u64, u32) {
    let client = EscrowContractClient::new(env, contract_id);

    let match_id = client.create_match(
        player1,
        player2,
        &100,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&match_id, player1);
    client.deposit(&match_id, player2);

    let submit_ledger = 1_000u32;
    env.ledger().set_sequence_number(submit_ledger);
    client.submit_result(&match_id, &Winner::Player1, oracle);

    (client, match_id, submit_ledger)
}

// ── Finalization before dispute window elapses is rejected ────────────────────

/// Calling `finalize_match` immediately after `submit_result` (the dispute
/// window is still open) must return `DisputePeriodNotElapsed`.
#[test]
fn test_finalize_match_rejects_immediately_after_submit() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let (client, match_id, _submit_ledger) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1954aa01",
    );

    // Still at the submit ledger — window has not elapsed at all.
    let result = client.try_finalize_match(&match_id);
    assert_eq!(
        result,
        Err(Ok(Error::DisputePeriodNotElapsed)),
        "finalize_match must be rejected while still at the submit ledger"
    );
}

/// Calling `finalize_match` one ledger before the dispute deadline must still
/// be rejected (strict less-than boundary).
#[test]
fn test_finalize_match_rejects_one_ledger_before_deadline() {
    let dispute_period = 100u32;
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(dispute_period);
    let (client, match_id, submit_ledger) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1954aa02",
    );

    // Advance to one ledger before the deadline.
    env.ledger()
        .set_sequence_number(submit_ledger + dispute_period - 1);

    let result = client.try_finalize_match(&match_id);
    assert_eq!(
        result,
        Err(Ok(Error::DisputePeriodNotElapsed)),
        "finalize_match must be rejected one ledger before the dispute deadline"
    );
}

/// The match state remains `PendingResult` while finalization is blocked.
#[test]
fn test_finalize_match_match_stays_pending_while_window_open() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let (client, match_id, submit_ledger) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1954aa03",
    );

    // Advance halfway through the window.
    env.ledger().set_sequence_number(submit_ledger + 100);

    let _ = client.try_finalize_match(&match_id);

    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::PendingResult,
        "match must remain in PendingResult while the dispute window is open"
    );
    // Funds must still be escrowed.
    assert_eq!(
        client.get_escrow_balance(&match_id),
        200,
        "escrow balance must remain unchanged while finalization is blocked"
    );
}

// ── Finalization after dispute window elapses succeeds ────────────────────────

/// Calling `finalize_match` exactly at the dispute deadline must succeed and
/// transition the match to `Completed`.
#[test]
fn test_finalize_match_succeeds_at_exact_deadline() {
    let dispute_period = 100u32;
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(dispute_period);
    let (client, match_id, submit_ledger) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1954bb01",
    );
    let token_client = TokenClient::new(&env, &token);

    // Advance to exactly the deadline.
    env.ledger()
        .set_sequence_number(submit_ledger + dispute_period);

    let result = client.try_finalize_match(&match_id);
    assert!(
        result.is_ok(),
        "finalize_match must succeed at exactly the dispute deadline: {:?}",
        result
    );

    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::Completed,
        "match must be Completed after successful finalization"
    );
    // Player1 was the oracle winner — they receive the full pot.
    assert_eq!(
        token_client.balance(&player1),
        1100,
        "oracle winner must receive the pot after finalization"
    );
    assert_eq!(
        client.get_escrow_balance(&match_id),
        0,
        "escrow must be empty after finalization"
    );
}

/// Calling `finalize_match` well past the deadline must also succeed.
#[test]
fn test_finalize_match_succeeds_well_past_deadline() {
    let dispute_period = 50u32;
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(dispute_period);
    let (client, match_id, submit_ledger) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1954bb02",
    );

    // Advance far past the deadline.
    env.ledger()
        .set_sequence_number(submit_ledger + dispute_period * 10);

    let result = client.try_finalize_match(&match_id);
    assert!(
        result.is_ok(),
        "finalize_match must succeed well past the dispute deadline: {:?}",
        result
    );

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
}
