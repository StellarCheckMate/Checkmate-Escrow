//! Tests for issue #1617: Token balance assertions for multi-token matches on
//! cancel, expire, and rollback.
//!
//! Each test asserts:
//! - The exact token_a and token_b balances for each player before and after
//!   the terminal operation.
//! - The escrow contract balance for both tokens returns to its pre-match value
//!   (i.e. zero additional tokens held) after the operation completes.

use super::*;
use oracle::{OracleContract, OracleContractClient};
use soroban_sdk::testutils::Ledger as _;

// ── Fixture ───────────────────────────────────────────────────────────────────

/// Full multi-token fixture — two tokens, two players, a funded oracle pool.
///
/// Returns:
/// `(env, admin, oracle_client, escrow_client, player1, player2, token_a, token_b, oracle_id)`
///
/// Starting balances:
///   player1: 1 000 token_a, 0 token_b
///   player2: 1 000 token_a, 500 token_b
///   escrow:    0 token_a,   0 token_b
///   oracle: 10 000 token_a, 10 000 token_b
fn setup_multi_token() -> (
    Env,
    Address,
    OracleContractClient<'static>,
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let oracle_admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let oracle_id = env.register_contract(None, OracleContract);
    let oracle_client = OracleContractClient::new(&env, &oracle_id);
    oracle_client.initialize(&oracle_admin);

    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    escrow_client.initialize(&oracle_id, &admin);
    escrow_client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
        dispute_bond_tier_schedule: soroban_sdk::vec![&env],
    });

    let token_a_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_a = token_a_id.address();
    let asset_a = StellarAssetClient::new(&env, &token_a);

    let token_b_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_b = token_b_id.address();
    let asset_b = StellarAssetClient::new(&env, &token_b);

    // player1 deposits token_a; player2 deposits token_a as well (m.token),
    // but may receive token_b on payout.  Give player2 token_b so that the
    // preferred-payout path also has a balance to check.
    asset_a.mint(&player1, &1_000);
    asset_a.mint(&player2, &1_000);
    asset_b.mint(&player2, &500);

    // Oracle pool used by the swap path.
    asset_a.mint(&oracle_id, &10_000);
    asset_b.mint(&oracle_id, &10_000);

    (
        env,
        admin,
        oracle_client,
        escrow_client,
        player1,
        player2,
        token_a,
        token_b,
        oracle_id,
    )
}

// ── Convenience balance snapshot ─────────────────────────────────────────────

/// Balances of both tokens for player1, player2, and the escrow contract.
#[derive(Debug)]
struct MultiBalanceSnapshot {
    p1_a: i128,
    p2_a: i128,
    p1_b: i128,
    p2_b: i128,
    escrow_a: i128,
    escrow_b: i128,
}

impl MultiBalanceSnapshot {
    fn capture(
        env: &Env,
        token_a: &Address,
        token_b: &Address,
        player1: &Address,
        player2: &Address,
        escrow: &Address,
    ) -> Self {
        let tc_a = token_client(env, token_a);
        let tc_b = token_client(env, token_b);
        Self {
            p1_a: tc_a.balance(player1),
            p2_a: tc_a.balance(player2),
            p1_b: tc_b.balance(player1),
            p2_b: tc_b.balance(player2),
            escrow_a: tc_a.balance(escrow),
            escrow_b: tc_b.balance(escrow),
        }
    }
}

// ── cancel_match — only player1 deposited (Pending state) ────────────────────

