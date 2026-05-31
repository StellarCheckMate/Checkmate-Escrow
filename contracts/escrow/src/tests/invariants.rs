/// Invariant-oriented tests for the escrow contract.
///
/// These tests verify two classes of properties that must hold across *all*
/// match outcomes:
///
/// 1. **Fund conservation** — the total number of tokens held by
///    `player1 + player2 + contract` never changes.  No tokens are minted or
///    burned by any escrow operation.
///
/// 2. **Terminal-state finality** — once a match reaches `Completed` or
///    `Cancelled`, no further state-mutating operations (deposit,
///    submit_result, cancel) are accepted, and the escrow balance is always 0.
use super::*;
use super::helpers::{
    assert_no_deposit_after_terminal, assert_no_submit_after_terminal,
    assert_terminal_state_zero_escrow, assert_total_balance, create_default_match,
    create_match_with_stake, fund_match, run_full_match, BalanceSnapshot,
};
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient},
};

// ── Fund conservation ────────────────────────────────────────────────────────

/// The total supply visible to the three accounts must be constant throughout
/// the entire Pending → Active → Completed (Player1 wins) lifecycle.
#[test]
fn test_fund_conservation_player1_wins() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let initial_total = 2_000i128; // 1000 each

    let id = create_default_match(&client, &env, &player1, &player2, &token, "inv_p1_win");

    // Before any deposit — total unchanged.
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);

    client.deposit(&id, &player1);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);

    client.deposit(&id, &player2);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);

    client.submit_result(&id, &Winner::Player1);
    // After payout player1 has 1100, player2 has 900, contract has 0.
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);
}

/// Fund conservation holds when Player2 wins.
#[test]
fn test_fund_conservation_player2_wins() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let initial_total = 2_000i128;

    let id = create_default_match(&client, &env, &player1, &player2, &token, "inv_p2_win");
    fund_match(&client, id, &player1, &player2);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);

    client.submit_result(&id, &Winner::Player2);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);
}

/// Fund conservation holds on a Draw (both players refunded).
#[test]
fn test_fund_conservation_draw() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let initial_total = 2_000i128;

    let id = create_default_match(&client, &env, &player1, &player2, &token, "inv_draw");
    fund_match(&client, id, &player1, &player2);
    client.submit_result(&id, &Winner::Draw);

    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);
}

/// Fund conservation holds when a match is cancelled after only player1 deposits.
#[test]
fn test_fund_conservation_cancel_after_player1_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let initial_total = 2_000i128;

    let id = create_default_match(&client, &env, &player1, &player2, &token, "inv_cancel_p1");
    client.deposit(&id, &player1);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);

    client.cancel_match(&id, &player1);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);
}

/// Fund conservation holds when a match is cancelled after only player2 deposits.
#[test]
fn test_fund_conservation_cancel_after_player2_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let initial_total = 2_000i128;

    let id = create_default_match(&client, &env, &player1, &player2, &token, "inv_cancel_p2");
    client.deposit(&id, &player2);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);

    client.cancel_match(&id, &player2);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);
}

/// Fund conservation holds when a match is cancelled with no deposits at all.
#[test]
fn test_fund_conservation_cancel_no_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let initial_total = 2_000i128;

    let id = create_default_match(&client, &env, &player1, &player2, &token, "inv_cancel_none");
    client.cancel_match(&id, &player1);
    assert_total_balance(&env, &token, &player1, &player2, &contract_id, initial_total);
}

/// Fund conservation holds across multiple concurrent matches.
///
/// Two independent matches run in parallel; each resolves differently.
/// The combined token supply across all four players and the contract must
/// remain constant throughout.
#[test]
fn test_fund_conservation_concurrent_matches() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let p1 = Address::generate(&env);
    let p2 = Address::generate(&env);
    let p3 = Address::generate(&env);
    let p4 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_id.address();
    let asset_client = StellarAssetClient::new(&env, &token);
    let token_client = TokenClient::new(&env, &token);

    for player in [&p1, &p2, &p3, &p4] {
        asset_client.mint(player, &1000);
    }

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let initial_total: i128 = [&p1, &p2, &p3, &p4]
        .iter()
        .map(|p| token_client.balance(p))
        .sum::<i128>()
        + token_client.balance(&contract_id);

    let m1 = create_default_match(&client, &env, &p1, &p2, &token, "inv_concurrent_1");
    let m2 = create_default_match(&client, &env, &p3, &p4, &token, "inv_concurrent_2");

    client.deposit(&m1, &p1);
    client.deposit(&m2, &p3);
    client.deposit(&m1, &p2);
    client.deposit(&m2, &p4);

    // Verify total is still conserved mid-flight.
    let mid_total: i128 = [&p1, &p2, &p3, &p4]
        .iter()
        .map(|p| token_client.balance(p))
        .sum::<i128>()
        + token_client.balance(&contract_id);
    assert_eq!(mid_total, initial_total, "fund conservation violated mid-flight");

    client.submit_result(&m1, &Winner::Player1);
    client.submit_result(&m2, &Winner::Draw);

    let final_total: i128 = [&p1, &p2, &p3, &p4]
        .iter()
        .map(|p| token_client.balance(p))
        .sum::<i128>()
        + token_client.balance(&contract_id);
    assert_eq!(final_total, initial_total, "fund conservation violated after resolution");
}

