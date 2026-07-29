use super::*;
use oracle::{OracleContract, OracleContractClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};

// ── Fixture helpers ──────────────────────────────────────────────────────────

/// Bring a match fully into `Active` state with both players deposited.
fn create_active_match(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> u64 {
    let id = client.create_match(
        player1,
        player2,
        &100,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&id, player1);
    client.deposit(&id, player2);
    id
}

/// Advance the ledger timestamp by `seconds` without changing the sequence
/// number. The rollback window is measured in seconds; ledger-bumping alone
/// would not move the timestamp forward far enough for a 24-hour test.
fn advance_timestamp(env: &Env, seconds: u64) {
    let current = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(current.checked_add(seconds).expect("test time overflow"));
}

// ── last_heartbeat tracking ──────────────────────────────────────────────────

#[test]
fn test_last_heartbeat_initialized_on_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(5_000);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "heartbeat_init"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(
        m.last_heartbeat, 5_000,
        "last_heartbeat must be initialized from ledger timestamp at creation"
    );
}

#[test]
fn test_last_heartbeat_refreshed_on_each_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(1_000);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "heartbeat_refresh"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&id).last_heartbeat, 1_000);

    // Advance time and have player1 deposit — last_heartbeat should follow.
    env.ledger().set_timestamp(2_500);
    client.deposit(&id, &player1);
    assert_eq!(
        client.get_match(&id).last_heartbeat,
        2_500,
        "last_heartbeat must refresh to current timestamp on each deposit"
    );

    // Player2 deposits at a later timestamp.
    env.ledger().set_timestamp(9_999);
    client.deposit(&id, &player2);
    assert_eq!(
        client.get_match(&id).last_heartbeat,
        9_999,
        "last_heartbeat must follow the latest deposit timestamp"
    );
}

// ── Within-window rollback ───────────────────────────────────────────────────

#[test]
fn test_rollback_within_window_refunds_both_players() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    env.ledger().set_timestamp(10_000);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_within");

    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(client.get_escrow_balance(&id), 200);

    // Advance 23h59m — well within the 24h window.
    advance_timestamp(&env, ROLLBACK_WINDOW_SECONDS - 60);

    client.dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "opponent_disconnected_lost_connection"),
    );

    let m = client.get_match(&id);
    assert_eq!(
        m.state,
        MatchState::Cancelled,
        "match must transition to Cancelled after a successful rollback"
    );
    assert_eq!(
        token_client.balance(&player1),
        1_000,
        "player1 stake must be refunded in full — no cancellation fee"
    );
    assert_eq!(
        token_client.balance(&player2),
        1_000,
        "player2 stake must be refunded in full — no cancellation fee"
    );
    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "escrow balance must be zero after rollback completes"
    );
}

#[test]
fn test_rollback_at_exact_window_boundary_succeeds() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    env.ledger().set_timestamp(0);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_boundary");

    // Advance exactly the window length — must still be allowed.
    advance_timestamp(&env, ROLLBACK_WINDOW_SECONDS);

    client.dispute_and_rollback_match(
        &id,
        &player2,
        &String::from_str(&env, "boundary_test"),
    );

    let m = client.get_match(&id);
    assert_eq!(
        m.state,
        MatchState::Cancelled,
        "rollback exactly at the window boundary must succeed"
    );
    assert_eq!(token_client.balance(&player1), 1_000);
    assert_eq!(token_client.balance(&player2), 1_000);
}

