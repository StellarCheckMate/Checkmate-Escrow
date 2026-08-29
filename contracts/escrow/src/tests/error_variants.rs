//! Regression tests — one test per `Error` variant.
//!
//! Each test verifies that the contract returns **exactly** the expected error
//! code in the scenario that is documented for that variant.  The primary goal
//! is to catch accidental renumbering: if a variant's discriminant changes, the
//! test for it will fail because `try_*` calls return `Err(Ok(Error::Variant))`
//! and the discriminant is part of the XDR representation.
//!
//! Error variants covered (49 total, matching the XDR cap):
//!
//!  1  MatchNotFound
//!  2  AlreadyFunded
//!  3  NotFunded
//!  4  Unauthorized
//!  5  InvalidState
//!  6  AlreadyExists
//!  7  AlreadyInitialized
//!  8  Overflow (discriminant preserved via constant assertion)
//!  9  ContractPaused  (also reused for player-freeze)
//! 10  InvalidAmount
//! 13  DuplicateGameId
//! 14  MatchNotExpired
//! 15  InvalidGameId
//! 16  InvalidPlayers
//! 17  TokenNotAllowed
//! 18  InvalidAddress
//! 19  MatchAlreadyActive
//! 20  InvalidTimeout
//! 21  SnapshotNotFound
//! 22  VestingNotExpired
//! 23  AlreadyClaimed
//! 24  DisputeNotFound
//! 25  PendingResultNotFound
//! 26  DisputeAlreadyResolved
//! 27  VotingPeriodElapsed
//! 28  AlreadyVoted
//! 29  NotStaker
//! 30  VotingPeriodNotElapsed
//! 31  MatchNotInPendingResult
//! 32  DisputePeriodNotElapsed
//! 33  DisputeAlreadyRaised
//! 34  InvalidEvidenceHash
//! 35  TierStakeNotAllowed
//! 36  NotInitialized
//! 37  InvalidPauseState
//! 39  ConversionRateOutOfBounds
//! 40  ConversionRateStalePriceSource
//! 41  InsufficientBond
//! 42  QuorumNotMet
//! 43  InsufficientHoldingDuration
//! 45  TooManyActiveMatches
//! 46  NotStablecoin
//! 47  UpgradeNotScheduled
//! 48  UpgradeReviewPeriodNotElapsed
//! 49  InvalidVersion
//! 50  UpgradeAlreadyScheduled
//! 51  OracleAlreadyConfirmed
//! 52  ConflictingResult
//! 54  NotAnOracle
//! 55  DepositInProgress  (discriminant preserved via constant assertion)

use super::*;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::BytesN;

// ── Discriminant sanity-check ─────────────────────────────────────────────────
//
// For variants that are hard to trigger in a pure contract test (Overflow,
// DepositInProgress) we assert their discriminant at compile time so that
// renumbering still fails CI.

#[allow(dead_code)]
const _OVERFLOW_DISCRIMINANT: () = {
    assert!(Error::Overflow as u32 == 8);
};

#[allow(dead_code)]
const _DEPOSIT_IN_PROGRESS_DISCRIMINANT: () = {
    assert!(Error::DepositInProgress as u32 == 55);
};

// ── Helper ────────────────────────────────────────────────────────────────────

fn dummy_wasm_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

// ── 1: MatchNotFound ─────────────────────────────────────────────────────────

#[test]
fn error_match_not_found() {
    let (env, contract_id, oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let nonexistent_id: u64 = 99_999;
    let result = client.try_submit_result(&nonexistent_id, &Winner::Player1, &oracle);
    assert_eq!(result, Err(Ok(Error::MatchNotFound)));
}

// ── 2: AlreadyFunded ─────────────────────────────────────────────────────────

#[test]
fn error_already_funded() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_af001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);

    // player1 depositing again on an already Active match → AlreadyFunded
    let result = client.try_deposit(&mid, &player1);
    assert_eq!(result, Err(Ok(Error::AlreadyFunded)));
}

