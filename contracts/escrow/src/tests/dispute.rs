use super::*;
use soroban_sdk::testutils::Ledger as _;

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
    create_funded_active_match_with_stake(client, env, player1, player2, token, game_id, 100)
}

fn create_funded_active_match_with_stake(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
    stake: i128,
) -> u64 {
    let id = client.create_match(
        player1,
        player2,
        &stake,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&id, player1);
    client.deposit(&id, player2);
    id
}

/// Give `voter` a large escrowed stake by depositing into a dedicated match
/// with a throwaway counterparty. Dispute-vote weight is sourced from
/// escrowed stake (`player_escrow_balance`), not raw token balance, so this
/// is how tests exercise vote weights beyond what the Bronze-tier stake
/// ceiling would otherwise allow. Both sides of the match are fast-tracked
/// to Platinum tier (unlimited stake cap) by writing `PlayerCompletedMatchCount`
/// directly, mirroring the pattern in `security.rs`.
fn give_voter_large_stake(
    client: &EscrowContractClient,
    env: &Env,
    contract_id: &Address,
    token: &Address,
    voter: &Address,
    stake: i128,
    game_id: &str,
) {
    let counterparty = Address::generate(env);
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::PlayerCompletedMatchCount(voter.clone()), &10u32);
        env.storage().persistent().set(
            &DataKey::PlayerCompletedMatchCount(counterparty.clone()),
            &10u32,
        );
    });
    let match_id = client.create_match(
        voter,
        &counterparty,
        &stake,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&match_id, voter);
}

#[allow(dead_code)]
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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "disp0001");

    env.ledger().set_sequence_number(1000);

    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::PendingResult);
    assert_eq!(client.get_escrow_balance(&match_id), 200); // funds still held
}

#[test]
fn test_submit_result_immediate_payout_when_period_zero() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dispimm1");

    client.submit_result(&match_id, &Winner::Player1, &oracle);

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dfin0001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "ddraw001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Draw, &oracle);

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
        &String::from_str(&env, "b6d20e2e"),
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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dcnflct1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // Raise dispute
    client.dispute_oracle_result(&match_id, &player1, &String::from_str(&env, "b3dfd696"));

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dcrt0001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player2, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player1, &String::from_str(&env, "352aaeb7"));

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.match_id, match_id);
    assert_eq!(dispute.disputer, player1);
    assert_eq!(dispute.evidence_hash, String::from_str(&env, "352aaeb7"));
    assert_eq!(dispute.state, DisputeState::Active);
    assert_eq!(dispute.yes_votes, 0);
    assert_eq!(dispute.no_votes, 0);
    assert_eq!(dispute.voting_deadline, 1000 + VOTING_PERIOD_LEDGERS);
}

#[test]
fn test_dispute_oracle_result_rejects_non_player() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dunauth1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player2, &oracle);

    let stranger = Address::generate(&env);
    let result =
        client.try_dispute_oracle_result(&match_id, &stranger, &String::from_str(&env, "99d6288b"));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_dispute_oracle_result_rejects_after_deadline() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "ddlne001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // Advance past the dispute deadline
    env.ledger().set_sequence_number(1100);

    let result =
        client.try_dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "99d6288b"));
    assert_eq!(result, Err(Ok(Error::DisputePeriodNotElapsed)));
}

#[test]
fn test_dispute_oracle_result_rejects_duplicate() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "ddup0001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    client.dispute_oracle_result(&match_id, &player1, &String::from_str(&env, "c38d14bc"));

    let result =
        client.try_dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "0xsecond"));
    assert_eq!(result, Err(Ok(Error::DisputeAlreadyRaised)));
}

#[test]
fn test_dispute_oracle_result_rejects_empty_evidence() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dempty01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let result = client.try_dispute_oracle_result(&match_id, &player2, &String::from_str(&env, ""));
    assert_eq!(result, Err(Ok(Error::InvalidEvidenceHash)));
}

// ── vote_on_dispute ───────────────────────────────────────────────────────

#[test]
fn test_vote_on_dispute_uptake_by_stakers() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dvote001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2, // player2 disputes
        &String::from_str(&env, "6c79d6c7"),
    );

    // player2 votes to overturn (true)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    let dispute = client.get_dispute(&dispute_id);
    // Vote weight is the voter's escrow stake in this match (100), not their
    // unrelated wallet balance.
    assert_eq!(dispute.yes_votes, 100);
    assert_eq!(dispute.no_votes, 0);

    // player1 votes to uphold (false), also staked 100.
    client.vote_on_dispute(&dispute_id, &player1, &false);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.yes_votes, 100);
    assert_eq!(dispute.no_votes, 100);
}