#[test]
fn test_rollback_by_either_player_succeeds() {
    // Symmetric test: both player1 → rollback, and player2 → rollback on
    // separate fresh matches, must both succeed within the window.
    let (env, contract_id, _oracle, p1a, p2a, token_a, _admin) = setup();
    let client_a = EscrowContractClient::new(&env, &contract_id);
    env.ledger().set_timestamp(100);
    let match_a = create_active_match(&client_a, &env, &p1a, &p2a, &token_a, "rb_p1_path");
    client_a.dispute_and_rollback_match(
        &match_a,
        &p1a,
        &String::from_str(&env, "p1_initiated"),
    );
    assert_eq!(
        client_a.get_match(&match_a).state,
        MatchState::Cancelled
    );

    let (env, contract_id, _oracle, p1b, p2b, token_b, _admin) = setup();
    let client_b = EscrowContractClient::new(&env, &contract_id);
    env.ledger().set_timestamp(100);
    let match_b = create_active_match(&client_b, &env, &p1b, &p2b, &token_b, "rb_p2_path");
    client_b.dispute_and_rollback_match(
        &match_b,
        &p2b,
        &String::from_str(&env, "p2_initiated"),
    );
    assert_eq!(
        client_b.get_match(&match_b).state,
        MatchState::Cancelled
    );
}

// ── After-window rejection ───────────────────────────────────────────────────

#[test]
fn test_rollback_after_window_returns_rollback_window_expired() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    env.ledger().set_timestamp(10_000);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_after");

    // Advance past the 24h window.
    advance_timestamp(&env, ROLLBACK_WINDOW_SECONDS + 1);

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "too_late"),
    );
    assert_eq!(
        result,
        Err(Ok(Error::RollbackWindowExpired)),
        "rollback past the window must be rejected with RollbackWindowExpired"
    );

    // The match must remain Active and stakes must remain in escrow.
    let m = client.get_match(&id);
    assert_eq!(
        m.state,
        MatchState::Active,
        "match state must not change on a failed rollback"
    );
    assert_eq!(
        client.get_escrow_balance(&id),
        200,
        "stakes must remain in escrow when rollback is rejected"
    );
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
}

#[test]
fn test_rollback_24h1s_after_heartbeat_is_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(0);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_well_past");

    // Advance 24h + 1s, no heartbeat refresh in between.
    advance_timestamp(&env, ROLLBACK_WINDOW_SECONDS + 1);

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player2,
        &String::from_str(&env, "well_past_window"),
    );
    assert_eq!(result, Err(Ok(Error::RollbackWindowExpired)));
}

// ── Authorization ────────────────────────────────────────────────────────────

#[test]
fn test_rollback_rejects_non_player_address() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_unauth");

    let stranger = Address::generate(&env);
    let result = client.try_dispute_and_rollback_match(
        &id,
        &stranger,
        &String::from_str(&env, "sneaky"),
    );
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "non-player caller must be rejected with Unauthorized"
    );
    assert_eq!(client.get_match(&id).state, MatchState::Active);
}

// ── State-transition guards ──────────────────────────────────────────────────

#[test]
fn test_rollback_rejects_pending_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create but DO NOT deposit — match remains Pending.
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "rb_pending"),
        &Platform::Lichess,
    );

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "wrong_path"),
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "rollback must reject match in Pending state — cancel_match is the right tool there"
    );
}

#[test]
fn test_rollback_rejects_completed_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_completed");
    client.submit_result(&id, &Winner::Player1);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "too_completed"),
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "completed matches must use the oracle-dispute path, not rollback"
    );
}

#[test]
fn test_rollback_rejects_paused_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_paused");
    client.pause_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Paused);

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "paused_rollback"),
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "rollback must reject Paused matches so resume/expire are the right tools"
    );
}

#[test]
fn test_rollback_rejects_already_cancelled_match() {
    // After a successful rollback, replay must be rejected to avoid double-pay
    // or weird state mutation on the post-cancelled match.
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(100);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_replay_guard");
    client.dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "first_rollback"),
    );
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player2,
        &String::from_str(&env, "second_rollback_attempt"),
    );
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "post-rollback replay must be rejected to prevent double-refund"
    );
}

// ── Reason validation ────────────────────────────────────────────────────────

#[test]
fn test_rollback_rejects_empty_reason() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_empty_reason");

    let result = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, ""),
    );
    assert_eq!(
        result,
        Err(Ok(Error::ReasonTooLong)),
        "empty reason must be rejected so all rollbacks are auditable"
    );
    assert_eq!(client.get_match(&id).state, MatchState::Active);
}

