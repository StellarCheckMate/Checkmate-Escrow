//! Upgrade simulation tests — validate the full contract upgrade path.
//!
//! These tests exercise every dimension of contract upgrade safety:
//!
//! - **State preservation**: matches, config, and oracle are intact after
//!   `migrate_state`.
//! - **Old match accessibility**: matches created before the migration remain
//!   readable and actionable after it.
//! - **Function-signature compatibility**: every public API function still
//!   accepts the same argument types and returns the correct types after
//!   migration.
//! - **Fee correctness**: fee tiers set before migration produce the same
//!   payout amounts after migration.
//! - **Storage-key stability**: the keys `Admin`, `Oracle`, `Paused`,
//!   `MatchTimeout`, `ContractVersion`, and `Match(id)` are accessible
//!   before and after migration.
//! - **Upgrade guard enforcement**: `execute_upgrade` requires the contract
//!   to be paused and the review period to have elapsed.
//! - **Rollback safety**: `cancel_upgrade` after `schedule_upgrade` leaves
//!   the contract in a clean state, allowing a reschedule.
//! - **Version monotonicity**: `migrate_state` refuses to downgrade.
//! - **Concurrent-match safety**: a match that is `Active` during migration
//!   can still be finalized by the oracle after migration.
//!
//! Run with:
//!   cargo test -p escrow --test upgrade_simulation_tests -- --nocapture

use escrow::errors::Error;
use escrow::types::{MatchState, Platform, ProtocolConfig, Winner};
use escrow::{EscrowContract, EscrowContractClient};
use escrow::{CONTRACT_VERSION, DEFAULT_MINIMUM_STAKE, UPGRADE_REVIEW_PERIOD_LEDGERS};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, BytesN, Env, String as SorobanString,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const STAKE: i128 = 50;
const MINT_AMOUNT: i128 = 10_000;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Core test fixture shared by every test in this file.
///
/// Returns `(env, contract_id, oracle, player1, player2, token, admin)`.
fn setup() -> (Env, Address, Address, Address, Address, Address, Address) {
    let mut env = Env::default();
    env.set_config(soroban_sdk::testutils::EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_id.address();
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &MINT_AMOUNT);
    asset.mint(&player2, &MINT_AMOUNT);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        maximum_stake: None,
        match_timeout_seconds: escrow::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
    });

    (env, contract_id, oracle, player1, player2, token, admin)
}

/// Returns a zeroed 32-byte hash used as a stand-in for a WASM hash.
/// Tests in this file do not upload real WASM; they only test the lifecycle
/// guards around scheduling and executing upgrades.
fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

/// Derives a Lichess-compliant (exactly 8 ASCII alphanumeric chars) game ID
/// from an arbitrary descriptive label, so call sites can use readable names
/// like "pre_migrate_active" instead of hand-picked hex strings.
fn lichess_game_id(label: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    label.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

/// Creates a match between player1 and player2 and returns its ID.
fn create_match<'a>(
    client: &EscrowContractClient<'a>,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> u64 {
    client.create_match(
        player1,
        player2,
        &STAKE,
        token,
        &SorobanString::from_str(env, &lichess_game_id(game_id)),
        &Platform::Lichess,
    )
}

/// Deposits from both players, bringing the match to `Active`.
fn fund_match(client: &EscrowContractClient, match_id: u64, p1: &Address, p2: &Address) {
    client.deposit(&match_id, p1);
    client.deposit(&match_id, p2);
}

// ── State preservation ─────────────────────────────────────────────────────────

/// Admin address stored before migration must equal the value returned after.
#[test]
fn test_admin_preserved_across_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let before = client.get_admin();
    client.migrate_state(&(CONTRACT_VERSION + 1));
    let after = client.get_admin();

    assert_eq!(
        before, after,
        "admin address must be identical after migrate_state"
    );
}

/// Oracle address stored before migration must equal the value returned after.
#[test]
fn test_oracle_preserved_across_migration() {
    let (env, contract_id, oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let before = client.get_oracle();
    client.migrate_state(&(CONTRACT_VERSION + 1));
    let after = client.get_oracle();

    assert_eq!(before, after);
    assert_eq!(after, oracle);
}

/// The `paused` flag is unaffected by `migrate_state`.
#[test]
fn test_pause_state_preserved_across_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Contract is not paused initially.
    assert!(!client.is_paused());

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Still not paused after migration.
    assert!(!client.is_paused());
}

