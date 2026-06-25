use super::*;

// ── Escrow balance conservation ───────────────────────────────────────────────
//
// Invariant: tokens are never created or destroyed by the escrow contract.
// Every token that enters via `deposit` must leave via `submit_result` or
// `cancel_match`.  The sum of all player balances plus the contract balance
// must equal the sum of all player balances before any interaction.

/// After a Player1 win the total supply across both players equals the
/// pre-match total — no tokens are minted or burned.
#[test]
fn test_balance_conservation_after_player1_wins() {
    let (env, contract_id, _oracle, player1, player2, token, _admin, match_id) =
        setup_with_funded_match();
    let tc = token_client(&env, &token);

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    EscrowContractClient::new(&env, &contract_id).submit_result(&match_id, &Winner::Player1);

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(
        total_after, total_before,
        "token supply must be conserved after Player1 wins"
    );
}

/// After a Player2 win the total supply is unchanged.
#[test]
fn test_balance_conservation_after_player2_wins() {
    let (env, contract_id, _oracle, player1, player2, token, _admin, match_id) =
        setup_with_funded_match();
    let tc = token_client(&env, &token);

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    EscrowContractClient::new(&env, &contract_id).submit_result(&match_id, &Winner::Player2);

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(
        total_after, total_before,
        "token supply must be conserved after Player2 wins"
    );
}

/// After a draw both players are refunded their stake — total supply unchanged.
#[test]
fn test_balance_conservation_after_draw() {
    let (env, contract_id, _oracle, player1, player2, token, _admin, match_id) =
        setup_with_funded_match();
    let tc = token_client(&env, &token);

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    EscrowContractClient::new(&env, &contract_id).submit_result(&match_id, &Winner::Draw);

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(
        total_after, total_before,
        "token supply must be conserved after a draw"
    );
    // Both players get their stake back exactly.
    assert_eq!(tc.balance(&player1), 1000, "player1 balance must be 1000 after draw");
    assert_eq!(tc.balance(&player2), 1000, "player2 balance must be 1000 after draw");
}

/// Cancelling after only player1 deposited returns the full stake to player1
/// and leaves the total supply unchanged.
#[test]
fn test_balance_conservation_after_cancel_with_one_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = token_client(&env, &token);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "conservation_cancel_one"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    client.cancel_match(&match_id, &player1);

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(
        total_after, total_before,
        "token supply must be conserved after cancel with one deposit"
    );
    assert_eq!(
        tc.balance(&contract_id),
        0,
        "contract must hold zero tokens after cancel"
    );
}

/// Cancelling after both players deposited refunds both and leaves the total
/// supply unchanged.
#[test]
fn test_balance_conservation_after_cancel_with_both_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = token_client(&env, &token);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "conservation_cancel_both"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    // Force back to Pending so cancel is allowed.
    env.as_contract(&contract_id, || {
        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .unwrap();
        m.state = MatchState::Pending;
        env.storage().persistent().set(&DataKey::Match(match_id), &m);
    });

    let total_before = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);

    client.cancel_match(&match_id, &player1);

    let total_after = tc.balance(&player1) + tc.balance(&player2) + tc.balance(&contract_id);
    assert_eq!(
        total_after, total_before,
        "token supply must be conserved after cancel with both deposits"
    );
    assert_eq!(
        tc.balance(&contract_id),
        0,
        "contract must hold zero tokens after cancel"
    );
}

/// The contract escrow balance equals exactly `depositors × stake_amount`
/// at every step of the deposit sequence.
#[test]
fn test_escrow_balance_tracks_deposits_exactly() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let stake = 250i128;

    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "balance_tracking_game"),
        &Platform::Lichess,
    );

    assert_eq!(client.get_escrow_balance(&match_id), 0);

    client.deposit(&match_id, &player1);
    assert_eq!(client.get_escrow_balance(&match_id), stake);

    client.deposit(&match_id, &player2);
    assert_eq!(client.get_escrow_balance(&match_id), stake * 2);
}

// ── Terminal state invariants ─────────────────────────────────────────────────
//
// Invariant: once a match reaches Completed or Cancelled it must be immutable —
// no further deposits, cancellations, or result submissions are accepted.

/// A Completed match rejects a second `submit_result`.
#[test]
fn test_completed_match_rejects_submit_result() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&match_id, &Winner::Player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);

    let result = client.try_submit_result(&match_id, &Winner::Player2);
    assert!(
        result.is_err(),
        "submit_result must be rejected on a Completed match"
    );
}

/// A Completed match rejects `cancel_match`.
#[test]
fn test_completed_match_rejects_cancel() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&match_id, &Winner::Player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);

    let result = client.try_cancel_match(&match_id, &player1);
    assert!(
        result.is_err(),
        "cancel_match must be rejected on a Completed match"
    );
}