#[test]
fn test_rollback_rejects_oversize_reason() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_big_reason");

    // Build a 257-byte reason — one over the 256-byte cap.
    let oversize = "a".repeat(257);
    let result = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, oversize.as_str()),
    );
    assert_eq!(
        result,
        Err(Ok(Error::ReasonTooLong)),
        "reasons above MAX_REASON_LEN (256 bytes) must be rejected"
    );
    assert_eq!(client.get_match(&id).state, MatchState::Active);
}

// ── Event emission ────────────────────────────────────────────────────────────

#[test]
fn test_rollback_emits_match_rollback_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(50_000);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_event");

    let reason = String::from_str(&env, "opponent_disconnected_midgame");
    client.dispute_and_rollback_match(&id, &player1, &reason);

    let events = env.events().all();
    // "match" is emitted as a long Symbol (top-level scope) and "rollback"
    // is emitted as a short Symbol (`symbol_short!`) consistent with the
    // convention used for "cancelled"/"expired"/"activated".
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "rollback").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "match/rollback event must be emitted on successful rollback"
    );

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_disputer, ev_reason): (u64, Address, String) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
    assert_eq!(ev_disputer, player1);
    assert_eq!(ev_reason, reason);
}

#[test]
fn test_rollback_event_not_emitted_when_window_expired() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = create_active_match(&client, &env, &player1, &player2, &token, "rb_no_event");
    advance_timestamp(&env, ROLLBACK_WINDOW_SECONDS + 10);

    let _ = client.try_dispute_and_rollback_match(
        &id,
        &player1,
        &String::from_str(&env, "too_late"),
    );

    let rollback_short: soroban_sdk::Val = symbol_short!("rollback").into_val(&env);
    let has_event = env
        .events()
        .all()
        .iter()
        .any(|(_, topics, _)| topics.contains(rollback_short));
    assert!(
        !has_event,
        "no match/rollback event must be emitted when rollback is rejected"
    );
}

// ── Multi-token coverage ─────────────────────────────────────────────────────

/// Inline copy of the `multi_token::setup_multi_token_fixture` helper. Inlined
/// because Rust's module-visibility rules make a sibling-module's
/// module-private `fn` unreachable from `dispute_rollback` (private to the
/// module and its descendants, not its siblings). The body is the canonical
/// fixture used by `tests/multi_token.rs` so behavior matches the existing
/// multi-token suite end-to-end: distinct SAC tokens for `token_a`/`token_b`,
/// pre-minted balances, a working `OracleContract` with `set_rate`/`get_rate`
/// so `create_match_with_conversion` passes its ±5 % oracle-rate check.
fn rollback_setup_multi_token_fixture() -> (
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
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle_admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    // Register Oracle Contract.
    let oracle_id = env.register_contract(None, OracleContract);
    let oracle_client = OracleContractClient::new(&env, &oracle_id);
    oracle_client.initialize(&oracle_admin);

    // Register Escrow Contract.
    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    escrow_client.initialize(&oracle_id, &admin);

    // Deploy two distinct tokens (e.g. USDC and XLM).
    let token_a_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_a_addr = token_a_id.address();
    let asset_a_client = StellarAssetClient::new(&env, &token_a_addr);

    let token_b_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_b_addr = token_b_id.address();
    let asset_b_client = StellarAssetClient::new(&env, &token_b_addr);

    // Both players need token_a for deposits (since `deposit()` always uses
    // `m.token`). Player2 also receives `token_b` so the post-rollback refund
    // can be observed in their balance change.
    asset_a_client.mint(&player1, &1000_0000000);
    asset_a_client.mint(&player2, &1000_0000000);
    asset_b_client.mint(&player2, &500_0000000);

    // Pre-fund the oracle contract for swap-path symmetry with the parent
    // suite. Not consumed by the rollback path itself, which transfers
    // directly out of the escrow contract.
    asset_a_client.mint(&oracle_id, &10000_0000000);
    asset_b_client.mint(&oracle_id, &10000_0000000);

    (
        env,
        admin,
        oracle_client,
        escrow_client,
        player1,
        player2,
        token_a_addr,
        token_b_addr,
        oracle_id,
    )
}