/// Match timeout set before migration must be unchanged after.
#[test]
fn test_match_timeout_preserved_across_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let custom_timeout: u64 = 100_000;
    client.set_match_timeout(&custom_timeout);

    let before = client.get_match_timeout();
    client.migrate_state(&(CONTRACT_VERSION + 1));
    let after = client.get_match_timeout();

    assert_eq!(before, after);
    assert_eq!(after, custom_timeout);
}

/// Protocol config fields are intact after migration.
#[test]
fn test_protocol_config_preserved_across_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let config = ProtocolConfig {
        vesting_duration_seconds: 300,
        cancellation_fee_basis_points: 50,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        maximum_stake: None,
        match_timeout_seconds: escrow::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
    };
    client.set_protocol_config(&config);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let restored = client.get_protocol_config();
    assert_eq!(restored.vesting_duration_seconds, 300);
    assert_eq!(restored.cancellation_fee_basis_points, 50);
    assert_eq!(restored.treasury, admin);
}

// ── Old match accessibility ────────────────────────────────────────────────────

/// A `Pending` match created before migration remains readable after.
#[test]
fn test_pending_match_readable_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "pre_migrate_pending",
    );

    // Migrate
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Match must still be readable
    let m = client.get_match(&match_id);
    assert_eq!(m.id, match_id);
    assert_eq!(m.state, MatchState::Pending);
    assert_eq!(m.stake_amount, STAKE);
}

/// An `Active` match created before migration remains readable after.
#[test]
fn test_active_match_readable_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "pre_migrate_active",
    );
    fund_match(&client, match_id, &player1, &player2);

    // Migrate
    client.migrate_state(&(CONTRACT_VERSION + 1));

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Active);
    assert!(m.player1_deposited);
    assert!(m.player2_deposited);
}

/// Escrow balance for a match created before migration is unchanged after.
#[test]
fn test_escrow_balance_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "balance_pre_migrate",
    );
    fund_match(&client, match_id, &player1, &player2);

    let balance_before = client.get_escrow_balance(&match_id);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let balance_after = client.get_escrow_balance(&match_id);
    assert_eq!(balance_before, balance_after);
    assert_eq!(balance_after, STAKE * 2);
}

/// `is_funded` flag for a match funded before migration stays `true` after.
#[test]
fn test_is_funded_flag_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "funded_pre_migrate",
    );
    fund_match(&client, match_id, &player1, &player2);

    assert!(client.is_funded(&match_id));
    client.migrate_state(&(CONTRACT_VERSION + 1));
    assert!(client.is_funded(&match_id));
}

// ── Oracle result submission after migration ───────────────────────────────────

/// An active match created before migration can be finalized by the oracle
/// after migration, and the winner receives the full payout.
#[test]
fn test_oracle_can_submit_result_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "oracle_post_migrate",
    );
    fund_match(&client, match_id, &player1, &player2);

    let p1_before = token_client.balance(&player1);

    // Migrate while the match is Active
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Oracle submits result after migration
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    let p1_after = token_client.balance(&player1);
    // Player1 receives both stakes (STAKE * 2)
    assert_eq!(p1_after - p1_before, STAKE * 2);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(m.winner, Winner::Player1);
}

/// Draw result after migration correctly refunds both players.
#[test]
fn test_draw_payout_correct_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "draw_post_migrate",
    );
    fund_match(&client, match_id, &player1, &player2);

    let p1_before = token_client.balance(&player1);
    let p2_before = token_client.balance(&player2);

    client.migrate_state(&(CONTRACT_VERSION + 1));
    client.submit_result(&match_id, &Winner::Draw, &oracle);
    client.claim_vested_payout(&match_id, &player1);
    client.claim_vested_payout(&match_id, &player2);

    let p1_after = token_client.balance(&player1);
    let p2_after = token_client.balance(&player2);

    assert_eq!(
        p1_after - p1_before,
        STAKE,
        "p1 must be refunded their stake"
    );
    assert_eq!(
        p2_after - p2_before,
        STAKE,
        "p2 must be refunded their stake"
    );
}

// ── Function-signature compatibility ──────────────────────────────────────────

/// create_match still accepts the same argument types after migration.
#[test]
fn test_create_match_api_compatible_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // If the function signature changed this would fail to compile or panic.
    let _match_id: u64 = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &SorobanString::from_str(&env, "c57e2890"),
        &Platform::Lichess,
    );
}