/// A Completed match rejects further `deposit` calls.
#[test]
fn test_completed_match_rejects_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &100);

    client.submit_result(&match_id, &Winner::Player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);

    let result = client.try_deposit(&match_id, &player2);
    assert!(
        result.is_err(),
        "deposit must be rejected on a Completed match"
    );
}

/// A Cancelled match rejects `submit_result`.
#[test]
fn test_cancelled_match_rejects_submit_result() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancelled_submit_guard"),
        &Platform::Lichess,
    );
    client.cancel_match(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Cancelled);

    let result = client.try_submit_result(&match_id, &Winner::Player1);
    assert!(
        result.is_err(),
        "submit_result must be rejected on a Cancelled match"
    );
}

/// A Cancelled match rejects further `deposit` calls.
#[test]
fn test_cancelled_match_rejects_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancelled_deposit_guard"),
        &Platform::Lichess,
    );
    client.cancel_match(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Cancelled);

    let result = client.try_deposit(&match_id, &player2);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "deposit must return InvalidState on a Cancelled match"
    );
}

/// A Cancelled match rejects a second `cancel_match`.
#[test]
fn test_cancelled_match_rejects_cancel() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "double_cancel_guard"),
        &Platform::Lichess,
    );
    client.cancel_match(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Cancelled);

    let result = client.try_cancel_match(&match_id, &player1);
    assert!(
        result.is_err(),
        "cancel_match must be rejected on an already Cancelled match"
    );
}

/// `get_escrow_balance` returns 0 for both terminal states.
#[test]
fn test_escrow_balance_is_zero_in_all_terminal_states() {
    // Completed
    let (env, contract_id, _oracle, player1, player2, token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&match_id, &Winner::Player1);
    assert_eq!(
        client.get_escrow_balance(&match_id),
        0,
        "escrow balance must be 0 after Completed"
    );

    // Cancelled (fresh match, no deposits)
    let cancel_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "terminal_cancel_balance"),
        &Platform::Lichess,
    );
    client.cancel_match(&cancel_id, &player1);
    assert_eq!(
        client.get_escrow_balance(&cancel_id),
        0,
        "escrow balance must be 0 after Cancelled"
    );
}

// ── is_funded invariant ───────────────────────────────────────────────────────
//
// Invariant: is_funded returns true if and only if both players have deposited.

/// is_funded is false after only one deposit, true after both.
#[test]
fn test_is_funded_requires_both_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "is_funded_invariant"),
        &Platform::Lichess,
    );

    assert!(!client.is_funded(&match_id), "is_funded must be false before any deposit");

    client.deposit(&match_id, &player1);
    assert!(!client.is_funded(&match_id), "is_funded must be false after only player1 deposits");

    client.deposit(&match_id, &player2);
    assert!(client.is_funded(&match_id), "is_funded must be true after both players deposit");
}

// ── escrow balance upper-bound invariant ─────────────────────────────────────
//
// Invariant: get_escrow_balance never exceeds 2 × stake_amount at any lifecycle step.

/// get_escrow_balance never exceeds 2 × stake_amount across the full lifecycle.
#[test]
fn test_escrow_balance_never_exceeds_two_stakes() {
    let stake = 300i128;

    // ── payout path ──────────────────────────────────────────────────────────
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "balance_cap_payout"),
        &Platform::Lichess,
    );

    assert!(client.get_escrow_balance(&match_id) <= stake * 2, "balance must not exceed 2×stake before deposits");

    client.deposit(&match_id, &player1);
    assert!(client.get_escrow_balance(&match_id) <= stake * 2, "balance must not exceed 2×stake after player1 deposits");

    client.deposit(&match_id, &player2);
    assert!(client.get_escrow_balance(&match_id) <= stake * 2, "balance must not exceed 2×stake after both deposit");

    client.submit_result(&match_id, &Winner::Player1);
    assert!(client.get_escrow_balance(&match_id) <= stake * 2, "balance must not exceed 2×stake after payout");

    // ── refund path (cancel) ─────────────────────────────────────────────────
    let (env2, contract_id2, _oracle2, player1b, player2b, token2, _admin2) = setup();
    let client2 = EscrowContractClient::new(&env2, &contract_id2);

    let cancel_id = client2.create_match(
        &player1b,
        &player2b,
        &stake,
        &token2,
        &String::from_str(&env2, "balance_cap_refund"),
        &Platform::Lichess,
    );

    client2.deposit(&cancel_id, &player1b);
    assert!(client2.get_escrow_balance(&cancel_id) <= stake * 2, "balance must not exceed 2×stake after player1 deposits (cancel path)");

    client2.cancel_match(&cancel_id, &player1b);
    assert!(client2.get_escrow_balance(&cancel_id) <= stake * 2, "balance must not exceed 2×stake after cancel refund");
}