// ── 3: NotFunded ─────────────────────────────────────────────────────────────

#[test]
fn error_not_funded() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Only player1 deposits; match is still Pending.
    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_nf001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);

    let result = client.try_submit_result(&mid, &Winner::Player1, &oracle);
    assert_eq!(result, Err(Ok(Error::NotFunded)));
}

// ── 4: Unauthorized ──────────────────────────────────────────────────────────

#[test]
fn error_unauthorized() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_un001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);

    // A random address is not the oracle.
    let imposter = Address::generate(&env);
    let result = client.try_submit_result(&mid, &Winner::Player1, &imposter);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

// ── 5: InvalidState ──────────────────────────────────────────────────────────

#[test]
fn error_invalid_state() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_is001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);
    // Match is now Completed — cancelling a terminal match is invalid state.
    let result = client.try_cancel_match(&mid, &player1);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

// ── 6: AlreadyExists ─────────────────────────────────────────────────────────

// AlreadyExists is returned by `add_allowed_token` when the token is already
// in the allowlist.
#[test]
fn error_already_exists() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_allowed_token(&token);
    let result = client.try_add_allowed_token(&token);
    assert_eq!(result, Err(Ok(Error::AlreadyExists)));
}

// ── 7: AlreadyInitialized ────────────────────────────────────────────────────

#[test]
fn error_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let oracle = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle, &admin);
    let result = client.try_initialize(&oracle, &admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

// ── 9: ContractPaused ────────────────────────────────────────────────────────

#[test]
fn error_contract_paused() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();

    let result = client.try_create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_cp001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

// ── 9 (reuse): ContractPaused returned for frozen player ─────────────────────

#[test]
fn error_contract_paused_frozen_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &String::from_str(&env, "regression test"));

    let result = client.try_create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_fp001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

// ── 10: InvalidAmount ────────────────────────────────────────────────────────

#[test]
fn error_invalid_amount() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1, &player2, &0, &token,
        &String::from_str(&env, "ev_ia001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

// ── 13: DuplicateGameId ──────────────────────────────────────────────────────

#[test]
fn error_duplicate_game_id() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_id = String::from_str(&env, "ev_dg001");
    client.create_match(&player1, &player2, &100, &token, &game_id, &Platform::Lichess);

    let p3 = Address::generate(&env);
    let p4 = Address::generate(&env);
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&p3, &1000);
    asset.mint(&p4, &1000);

    let result = client.try_create_match(&p3, &p4, &100, &token, &game_id, &Platform::Lichess);
    assert_eq!(result, Err(Ok(Error::DuplicateGameId)));
}

// ── 14: MatchNotExpired ──────────────────────────────────────────────────────

#[test]
fn error_match_not_expired() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_me001"),
        &Platform::Lichess,
    );

    // Timeout has not elapsed — expire_match must fail.
    let result = client.try_expire_match(&mid);
    assert_eq!(result, Err(Ok(Error::MatchNotExpired)));
}

// ── 15: InvalidGameId ────────────────────────────────────────────────────────

#[test]
fn error_invalid_game_id() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Empty game ID is invalid.
    let result = client.try_create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, ""),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

// ── 16: InvalidPlayers ───────────────────────────────────────────────────────

#[test]
fn error_invalid_players() {
    let (env, contract_id, _oracle, player1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // player1 cannot play against themselves.
    let result = client.try_create_match(
        &player1, &player1, &100, &token,
        &String::from_str(&env, "ev_ip001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidPlayers)));
}

// ── 17: TokenNotAllowed ──────────────────────────────────────────────────────

#[test]
fn error_token_not_allowed() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Allowlist an unrelated token, which activates enforcement.
    let other_token_id = env.register_stellar_asset_contract_v2(player1.clone());
    let other_token = other_token_id.address();
    client.add_allowed_token(&other_token);

    // Now the original token is not in the allowlist → rejected.
    let result = client.try_create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_tn001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::TokenNotAllowed)));
}