/// deposit still works on a match created after migration.
#[test]
fn test_deposit_api_compatible_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let match_id = create_match(&client, &env, &player1, &player2, &token, "deposit_compat");
    // Should not panic
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    assert!(client.is_funded(&match_id));
}

/// cancel_match still works after migration.
#[test]
fn test_cancel_match_api_compatible_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(&client, &env, &player1, &player2, &token, "cancel_compat");

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // cancel_match while still Pending — must succeed
    client.cancel_match(&match_id, &player1);
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Cancelled);
}

/// get_player_matches returns consistent data after migration.
#[test]
fn test_get_player_matches_api_compatible_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "player_matches_compat_1",
    );
    create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "player_matches_compat_2",
    );

    let before = client.get_player_matches(&player1);
    client.migrate_state(&(CONTRACT_VERSION + 1));
    let after = client.get_player_matches(&player1);

    assert_eq!(before.len(), after.len());
}

// ── Fee correctness ────────────────────────────────────────────────────────────

/// A protocol fee configured before migration produces the same payout
/// deduction after. Note: `set_fee_tiers`/`calculate_fee_by_tier` is a
/// separate, standalone calculator — `claim_vested_payout` only ever reads
/// `ProtocolConfig::protocol_fee_bps` (see `compute_protocol_fee`), so that's
/// the mechanism this test needs to configure to actually affect payout.
#[test]
fn test_fee_tiers_preserved_and_applied_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    // Set a flat 1% protocol fee (taken to treasury on payout).
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        maximum_stake: None,
        match_timeout_seconds: escrow::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 100, // 1%
        fee_recipient: admin.clone(),
    });

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "fee_post_migrate",
    );
    fund_match(&client, match_id, &player1, &player2);

    let treasury_before = token_client.balance(&admin);
    let p1_before = token_client.balance(&player1);

    // Migrate while match is active
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Submit result after migration — fee must still be 1%
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    let treasury_after = token_client.balance(&admin);
    let p1_after = token_client.balance(&player1);

    let fee = (STAKE * 2 * 100) / 10_000; // 1% of pot
    let expected_payout = STAKE * 2 - fee;

    assert_eq!(
        treasury_after - treasury_before,
        fee,
        "treasury must receive the 1% fee after post-migration payout"
    );
    assert_eq!(
        p1_after - p1_before,
        expected_payout,
        "winner receives pot minus fee after post-migration payout"
    );
}

// ── Storage-key stability ──────────────────────────────────────────────────────

/// validate_state passes before and after migration — confirms all required
/// storage keys are present and well-formed.
#[test]
fn test_validate_state_passes_before_and_after_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Must not panic before migration
    client.validate_state();

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Must not panic after migration either
    client.validate_state();
}

/// Contract version stored in instance storage increments correctly.
#[test]
fn test_version_storage_key_increments_correctly() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_version(), CONTRACT_VERSION);
    client.migrate_state(&(CONTRACT_VERSION + 1));
    assert_eq!(client.get_version(), CONTRACT_VERSION + 1);
    client.migrate_state(&(CONTRACT_VERSION + 2));
    assert_eq!(client.get_version(), CONTRACT_VERSION + 2);
}

// ── Upgrade guard enforcement ──────────────────────────────────────────────────

/// execute_upgrade is rejected when the contract is not paused.
#[test]
fn test_execute_upgrade_rejected_when_not_paused() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.schedule_upgrade(&dummy_hash(&env));

    env.ledger().with_mut(|l| {
        l.sequence_number += UPGRADE_REVIEW_PERIOD_LEDGERS + 1;
    });

    let result = client.try_execute_upgrade();
    assert!(
        matches!(result, Err(Ok(Error::InvalidPauseState))),
        "execute_upgrade must be rejected when contract is not paused"
    );
}

/// execute_upgrade is rejected when the review period has not elapsed.
#[test]
fn test_execute_upgrade_rejected_before_review_period() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.schedule_upgrade(&dummy_hash(&env));
    client.pause(&admin);

    // Do NOT advance ledger — review period has not elapsed.
    let result = client.try_execute_upgrade();
    assert!(
        matches!(result, Err(Ok(Error::UpgradeReviewPeriodNotElapsed))),
        "execute_upgrade must be rejected before review period elapses"
    );
}

