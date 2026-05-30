use super::*;
use soroban_sdk::testutils::Address as _;

#[test]
fn test_initialize_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "escrow").into_val(&env),
        symbol_short!("init").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "escrow initialized event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_oracle, ev_admin): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_oracle, oracle);
    assert_eq!(ev_admin, admin);
}

#[test]
fn test_create_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_ev2"),
        &Platform::Lichess,
    );

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("created").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match created event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_p1, ev_p2, ev_stake): (u64, Address, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
    assert_eq!(ev_p1, player1);
    assert_eq!(ev_p2, player2);
    assert_eq!(ev_stake, 100);
}

#[test]
fn test_submit_result_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_evt"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("completed").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match completed event not emitted");

    let (_, _, data) = matched.unwrap();
    let decoded: (u64, Winner) = <(u64, Winner)>::try_from_val(&env, &data).unwrap();
    assert_eq!(decoded, (id, Winner::Player1));
}

#[test]
fn test_cancel_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_cancel"),
        &Platform::Lichess,
    );

    client.cancel_match(&id, &player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("cancelled").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match cancelled event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
}

#[test]
fn test_cancel_match_no_deposits_emits_no_token_transfers() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_no_deposit_cancel"),
        &Platform::Lichess,
    );

    client.cancel_match(&id, &player1);

    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    let transfer_topic: soroban_sdk::Val = soroban_sdk::symbol_short!("transfer").into_val(&env);
    let has_transfer = env
        .events()
        .all()
        .iter()
        .any(|(_, topics, _)| topics.contains(transfer_topic));
    assert!(
        !has_transfer,
        "no token transfer events should be emitted when no deposits were made"
    );
}

#[test]
fn test_pause_emits_paused_event() {
    let (env, contract_id, ..) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("paused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "paused event not emitted");
}

#[test]
fn test_update_oracle_emits_oracle_up_event_with_addresses() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_oracle = Address::generate(&env);
    let old_oracle: Address = client.get_oracle();

    client.update_oracle(&new_oracle);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        soroban_sdk::symbol_short!("oracle_up").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "oracle_up event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_old, ev_new): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_old, old_oracle);
    assert_eq!(ev_new, new_oracle);
}

#[test]
fn test_submit_result_emits_completed_event_with_correct_winner() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "event_test"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.submit_result(&match_id, &Winner::Player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("completed").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match completed event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_winner): (u64, Winner) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_winner, Winner::Player1);
}

#[test]
fn test_deposit_emits_event_for_player1() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "deposit_p1_event"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("deposit").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "deposit event not emitted for player1");

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_player): (u64, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_player, player1);
}