/// Assert that cancelling a pending multi-token match (only player1 deposited)
/// returns player1's token_a stake exactly, leaves player2 untouched, and
/// resets the escrow balance for both tokens to zero.
#[test]
fn test_1617_cancel_returns_deposited_token_a_to_player1() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000; // 1 token_a = 5 token_b
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake: i128 = 100;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617aa01"),
        &Platform::Lichess,
    );

    // Only player1 deposits — match stays Pending.
    escrow_client.deposit(&match_id, &player1);

    // Snapshot after deposit, before cancel.
    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(before.p1_a, 900, "player1 should have 900 token_a after deposit");
    assert_eq!(before.p2_a, 1_000, "player2 untouched");
    assert_eq!(before.p1_b, 0, "player1 has no token_b");
    assert_eq!(before.p2_b, 500, "player2 token_b untouched");
    assert_eq!(before.escrow_a, stake, "escrow holds player1's stake");
    assert_eq!(before.escrow_b, 0, "escrow holds no token_b yet");

    escrow_client.cancel_match(&match_id, &player1);

    let after = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(after.p1_a, 1_000, "player1 stake refunded in full");
    assert_eq!(after.p2_a, 1_000, "player2 token_a unchanged");
    assert_eq!(after.p1_b, 0, "player1 token_b unchanged");
    assert_eq!(after.p2_b, 500, "player2 token_b unchanged");
    assert_eq!(after.escrow_a, 0, "escrow token_a back to pre-match value");
    assert_eq!(after.escrow_b, 0, "escrow token_b back to pre-match value");
}

/// When both players have deposited (Active state) cancellation is only allowed
/// via the rollback path, but here we verify that a cancel on a Pending match
/// with only player2's deposit is also fully refunded.
#[test]
fn test_1617_cancel_returns_deposited_token_a_to_player2() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake: i128 = 100;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617aa02"),
        &Platform::Lichess,
    );

    // Only player2 deposits.
    escrow_client.deposit(&match_id, &player2);

    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(before.p1_a, 1_000, "player1 untouched");
    assert_eq!(before.p2_a, 900, "player2 deposited 100 token_a");
    assert_eq!(before.escrow_a, stake, "escrow holds player2 stake in token_a");
    assert_eq!(before.escrow_b, 0, "escrow holds no token_b");

    escrow_client.cancel_match(&match_id, &player2);

    let after = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(after.p2_a, 1_000, "player2 token_a refunded in full");
    assert_eq!(after.p1_a, 1_000, "player1 token_a unchanged");
    assert_eq!(after.p1_b, 0, "player1 token_b unchanged");
    assert_eq!(after.p2_b, 500, "player2 token_b unchanged (not used for deposit)");
    assert_eq!(after.escrow_a, 0, "escrow token_a back to zero");
    assert_eq!(after.escrow_b, 0, "escrow token_b back to zero");
}

/// Cancel a Pending match with NO deposits — both balances should be unchanged
/// and the escrow should remain at zero for both tokens.
#[test]
fn test_1617_cancel_no_deposits_escrow_stays_zero() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617aa03"),
        &Platform::Lichess,
    );

    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );

    escrow_client.cancel_match(&match_id, &player1);

    let after = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );

    assert_eq!(before.p1_a, after.p1_a, "player1 token_a unchanged");
    assert_eq!(before.p2_a, after.p2_a, "player2 token_a unchanged");
    assert_eq!(before.p1_b, after.p1_b, "player1 token_b unchanged");
    assert_eq!(before.p2_b, after.p2_b, "player2 token_b unchanged");
    assert_eq!(after.escrow_a, 0, "escrow token_a stays zero");
    assert_eq!(after.escrow_b, 0, "escrow token_b stays zero");
}

// ── expire_match — pending multi-token match ──────────────────────────────────