/// execute_upgrade is rejected when no upgrade is scheduled.
#[test]
fn test_execute_upgrade_rejected_with_no_scheduled_upgrade() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

    let result = client.try_execute_upgrade();
    assert!(
        matches!(result, Err(Ok(Error::UpgradeNotScheduled))),
        "execute_upgrade must return UpgradeNotScheduled when none is pending"
    );
}

// ── Rollback safety ────────────────────────────────────────────────────────────

/// cancel_upgrade after schedule_upgrade leaves the contract in a clean state
/// and allows a fresh schedule_upgrade to succeed.
#[test]
fn test_cancel_upgrade_allows_reschedule() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let hash = dummy_hash(&env);
    client.schedule_upgrade(&hash);
    client.cancel_upgrade();

    // Rescheduling must succeed without UpgradeAlreadyScheduled
    client.schedule_upgrade(&hash);

    // Version is unchanged — no migration ran
    assert_eq!(client.get_version(), CONTRACT_VERSION);
}

/// After cancel_upgrade, the contract is fully operational (matches can be
/// created and funded).
#[test]
fn test_contract_operational_after_cancelled_upgrade() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.schedule_upgrade(&dummy_hash(&env));
    client.cancel_upgrade();

    // Normal operations must still work
    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "post_cancel_match",
    );
    fund_match(&client, match_id, &player1, &player2);

    assert!(client.is_funded(&match_id));
}

// ── Version monotonicity ───────────────────────────────────────────────────────

/// migrate_state rejects a target version equal to the current version.
#[test]
fn test_migrate_state_rejects_same_version() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_migrate_state(&CONTRACT_VERSION);
    assert!(
        matches!(result, Err(Ok(Error::InvalidVersion))),
        "migrate_state to the current version must return InvalidVersion"
    );
}

/// migrate_state rejects a target version lower than the current version.
#[test]
fn test_migrate_state_rejects_downgrade() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Advance to v+2 first
    client.migrate_state(&(CONTRACT_VERSION + 2));

    // Attempt to go back to v+1 — must fail
    let result = client.try_migrate_state(&(CONTRACT_VERSION + 1));
    assert!(
        matches!(result, Err(Ok(Error::InvalidVersion))),
        "migrate_state downgrade must return InvalidVersion"
    );
}

/// migrate_state is idempotent in the sense that running it at the same target
/// is always rejected (prevents accidental double-migrations).
#[test]
fn test_migrate_state_double_migration_rejected() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Running the same migration again must be rejected
    let result = client.try_migrate_state(&(CONTRACT_VERSION + 1));
    assert!(
        matches!(result, Err(Ok(Error::InvalidVersion))),
        "re-running the same migration must return InvalidVersion"
    );
}

// ── Concurrent-match safety ───────────────────────────────────────────────────

/// Multiple matches across different states (Pending, Active) all survive
/// migration with their states intact.
#[test]
fn test_multiple_matches_survive_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();

    // Need extra players for additional matches
    let asset = StellarAssetClient::new(&env, &token);
    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);
    asset.mint(&player3, &MINT_AMOUNT);
    asset.mint(&player4, &MINT_AMOUNT);

    let client = EscrowContractClient::new(&env, &contract_id);

    // Match 1: Pending (no deposits)
    let pending_id = create_match(&client, &env, &player1, &player2, &token, "multi_pending");

    // Match 2: Active (both deposited)
    let active_id = create_match(&client, &env, &player3, &player4, &token, "multi_active");
    fund_match(&client, active_id, &player3, &player4);

    // Migrate
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Verify both matches intact
    let pending = client.get_match(&pending_id);
    assert_eq!(pending.state, MatchState::Pending);

    let active = client.get_match(&active_id);
    assert_eq!(active.state, MatchState::Active);
    assert_eq!(client.get_escrow_balance(&active_id), STAKE * 2);
}

/// A match created after migration can be completed normally, confirming the
/// contract remains fully functional post-migration.
#[test]
fn test_new_match_completes_normally_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "post_migrate_new",
    );
    fund_match(&client, match_id, &player1, &player2);

    let p2_before = token_client.balance(&player2);

    client.submit_result(&match_id, &Winner::Player2, &oracle);
    client.claim_vested_payout(&match_id, &player2);

    let p2_after = token_client.balance(&player2);
    assert_eq!(p2_after - p2_before, STAKE * 2);
}

// ── DataKey variant coverage across all match states ──────────────────────────
//
// The tests below enumerate every DataKey variant used in v0.1.0 and assert
// that its value is identical before and after `migrate_state`. They exercise
// matches in every reachable state: Pending, Active, Completed, Cancelled.
// Any regression in key naming or serialisation will cause these assertions
// to fail during the upgrade simulation.

