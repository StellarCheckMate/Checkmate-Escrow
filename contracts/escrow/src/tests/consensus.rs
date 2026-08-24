//! Tests for multi-oracle consensus result verification (issue #956).
//!
//! Covers:
//! - Happy path: 2-of-2 and 2-of-3 consensus triggers payout
//! - Draw payout via consensus
//! - Partial votes do not trigger payout
//! - Conflicting oracle votes return `ConflictingResult`
//! - Duplicate vote by same oracle returns `OracleAlreadyConfirmed`
//! - Unapproved oracle returns `NotAnOracle`
//! - Paused contract blocks `submit_result_consensus`
//! - `get_oracle_confirmations` returns accurate count
//! - Admin management: add/remove approved oracles, set required confirmations
//! - `set_required_oracle_confirmations(0)` returns `InvalidAmount`
//! - Consensus on a non-active (pending, completed, cancelled) match returns `InvalidState`
//! - Fund conservation: tokens are preserved through consensus payout

use super::*;
use soroban_sdk::token::StellarAssetClient;

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Set up an environment with `n` approved oracles, each funded and registered.
/// Returns (env, contract_id, admin, player1, player2, token, oracle_list, match_id)
/// The match is already in Active state (both players deposited).
fn setup_consensus(
    n_oracles: usize,
    required: u32,
) -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    soroban_sdk::Vec<Address>,
    u64,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = token_id.address();
    let asset_client = StellarAssetClient::new(&env, &token_addr);
    asset_client.mint(&player1, &1000);
    asset_client.mint(&player2, &1000);

    // Use a throwaway primary oracle for initialization.
    let primary_oracle = Address::generate(&env);
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&primary_oracle, &admin);
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: crate::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: crate::DEFAULT_MINIMUM_STAKE,
    });

    // Register approved oracles.
    let mut oracle_list = soroban_sdk::vec![&env];
    for _ in 0..n_oracles {
        let o = Address::generate(&env);
        client.add_approved_oracle(&o);
        oracle_list.push_back(o);
    }

    // Set required confirmations.
    client.set_required_confirmations(&required);

    // Create and fund a match.
    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token_addr,
        &String::from_str(&env, "6f402c9e"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    (
        env,
        contract_id,
        admin,
        player1,
        player2,
        token_addr,
        oracle_list,
        match_id,
    )
}

// ── Admin management tests ────────────────────────────────────────────────────

#[test]
fn test_add_approved_oracle_admin_only() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let new_oracle = Address::generate(&env);
    // Should not panic; admin is mocked
    client.add_approved_oracle(&new_oracle);
    let oracles = client.get_approved_oracles();
    assert_eq!(oracles.len(), 1);
    assert_eq!(oracles.get(0).unwrap(), new_oracle);
}

#[test]
fn test_add_approved_oracle_idempotent() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let new_oracle = Address::generate(&env);
    client.add_approved_oracle(&new_oracle);
    client.add_approved_oracle(&new_oracle); // duplicate — should not double-add
    let oracles = client.get_approved_oracles();
    assert_eq!(oracles.len(), 1);
}

#[test]
fn test_remove_approved_oracle() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let o1 = Address::generate(&env);
    let o2 = Address::generate(&env);
    client.add_approved_oracle(&o1);
    client.add_approved_oracle(&o2);
    client.remove_approved_oracle(&o1);
    let oracles = client.get_approved_oracles();
    assert_eq!(oracles.len(), 1);
    assert_eq!(oracles.get(0).unwrap(), o2);
}

#[test]
fn test_get_approved_oracles_empty_by_default() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    assert_eq!(client.get_approved_oracles().len(), 0);
}

#[test]
fn test_set_required_oracle_confirmations_stores_value() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.set_required_confirmations(&3);
    assert_eq!(client.get_required_confirmations(), 3);
}

#[test]
fn test_set_required_oracle_confirmations_default_is_two() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    assert_eq!(client.get_required_confirmations(), 2);
}

#[test]
fn test_set_required_oracle_confirmations_zero_returns_invalid_amount() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let result = client.try_set_required_confirmations(&0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_add_approved_oracle_rejects_contract_itself() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let result = client.try_add_approved_oracle(&contract_id);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
}