#[test]
fn test_rollback_multi_token_match_refunds_each_player_in_their_token() {
    let (
        env,
        _admin,
        oracle_client,
        escrow_client,
        player1,
        player2,
        token_a,
        token_b,
        _oracle_id,
    ) = rollback_setup_multi_token_fixture();

    // Fix the oracle rate and use the same value as the match's `rate` so
    // both sit at the boundary (rate == oracle_rate is within ±5 %).
    let oracle_rate = 50_000_000; // 1 token_a = 5 token_b at 1e7 scale
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000; // 100 token_a
    let rate = 50_000_000; // 5.0 multiplier

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "rb_multi_token"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Match is Active with both deposits in token_a (the per-deposit path
    // always uses `m.token`, never `m.token_b`). Player2 still holds
    // 500 token_b from the fixture pre-mint.
    let m = escrow_client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Active);
    assert_eq!(m.token_b, Some(token_b.clone()));
    assert_eq!(m.conversion_rate, Some(rate));

    // Cap the timestamp so we're well inside the 24h rollback window.
    env.ledger().set_timestamp(0);
    env.ledger().set_sequence_number(100);

    // Pre-fund the escrow contract with token_b so the is_multi_token
    // refund branch has the funds available to transfer back to player2.
    // In production this budget would arrive through the oracle's
    // on-chain `swap`, but for a deterministic unit test we mint directly.
    StellarAssetClient::new(&env, &token_b).mint(&escrow_client.address, &500_0000000);

    escrow_client.dispute_and_rollback_match(
        &match_id,
        &player1,
        &String::from_str(&env, "multi_token_disconnect"),
    );

    let m_after = escrow_client.get_match(&match_id);
    assert_eq!(
        m_after.state,
        MatchState::Cancelled,
        "multi-token rollback must transition the match to Cancelled"
    );

    // Player1 refunded in token_a (m.token path): 100 token_a back.
    assert_eq!(
        token_client(&env, &token_a).balance(&player1),
        1000_0000000,
        "player1 must be refunded their full token_a stake on multi-token rollback"
    );
    // Player2 refunded in token_b (m.token_b path): 500 token_b. amount_b =
    // stake * conversion_rate / 10_000_000 = 100 * 5 = 500.
    assert_eq!(
        token_client(&env, &token_b).balance(&player2),
        1000_0000000,
        "player2 must be refunded the converted full token_b stake on multi-token rollback"
    );
    // Escrow balances: token_a drained by the player1 refund (200 - 100 = 100);
    // token_b drained by the player2 refund (500 - 500 = 0).
    assert_eq!(
        token_client(&env, &token_a).balance(&escrow_client.address),
        100_0000000,
        "escrow token_a balance must drop by exactly one stake after multi-token rollback"
    );
    assert_eq!(
        token_client(&env, &token_b).balance(&escrow_client.address),
        0,
        "escrow token_b balance must drop to zero after the multi-token refund"
    );
}

#[test]
fn test_rollback_multi_token_emits_match_rollback_event() {
    // Smoke test: confirm `dispute_and_rollback_match` emits the same
    // (`match`, `rollback`) event for a multi-token match as it does for a
    // single-token one — i.e. the multi-token refund path doesn't accidentally
    // alter or suppress the event topic.
    let (
        env,
        _admin,
        oracle_client,
        escrow_client,
        player1,
        player2,
        token_a,
        token_b,
        _oracle_id,
    ) = rollback_setup_multi_token_fixture();

    oracle_client.set_rate(&token_a, &token_b, &50_000_000);

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &50_000_000,
        &String::from_str(&env, "rb_multi_token_event"),
        &Platform::Lichess,
    );
    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    StellarAssetClient::new(&env, &token_b).mint(&escrow_client.address, &500_0000000);

    let reason = String::from_str(&env, "midgame_disconnect_multitoken");
    env.ledger().set_timestamp(0);

    escrow_client.dispute_and_rollback_match(&match_id, &player1, &reason);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "rollback").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "match/rollback event must still be emitted on multi-token rollback"
    );
}