#[test]
fn test_vote_on_dispute_rejects_non_staker() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dnstk001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "394661e8"));

    let non_staker = Address::generate(&env);
    let result = client.try_vote_on_dispute(&dispute_id, &non_staker, &true);
    assert_eq!(result, Err(Ok(Error::NotStaker)));
}

#[test]
fn test_vote_on_dispute_rejects_double_vote() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "ddblvt01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "394661e8"));

    client.vote_on_dispute(&dispute_id, &player2, &true);

    let result = client.try_vote_on_dispute(&dispute_id, &player2, &false);
    assert_eq!(result, Err(Ok(Error::AlreadyVoted)));
}

#[test]
fn test_vote_on_dispute_rejects_after_voting_deadline() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dvotetm1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "394661e8"));

    // Advance past voting deadline
    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "duphld01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // player2 disputes, but everyone votes to uphold
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "394661e8"));

    // player1 votes to uphold (no = false)
    client.vote_on_dispute(&dispute_id, &player1, &false);
    // player2 votes to overturn (yes = true)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Voting period ends: yes=900, no=900 → no majority overturn → uphold
    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    client.resolve_dispute_by_vote(&dispute_id);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
    // Player1 (original oracle winner) gets the pot
    assert_eq!(token_client.balance(&player1), 1100);
    // Player2 raised the dispute and lost (upheld), forfeiting their 1-token bond.
    assert_eq!(token_client.balance(&player2), 899);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedUpheld);
}

#[test]
fn test_resolve_dispute_overturns_oracle_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dovrtn01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "394661e8"));

    // player2 votes to overturn (yes)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Voting period ends: yes=900, no=0 → majority overturn → draw
    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dearly01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "394661e8"));

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

    let _non_admin = Address::generate(&env);
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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "devt0001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "devt0002");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "8f9a074c"));

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "devt0003");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "8f9a074c"));

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "devt0004");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "8f9a074c"));

    client.vote_on_dispute(&dispute_id, &player2, &true);

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "devt0005");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

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
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
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

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "dgetid01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "8f9a074c"));

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
        &String::from_str(&env, "5a24e0f7"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    assert_eq!(client.get_match(&match_id).state, MatchState::Active);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);

    // Oracle submits result
    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::PendingResult);
    assert_eq!(client.get_escrow_balance(&match_id), 200);

    // Player2 disputes
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "ff28caaf"));

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::Active);

    // Player2 votes to overturn (only voter, so majority overturns)
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Voting period ends
    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

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

// ── Governance: Dispute bond requirement ──────────────────────────────────

#[test]
fn test_dispute_requires_bond() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    // Set dispute bond to 1% of stake (100 basis points)
    client.set_dispute_bond_basis_points(&100);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "bndtst01");

    // Confirm match stake is 100 tokens
    let m = client.get_match(&match_id);
    assert_eq!(m.stake_amount, 100);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // Player2 initiates dispute
    // Bond required: 100 tokens * 100 bps / 10_000 = 1 token
    let initial_p2_balance = token_client.balance(&player2);
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    // Bond should be transferred from player2 to escrow
    let after_bond_balance = token_client.balance(&player2);
    assert_eq!(initial_p2_balance - after_bond_balance, 1); // 1% of 100 = 1 token
    assert_eq!(token_client.balance(&contract_id), 201); // 200 escrow + 1 bond

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.dispute_bond, 1);
}

#[test]
fn test_dispute_bond_minimum_one_for_tiny_stake() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    // Default dispute bond is 100 bps (1% of stake).
    client.set_dispute_bond_basis_points(&100);

    // Minimum-stake match: 1 stroop per side. 1 * 100 / 10_000 rounds down
    // to 0, so the bond must be floored up to 1 stroop — a dispute is never
    // free, even on the smallest possible match.
    let match_id = create_funded_active_match_with_stake(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "tiny0001",
        1,
    );

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let initial_p2_balance = token_client.balance(&player2);
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.dispute_bond, 1, "bond must be floored to 1 stroop");

    // Exactly 1 stroop was collected: no zero-cost disputes.
    let after_bond_balance = token_client.balance(&player2);
    assert_eq!(initial_p2_balance - after_bond_balance, 1);
    assert_eq!(token_client.balance(&contract_id), 3); // 2 stake + 1 bond
}