/// DataKey::Match(id) — Cancelled state: match created, player1 deposits, then
/// player1 cancels. Cancellation state must survive migration.
#[test]
fn test_cancelled_match_readable_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "cancelled_key_test",
    );
    client.deposit(&match_id, &player1);
    client.cancel_match(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Cancelled);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::Cancelled,
        "Cancelled match must survive migration"
    );
    assert_eq!(m.id, match_id);
    assert_eq!(m.stake_amount, STAKE);
}

/// DataKey::Match(id) — Completed state: full match completed before migration.
#[test]
fn test_completed_match_readable_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "completed_key_test",
    );
    fund_match(&client, match_id, &player1, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::Completed,
        "Completed match must survive migration"
    );
    assert_eq!(m.winner, Winner::Player1);
}

/// All four match states (Pending, Active, Completed, Cancelled) co-exist and
/// survive a single migration run with their states intact.
#[test]
fn test_all_match_states_survive_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();

    let asset = StellarAssetClient::new(&env, &token);
    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);
    let player5 = Address::generate(&env);
    let player6 = Address::generate(&env);
    asset.mint(&player3, &MINT_AMOUNT);
    asset.mint(&player4, &MINT_AMOUNT);
    asset.mint(&player5, &MINT_AMOUNT);
    asset.mint(&player6, &MINT_AMOUNT);

    let client = EscrowContractClient::new(&env, &contract_id);

    // Pending: no deposits
    let pending_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "allstates_pending",
    );

    // Active: both deposited
    let active_id = create_match(
        &client,
        &env,
        &player3,
        &player4,
        &token,
        "allstates_active",
    );
    fund_match(&client, active_id, &player3, &player4);

    // Completed: oracle submitted result
    let completed_id = create_match(
        &client,
        &env,
        &player5,
        &player6,
        &token,
        "allstates_completed",
    );
    fund_match(&client, completed_id, &player5, &player6);
    client.submit_result(&completed_id, &Winner::Draw, &oracle);
    client.claim_vested_payout(&completed_id, &player5);
    client.claim_vested_payout(&completed_id, &player6);

    // Cancelled: player1 deposited then cancelled
    let cancelled_id = create_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "allstates_cancelled",
    );
    client.deposit(&cancelled_id, &player1);
    client.cancel_match(&cancelled_id, &player2);

    // Migrate
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Assert each state is intact
    assert_eq!(client.get_match(&pending_id).state, MatchState::Pending);
    assert_eq!(client.get_match(&active_id).state, MatchState::Active);
    assert_eq!(client.get_match(&completed_id).state, MatchState::Completed);
    assert_eq!(client.get_match(&cancelled_id).state, MatchState::Cancelled);

    // Escrow balances
    assert_eq!(client.get_escrow_balance(&pending_id), 0);
    assert_eq!(client.get_escrow_balance(&active_id), STAKE * 2);
    assert_eq!(client.get_escrow_balance(&completed_id), 0);
    assert_eq!(client.get_escrow_balance(&cancelled_id), 0);
}

/// DataKey::GameId — duplicate game_id rejection works after migration,
/// confirming the presence-key set before migration is still readable.
#[test]
fn test_game_id_key_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Use a fixed 8-char game_id so we can attempt to reuse it.
    let game_id = SorobanString::from_str(&env, "gameidaa");
    client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &game_id,
        &Platform::Lichess,
    );

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Attempting to create a new match with the same game_id must fail —
    // DataKey::GameId(game_id) is still present after migration.
    let result = client.try_create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &game_id,
        &Platform::Lichess,
    );
    assert!(
        result.is_err(),
        "duplicate game_id must still be rejected after migration"
    );
}

/// DataKey::PlayerMatches(Address) — player's match list is intact after migration.
#[test]
fn test_player_matches_key_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_match(&client, &env, &player1, &player2, &token, "pm_key_test_1");
    create_match(&client, &env, &player1, &player2, &token, "pm_key_test_2");
    create_match(&client, &env, &player1, &player2, &token, "pm_key_test_3");

    let count_before = client.get_player_matches(&player1).len();

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let count_after = client.get_player_matches(&player1).len();
    assert_eq!(
        count_before, count_after,
        "DataKey::PlayerMatches must be identical before and after migration"
    );
    assert_eq!(count_after, 3);
}

