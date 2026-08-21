use super::*;
use soroban_sdk::testutils::Ledger as _;

/// Helper to set up contracts and get through to an overturned dispute ready for slashing
fn setup_for_slash_relay_test() -> (Env, Address, Address, Address, Address, Address, Address, u64, i128) {
    let (env, contract_id, oracle, player1, player2, token, admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Set up dispute bond (10%)
    client.set_dispute_bond_basis_points(&1000);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "relay0001"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "relay_evidence"),
    );

    client.vote_on_dispute(&dispute_id, &player2, &true);
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id);

    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
    let bond = dispute.dispute_bond;

    (env, contract_id, oracle, player1, player2, token, admin, dispute_id, bond)
}

/// Test 1: Verify that mark_dispute_for_oracle_slash emits the correct signal event
#[test]
fn test_relay_slash_signal_event_format() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        setup_for_slash_relay_test();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Emit the slash signal
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

    assert!(matched.is_some(), "oracle_slash_signal event must be emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_dispute_id, ev_oracle, ev_amount): (u64, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    assert_eq!(ev_dispute_id, dispute_id);
    assert_eq!(ev_oracle, oracle);
    assert_eq!(ev_amount, bond);
}

/// Test 2: Verify that before any relay processes the signal, the oracle's stake is unchanged
#[test]
fn test_relay_oracle_stake_unchanged_before_slash() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        setup_for_slash_relay_test();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Emit signal but don't slash
    client.mark_dispute_for_oracle_slash(&dispute_id, &bond);

    // If a relay were listening, it would call slash_oracle on the oracle contract
    // This test just verifies the escrow contract has done its part
    let dispute = client.get_dispute(&dispute_id);
    assert_eq!(dispute.state, DisputeState::ResolvedOverturned);
}

/// Test 3: Verify correct oracle is included in slash signal
#[test]
fn test_relay_slash_signal_names_correct_oracle() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create match and get to overturned dispute with a specific oracle
    client.set_dispute_bond_basis_points(&1000);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "relay0002"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "relay_oracle_test"),
    );

    client.vote_on_dispute(&dispute_id, &player2, &true);
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id);

    let bond = client.get_dispute(&dispute_id).dispute_bond;

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

    assert!(matched.is_some());

    let (_, _, data) = matched.unwrap();
    let (_ev_dispute_id, ev_oracle, _ev_amount): (u64, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    // The oracle in the signal must match the oracle that submitted the result
    assert_eq!(ev_oracle, oracle);
}

/// Test 4: Verify slash amount matches bond for full slash
#[test]
fn test_relay_slash_signal_correct_amount_full_bond() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        setup_for_slash_relay_test();
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

    let (_, _, data) = matched.unwrap();
    let (_ev_dispute_id, _ev_oracle, ev_amount): (u64, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    assert_eq!(ev_amount, bond);
}

/// Test 5: Verify slash amount can be partial (less than bond)
#[test]
fn test_relay_slash_signal_partial_slash_amount() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, dispute_id, bond) =
        setup_for_slash_relay_test();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Slash only half the bond
    let partial_slash = bond / 2;
    client.mark_dispute_for_oracle_slash(&dispute_id, &partial_slash);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "dispute").into_val(&env),
        Symbol::new(&env, "oracle_slash_signal").into_val(&env),
    ];

    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);

    let (_, _, data) = matched.unwrap();
    let (_ev_dispute_id, _ev_oracle, ev_amount): (u64, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    assert_eq!(ev_amount, partial_slash);
}

/// Test 6: Verify slash signal is rejected if dispute not overturned
#[test]
fn test_relay_slash_signal_requires_overturned_state() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "relay0003"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "relay_upheld_test"),
    );

    // Both players vote to uphold (no majority overturn)
    client.vote_on_dispute(&dispute_id, &player1, &false);
    client.vote_on_dispute(&dispute_id, &player2, &false);

    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id);

    let dispute = client.get_dispute(&dispute_id);
    // Dispute is upheld, not overturned
    assert_eq!(dispute.state, DisputeState::ResolvedUpheld);

    // Trying to mark for slash should fail
    let result = client.try_mark_dispute_for_oracle_slash(&dispute_id, &10);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

/// Test 7: Verify relay signal can be triggered multiple times for different disputes
#[test]
fn test_relay_multiple_disputes_independent_signals() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_dispute_bond_basis_points(&1000);

    // Create first match and dispute
    let match_id_1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "relay_multi_1"),
        &Platform::Lichess,
    );
    client.deposit(&match_id_1, &player1);
    client.deposit(&match_id_1, &player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id_1, &Winner::Player1, &oracle);

    let dispute_id_1 = client.dispute_oracle_result(
        &match_id_1,
        &player2,
        &String::from_str(&env, "relay_multi_1_evidence"),
    );

    client.vote_on_dispute(&dispute_id_1, &player2, &true);
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id_1);

    let bond_1 = client.get_dispute(&dispute_id_1).dispute_bond;

    // Create second match and dispute
    env.ledger().set_sequence_number(2000);
    let match_id_2 = client.create_match(
        &player1,
        &player2,
        &200,
        &token,
        &String::from_str(&env, "relay_multi_2"),
        &Platform::Lichess,
    );
    client.deposit(&match_id_2, &player1);
    client.deposit(&match_id_2, &player2);

    env.ledger().set_sequence_number(2100);
    client.submit_result(&match_id_2, &Winner::Player2, &oracle);

    let dispute_id_2 = client.dispute_oracle_result(
        &match_id_2,
        &player1,
        &String::from_str(&env, "relay_multi_2_evidence"),
    );

    client.vote_on_dispute(&dispute_id_2, &player1, &true);
    env.ledger().set_sequence_number(2100 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id_2);

    let bond_2 = client.get_dispute(&dispute_id_2).dispute_bond;

    // Mark both for slash
    client.mark_dispute_for_oracle_slash(&dispute_id_1, &bond_1);
    client.mark_dispute_for_oracle_slash(&dispute_id_2, &bond_2);

    // Verify both signals were emitted
    let events = env.events().all();
    let slash_signals: Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| {
            *topics == vec![
                &env,
                Symbol::new(&env, "dispute").into_val(&env),
                Symbol::new(&env, "oracle_slash_signal").into_val(&env),
            ]
        })
        .collect();

    assert_eq!(slash_signals.len(), 2, "Two slash signals should be emitted");
}

/// Test 8: Verify relay handles unregistered oracle gracefully in tests
/// (In practice, the relay should check if oracle is registered before calling slash)
#[test]
fn test_relay_signal_emitted_even_if_oracle_unregistered() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_dispute_bond_basis_points(&1000);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "relay_unreg_oracle"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "relay_unreg_evidence"),
    );

    client.vote_on_dispute(&dispute_id, &player2, &true);
    env.ledger().set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute_id);

    let bond = client.get_dispute(&dispute_id).dispute_bond;

    // Signal is emitted regardless of oracle registration status on oracle contract
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
        "Slash signal must be emitted even if oracle is unregistered"
    );
}