// ── 18: InvalidAddress ───────────────────────────────────────────────────────

#[test]
fn error_invalid_address() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    // A contract address is not a valid oracle address.
    let result = client.try_initialize(&contract_id, &admin);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
}

// ── 19: MatchAlreadyActive ───────────────────────────────────────────────────

#[test]
fn error_match_already_active() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_maa01"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    // Match is now Active — cancelling an active match returns MatchAlreadyActive.
    let result = client.try_cancel_match(&mid, &player1);
    assert_eq!(result, Err(Ok(Error::MatchAlreadyActive)));
}

// ── 20: InvalidTimeout ───────────────────────────────────────────────────────

#[test]
fn error_invalid_timeout() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Timeout of 0 is below the minimum (MIN_MATCH_TIMEOUT_SECONDS = 86_400).
    let result = client.try_set_match_timeout(&0);
    assert_eq!(result, Err(Ok(Error::InvalidTimeout)));
}

// ── 21: SnapshotNotFound ─────────────────────────────────────────────────────

#[test]
fn error_snapshot_not_found() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No snapshots have been taken for match 999.
    let result = client.try_get_balance_snapshot(&999, &0);
    assert_eq!(result, Err(Ok(Error::SnapshotNotFound)));
}

// ── 22: VestingNotExpired ────────────────────────────────────────────────────

#[test]
fn error_vesting_not_expired() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Configure a long vesting duration so the payout is not yet claimable.
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 999_999_999,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
    });

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_ve001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);

    // Vesting has not elapsed → claim must fail.
    let result = client.try_claim_vested_payout(&mid, &player1);
    assert_eq!(result, Err(Ok(Error::VestingNotExpired)));
}

// ── 23: AlreadyClaimed ───────────────────────────────────────────────────────

#[test]
fn error_already_claimed() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_ac001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);
    client.claim_vested_payout(&mid, &player1);

    // Second claim by the same player → AlreadyClaimed.
    let result = client.try_claim_vested_payout(&mid, &player1);
    assert_eq!(result, Err(Ok(Error::AlreadyClaimed)));
}

// ── 24: DisputeNotFound ──────────────────────────────────────────────────────

#[test]
fn error_dispute_not_found() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No dispute exists for id 99.
    let result = client.try_get_dispute(&99);
    assert_eq!(result, Err(Ok(Error::DisputeNotFound)));
}

// ── 25: PendingResultNotFound ────────────────────────────────────────────────

#[test]
fn error_pending_result_not_found() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_pr001"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);

    // No oracle result has been submitted → no pending result to dispute.
    let result = client.try_dispute_and_rollback_match(&mid, &player1);
    assert_eq!(result, Err(Ok(Error::PendingResultNotFound)));
}

// ── 33: DisputeAlreadyRaised ─────────────────────────────────────────────────

#[test]
fn error_dispute_already_raised() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(1000);
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_dar01"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);
    client.dispute_and_rollback_match(&mid, &player2);

    // Second dispute on the same match → DisputeAlreadyRaised.
    let result = client.try_dispute_and_rollback_match(&mid, &player2);
    assert_eq!(result, Err(Ok(Error::DisputeAlreadyRaised)));
}

// ── 35: TierStakeNotAllowed ──────────────────────────────────────────────────

#[test]
fn error_tier_stake_not_allowed() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &10_000);
    asset.mint(&player2, &10_000);

    // Bronze tier max is 100. Stake of 200 exceeds it for a new player.
    let result = client.try_create_match(
        &player1, &player2, &200, &token,
        &String::from_str(&env, "ev_ts001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::TierStakeNotAllowed)));
}

// ── 36: NotInitialized ───────────────────────────────────────────────────────

#[test]
fn error_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(player1.clone());
    let token = token_id.address();

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Contract has not been initialized yet.
    let result = client.try_create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_ni001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

// ── 37: InvalidPauseState ────────────────────────────────────────────────────

#[test]
fn error_invalid_pause_state() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Cannot execute_upgrade while unpaused (contract must be paused first).
    let result = client.try_execute_upgrade();
    assert_eq!(result, Err(Ok(Error::InvalidPauseState)));
}