/// DataKey::OracleRecord(match_id) — oracle audit record survives migration.
#[test]
fn test_oracle_record_key_preserved_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_id_str = "oracrecaa";
    let match_id = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &SorobanString::from_str(&env, game_id_str),
        &Platform::Lichess,
    );
    fund_match(&client, match_id, &player1, &player2);
    client.submit_result_with_oracle_record(
        &match_id,
        &Winner::Player1,
        &SorobanString::from_str(&env, game_id_str),
    );
    client.claim_vested_payout(&match_id, &player1);

    let _ = oracle; // suppress unused warning

    // After migration, get_oracle_record must still return the stored game_id.
    client.migrate_state(&(CONTRACT_VERSION + 1));

    let record = client.get_oracle_record(&match_id);
    assert_eq!(
        record,
        SorobanString::from_str(&env, game_id_str),
        "DataKey::OracleRecord must survive migration"
    );
}

/// DataKey::AllowedToken / AllowlistEnforced — allowlist state survives migration.
#[test]
fn test_allowlist_keys_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Add token to allowlist — sets AllowedToken(token), AllowedTokenCount, AllowlistEnforced.
    client.add_allowed_token(&token);
    assert!(client.is_token_allowed(&token));

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Allowlist state must be intact.
    assert!(
        client.is_token_allowed(&token),
        "DataKey::AllowedToken must survive migration"
    );

    // Creating a match with the allowed token must succeed.
    let _id = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &SorobanString::from_str(&env, "allow001"),
        &Platform::Lichess,
    );

    // Creating a match with a random non-allowlisted token must fail.
    let other_token_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let other_token = other_token_id.address();
    let result = client.try_create_match(
        &player1,
        &player2,
        &STAKE,
        &other_token,
        &SorobanString::from_str(&env, "allow002"),
        &Platform::Lichess,
    );
    assert!(
        result.is_err(),
        "non-allowlisted token must still be rejected after migration"
    );
}

/// DataKey::Snapshot / SnapshotCount — balance snapshot ring buffer survives migration.
#[test]
fn test_snapshot_keys_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_match(&client, &env, &player1, &player2, &token, "snap_key_test");
    fund_match(&client, match_id, &player1, &player2);

    // At least the creation + two deposit snapshots have been recorded.
    let count_before = client.get_snapshot_count(&match_id);
    assert!(
        count_before >= 3,
        "at least 3 snapshots expected (created + 2 deposits)"
    );

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let count_after = client.get_snapshot_count(&match_id);
    assert_eq!(
        count_before, count_after,
        "DataKey::SnapshotCount must be unchanged after migration"
    );

    // The first snapshot (Created) must still be readable.
    let snap = client.get_snapshot(&match_id, &0);
    assert_eq!(
        snap.match_id, match_id,
        "DataKey::Snapshot(id,0) must survive migration"
    );
}

/// DataKey::Oracle / Admin / ContractVersion — instance keys readable after migration.
#[test]
fn test_instance_keys_readable_after_migration() {
    let (env, contract_id, oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_oracle(), oracle);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_version(), CONTRACT_VERSION);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    assert_eq!(
        client.get_oracle(),
        oracle,
        "DataKey::Oracle must survive migration"
    );
    assert_eq!(
        client.get_admin(),
        admin,
        "DataKey::Admin must survive migration"
    );
    assert_eq!(
        client.get_version(),
        CONTRACT_VERSION + 1,
        "DataKey::ContractVersion must be incremented by migration"
    );
}

/// DataKey::Paused — paused flag survives migration in both states.
#[test]
fn test_paused_key_preserved_across_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Pause before migration.
    client.pause(&admin);
    assert!(client.is_paused());

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Paused flag must be preserved.
    assert!(client.is_paused(), "DataKey::Paused must survive migration");

    // Unpause and migrate again.
    client.unpause(&admin);
    client.migrate_state(&(CONTRACT_VERSION + 2));

    assert!(
        !client.is_paused(),
        "DataKey::Paused=false must survive second migration"
    );
}