/// expire_match must refund player1's token_a stake, leave player2's token_b
/// untouched, and reset the escrow balance for both tokens to zero.
#[test]
fn test_1617_expire_refunds_player1_token_a_and_clears_escrow() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake: i128 = 100;

    // Use minimum timeout so we advance as few ledgers as possible.
    escrow_client.set_match_timeout(&MIN_MATCH_TIMEOUT_SECONDS);
    env.ledger().set_sequence_number(100);

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617bb01"),
        &Platform::Lichess,
    );

    // Only player1 deposits.
    escrow_client.deposit(&match_id, &player1);

    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(before.p1_a, 900);
    assert_eq!(before.escrow_a, stake);
    assert_eq!(before.escrow_b, 0);

    // Advance ledger past the timeout (MIN_MATCH_TIMEOUT_SECONDS / 5 = 17_280 ledgers).
    let timeout_ledgers = (MIN_MATCH_TIMEOUT_SECONDS / 5) as u32;
    env.ledger()
        .set_sequence_number(100 + timeout_ledgers);

    // Extend TTLs so the data is accessible at the new sequence number.
    env.deployer().extend_ttl_for_contract_instance(
        escrow_client.address.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_code(
        escrow_client.address.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_contract_instance(token_a.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.deployer()
        .extend_ttl_for_code(token_a.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

    escrow_client.expire_match(&match_id);

    let after = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(after.p1_a, 1_000, "player1 stake refunded in full");
    assert_eq!(after.p2_a, 1_000, "player2 token_a unchanged");
    assert_eq!(after.p1_b, 0, "player1 token_b unchanged");
    assert_eq!(after.p2_b, 500, "player2 token_b unchanged");
    assert_eq!(after.escrow_a, 0, "escrow token_a back to pre-match value");
    assert_eq!(after.escrow_b, 0, "escrow token_b back to pre-match value");
}

/// expire_match with no deposits should not move any tokens and should leave
/// the escrow balance at zero for both tokens.
#[test]
fn test_1617_expire_no_deposits_escrow_stays_zero() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    escrow_client.set_match_timeout(&MIN_MATCH_TIMEOUT_SECONDS);
    env.ledger().set_sequence_number(100);

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617bb02"),
        &Platform::Lichess,
    );

    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );

    let timeout_ledgers = (MIN_MATCH_TIMEOUT_SECONDS / 5) as u32;
    env.ledger()
        .set_sequence_number(100 + timeout_ledgers);

    env.deployer().extend_ttl_for_contract_instance(
        escrow_client.address.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_code(
        escrow_client.address.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_contract_instance(token_a.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.deployer()
        .extend_ttl_for_code(token_a.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

    escrow_client.expire_match(&match_id);

    let after = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(before.p1_a, after.p1_a, "player1 token_a unchanged");
    assert_eq!(before.p2_a, after.p2_a, "player2 token_a unchanged");
    assert_eq!(before.p1_b, after.p1_b, "player1 token_b unchanged");
    assert_eq!(before.p2_b, after.p2_b, "player2 token_b unchanged");
    assert_eq!(after.escrow_a, 0, "escrow token_a stays zero");
    assert_eq!(after.escrow_b, 0, "escrow token_b stays zero");
}

// ── dispute_and_rollback_match — active multi-token match ─────────────────────

/// Mutual rollback of an active multi-token match must refund each player
/// their token_a stake (the deposit token) and leave escrow at zero for both
/// tokens.
#[test]
fn test_1617_rollback_refunds_both_players_token_a_and_clears_escrow() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake: i128 = 100;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617cc01"),
        &Platform::Lichess,
    );

    // Both players deposit — match becomes Active.
    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(before.p1_a, 900, "player1 deposited 100 token_a");
    assert_eq!(before.p2_a, 900, "player2 deposited 100 token_a");
    assert_eq!(before.p1_b, 0, "player1 has no token_b");
    assert_eq!(before.p2_b, 500, "player2 token_b untouched");
    assert_eq!(before.escrow_a, stake * 2, "escrow holds both stakes in token_a");
    assert_eq!(before.escrow_b, 0, "escrow holds no token_b");

    // Mutual rollback — both players must consent.
    let reason = String::from_str(&env, "1617rollback");
    escrow_client.dispute_and_rollback_match(&match_id, &player1, &reason);
    escrow_client.dispute_and_rollback_match(&match_id, &player2, &reason);

    let after = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );
    assert_eq!(after.p1_a, 1_000, "player1 token_a stake refunded in full");
    assert_eq!(after.p2_a, 1_000, "player2 token_a stake refunded in full");
    assert_eq!(after.p1_b, 0, "player1 token_b unchanged (none held)");
    assert_eq!(after.p2_b, 500, "player2 token_b unchanged (refund is in token_a)");
    assert_eq!(after.escrow_a, 0, "escrow token_a back to pre-match value");
    assert_eq!(after.escrow_b, 0, "escrow token_b back to pre-match value");
}

/// First rollback vote must NOT move any tokens — the match stays Active and
/// both escrow balances remain unchanged until the second player consents.
#[test]
fn test_1617_rollback_first_vote_does_not_move_tokens() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake: i128 = 100;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617cc02"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    let before = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );

    // Only player1 votes — no refund yet.
    let reason = String::from_str(&env, "1617partial");
    escrow_client.dispute_and_rollback_match(&match_id, &player1, &reason);

    let mid = MultiBalanceSnapshot::capture(
        &env,
        &token_a,
        &token_b,
        &player1,
        &player2,
        &escrow_client.address,
    );

    assert_eq!(
        before.p1_a, mid.p1_a,
        "player1 token_a unchanged after first vote"
    );
    assert_eq!(
        before.p2_a, mid.p2_a,
        "player2 token_a unchanged after first vote"
    );
    assert_eq!(
        before.p1_b, mid.p1_b,
        "player1 token_b unchanged after first vote"
    );
    assert_eq!(
        before.p2_b, mid.p2_b,
        "player2 token_b unchanged after first vote"
    );
    assert_eq!(
        before.escrow_a, mid.escrow_a,
        "escrow token_a unchanged after first vote"
    );
    assert_eq!(
        before.escrow_b, mid.escrow_b,
        "escrow token_b unchanged after first vote"
    );

    assert_eq!(
        escrow_client.get_match(&match_id).state,
        MatchState::Active,
        "match must remain Active after only one consent"
    );
}

