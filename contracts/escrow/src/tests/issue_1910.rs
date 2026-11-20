//! Tests for Issue #1910 — Security: dispute_oracle_result does not bound the
//! number of disputes per match.
//!
//! Without a cap, a caller could open an unbounded number of disputes for the
//! same match, consuming dispute-period budget and burning oracle capacity.
//! The fix enforces at most one open dispute per match via the
//! `MatchDispute(match_id)` → dispute_id mapping, returning
//! `DisputeAlreadyRaised` when a second dispute is attempted.

use super::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A valid 64-hex-char evidence hash (32 bytes, non-zero).
const EVIDENCE_HASH: &str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

/// Bring a match to `PendingResult` state so disputes can be opened.
///
/// Returns `(client, match_id)`.
fn setup_pending_result(
    env: &Env,
    contract_id: &Address,
    oracle: &Address,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> (EscrowContractClient, u64) {
    let client = EscrowContractClient::new(env, contract_id);

    // Set a non-zero dispute period so the match enters PendingResult.
    client.set_dispute_period(&1000u32);

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
    client.submit_result(&match_id, &Winner::Player1, oracle);

    // Confirm the match is now in PendingResult state.
    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::PendingResult,
        "match must be in PendingResult after submit_result with dispute period"
    );

    (client, match_id)
}

// ── Rejection tests ───────────────────────────────────────────────────────────

/// A second call to `dispute_oracle_result` on the same match (while the first
/// dispute is still open) must be rejected with `DisputeAlreadyRaised`.
#[test]
fn test_dispute_oracle_result_second_dispute_is_rejected() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let (client, match_id) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1910aa01",
    );

    // First dispute — must succeed.
    let first_result = client.try_dispute_oracle_result(
        &match_id,
        &player1,
        &String::from_str(&env, EVIDENCE_HASH),
    );
    assert!(
        first_result.is_ok(),
        "first dispute must be accepted; got {:?}",
        first_result
    );

    // Second dispute on the same match — must be rejected.
    let second_result = client.try_dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, EVIDENCE_HASH),
    );
    assert_eq!(
        second_result,
        Err(Ok(Error::DisputeAlreadyRaised)),
        "second dispute on same match must return DisputeAlreadyRaised"
    );
}

/// The same player trying to open a second dispute must also be rejected.
#[test]
fn test_dispute_oracle_result_same_player_second_dispute_is_rejected() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let (client, match_id) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1910bb01",
    );

    // First dispute from player1.
    client
        .try_dispute_oracle_result(
            &match_id,
            &player1,
            &String::from_str(&env, EVIDENCE_HASH),
        )
        .expect("first dispute must succeed");

    // Player1 tries again.
    let result = client.try_dispute_oracle_result(
        &match_id,
        &player1,
        &String::from_str(&env, EVIDENCE_HASH),
    );
    assert_eq!(
        result,
        Err(Ok(Error::DisputeAlreadyRaised)),
        "repeated dispute from the same player must return DisputeAlreadyRaised"
    );
}

/// Sanity-check: the first dispute on a fresh PendingResult match is accepted.
#[test]
fn test_dispute_oracle_result_first_dispute_is_accepted() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let (client, match_id) = setup_pending_result(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1910cc01",
    );

    let result = client.try_dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, EVIDENCE_HASH),
    );
    assert!(
        result.is_ok(),
        "first dispute must succeed; got {:?}",
        result
    );
}