/// DataKey::MatchCount — counter value is intact after migration so that new
/// match IDs continue from where they left off (no ID collisions).
#[test]
fn test_match_count_key_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create a few matches to advance the counter.
    create_match(&client, &env, &player1, &player2, &token, "mc_key_1");
    create_match(&client, &env, &player1, &player2, &token, "mc_key_2");
    let last_id = create_match(&client, &env, &player1, &player2, &token, "mc_key_3");

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Next match after migration must have an ID > last_id (no reset or collision).
    let next_id = create_match(&client, &env, &player1, &player2, &token, "mc_key_4");
    assert!(
        next_id > last_id,
        "DataKey::MatchCount must preserve counter so new IDs never collide post-migration"
    );
}

/// DataKey::PendingUpgradeHash / UpgradeScheduledAt — scheduled upgrade state
/// is cleared after cancel_upgrade, and the cancel survives a migration.
#[test]
fn test_pending_upgrade_hash_cleared_by_cancel_then_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.schedule_upgrade(&dummy_hash(&env));
    client.cancel_upgrade();

    // After cancel, rescheduling must succeed — PendingUpgradeHash was cleared.
    client.schedule_upgrade(&dummy_hash(&env));
    client.cancel_upgrade();

    // Migration proceeds cleanly (version increments, no upgrade artifacts remain).
    client.migrate_state(&(CONTRACT_VERSION + 1));
    assert_eq!(client.get_version(), CONTRACT_VERSION + 1);

    // Scheduling again post-migration also works.
    client.schedule_upgrade(&dummy_hash(&env));
    client.cancel_upgrade();
    assert_eq!(client.get_version(), CONTRACT_VERSION + 1);
}

/// DataKey::ProtocolConfig (fee_recipient / treasury) — full config struct
/// survives migration with every field intact.
#[test]
fn test_protocol_config_all_fields_preserved_after_migration() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let config = ProtocolConfig {
        vesting_duration_seconds: 600,
        cancellation_fee_basis_points: 75,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        maximum_stake: Some(100_000),
        match_timeout_seconds: escrow::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 250, // 2.5%
        fee_recipient: admin.clone(),
    };
    client.set_protocol_config(&config);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let restored = client.get_protocol_config();
    assert_eq!(restored.vesting_duration_seconds, 600);
    assert_eq!(restored.cancellation_fee_basis_points, 75);
    assert_eq!(restored.maximum_stake, Some(100_000));
    assert_eq!(restored.protocol_fee_bps, 250);
    assert_eq!(restored.fee_recipient, admin);
}

/// DataKey::PlayerActiveMatchCount — active-match count per player is
/// preserved across migration.
#[test]
fn test_player_active_match_count_preserved_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create and activate two matches for player1.
    let id1 = create_match(&client, &env, &player1, &player2, &token, "pamc_test_1");
    fund_match(&client, id1, &player1, &player2);

    let id2 = create_match(&client, &env, &player1, &player2, &token, "pamc_test_2");
    fund_match(&client, id2, &player1, &player2);

    let count_before = client.get_active_match_count(&player1);
    assert_eq!(count_before, 2);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let count_after = client.get_active_match_count(&player1);
    assert_eq!(
        count_before, count_after,
        "DataKey::PlayerActiveMatchCount must survive migration"
    );
}

/// DataKey::Stats — platform statistics survive migration with correct values.
#[test]
fn test_stats_key_preserved_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Contribute to stats via a completed match.
    let match_id = create_match(&client, &env, &player1, &player2, &token, "stats_test");
    fund_match(&client, match_id, &player1, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    let stats_before = client.get_stats();

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let stats_after = client.get_stats();
    assert_eq!(
        stats_before.total_matches_created, stats_after.total_matches_created,
        "DataKey::Stats::total_matches_created must survive migration"
    );
    assert_eq!(
        stats_before.total_volume, stats_after.total_volume,
        "DataKey::Stats::total_volume must survive migration"
    );
    assert_eq!(
        stats_before.total_payouts, stats_after.total_payouts,
        "DataKey::Stats::total_payouts must survive migration"
    );
}

/// DataKey::PlayerCompletedMatchCount — completed-match count per player
/// is preserved across migration.
#[test]
fn test_player_completed_match_count_preserved_after_migration() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Complete a match to increment the counter.
    let match_id = create_match(&client, &env, &player1, &player2, &token, "pcmc_test");
    fund_match(&client, match_id, &player1, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    let count_before = client.get_completed_match_count(&player1);
    assert!(
        count_before >= 1,
        "player1 should have at least one completed match"
    );

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let count_after = client.get_completed_match_count(&player1);
    assert_eq!(
        count_before, count_after,
        "DataKey::PlayerCompletedMatchCount must survive migration"
    );
}
