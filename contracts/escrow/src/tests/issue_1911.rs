//! Tests for Issue #1911 — Security: vote_on_dispute does not exclude the
//! disputed oracle from voting on its own result.
//!
//! When an oracle submits a result that gets disputed, the same oracle address
//! must not be allowed to cast a vote that influences its own outcome. The fix
//! stores the oracle address under `DisputeOracle(dispute_id)` at dispute-
//! creation time and rejects any `vote_on_dispute` call where `voter` matches
//! that stored address, returning `Unauthorized`.

use super::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// A valid 64-hex-char evidence hash.
const EVIDENCE_HASH: &str = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

/// Set up a match in `PendingResult` state and open a dispute on it.
///
/// Returns `(client, match_id, dispute_id)`.
fn setup_active_dispute(
    env: &Env,
    contract_id: &Address,
    oracle: &Address,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> (EscrowContractClient, u64, u64) {
    let client = EscrowContractClient::new(env, contract_id);
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

    let dispute_id = client
        .try_dispute_oracle_result(
            &match_id,
            &player1,
            &String::from_str(env, EVIDENCE_HASH),
        )
        .expect("opening dispute must succeed");

    (client, match_id, dispute_id)
}

// ── Oracle exclusion test ─────────────────────────────────────────────────────

/// The oracle whose result is being disputed must not be able to vote on the
/// dispute, regardless of whether it holds a staker balance.
#[test]
fn test_vote_on_dispute_disputed_oracle_cannot_vote() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();

    // Give the oracle a token balance so it would otherwise pass the staker check.
    let asset_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    asset_client.mint(&oracle, &10_000);

    // Record a balance snapshot for the oracle so the snapshot-based eligibility
    // check doesn't reject it before the oracle-exclusion check runs.
    // We do this by fast-patching — just mint and ensure a snapshot exists.
    // In practice the oracle exclusion check runs before the staker weight check,
    // so even with a qualifying snapshot the vote must be rejected.

    let (client, _match_id, dispute_id) = setup_active_dispute(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1911aa01",
    );

    let result = client.try_vote_on_dispute(&dispute_id, &oracle, &true);

    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "the disputed oracle must be rejected with Unauthorized when attempting to vote"
    );
}

/// A third-party address (not the oracle) is not excluded by the oracle-
/// exclusion guard. If it passes other checks it should be allowed to vote.
/// Here we confirm the guard does not over-block: the error returned for a
/// non-oracle address should be a staker/balance error, not Unauthorized.
#[test]
fn test_vote_on_dispute_non_oracle_address_not_blocked_by_oracle_guard() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let third_party = Address::generate(&env);

    let (client, _match_id, dispute_id) = setup_active_dispute(
        &env,
        &contract_id,
        &oracle,
        &player1,
        &player2,
        &token,
        "1911bb01",
    );

    // The third party has no balance and no snapshot, so it hits NotStaker or
    // InsufficientHoldingDuration — NOT Unauthorized (which would indicate the
    // oracle-exclusion check is incorrectly blocking non-oracle voters).
    let result = client.try_vote_on_dispute(&dispute_id, &third_party, &true);

    assert!(
        result != Err(Ok(Error::Unauthorized)),
        "a non-oracle address must not be blocked by the oracle-exclusion guard; \
         got Unauthorized which is reserved for the disputed oracle only"
    );
}