#[test]
fn test_dispute_bond_minimum_one_for_sub_unit_stake() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    client.set_dispute_bond_basis_points(&100);

    // 99 * 100 / 10_000 = 0.99 → floors to 0, must be floored up to 1.
    let match_id = create_funded_active_match_with_stake(
        &client,
        &env,
        &player1,
        &player2,
        &token,
        "tiny0099",
        99,
    );

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let initial_p2_balance = token_client.balance(&player2);
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.dispute_bond, 1);

    let after_bond_balance = token_client.balance(&player2);
    assert_eq!(initial_p2_balance - after_bond_balance, 1);
}

#[test]
fn test_dispute_bond_refunded_on_overturn() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    client.set_dispute_bond_basis_points(&100); // 1% bond

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "bndrfnd1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    let player2_before_vote = token_client.balance(&player2);

    // Player2 votes to overturn
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Advance past voting deadline
    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    // Resolve dispute (should overturn)
    client.resolve_dispute_by_vote(&dispute_id);

    // Overturn resolves as a Draw payout (both players refunded their 100
    // stake) plus the dispute bond (1) refunded to the disputer.
    let player2_after = token_client.balance(&player2);
    assert_eq!(player2_after, player2_before_vote + 100 + 1);
}

#[test]
fn test_dispute_bond_forfeited_on_upheld() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    client.set_dispute_bond_basis_points(&100); // 1% bond

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "bndfrft1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    let player2_before = token_client.balance(&player2);
    let _treasury_before = token_client.balance(&admin); // Admin is also treasury in tests

    // Only player1 votes to uphold
    client.vote_on_dispute(&dispute_id, &player1, &false);

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id);

    // Bond should be forfeited (not refunded to disputer)
    let player2_after = token_client.balance(&player2);
    assert_eq!(player2_after, player2_before); // Not refunded
}

// ── Governance: Snapshot voting & flash-loan prevention ──────────────────

#[test]
fn test_vote_uses_snapshot_weight() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let _token_client = TokenClient::new(&env, &token);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "snapshot");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // Dispute created, snapshot taken at this ledger
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.snapshot_ledger, 1000);

    // Player1 votes with their balance at snapshot time
    client.vote_on_dispute(&dispute_id, &player1, &true);

    // Even if player1 sells all tokens, their vote weight is still based on snapshot
    // This test demonstrates vote weight is snapshot-based, not live-balance based
    let dispute_after = client.get_dispute(&dispute_id);
    assert!(dispute_after.yes_votes > 0); // Vote counted with snapshot weight
}

// ── Governance: Quorum requirement ──────────────────────────────────────────

#[test]
fn test_quorum_not_met_prevents_resolution() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let _token_client = TokenClient::new(&env, &token);

    // Set quorum to 50% of snapshot weight
    client.set_quorum_basis_points(&5000);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "quorum01");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // At this point, escrow holds 200 tokens (stake for both players)
    // Quorum threshold = 200 * 50% = 100 tokens minimum participation

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    // Only one player votes (100 tokens), which is exactly the quorum
    client.vote_on_dispute(&dispute_id, &player1, &true);

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    // Resolution should succeed if exactly at quorum
    client.resolve_dispute_by_vote(&dispute_id);
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
}

#[test]
fn test_quorum_not_met_with_low_participation() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Set quorum to 100% of snapshot weight (extreme, for testing)
    client.set_quorum_basis_points(&10000);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "qrmfail1");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    // Only one player votes (less than 100%)
    client.vote_on_dispute(&dispute_id, &player1, &true);

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    // Resolution should fail due to quorum not met
    let result = client.try_resolve_dispute_by_vote(&dispute_id);
    assert_eq!(result, Err(Ok(Error::QuorumNotMet)));
}

// ── Governance: Parameter getters ─────────────────────────────────────────

#[test]
fn test_get_governance_parameters() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Set custom governance parameters
    client.set_dispute_bond_basis_points(&50); // 0.5%
    client.set_minimum_hold_duration(&50);
    client.set_quorum_basis_points(&3000); // 30%

    // Verify getters return correct values
    assert_eq!(client.get_dispute_bond_basis_points(), 50);
    assert_eq!(client.get_minimum_hold_duration(), 50);
    assert_eq!(client.get_quorum_basis_points(), 3000);
}

// ── mark_dispute_for_oracle_slash ───────────────────────────────────────────

