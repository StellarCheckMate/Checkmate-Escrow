//! End-to-end tests: deploy the release WASM contracts and drive the complete
//! match lifecycle — create match → deposit ×2 → oracle records result →
//! escrow settles → players claim the vested payout — verifying state and
//! balances at every step.

use e2e_tests::{deploy_and_fund, MINT_AMOUNT, STAKE, World};
use escrow::{
    errors::Error as EscrowError,
    types::{MatchState, Platform as EscrowPlatform, Winner as EscrowWinner},
};
use oracle::{
    errors::Error as OracleError,
    types::{Platform as OraclePlatform, Winner as OracleWinner},
};
use soroban_sdk::{testutils::Address as _, Address, String};

/// Realistic Lichess game id (exactly 8 alphanumeric chars).
const GAME_ID: &str = "abcd1234";

/// How long the oracle took to fetch + verify the result, in ms.
const RESPONSE_TIME_MS: u64 = 250;

/// Confidence score the oracle attaches to the result (0-100).
const CONFIDENCE: Option<u8> = Some(95);

/// Drives the full lifecycle for a given outcome and asserts every invariant
/// along the way:
///
/// 1. deploy (done in `deploy_and_fund`) — release WASM, not in-crate code
/// 2. create match → `Pending`
/// 3. deposit ×2 → `Active`, escrow holds `2 × stake`
/// 4. oracle records the verified result
/// 5. escrow settles → `Completed`, payout vests
/// 6. players claim → winner takes the pot, contract retains nothing
fn assert_full_lifecycle(winner: EscrowWinner, oracle_result: OracleWinner) {
    let World {
        env,
        escrow,
        oracle,
        token,
        token_client,
        oracle_admin,
        player1,
        player2,
        ..
    } = deploy_and_fund();

    // ── 1. Create match ──────────────────────────────────────────────────
    let id = escrow.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, GAME_ID),
        &EscrowPlatform::Lichess,
    );
    assert_eq!(id, 0, "first match should be assigned id 0");

    let pending = escrow.get_match(&id);
    assert_eq!(pending.state, MatchState::Pending, "match starts Pending");
    assert_eq!(escrow.get_escrow_balance(&id), 0, "nothing in escrow yet");

    // ── 2. Both players deposit ──────────────────────────────────────────
    escrow.deposit(&id, &player1);
    assert!(!escrow.is_funded(&id), "not funded after only player1 deposits");
    assert_eq!(escrow.get_escrow_balance(&id), STAKE);

    escrow.deposit(&id, &player2);
    assert!(escrow.is_funded(&id), "funded once both players deposit");
    assert_eq!(escrow.get_escrow_balance(&id), 2 * STAKE);
    assert_eq!(escrow.get_match(&id).state, MatchState::Active);

    // The escrow contract holds 2× stake; players are down their stakes.
    assert_eq!(token_client.balance(&escrow.address), 2 * STAKE);
    assert_eq!(token_client.balance(&player1), MINT_AMOUNT - STAKE);
    assert_eq!(token_client.balance(&player2), MINT_AMOUNT - STAKE);

    // ── 3. Oracle records the verified result ────────────────────────────
    oracle.submit_result(
        &id,
        &String::from_str(&env, GAME_ID),
        &OraclePlatform::Lichess,
        &oracle_result,
        &RESPONSE_TIME_MS,
        &CONFIDENCE,
    );
    assert!(oracle.has_result(&id), "oracle stores the result");
    let entry = oracle.get_result(&id);
    assert_eq!(entry.game_id, String::from_str(&env, GAME_ID));
    assert_eq!(entry.result, oracle_result);
    assert_eq!(entry.confidence, CONFIDENCE);

    // ── 4. Oracle settles the match in escrow → payout vests ─────────────
    escrow.submit_result(&id, &winner, &oracle_admin, &CONFIDENCE);
    let settled = escrow.get_match(&id);
    assert_eq!(settled.state, MatchState::Completed, "match completes on settle");
    assert_eq!(settled.winner, winner);
    assert_eq!(escrow.get_escrow_balance(&id), 0);

    // ── 5. Players claim the vested payout ───────────────────────────────
    match winner {
        EscrowWinner::Player1 => {
            escrow.claim_vested_payout(&id, &player1);
            // The loser cannot claim anything — the pot goes to the winner.
            assert_eq!(
                escrow.try_claim_vested_payout(&id, &player2),
                Err(Ok(EscrowError::Unauthorized)),
                "loser must not be able to claim a payout"
            );
            assert_eq!(token_client.balance(&player1), MINT_AMOUNT + STAKE);
            assert_eq!(token_client.balance(&player2), MINT_AMOUNT - STAKE);
        }
        EscrowWinner::Player2 => {
            escrow.claim_vested_payout(&id, &player2);
            assert_eq!(
                escrow.try_claim_vested_payout(&id, &player1),
                Err(Ok(EscrowError::Unauthorized)),
                "loser must not be able to claim a payout"
            );
            assert_eq!(token_client.balance(&player1), MINT_AMOUNT - STAKE);
            assert_eq!(token_client.balance(&player2), MINT_AMOUNT + STAKE);
        }
        EscrowWinner::Draw => {
            escrow.claim_vested_payout(&id, &player1);
            escrow.claim_vested_payout(&id, &player2);
            assert_eq!(token_client.balance(&player1), MINT_AMOUNT);
            assert_eq!(token_client.balance(&player2), MINT_AMOUNT);
        }
        EscrowWinner::None => unreachable!("tests only settle real outcomes"),
    }

    assert_eq!(token_client.balance(&escrow.address), 0, "contract must not retain funds after payout");
}