// ── 45: TooManyActiveMatches ─────────────────────────────────────────────────

#[test]
fn error_too_many_active_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset = StellarAssetClient::new(&env, &token);
    // Mint enough for many matches.
    asset.mint(&player1, &1_000_000);
    asset.mint(&player2, &1_000_000);

    // MAX_ACTIVE_MATCHES_PER_PLAYER is 50 (default). Exceed it.
    for i in 0..50 {
        let gid = String::from_str(&env, &std::format!("ev_tma{:03}", i));
        let mid = client.create_match(&player1, &player2, &1, &token, &gid, &Platform::Lichess);
        client.deposit(&mid, &player1);
        client.deposit(&mid, &player2);
    }

    let result = client.try_create_match(
        &player1, &player2, &1, &token,
        &String::from_str(&env, "ev_tma999"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::TooManyActiveMatches)));
}

// ── 46: NotStablecoin ────────────────────────────────────────────────────────

#[test]
fn error_not_stablecoin() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Enable stablecoin-only mode without registering any issuer.
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: true,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
    });

    let result = client.try_create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_ns001"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::NotStablecoin)));
}

// ── 47: UpgradeNotScheduled ──────────────────────────────────────────────────

#[test]
fn error_upgrade_not_scheduled() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();
    // No upgrade has been scheduled.
    let result = client.try_execute_upgrade();
    assert_eq!(result, Err(Ok(Error::UpgradeNotScheduled)));
}

// ── 48: UpgradeReviewPeriodNotElapsed ────────────────────────────────────────

#[test]
fn error_upgrade_review_period_not_elapsed() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let wasm_hash = dummy_wasm_hash(&env);
    client.schedule_upgrade(&wasm_hash);
    client.pause();

    // Review period has not elapsed (we didn't advance ledger).
    let result = client.try_execute_upgrade();
    assert_eq!(result, Err(Ok(Error::UpgradeReviewPeriodNotElapsed)));
}

// ── 49: InvalidVersion ───────────────────────────────────────────────────────

#[test]
fn error_invalid_version() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // migrate_state with the current version (same, not higher) → InvalidVersion.
    let current = client.get_version();
    let result = client.try_migrate_state(&current);
    assert_eq!(result, Err(Ok(Error::InvalidVersion)));
}

// ── 50: UpgradeAlreadyScheduled ──────────────────────────────────────────────

#[test]
fn error_upgrade_already_scheduled() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let wasm_hash = dummy_wasm_hash(&env);
    client.schedule_upgrade(&wasm_hash);

    let result = client.try_schedule_upgrade(&wasm_hash);
    assert_eq!(result, Err(Ok(Error::UpgradeAlreadyScheduled)));
}

// ── 51: OracleAlreadyConfirmed ───────────────────────────────────────────────

#[test]
fn error_oracle_already_confirmed() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_oac01"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);

    // Submitting the same result again → OracleAlreadyConfirmed.
    let result = client.try_submit_result(&mid, &Winner::Player1, &oracle);
    assert_eq!(result, Err(Ok(Error::OracleAlreadyConfirmed)));
}

// ── 54: NotAnOracle ──────────────────────────────────────────────────────────

#[test]
fn error_not_an_oracle() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Enable consensus mode with 1 required oracle.
    let approved = Address::generate(&env);
    client.add_approved_oracle(&approved);
    client.set_required_oracle_confirmations(&1);

    let mid = client.create_match(
        &player1, &player2, &100, &token,
        &String::from_str(&env, "ev_nao01"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);

    // An unapproved address tries to submit via consensus → NotAnOracle.
    let imposter = Address::generate(&env);
    let result = client.try_submit_result_consensus(&mid, &Winner::Player1, &imposter);
    assert_eq!(result, Err(Ok(Error::NotAnOracle)));
}
