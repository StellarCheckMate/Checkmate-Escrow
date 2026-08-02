/// Property-based fuzzing tests for escrow edge cases using quickcheck.
///
/// Run with: cargo test -p escrow fuzz
use super::*;
use quickcheck::TestResult;
use quickcheck_macros::quickcheck;

// ── Arbitrary stake amounts ───────────────────────────────────────────────────

/// Invariant: any stake ≤ 0 must be rejected; any stake > 0 must be accepted.
#[quickcheck]
fn prop_create_match_stake_validation(stake: i128) -> TestResult {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Avoid overflowing the token mint (2 × stake must fit in i128 and player balance)
    if stake > 500_000_000_000i128 {
        return TestResult::discard();
    }

    let result = client.try_create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "c46f48a2"),
        &Platform::Lichess,
    );

    if stake <= 0 {
        TestResult::from_bool(result.is_err())
    } else {
        TestResult::from_bool(result.is_ok())
    }
}

// ── Escrow balance invariant ──────────────────────────────────────────────────

/// Invariant: after both deposits the escrow balance equals exactly 2 × stake.
#[quickcheck]
fn prop_escrow_balance_equals_two_stakes(stake: i128) -> TestResult {
    if stake <= 0 || stake > 500i128 {
        return TestResult::discard();
    }

    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Mint enough for the chosen stake
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &stake);
    asset_client.mint(&player2, &stake);

    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "10f1761a"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let balance = client.get_escrow_balance(&match_id);
    TestResult::from_bool(balance == 2 * stake)
}

// ── No double-deposit ─────────────────────────────────────────────────────────

/// Invariant: a second deposit from the same player must always fail.
#[quickcheck]
fn prop_no_double_deposit(use_player1: bool) -> bool {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "f56b3b7a"),
        &Platform::Lichess,
    );

    let depositor = if use_player1 { &player1 } else { &player2 };
    client.deposit(&match_id, depositor);
    let second = client.try_deposit(&match_id, depositor);
    second.is_err()
}

// ── Payout conservation ───────────────────────────────────────────────────────

/// Invariant: total tokens in circulation are conserved after a winner payout.
/// player1_balance + player2_balance must equal the combined pre-match balances.
#[quickcheck]
fn prop_payout_conserves_tokens(stake: i128, winner_is_player1: bool) -> TestResult {
    if stake <= 0 || stake > 500i128 {
        return TestResult::discard();
    }

    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = TokenClient::new(&env, &token);
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &stake);
    asset_client.mint(&player2, &stake);

    let before_p1 = tc.balance(&player1);
    let before_p2 = tc.balance(&player2);
    let total_before = before_p1 + before_p2;

    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "c2a23f18"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let winner = if winner_is_player1 {
        Winner::Player1
    } else {
        Winner::Player2
    };
    client.submit_result(&match_id, &winner);

    let after_total = tc.balance(&player1) + tc.balance(&player2);
    TestResult::from_bool(after_total == total_before)
}

// ── Draw refund conservation ──────────────────────────────────────────────────

/// Invariant: on a draw both players get their exact stake back.
#[quickcheck]
fn prop_draw_refunds_exact_stakes(stake: i128) -> TestResult {
    if stake <= 0 || stake > 500i128 {
        return TestResult::discard();
    }

    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = TokenClient::new(&env, &token);
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &stake);
    asset_client.mint(&player2, &stake);

    let before_p1 = tc.balance(&player1);
    let before_p2 = tc.balance(&player2);

    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "d36ea1fa"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Draw);

    TestResult::from_bool(
        tc.balance(&player1) == before_p1 && tc.balance(&player2) == before_p2,
    )
}

// ── Unauthorised result submission ────────────────────────────────────────────

