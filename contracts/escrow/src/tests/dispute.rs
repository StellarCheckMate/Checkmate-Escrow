use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};

// ── Dispute period configuration ──────────────────────────────────────────

fn setup_with_dispute_period(
    period: u32,
) -> (Env, Address, Address, Address, Address, Address, Address) {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.set_dispute_period(&period);
    (env, contract_id, oracle, player1, player2, token, admin)
}

fn create_funded_active_match(
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

fn advance_ledger(env: &Env, ledgers: u32) {
    let current = env.ledger().sequence();
    env.ledger().set_sequence_number(current + ledgers);
}

// ── submit_result with dispute period (delayed payout) ────────────────────

#[test]
fn test_submit_result_with_dispute_period_enters_pending_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp1");

    env.ledger().set_sequence_number(1000);

    client.submit_result(&match_id, &Winner::Player1);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::PendingResult);
    assert_eq!(client.get_escrow_balance(&match_id), 200); // funds still held
}

#[test]
fn test_submit_result_immediate_payout_when_period_zero() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_imm");

    client.submit_result(&match_id, &Winner::Player1);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(client.get_escrow_balance(&match_id), 0);
}

// ── finalize_match ────────────────────────────────────────────────────────

#[test]
fn test_finalize_match_after_dispute_period() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_fin1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    // Balance unchanged before finalization
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(token_client.balance(&contract_id), 200);

    // Still within dispute period — finalize fails
    env.ledger().set_sequence_number(1050);
    let result = client.try_finalize_match(&match_id);
    assert_eq!(result, Err(Ok(Error::DisputePeriodNotElapsed)));

    // After dispute deadline
    env.ledger().set_sequence_number(1100);
    client.finalize_match(&match_id);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(client.get_escrow_balance(&match_id), 0);
}

#[test]
fn test_finalize_match_with_draw() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_draw");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Draw);

    env.ledger().set_sequence_number(1100);
    client.finalize_match(&match_id);

    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 1000);
}

#[test]
fn test_finalize_match_fails_on_non_pending_result_state() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "disp_bad_state"),
        &Platform::Lichess,
    );

    let result = client.try_finalize_match(&match_id);
    assert_eq!(result, Err(Ok(Error::MatchNotInPendingResult)));
}

#[test]
fn test_finalize_match_fails_when_dispute_raised() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_conflict");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    // Raise dispute
    client.dispute_oracle_result(
        &match_id,
        &player1,
        &String::from_str(&env, "0xabcd1234"),
    );

    // finalize_match should now fail
    env.ledger().set_sequence_number(1200);
    let result = client.try_finalize_match(&match_id);
    assert_eq!(result, Err(Ok(Error::DisputeAlreadyRaised)));
}

// ── dispute_oracle_result ─────────────────────────────────────────────────

#[test]
fn test_dispute_oracle_result_creates_dispute() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_create");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player2);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player1,
        &String::from_str(&env, "0xdeadbeef"),
    );

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.match_id, match_id);
    assert_eq!(dispute.disputer, player1);
    assert_eq!(dispute.evidence_hash, String::from_str(&env, "0xdeadbeef"));
    assert_eq!(dispute.state, DisputeState::Active);
    assert_eq!(dispute.yes_votes, 0);
    assert_eq!(dispute.no_votes, 0);
    assert_eq!(dispute.voting_deadline, 1000 + VOTING_PERIOD_LEDGERS);
}

#[test]
fn test_dispute_oracle_result_rejects_non_player() {
    let (env, contract_id, oracle, player1, player2, token, admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_unauth");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player2);

    let stranger = Address::generate(&env);
    let result = client.try_dispute_oracle_result(
        &match_id,
        &stranger,
        &String::from_str(&env, "0xbeef"),
    );
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_dispute_oracle_result_rejects_after_deadline() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_deadline");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    // Advance past the dispute deadline
    env.ledger().set_sequence_number(1100);

    let result = client.try_dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xbeef"),
    );
    assert_eq!(result, Err(Ok(Error::DisputePeriodNotElapsed)));
}

#[test]
fn test_dispute_oracle_result_rejects_duplicate() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_dup");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    client.dispute_oracle_result(
        &match_id,
        &player1,
        &String::from_str(&env, "0xfirst"),
    );

    let result = client.try_dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xsecond"),
    );
    assert_eq!(result, Err(Ok(Error::DisputeAlreadyRaised)));
}

#[test]
fn test_dispute_oracle_result_rejects_empty_evidence() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_empty");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let result = client.try_dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, ""),
    );
    assert_eq!(result, Err(Ok(Error::InvalidEvidenceHash)));
}

// ── vote_on_dispute ───────────────────────────────────────────────────────

#[test]
fn test_vote_on_dispute_uptake_by_stakers() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_vote1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2, // player2 disputes
        &String::from_str(&env, "0xevidence"),
    );

    // player2 votes to overturn (true)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    let dispute = client.get_dispute(&dispute_id);
    // player2 has 900 tokens left after deposit
    assert_eq!(dispute.yes_votes, 900);
    assert_eq!(dispute.no_votes, 0);

    // player1 votes to uphold (false), has 900 tokens
    client.vote_on_dispute(&dispute_id, &player1, &false);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.yes_votes, 900);
    assert_eq!(dispute.no_votes, 900);
}