// ── heartbeat_match coverage ──────────────────────────────────────────────────

#[test]
fn test_heartbeat_match_by_player1_refreshes_last_heartbeat() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(0);
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "hb_player1"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&id).last_heartbeat, 0);

    // Defer both deposits to known checkpoints so we know the post-deposit
    // last_heartbeat exactly.
    env.ledger().set_timestamp(1_000);
    client.deposit(&id, &player1);
    env.ledger().set_timestamp(2_000);
    client.deposit(&id, &player2);
    assert_eq!(
        client.get_match(&id).last_heartbeat,
        2_000,
        "last_heartbeat must reflect most recent deposit"
    );

    // Advance the clock past the 24h window without a heartbeat — naive
    // rollback would be rejected.
    env.ledger().set_timestamp(2_000 + ROLLBACK_WINDOW_SECONDS + 100);

    // Player1 heartbeats, refreshing last_heartbeat to the current ts.
    client.heartbeat_match(&id, &player1);
    let m = client.get_match(&id);
    assert_eq!(
        m.last_heartbeat,
        2_000 + ROLLBACK_WINDOW_SECONDS + 100,
        "heartbeat_match must refresh last_heartbeat to the current timestamp"
    );
    assert_eq!(m.state, MatchState::Active, "heartbeat must not change state");
}

#[test]
fn test_heartbeat_match_by_player2_refreshes_last_heartbeat() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(5_000);
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "hb_player2"),
        &Platform::Lichess,
    );
    env.ledger().set_timestamp(6_000);
    client.deposit(&id, &player1);
    env.ledger().set_timestamp(7_000);
    client.deposit(&id, &player2);

    env.ledger().set_timestamp(7_000 + ROLLBACK_WINDOW_SECONDS + 50);

    client.heartbeat_match(&id, &player2);
    let m = client.get_match(&id);
    assert_eq!(
        m.last_heartbeat,
        7_000 + ROLLBACK_WINDOW_SECONDS + 50,
        "heartbeat_match from player2 must also refresh last_heartbeat symmetrically"
    );
}

#[test]
fn test_heartbeat_match_rejects_non_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(123_456);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "hb_unauth");
    let before = client.get_match(&id).last_heartbeat;

    let stranger = Address::generate(&env);
    let result = client.try_heartbeat_match(&id, &stranger);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "non-player heartbeats must be rejected with Unauthorized"
    );

    // Snapshot equality: the rejected call must leave last_heartbeat exactly
    // as it was before, regardless of default-env timestamp variability.
    assert_eq!(
        client.get_match(&id).last_heartbeat, before,
        "rejected heartbeat must not move last_heartbeat"
    );
}

#[test]
fn test_heartbeat_match_rejects_non_active_states() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Pending state — heartbeat rejected.
    let pending_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "hb_pending"),
        &Platform::Lichess,
    );
    let r = client.try_heartbeat_match(&pending_id, &player1);
    assert_eq!(
        r,
        Err(Ok(Error::InvalidState)),
        "heartbeat_match must reject Pending matches"
    );

    // Paused state — heartbeat rejected (resume_match is the right tool).
    let paused_id = create_active_match(&client, &env, &player1, &player2, &token, "hb_paused");
    client.pause_match(&paused_id, &player1);
    let r = client.try_heartbeat_match(&paused_id, &player1);
    assert_eq!(
        r,
        Err(Ok(Error::InvalidState)),
        "heartbeat_match must reject Paused matches — use resume_match"
    );

    // Completed state — heartbeat rejected.
    let done_id = create_active_match(&client, &env, &player1, &player2, &token, "hb_completed");
    client.submit_result(&done_id, &Winner::Player1);
    let r = client.try_heartbeat_match(&done_id, &player1);
    assert_eq!(
        r,
        Err(Ok(Error::InvalidState)),
        "heartbeat_match must reject Completed matches — refund dispute window is past"
    );

    // Cancelled state — heartbeat rejected.
    let cancel_id = create_active_match(&client, &env, &player1, &player2, &token, "hb_cancelled");
    client.dispute_and_rollback_match(
        &cancel_id,
        &player1,
        &String::from_str(&env, "before hb test"),
    );
    let r = client.try_heartbeat_match(&cancel_id, &player1);
    assert_eq!(
        r,
        Err(Ok(Error::InvalidState)),
        "heartbeat_match must reject terminal Cancelled matches"
    );
}