#[test]
fn test_full_lifecycle_player1_wins() {
    assert_full_lifecycle(EscrowWinner::Player1, OracleWinner::Player1);
}

#[test]
fn test_full_lifecycle_player2_wins() {
    assert_full_lifecycle(EscrowWinner::Player2, OracleWinner::Player2);
}

#[test]
fn test_full_lifecycle_draw() {
    assert_full_lifecycle(EscrowWinner::Draw, OracleWinner::Draw);
}

/// An impostor account must not be able to settle a match: the match stays
/// Active and the funds stay in escrow.
#[test]
fn test_non_oracle_cannot_submit_result() {
    let World { env, escrow, token, token_client, player1, player2, .. } = deploy_and_fund();

    let id = escrow.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "efgh5678"),
        &EscrowPlatform::Lichess,
    );
    escrow.deposit(&id, &player1);
    escrow.deposit(&id, &player2);

    let impostor = Address::generate(&env);
    let result = escrow.try_submit_result(&id, &EscrowWinner::Player1, &impostor, &None);
    assert_eq!(
        result,
        Err(Ok(EscrowError::Unauthorized)),
        "non-oracle submit_result must be rejected"
    );

    // Match untouched — still Active with both stakes still in escrow.
    assert_eq!(escrow.get_match(&id).state, MatchState::Active);
    assert_eq!(token_client.balance(&escrow.address), 2 * STAKE);
}

/// The oracle contract must refuse a second result submission for the same
/// match — no overwriting a recorded result.
#[test]
fn test_oracle_duplicate_submit_rejected() {
    let World { env, escrow, oracle, token, player1, player2, .. } = deploy_and_fund();

    let id = escrow.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "ijkl9012"),
        &EscrowPlatform::Lichess,
    );

    oracle.submit_result(
        &id,
        &String::from_str(&env, "ijkl9012"),
        &OraclePlatform::Lichess,
        &OracleWinner::Player1,
        &RESPONSE_TIME_MS,
        &CONFIDENCE,
    );
    assert!(oracle.has_result(&id), "first submission is recorded");

    let result = oracle.try_submit_result(
        &id,
        &String::from_str(&env, "ijkl9012"),
        &OraclePlatform::Lichess,
        &OracleWinner::Player2,
        &RESPONSE_TIME_MS,
        &CONFIDENCE,
    );
    assert_eq!(
        result,
        Err(Ok(OracleError::AlreadySubmitted)),
        "a second submission for the same match must be rejected"
    );

    // The originally recorded result is untouched.
    assert_eq!(oracle.get_result(&id).result, OracleWinner::Player1);
}

/// A player cannot claim a payout before the oracle settles the match.
#[test]
fn test_claim_before_settlement_rejected() {
    let World { env, escrow, token, player1, player2, .. } = deploy_and_fund();

    let id = escrow.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "mnop3456"),
        &EscrowPlatform::Lichess,
    );
    escrow.deposit(&id, &player1);
    escrow.deposit(&id, &player2);
    assert_eq!(escrow.get_match(&id).state, MatchState::Active);

    let result = escrow.try_claim_vested_payout(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(EscrowError::InvalidState)),
        "claiming before settlement must be rejected"
    );
}

/// The oracle cannot settle a match that was never fully funded.
#[test]
fn test_submit_result_on_unfunded_match_rejected() {
    let World {
        env,
        escrow,
        oracle_admin,
        token,
        token_client,
        player1,
        player2,
        ..
    } = deploy_and_fund();

    let id = escrow.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "qrst7890"),
        &EscrowPlatform::Lichess,
    );
    // Only player1 deposits — match stays Pending and unfunded.
    escrow.deposit(&id, &player1);

    let result = escrow.try_submit_result(&id, &EscrowWinner::Player1, &oracle_admin, &CONFIDENCE);
    assert_eq!(
        result,
        Err(Ok(EscrowError::NotFunded)),
        "settling an unfunded match must be rejected"
    );

    assert_eq!(escrow.get_match(&id).state, MatchState::Pending);
    assert_eq!(token_client.balance(&escrow.address), STAKE);
}