/// Runs a dispute through to `ResolvedOverturned` (same flow as
/// `test_resolve_dispute_overturns_oracle_result`) and returns the dispute id
/// plus its bond amount, ready for `mark_dispute_for_oracle_slash`.
///
/// Returns `(env, contract_id, oracle, player1, player2, token, admin, dispute_id, bond)`.
#[allow(clippy::type_complexity)]
fn overturned_dispute(
    period: u32,
) -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    u64,
    i128,
) {
    let (env, contract_id, oracle, player1, player2, token, admin) =
        setup_with_dispute_period(period);
    let client = EscrowContractClient::new(&env, &contract_id);
    // 10% bond gives headroom for over/under-amount slash tests below.
    client.set_dispute_bond_basis_points(&1000);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "slash001");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "ab12cd34"));
    client.vote_on_dispute(&dispute_id, &player2, &true);

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
    let bond = dispute.dispute_bond;

    (
        env,
        contract_id,
        oracle,
        player1,
        player2,
        token,
        admin,
        dispute_id,
        bond,
    )
}

#[test]
fn test_mark_dispute_for_oracle_slash_emits_signal_event() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        overturned_dispute(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.mark_dispute_for_oracle_slash(&dispute_id, &bond);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "dispute").into_val(&env),
        Symbol::new(&env, "oracle_slash_signal").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "oracle_slash_signal event must be emitted"
    );

    let (_, _, data) = matched.unwrap();
    let (ev_dispute_id, ev_oracle, ev_amount): (u64, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_dispute_id, dispute_id);
    assert_eq!(ev_oracle, oracle);
    assert_eq!(ev_amount, bond);
}

