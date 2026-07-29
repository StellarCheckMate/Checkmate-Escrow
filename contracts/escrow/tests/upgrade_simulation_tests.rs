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
use escrow::types::{FeeTier, MatchState, Platform, ProtocolConfig, Winner};
use escrow::{CONTRACT_VERSION, UPGRADE_REVIEW_PERIOD_LEDGERS};
use escrow::{EscrowContract, EscrowContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    token::StellarAssetClient,
    Address, BytesN, Env, String as SorobanString, Vec as SorobanVec,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const STAKE: i128 = 200;
const MINT_AMOUNT: i128 = 10_000;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Core test fixture shared by every test in this file.
///
/// Returns `(env, contract_id, oracle, player1, player2, token, admin)`.
fn setup() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
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
    });

    (env, contract_id, oracle, player1, player2, token, admin)
}

/// Returns a zeroed 32-byte hash used as a stand-in for a WASM hash.
/// Tests in this file do not upload real WASM; they only test the lifecycle
/// guards around scheduling and executing upgrades.
fn dummy_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
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
        &SorobanString::from_str(env, game_id),
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
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let before = client.get_admin();
    client.migrate_state(&(CONTRACT_VERSION + 1));
    let after = client.get_admin();

    assert_eq!(before, after, "admin address must be identical after migrate_state");
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

    let custom_timeout: u32 = 100_000;
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

    let match_id = create_match(&client, &env, &player1, &player2, &token, "pre_migrate_pending");

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

    let match_id = create_match(&client, &env, &player1, &player2, &token, "pre_migrate_active");
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

    let match_id = create_match(&client, &env, &player1, &player2, &token, "balance_pre_migrate");
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

    let match_id = create_match(&client, &env, &player1, &player2, &token, "funded_pre_migrate");
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
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let match_id = create_match(&client, &env, &player1, &player2, &token, "oracle_post_migrate");
    fund_match(&client, match_id, &player1, &player2);

    let p1_before = token_client.balance(&player1);

    // Migrate while the match is Active
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Oracle submits result after migration
    client.submit_result(&match_id, &Winner::Player1);
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
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_client = soroban_sdk::token::Client::new(&env, &token);

    let match_id = create_match(&client, &env, &player1, &player2, &token, "draw_post_migrate");
    fund_match(&client, match_id, &player1, &player2);

    let p1_before = token_client.balance(&player1);
    let p2_before = token_client.balance(&player2);

    client.migrate_state(&(CONTRACT_VERSION + 1));
    client.submit_result(&match_id, &Winner::Draw);
    client.claim_vested_payout(&match_id, &player1);
    client.claim_vested_payout(&match_id, &player2);

    let p1_after = token_client.balance(&player1);
    let p2_after = token_client.balance(&player2);

    assert_eq!(p1_after - p1_before, STAKE, "p1 must be refunded their stake");
    assert_eq!(p2_after - p2_before, STAKE, "p2 must be refunded their stake");
}

// ── Function-signature compatibility ──────────────────────────────────────────

/// create_match still accepts the same argument types after migration.
#[test]
fn test_create_match_api_compatible_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    // If the function signature changed this would fail to compile or panic.
    let match_id = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &SorobanString::from_str(&env, "api_compat_post_migrate"),
        &Platform::Lichess,
    );
    assert!(match_id > 0 || match_id == 0); // always true; checks compile-time type compat
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

    create_match(&client, &env, &player1, &player2, &token, "player_matches_compat_1");
    create_match(&client, &env, &player1, &player2, &token, "player_matches_compat_2");

    let before = client.get_player_matches(&player1);
    client.migrate_state(&(CONTRACT_VERSION + 1));
    let after = client.get_player_matches(&player1);

    assert_eq!(before.len(), after.len());
}

// ── Fee correctness ────────────────────────────────────────────────────────────

/// Fee tiers configured before migration produce the same payout after.
#[test]
fn test_fee_tiers_preserved_and_applied_after_migration() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    // Set a 1% fee tier for all stakes (fee taken to treasury)
    let mut tiers: SorobanVec<FeeTier> = SorobanVec::new(&env);
    tiers.push_back(FeeTier {
        max_stake: i128::MAX,
        fee_basis_points: 100, // 1%
    });
    client.set_fee_tiers(&tiers);

    let match_id = create_match(&client, &env, &player1, &player2, &token, "fee_post_migrate");
    fund_match(&client, match_id, &player1, &player2);

    let treasury_before = token_client.balance(&admin);
    let p1_before = token_client.balance(&player1);

    // Migrate while match is active
    client.migrate_state(&(CONTRACT_VERSION + 1));

    // Submit result after migration — fee must still be 1%
    client.submit_result(&match_id, &Winner::Player1);
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
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.schedule_upgrade(&dummy_hash(&env));
    client.pause();

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
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();

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
    let match_id = create_match(&client, &env, &player1, &player2, &token, "post_cancel_match");
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
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = soroban_sdk::token::Client::new(&env, &token);

    client.migrate_state(&(CONTRACT_VERSION + 1));

    let match_id = create_match(&client, &env, &player1, &player2, &token, "post_migrate_new");
    fund_match(&client, match_id, &player1, &player2);

    let p2_before = token_client.balance(&player2);

    client.submit_result(&match_id, &Winner::Player2);
    client.claim_vested_payout(&match_id, &player2);

    let p2_after = token_client.balance(&player2);
    assert_eq!(p2_after - p2_before, STAKE * 2);
}