#[test]
fn test_heartbeat_match_emits_event_with_match_id_player_and_timestamp() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(42_000);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "hb_event");

    env.ledger().set_timestamp(99_999);
    client.heartbeat_match(&id, &player2);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "heartbeat").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "match/heartbeat event must be emitted on heartbeat"
    );

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_player, ev_timestamp): (u64, Address, u64) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, id);
    assert_eq!(ev_player, player2);
    assert_eq!(ev_timestamp, 99_999);
}

#[test]
fn test_heartbeat_match_keeps_rollback_window_alive_past_24h() {
    // Integration round-trip: an Active match would normally be rejected
    // from `dispute_and_rollback_match` after 24h of in-game inactivity.
    // A heartbeat right before the deadline must reset the clock and let
    // the rollback succeed in the next window.
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(0);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "hb_keep_alive");

    // Advance to just inside the window, heartbeat, then advance past the
    // original 24h bound — the post-heartbeat window is the new bound,
    // not the original one.
    adv_to(&env, ROLLBACK_WINDOW_SECONDS - 30);
    client.heartbeat_match(&id, &player1);

    // Past the original 24h mark where the original deposit heartbeat
    // would have been stale — but the heartbeat refreshed it.
    adv_to(&env, ROLLBACK_WINDOW_SECONDS + 60);

    client.dispute_and_rollback_match(
        &id,
        &player2,
        &String::from_str(&env, "post_heartbeat_disconnect"),
    );

    let m = client.get_match(&id);
    assert_eq!(
        m.state,
        MatchState::Cancelled,
        "heartbeat must keep the rollback window alive so subsequent dispute succeeds"
    );
}

#[test]
fn test_heartbeat_match_does_not_move_escrow_balance() {
    // Confirms the heartbeats are pure timestamp updates — no token transfer
    // is performed anywhere, neither to nor from the contract.
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_timestamp(0);
    let id = create_active_match(&client, &env, &player1, &player2, &token, "hb_no_xfer");
    let escrow_before = client.get_escrow_balance(&id);
    assert_eq!(
        escrow_before, 200,
        "after both deposits, escrow should hold the full pot"
    );

    env.ledger().set_timestamp(ROLLBACK_WINDOW_SECONDS - 1);
    client.heartbeat_match(&id, &player1);
    assert_eq!(
        client.get_escrow_balance(&id),
        escrow_before,
        "heartbeat must not move any tokens out of (or into) escrow"
    );

    env.ledger().set_timestamp(ROLLBACK_WINDOW_SECONDS + 99);
    client.heartbeat_match(&id, &player2);
    assert_eq!(
        client.get_escrow_balance(&id),
        escrow_before,
        "a second heartbeat must also leave the escrow balance untouched"
    );
}

/// Advance the ledger timestamp by `seconds` while leaving the sequence
/// number untouched. Pulled out of the timer test so multiple heartbeat
/// tests can share it without re-declaring the helper each time.
fn adv_to(env: &Env, seconds: u64) {
    let current = env.ledger().timestamp();
    env.ledger()
        .set_timestamp(current.checked_add(seconds).expect("test time overflow"));
}