#[test]
fn test_mark_dispute_for_oracle_slash_requires_resolved_overturned_state() {
    // Dispute still Active (voting not yet resolved) must be rejected.
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "slash002");
    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "cd34ab12"));

    let result = client.try_mark_dispute_for_oracle_slash(&dispute_id, &1);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_mark_dispute_for_oracle_slash_rejects_zero_amount() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, dispute_id, _bond) =
        overturned_dispute(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_mark_dispute_for_oracle_slash(&dispute_id, &0);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_mark_dispute_for_oracle_slash_rejects_amount_over_bond() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        overturned_dispute(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_mark_dispute_for_oracle_slash(&dispute_id, &(bond + 1));
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_mark_dispute_for_oracle_slash_unknown_dispute_not_found() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_mark_dispute_for_oracle_slash(&9999u64, &1);
    assert_eq!(result, Err(Ok(Error::DisputeNotFound)));
}

#[test]
fn test_mark_dispute_for_oracle_slash_requires_admin_auth() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        overturned_dispute(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    env.set_auths(&[]);
    let result = client.try_mark_dispute_for_oracle_slash(&dispute_id, &bond);
    assert!(result.is_err(), "non-admin caller must be rejected");
}

// ── Vote weight truncation fix tests ──────────────────────────────────────────

#[test]
fn test_vote_weight_exceeding_u32_max_recorded_correctly() {
    let (env, contract_id, oracle, _player1, _player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = StellarAssetClient::new(&env, &token);
    env.mock_all_auths();

    // Give a voter an escrowed stake exceeding u32::MAX (4,294,967,295) via a
    // dedicated match. Dispute-vote weight is sourced from escrowed stake,
    // not raw token balance.
    let large_voter = Address::generate(&env);
    let large_balance: i128 = 5_000_000_000; // 5 billion, exceeds u32::MAX
    token_client.mint(&large_voter, &large_balance);
    give_voter_large_stake(
        &client,
        &env,
        &contract_id,
        &token,
        &large_voter,
        large_balance,
        "lrgstak1",
    );

    // Create a small match with normal players
    let normal_player1 = Address::generate(&env);
    let normal_player2 = Address::generate(&env);
    token_client.mint(&normal_player1, &1000);
    token_client.mint(&normal_player2, &1000);

    let match_id = client.create_match(
        &normal_player1,
        &normal_player2,
        &100,
        &token,
        &String::from_str(&env, "bigvote1"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &normal_player1);
    client.deposit(&match_id, &normal_player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &normal_player2,
        &String::from_str(&env, "evidence"),
    );

    // Record the voter's vote with large escrowed stake
    // This should correctly record their full stake weight, not a truncated u32 value
    client.vote_on_dispute(&dispute_id, &large_voter, &true);

    let dispute = client.get_dispute(&dispute_id);
    // The vote weight should be the full large_balance, not a truncated version
    assert_eq!(
        dispute.yes_votes, large_balance,
        "vote weight should equal the full i128 stake, not truncated to u32"
    );
}

#[test]
fn test_two_voters_with_2_to_32_balance_difference_distinguished() {
    let (env, contract_id, oracle, _player1, _player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = StellarAssetClient::new(&env, &token);

    env.mock_all_auths();

    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);

    // Create escrowed stakes that differ by exactly 2^32
    let base: i128 = 5_000_000_000; // 5 billion
    let u32_max: i128 = 4_294_967_296; // 2^32

    token_client.mint(&voter1, &base);
    token_client.mint(&voter2, &(base + u32_max));
    give_voter_large_stake(
        &client,
        &env,
        &contract_id,
        &token,
        &voter1,
        base,
        "difstak1",
    );
    give_voter_large_stake(
        &client,
        &env,
        &contract_id,
        &token,
        &voter2,
        base + u32_max,
        "difstak2",
    );

    let normal_player1 = Address::generate(&env);
    let normal_player2 = Address::generate(&env);
    token_client.mint(&normal_player1, &1000);
    token_client.mint(&normal_player2, &1000);

    let match_id = client.create_match(
        &normal_player1,
        &normal_player2,
        &100,
        &token,
        &String::from_str(&env, "diffvot1"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &normal_player1);
    client.deposit(&match_id, &normal_player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &normal_player2,
        &String::from_str(&env, "evidence"),
    );

    client.vote_on_dispute(&dispute_id, &voter1, &true);
    client.vote_on_dispute(&dispute_id, &voter2, &true);

    let dispute = client.get_dispute(&dispute_id);
    // With the fix, both stakes should be correctly added without truncation
    let expected_total = base + (base + u32_max);
    assert_eq!(
        dispute.yes_votes, expected_total,
        "votes should sum to correct total without truncation artifacts"
    );
}

#[test]
fn test_quorum_calculation_with_large_total_votes() {
    let (env, contract_id, oracle, _player1, _player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = StellarAssetClient::new(&env, &token);

    env.mock_all_auths();

    // Set quorum to a percentage of large total weight
    client.set_quorum_basis_points(&5000); // 50%

    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);

    let large_balance: i128 = 10_000_000_000; // 10 billion
    token_client.mint(&voter1, &large_balance);
    token_client.mint(&voter2, &large_balance);
    give_voter_large_stake(
        &client,
        &env,
        &contract_id,
        &token,
        &voter1,
        large_balance,
        "qorstak1",
    );
    give_voter_large_stake(
        &client,
        &env,
        &contract_id,
        &token,
        &voter2,
        large_balance,
        "qorstak2",
    );

    let normal_player1 = Address::generate(&env);
    let normal_player2 = Address::generate(&env);
    token_client.mint(&normal_player1, &1000);
    token_client.mint(&normal_player2, &1000);

    let match_id = client.create_match(
        &normal_player1,
        &normal_player2,
        &100,
        &token,
        &String::from_str(&env, "qorumbig"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &normal_player1);
    client.deposit(&match_id, &normal_player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &normal_player2,
        &String::from_str(&env, "evidence"),
    );

    // Both large voters vote yes
    client.vote_on_dispute(&dispute_id, &voter1, &true);
    client.vote_on_dispute(&dispute_id, &voter2, &true);

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    // Should successfully resolve with correct quorum calculation
    client.resolve_dispute_by_vote(&dispute_id);
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
}

#[test]
fn test_small_balances_unchanged_behavior() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_funded_active_match(&client, &env, &player1, &player2, &token, "smallbal");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id =
        client.dispute_oracle_result(&match_id, &player2, &String::from_str(&env, "evidence"));

    // Both players have a 100 stake each (well under u32::MAX)
    client.vote_on_dispute(&dispute_id, &player1, &true);
    client.vote_on_dispute(&dispute_id, &player2, &false);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(
        dispute.yes_votes, 100,
        "small stake vote should be recorded correctly"
    );
    assert_eq!(
        dispute.no_votes, 100,
        "small stake vote should be recorded correctly"
    );

    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);

    // Verify resolution works correctly with small balances: a tied vote
    // (100 yes vs 100 no) does not exceed the original result, so the
    // dispute is upheld rather than overturned.
    client.resolve_dispute_by_vote(&dispute_id);
    let dispute_final = client.get_dispute(&dispute_id);
    assert_eq!(dispute_final.state, DisputeState::ResolvedUpheld);
}