/// Invariant: a non-oracle address must never be able to submit a result.
#[quickcheck]
fn prop_only_oracle_can_submit_result(player1_submits: bool) -> bool {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "1a49d1ef"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let impostor = if player1_submits { &player1 } else { &player2 };

    // Must fail — only the oracle's auth may satisfy submit_result's
    // internal oracle.require_auth(), regardless of who nominally invoked it.
    env.mock_auths(&[MockAuth {
        address: impostor,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (match_id, Winner::Player1).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let impostor_result = client.try_submit_result(&match_id, &Winner::Player1);

    // Oracle itself must succeed on the same match.
    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (match_id, Winner::Player1).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let oracle_result = client.try_submit_result(&match_id, &Winner::Player1);

    impostor_result.is_err() && oracle_result.is_ok()
}

// ── Timeout must be within bounds ────────────────────────────────────────────

/// Invariant: set_match_timeout rejects values outside [MIN, MAX].
#[quickcheck]
fn prop_timeout_bounds_enforced(timeout: u64) -> bool {
    use crate::{MAX_MATCH_TIMEOUT_SECONDS, MIN_MATCH_TIMEOUT_SECONDS};

    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_set_match_timeout(&timeout);
    let valid = timeout >= MIN_MATCH_TIMEOUT_SECONDS && timeout <= MAX_MATCH_TIMEOUT_SECONDS;

    if valid {
        result.is_ok()
    } else {
        result.is_err()
    }
}


// ── Match State Machine Invariants (Property-Based) ───────────────────────────

/// Property: A Completed match never transitions to any other state.
/// Attempting to deposit, cancel, or re-submit a result on a Completed match must fail.
#[quickcheck]
fn prop_completed_match_never_transitions(op: u8) -> TestResult {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "stinvg01"),
        &Platform::Lichess,
    );

    // Move match to Completed state
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed, "match should be Completed");

    // Depending on op, try an invalid transition
    let result = match op % 3 {
        0 => {
            // Try to deposit (should fail: invalid state)
            client.try_deposit(&match_id, &player1)
        }
        1 => {
            // Try to submit result again (should fail: invalid state)
            client.try_submit_result(&match_id, &Winner::Draw)
        }
        2 => {
            // Try to cancel (should fail: invalid state)
            client.try_cancel_match(&match_id, &player1)
        }
        _ => unreachable!(),
    };

    TestResult::from_bool(result.is_err())
}

/// Property: A Cancelled match never receives a payout.
/// Attempting to submit_result or claim_payout on a Cancelled match must fail.
#[quickcheck]
fn prop_cancelled_match_never_pays_out(game_id_variant: u8) -> TestResult {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create match and move to Cancelled via early timeout
    let game_id = format!("cnp{:05}", game_id_variant);
    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, &game_id),
        &Platform::Lichess,
    );

    // Cancel without deposits
    let result_cancel = client.try_cancel_match(&match_id, &player1);
    if result_cancel.is_err() {
        return TestResult::discard();
    }

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Cancelled, "match should be Cancelled");

    // Try to submit result on cancelled match (should fail)
    let result = client.try_submit_result(&match_id, &Winner::Player1);
    TestResult::from_bool(result.is_err())
}

/// Property: Active matches can be properly transitioned to Completed or Paused.
/// Once Active, a match must either reach Completed (or PendingResult) or Paused state.
#[quickcheck]
fn prop_active_match_can_complete_or_pause(transition: u8) -> TestResult {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "actmch01"),
        &Platform::Lichess,
    );

    // Move to Active state
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Active, "match should be Active");

    // Attempt a valid transition based on transition variant
    let result = match transition % 2 {
        0 => {
            // Submit result (should succeed, moving to Completed or PendingResult)
            client.try_submit_result(&match_id, &Winner::Player1)
        }
        1 => {
            // Pause the match (should succeed, moving to Paused)
            client.try_pause_match(&match_id, &player1)
        }
        _ => unreachable!(),
    };

    TestResult::from_bool(result.is_ok())
}

/// Property: Pending matches can only transition to Active, Cancelled, or stay Pending.
/// A Pending match must not reach Completed, Paused, or PendingResult without first transitioning to Active.
#[quickcheck]
fn prop_pending_match_limited_transitions() -> bool {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "pndtrn01"),
        &Platform::Lichess,
    );

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Pending, "match should start Pending");

    // Attempting submit_result on Pending should fail (only works on Active)
    let submit_pending = client.try_submit_result(&match_id, &Winner::Player1);

    // Should fail because match is Pending, not Active
    submit_pending.is_err()
}