// ── Consensus happy-path tests ────────────────────────────────────────────────

#[test]
fn test_consensus_two_of_two_pays_out_player1() {
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    // First vote — payout should NOT happen yet.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 1);
    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::Active,
        "payout must not happen after 1 of 2 votes"
    );

    // Second vote — payout should execute.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 2);
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);

    client.claim_vested_payout(&match_id, &player1);

    // Player1 wins the pot (200).
    assert_eq!(tc.balance(&player1), p1_before + 200);
    assert_eq!(tc.balance(&player2), p2_before);
}

#[test]
fn test_consensus_two_of_two_pays_out_player2() {
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    client.submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(1).unwrap());

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    client.claim_vested_payout(&match_id, &player2);
    assert_eq!(tc.balance(&player1), p1_before);
    assert_eq!(tc.balance(&player2), p2_before + 200);
}

#[test]
fn test_consensus_draw_refunds_both_players() {
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    client.submit_result_consensus(&match_id, &Winner::Draw, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Draw, &oracles.get(1).unwrap());

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    client.claim_vested_payout(&match_id, &player1);
    client.claim_vested_payout(&match_id, &player2);
    // Both get their stake back.
    assert_eq!(tc.balance(&player1), p1_before + 100);
    assert_eq!(tc.balance(&player2), p2_before + 100);
}

#[test]
fn test_consensus_two_of_three_reaches_threshold_at_second_vote() {
    let (env, contract_id, _admin, player1, _player2, token, oracles, match_id) =
        setup_consensus(3, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);

    // Vote 1.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);

    // Vote 2 — threshold reached.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
    client.claim_vested_payout(&match_id, &player1);
    assert_eq!(tc.balance(&player1), p1_before + 200);
}

#[test]
fn test_consensus_three_of_three_requires_all_votes() {
    let (env, contract_id, _admin, player1, _player2, token, oracles, match_id) =
        setup_consensus(3, 3);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(2).unwrap());
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
    client.claim_vested_payout(&match_id, &player1);
    assert_eq!(tc.balance(&player1), p1_before + 200);
}

// ── Partial vote / no-payout tests ───────────────────────────────────────────

#[test]
fn test_partial_consensus_does_not_trigger_payout() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(3, 3);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 2);
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);
}

#[test]
fn test_get_oracle_confirmations_starts_at_zero() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    assert_eq!(client.get_oracle_confirmations(&0), 0);
}

// ── Disagreement / conflict tests ─────────────────────────────────────────────

#[test]
fn test_conflicting_vote_returns_conflicting_result() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Oracle 0 votes Player1.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());

    // Oracle 1 votes Player2 — conflict.
    let result =
        client.try_submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(1).unwrap());
    assert_eq!(result, Err(Ok(Error::ConflictingResult)));
}

#[test]
fn test_conflicting_draw_vote_returns_conflicting_result() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());

    let result =
        client.try_submit_result_consensus(&match_id, &Winner::Draw, &oracles.get(1).unwrap());
    assert_eq!(result, Err(Ok(Error::ConflictingResult)));
}

// ── Duplicate vote tests ───────────────────────────────────────────────────────

#[test]
fn test_duplicate_oracle_vote_returns_already_confirmed() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let oracle = oracles.get(0).unwrap();

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracle);

    let result = client.try_submit_result_consensus(&match_id, &Winner::Player1, &oracle);
    assert_eq!(result, Err(Ok(Error::OracleAlreadyConfirmed)));
}

// ── Unauthorized oracle tests ─────────────────────────────────────────────────