#[test]
fn test_vote_on_dispute_rejects_non_staker() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_nostake");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xevid"),
    );

    let non_staker = Address::generate(&env);
    let result = client.try_vote_on_dispute(&dispute_id, &non_staker, &true);
    assert_eq!(result, Err(Ok(Error::NotStaker)));
}

#[test]
fn test_vote_on_dispute_rejects_double_vote() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_doublevote");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xevid"),
    );

    client.vote_on_dispute(&dispute_id, &player2, &true);

    let result = client.try_vote_on_dispute(&dispute_id, &player2, &false);
    assert_eq!(result, Err(Ok(Error::AlreadyVoted)));
}

#[test]
fn test_vote_on_dispute_rejects_after_voting_deadline() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_votetm");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xevid"),
    );

    // Advance past voting deadline
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    let result = client.try_vote_on_dispute(&dispute_id, &player1, &true);
    assert_eq!(result, Err(Ok(Error::VotingPeriodElapsed)));
}

// ── resolve_dispute_by_vote ────────────────────────────────────────────────

#[test]
fn test_resolve_dispute_upholds_oracle_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_uphold");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    // player2 disputes, but everyone votes to uphold
    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xevid"),
    );

    // player1 votes to uphold (no = false)
    client.vote_on_dispute(&dispute_id, &player1, &false);
    // player2 votes to overturn (yes = true)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Voting period ends: yes=900, no=900 → no majority overturn → uphold
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    client.resolve_dispute_by_vote(&dispute_id);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    // Player1 (original oracle winner) gets the pot
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedUpheld);
}

#[test]
fn test_resolve_dispute_overturns_oracle_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_overturn");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xevid"),
    );

    // player2 votes to overturn (yes)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Voting period ends: yes=900, no=0 → majority overturn → draw
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    client.resolve_dispute_by_vote(&dispute_id);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    // Both get their stake back (draw)
    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 1000);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
}

#[test]
fn test_resolve_dispute_fails_before_voting_deadline() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_early");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xevid"),
    );

    // Try to resolve before voting deadline
    let result = client.try_resolve_dispute_by_vote(&dispute_id);
    assert_eq!(result, Err(Ok(Error::VotingPeriodNotElapsed)));
}

#[test]
fn test_resolve_dispute_fails_for_nonexistent_dispute() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_resolve_dispute_by_vote(&9999u64);
    assert_eq!(result, Err(Ok(Error::DisputeNotFound)));
}

// ── set_dispute_period ────────────────────────────────────────────────────

#[test]
fn test_set_dispute_period_admin_only() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let non_admin = Address::generate(&env);
    env.set_auths(&[]);

    let result = client.try_set_dispute_period(&100u32);
    assert!(result.is_err());
}

// ── Events ────────────────────────────────────────────────────────────────

#[test]
fn test_pending_result_event_emitted() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_evt1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "pending_result").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match/pending_result event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_winner, ev_deadline): (u64, Winner, u32) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, match_id);
    assert_eq!(ev_winner, Winner::Player1);
    assert_eq!(ev_deadline, 1100);
}

#[test]
fn test_dispute_created_event_emitted() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_evt2");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xhash"),
    );

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "dispute").into_val(&env),
        Symbol::new(&env, "created").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "dispute/created event not emitted");
}

#[test]
fn test_dispute_voted_event_emitted() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_evt3");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xhash"),
    );

    client.vote_on_dispute(&dispute_id, &player2, &true);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "dispute").into_val(&env),
        Symbol::new(&env, "voted").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "dispute/voted event not emitted");
}

#[test]
fn test_dispute_resolved_event_emitted() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_evt4");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xhash"),
    );

    client.vote_on_dispute(&dispute_id, &player2, &true);

    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    client.resolve_dispute_by_vote(&dispute_id);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "dispute").into_val(&env),
        Symbol::new(&env, "resolved").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "dispute/resolved event not emitted");
}

#[test]
fn test_finalized_event_emitted() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_evt5");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    env.ledger().set_sequence_number(1100);
    client.finalize_match(&match_id);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "finalized").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match/finalized event not emitted");
}

// ── Accessors ─────────────────────────────────────────────────────────────

#[test]
fn test_get_dispute_period_returns_configured_value() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_dispute_period(), 0);

    client.set_dispute_period(&500);
    assert_eq!(client.get_dispute_period(), 500);
}

#[test]
fn test_get_match_dispute_id_returns_id() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_funded_active_match(&client, &env, &player1, &player2, &token, "disp_getid");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xhash"),
    );

    let stored = client.get_match_dispute_id(&match_id);
    assert_eq!(stored, dispute_id);
}

// ── Full lifecycle ────────────────────────────────────────────────────────

#[test]
fn test_full_dispute_lifecycle_overturned() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    // Create match, deposit both players
    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "full_disp_lifecycle"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    assert_eq!(client.get_match(&match_id).state, MatchState::Active);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);

    // Oracle submits result
    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::PendingResult);
    assert_eq!(client.get_escrow_balance(&match_id), 200);

    // Player2 disputes
    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "0xcheating_evidence"),
    );

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::Active);

    // Player2 votes to overturn (only voter, so majority overturns)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Voting period ends
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    // Resolve
    client.resolve_dispute_by_vote(&dispute_id);

    // Match completed, draw outcome (both get stakes back)
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 1000);
    assert_eq!(client.get_escrow_balance(&match_id), 0);

    // Dispute resolved as overturned
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
}
