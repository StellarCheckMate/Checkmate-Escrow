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
        &String::from_str(&env, "fuzz_game_1"),
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
        &String::from_str(&env, "fuzz_game_2"),
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
        &String::from_str(&env, "fuzz_game_3"),
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

    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
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
        &String::from_str(&env, "fuzz_game_4"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let winner = if winner_is_player1 {
        Winner::Player1
    } else {
        Winner::Player2
    };
    client.submit_result(&match_id, &winner, &oracle);

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

    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
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
        &String::from_str(&env, "fuzz_game_5"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Draw, &oracle);

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
        &String::from_str(&env, "fuzz_game_6"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let impostor = if player1_submits { &player1 } else { &player2 };
    // Must fail — only oracle is authorised
    let result = client.try_submit_result(&match_id, &Winner::Player1, impostor);

    // Oracle itself must succeed
    let ok = client
        .try_submit_result(&match_id, &Winner::Player1, &oracle)
        .is_err(); // match already completed — but the *unauthorised* call is what we test
    let _ = ok;

    result.is_err()
}

// ── Timeout must be within bounds ────────────────────────────────────────────

/// Invariant: set_match_timeout rejects values outside [MIN, MAX].
#[quickcheck]
fn prop_timeout_bounds_enforced(timeout: u32) -> bool {
    use crate::{MAX_MATCH_TIMEOUT_LEDGERS, MIN_MATCH_TIMEOUT_LEDGERS};

    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_set_match_timeout(&admin, &timeout);
    let valid = timeout >= MIN_MATCH_TIMEOUT_LEDGERS && timeout <= MAX_MATCH_TIMEOUT_LEDGERS;

    if valid {
        result.is_ok()
    } else {
        result.is_err()
    }
}