#[test]
fn test_unapproved_oracle_returns_not_an_oracle() {
    let (env, contract_id, _admin, _p1, _p2, _token, _oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let rogue = Address::generate(&env);

    let result = client.try_submit_result_consensus(&match_id, &Winner::Player1, &rogue);
    assert_eq!(result, Err(Ok(Error::NotAnOracle)));
}

// ── Contract paused tests ─────────────────────────────────────────────────────

#[test]
fn test_consensus_blocked_when_paused() {
    let (env, contract_id, admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

    let result =
        client.try_submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

// ── Invalid state tests ────────────────────────────────────────────────────────

#[test]
fn test_consensus_on_pending_match_returns_invalid_state() {
    let (env, contract_id, _admin, player1, player2, token, oracles, _funded_match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create a fresh match that has NOT been funded.
    let pending_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ba217f6b"),
        &Platform::Lichess,
    );

    let result =
        client.try_submit_result_consensus(&pending_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_consensus_on_completed_match_returns_invalid_state() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Complete the match with 2 votes.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);

    // Try to vote again (with a third oracle added).
    let extra_oracle = Address::generate(&env);
    client.add_approved_oracle(&extra_oracle);
    let result = client.try_submit_result_consensus(&match_id, &Winner::Player1, &extra_oracle);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_consensus_on_nonexistent_match_returns_not_found() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, _match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result =
        client.try_submit_result_consensus(&9999, &Winner::Player1, &oracles.get(0).unwrap());
    assert_eq!(result, Err(Ok(Error::MatchNotFound)));
}

// ── Fund conservation tests ───────────────────────────────────────────────────

#[test]
fn test_consensus_fund_conservation_player1_wins() {
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(total_before, total_after, "tokens must be conserved");
}

#[test]
fn test_consensus_fund_conservation_draw() {
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    client.submit_result_consensus(&match_id, &Winner::Draw, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Draw, &oracles.get(1).unwrap());

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(total_before, total_after, "tokens must be conserved");
}

// ── Interaction with submit_result (legacy path) ──────────────────────────────

#[test]
fn test_legacy_submit_result_still_works_independently() {
    let (env, contract_id, oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let match_id = client.create_match(
        &player1,
        &Address::generate(&env),
        &100,
        &token,
        &String::from_str(&env, "58900fc4"),
        &Platform::Lichess,
    );
    // mint for new player2
    let asset_client = StellarAssetClient::new(&env, &token);
    let player2 = client.get_match(&match_id).player2;
    asset_client.mint(&player2, &100);

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let p1_before = tc.balance(&player1);
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);
    assert_eq!(tc.balance(&player1), p1_before + 200);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
}

// ── Confirmation count accuracy ───────────────────────────────────────────────

#[test]
fn test_get_oracle_confirmations_increments_correctly() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(3, 3);
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_oracle_confirmations(&match_id), 0);
    client.submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(0).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 1);
    client.submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(1).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 2);
    client.submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(2).unwrap());
    assert_eq!(client.get_oracle_confirmations(&match_id), 3);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
}

// ── Escrow balance zero after payout ─────────────────────────────────────────

#[test]
fn test_escrow_balance_zero_after_consensus_payout() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());

    assert_eq!(client.get_escrow_balance(&match_id), 0);
}

// ── Active-matches list is cleaned up after consensus ────────────────────────

#[test]
fn test_active_matches_cleared_after_consensus_payout() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    let active_before = client.get_active_matches();
    assert!(active_before.iter().any(|m| m.id == match_id));

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());

    let active_after = client.get_active_matches();
    assert!(!active_after.iter().any(|m| m.id == match_id));
}

// ── Deadlock handling tests ──────────────────────────────────────────────────

#[test]
fn test_conflicting_vote_does_not_persist_any_state() {
    // A conflicting vote returns `ConflictingResult`, and — because Soroban rolls
    // back all storage writes made during a call that returns `Err` — it cannot
    // leave any trace behind either: the confirmation count and deadlock status
    // are exactly as if the call never happened.
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());

    let result =
        client.try_submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(1).unwrap());
    assert_eq!(result, Err(Ok(Error::ConflictingResult)));

    assert_eq!(client.get_oracle_confirmations(&match_id), 1);
    assert!(!client.is_oracle_deadlocked(&match_id));
}

#[test]
fn test_deadlock_detected_when_threshold_unreachable() {
    // Deadlock can only ever be flagged from a vote that is itself accepted
    // (a conflicting vote's writes are rolled back — see
    // `test_conflicting_vote_does_not_persist_any_state`). It's legitimately
    // reachable when the required-confirmation count exceeds the number of
    // approved oracles: even a single accepted vote can make the threshold
    // mathematically unreachable.
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 3);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Only 2 approved oracles but 3 confirmations required — unreachable from the start.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());

    assert!(client.is_oracle_deadlocked(&match_id));
}