/// Fund conservation holds for a non-standard stake amount (odd number, large value).
#[test]
fn test_fund_conservation_large_stake() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Mint extra tokens so both players can cover the large stake.
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &9_000);
    asset_client.mint(&player2, &9_000);

    let token_client = TokenClient::new(&env, &token);
    let initial_total = token_client.balance(&player1)
        + token_client.balance(&player2)
        + token_client.balance(&contract_id);

    let id = create_match_with_stake(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "inv_large_stake",
        500,
    );
    fund_match(&client, id, &player1, &player2);
    client.submit_result(&id, &Winner::Player2);

    let final_total = token_client.balance(&player1)
        + token_client.balance(&player2)
        + token_client.balance(&contract_id);
    assert_eq!(final_total, initial_total, "fund conservation violated for large stake");
}

// ── Terminal-state finality ──────────────────────────────────────────────────

/// After `submit_result` (Player1 wins), the match is `Completed`:
/// - escrow balance is 0
/// - further deposits are rejected
/// - a second submit_result is rejected
#[test]
fn test_terminal_completed_player1_wins_blocks_further_ops() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = run_full_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "term_p1_win",
        &Winner::Player1,
    );

    assert_terminal_state_zero_escrow(&client, id);
    assert_no_deposit_after_terminal(&client, id, &player1);
    assert_no_submit_after_terminal(&client, id);
}

/// After `submit_result` (Player2 wins), the match is `Completed` and locked.
#[test]
fn test_terminal_completed_player2_wins_blocks_further_ops() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = run_full_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "term_p2_win",
        &Winner::Player2,
    );

    assert_terminal_state_zero_escrow(&client, id);
    assert_no_deposit_after_terminal(&client, id, &player2);
    assert_no_submit_after_terminal(&client, id);
}

/// After `submit_result` (Draw), the match is `Completed` and locked.
#[test]
fn test_terminal_completed_draw_blocks_further_ops() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = run_full_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "term_draw",
        &Winner::Draw,
    );

    assert_terminal_state_zero_escrow(&client, id);
    assert_no_deposit_after_terminal(&client, id, &player1);
    assert_no_submit_after_terminal(&client, id);
}

/// After `cancel_match` (no deposits), the match is `Cancelled` and locked.
#[test]
fn test_terminal_cancelled_no_deposits_blocks_further_ops() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_default_match(&client, &env, &player1, &player2, &token, "term_cancel_none");
    client.cancel_match(&id, &player1);

    assert_terminal_state_zero_escrow(&client, id);
    assert_no_deposit_after_terminal(&client, id, &player1);
    assert_no_submit_after_terminal(&client, id);
}

/// After `cancel_match` (player1 deposited), the match is `Cancelled` and locked.
#[test]
fn test_terminal_cancelled_after_deposit_blocks_further_ops() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_default_match(&client, &env, &player1, &player2, &token, "term_cancel_p1");
    client.deposit(&id, &player1);
    client.cancel_match(&id, &player1);

    assert_terminal_state_zero_escrow(&client, id);
    assert_no_deposit_after_terminal(&client, id, &player1);
    assert_no_submit_after_terminal(&client, id);
}

/// `cancel_match` on an already-`Cancelled` match must be rejected.
#[test]
fn test_terminal_double_cancel_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_default_match(&client, &env, &player1, &player2, &token, "term_double_cancel");
    client.cancel_match(&id, &player1);

    let result = client.try_cancel_match(&id, &player1);
    assert!(
        result.is_err(),
        "second cancel_match on a Cancelled match must be rejected"
    );
}

/// `cancel_match` on a `Completed` match must be rejected.
#[test]
fn test_terminal_cancel_after_complete_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = run_full_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "term_cancel_after_complete",
        &Winner::Player1,
    );

    let result = client.try_cancel_match(&id, &player1);
    assert!(
        result.is_err(),
        "cancel_match on a Completed match must be rejected"
    );
}

/// `completed_ledger` is set on both `Completed` and `Cancelled` terminal states.
#[test]
fn test_terminal_completed_ledger_is_set() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Completed path
    let id_complete = run_full_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "term_ledger_complete",
        &Winner::Player1,
    );
    let m = client.get_match(&id_complete);
    assert!(
        m.completed_ledger.is_some(),
        "completed_ledger must be set after submit_result"
    );

    // Cancelled path
    let id_cancel = create_default_match(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "term_ledger_cancel",
    );
    client.cancel_match(&id_cancel, &player1);
    let m = client.get_match(&id_cancel);
    assert!(
        m.completed_ledger.is_some(),
        "completed_ledger must be set after cancel_match"
    );
}

// ── Issue #72: InvalidGameId for empty game_id ───────────────────────────────

/// Calling `try_create_match` with an empty `game_id` must return
/// `Error::InvalidGameId`.  This mirrors issue #72.
#[test]
fn test_create_match_with_empty_game_id_returns_invalid_game_id() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, ""),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidGameId)),
        "create_match must reject an empty game_id with InvalidGameId"
    );
}

/// A game_id of exactly `MAX_GAME_ID_LEN` (64) bytes is accepted.
#[test]
fn test_create_match_with_max_length_game_id_succeeds() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 64 'a' characters — exactly at the limit.
    let max_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    assert_eq!(max_id.len(), 64);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, max_id),
        &Platform::Lichess,
    );
    assert!(
        result.is_ok(),
        "create_match must accept a game_id of exactly 64 bytes"
    );
}

/// A game_id of 65 bytes (one over the limit) is rejected with `InvalidGameId`.
#[test]
fn test_create_match_with_oversized_game_id_returns_invalid_game_id() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 65 'a' characters — one over the limit.
    let oversized_id = "aaaaaaaaaabbbbbbbbbbccccccccccddddddddddeeeeeeeeeeffffffffffffffff1";
    assert_eq!(oversized_id.len(), 65);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, oversized_id),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidGameId)),
        "create_match must reject a game_id longer than 64 bytes"
    );
}