/// Fund-conservation invariant: the sum of all token_a and token_b balances
/// across both players and the escrow contract must be identical before and
/// after a mutual rollback.
#[test]
fn test_1617_rollback_fund_conservation_both_tokens() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, oracle_id) =
        setup_multi_token();

    let oracle_rate: i128 = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake: i128 = 100;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &oracle_rate,
        &String::from_str(&env, "1617cc03"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Capture total token_a and token_b in circulation (players + escrow + oracle pool).
    let tc_a = token_client(&env, &token_a);
    let tc_b = token_client(&env, &token_b);
    let total_a_before =
        tc_a.balance(&player1) + tc_a.balance(&player2) + tc_a.balance(&escrow_client.address) + tc_a.balance(&oracle_id);
    let total_b_before =
        tc_b.balance(&player1) + tc_b.balance(&player2) + tc_b.balance(&escrow_client.address) + tc_b.balance(&oracle_id);

    let reason = String::from_str(&env, "1617conservation");
    escrow_client.dispute_and_rollback_match(&match_id, &player1, &reason);
    escrow_client.dispute_and_rollback_match(&match_id, &player2, &reason);

    let total_a_after =
        tc_a.balance(&player1) + tc_a.balance(&player2) + tc_a.balance(&escrow_client.address) + tc_a.balance(&oracle_id);
    let total_b_after =
        tc_b.balance(&player1) + tc_b.balance(&player2) + tc_b.balance(&escrow_client.address) + tc_b.balance(&oracle_id);

    assert_eq!(
        total_a_before, total_a_after,
        "token_a must be conserved across rollback"
    );
    assert_eq!(
        total_b_before, total_b_after,
        "token_b must be conserved across rollback"
    );

    // Terminal escrow state: both tokens at zero.
    assert_eq!(
        tc_a.balance(&escrow_client.address),
        0,
        "escrow token_a must be zero after rollback"
    );
    assert_eq!(
        tc_b.balance(&escrow_client.address),
        0,
        "escrow token_b must be zero after rollback"
    );
}