#[test]
fn test_normal_consensus_path_unaffected() {
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    // Both oracles vote for Player1 — should proceed normally to payout.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert!(!client.is_oracle_deadlocked(&match_id));

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    assert!(!client.is_oracle_deadlocked(&match_id));

    // Payout should have happened.
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(m.winner, Winner::Player1);

    client.claim_vested_payout(&match_id, &player1);

    // Player1 should have received stake + winnings.
    let p1_after = tc.balance(&player1);
    assert!(p1_after > p1_before);
    let p2_after = tc.balance(&player2);
    assert!(p2_after == p2_before);
}

#[test]
fn test_admin_can_resolve_deadlocked_match() {
    // Same unreachable-threshold setup as `test_deadlock_detected_when_threshold_unreachable`:
    // 2 approved oracles, 3 required confirmations.
    let (env, contract_id, _admin, player1, player2, token, oracles, match_id) =
        setup_consensus(2, 3);
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = soroban_sdk::token::Client::new(&env, &token);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert!(client.is_oracle_deadlocked(&match_id));

    // Admin resolves deadlock by choosing Player1 as the winner.
    client.resolve_oracle_deadlock(&match_id, &Winner::Player1);

    // Payout should have happened.
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(m.winner, Winner::Player1);

    client.claim_vested_payout(&match_id, &player1);

    // Player1 should have received winnings.
    let p1_after = tc.balance(&player1);
    assert!(p1_after > p1_before);
    let p2_after = tc.balance(&player2);
    assert!(p2_after == p2_before);
}

#[test]
fn test_cannot_resolve_non_deadlocked_match() {
    let (env, contract_id, _admin, _p1, _p2, _token, _oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Match is not deadlocked — cannot resolve it.
    let result = client.try_resolve_oracle_deadlock(&match_id, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_resolve_oracle_deadlock_blocked_while_paused() {
    let (env, contract_id, admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(2, 3);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Reach deadlock (2 oracles, 3 required — unreachable from the first vote).
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert!(client.is_oracle_deadlocked(&match_id));

    client.pause(&admin);

    let result = client.try_resolve_oracle_deadlock(&match_id, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_resolve_oracle_deadlock_rejects_non_active_match() {
    let (env, contract_id, _admin, player1, _p2, _token, oracles, match_id) = setup_consensus(2, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Complete the match normally via consensus — it's no longer Active.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(1).unwrap());
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    client.claim_vested_payout(&match_id, &player1);

    let result = client.try_resolve_oracle_deadlock(&match_id, &Winner::Player2);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_no_deadlock_with_enough_remaining_oracles() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(3, 3);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Oracle 0 votes Player1: 1 of 3 required, 3 oracles total — still reachable.
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert!(!client.is_oracle_deadlocked(&match_id));

    // Oracle 1 votes Player2 — conflict. The call returns `ConflictingResult` and,
    // per `test_conflicting_vote_does_not_persist_any_state`, leaves no trace: the
    // confirmation count and deadlock status are unchanged by the attempt.
    let result =
        client.try_submit_result_consensus(&match_id, &Winner::Player2, &oracles.get(1).unwrap());
    assert_eq!(result, Err(Ok(Error::ConflictingResult)));

    assert_eq!(client.get_oracle_confirmations(&match_id), 1);
    assert!(!client.is_oracle_deadlocked(&match_id));
}

#[test]
fn test_deadlock_not_triggered_with_partial_votes() {
    let (env, contract_id, _admin, _p1, _p2, _token, oracles, match_id) = setup_consensus(3, 2);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Oracle 0 votes Player1 (count = 1, need 2, possible max = 1 + 2 = 3, no deadlock).
    client.submit_result_consensus(&match_id, &Winner::Player1, &oracles.get(0).unwrap());
    assert!(!client.is_oracle_deadlocked(&match_id));
}
