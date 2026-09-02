use super::*;
use soroban_sdk::testutils::Ledger as _;

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

// #1103 — create_match emits match/created event
#[test]
fn test_create_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "7773b359"),
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
fn test_deposit_emits_event_for_partial_funding() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "23013e94"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("deposit").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match deposit event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_player, ev_state): (u64, Address, Option<MatchState>) =
        <(u64, Address, Option<MatchState>)>::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
    assert_eq!(ev_player, player1);
    assert_eq!(ev_state, None);
}

#[test]
fn test_deposit_emits_event_with_state_when_match_activates() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "5ebcec73"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("deposit").into_val(&env),
    ];
    let matched = events
        .iter()
        .rev()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "match deposit event not emitted on activation"
    );

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_player, ev_state): (u64, Address, Option<MatchState>) =
        <(u64, Address, Option<MatchState>)>::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
    assert_eq!(ev_player, player2);
    assert_eq!(ev_state, Some(MatchState::Active));
}

#[test]
fn test_submit_result_emits_event() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "5f058e93"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);

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
    let decoded: (u64, Winner, i128) = <(u64, Winner, i128)>::try_from_val(&env, &data).unwrap();
    assert_eq!(decoded, (id, Winner::Player1, 200));
}

// #1104 — cancel_match emits match/cancelled event
#[test]
fn test_cancel_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "77d28b33"),
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
        &String::from_str(&env, "039d345a"),
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
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

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
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "e982d701"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.submit_result(&match_id, &Winner::Player1, &oracle);

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
    let (ev_match_id, ev_winner, ev_payout): (u64, Winner, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_winner, Winner::Player1);
    assert_eq!(ev_payout, 200);
}

// #1161 — submit_result emits match/completed with the payout amount
#[test]
fn test_submit_result_event_includes_payout_amount() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stake_amount = 100;
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake_amount,
        &token,
        &String::from_str(&env, "87187534"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player2, &oracle);

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
    let (ev_match_id, ev_winner, ev_payout_amount): (u64, Winner, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_winner, Winner::Player2);
    assert_eq!(ev_payout_amount, stake_amount * 2);
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
        &String::from_str(&env, "4edf4769"),
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
    let (ev_match_id, ev_player, ev_state): (u64, Address, Option<MatchState>) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_player, player1);
    assert_eq!(ev_state, None);
}

#[test]
fn test_deposit_emits_event_for_player2_and_includes_final_state() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "8c5d122d"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let events = env.events().all();
    let deposit_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("deposit").into_val(&env),
    ];
    let deposit_events: Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| *topics == deposit_topics)
        .collect();
    assert_eq!(
        deposit_events.len(),
        2,
        "two deposit events should be emitted"
    );

    let (_, _, data) = deposit_events[1];
    let (ev_match_id, ev_player, _ev_state): (u64, Address, Option<MatchState>) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_player, player2);

    let activated_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("activated").into_val(&env),
    ];
    let activated_matched = events
        .iter()
        .find(|(_, topics, _)| *topics == activated_topics);
    assert!(
        activated_matched.is_some(),
        "activated event should be emitted after player2 deposits"
    );
}

#[test]
fn test_set_match_timeout_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let old_timeout = DEFAULT_MATCH_TIMEOUT_SECONDS;
    let new_timeout = 5_184_000u64;

    client.set_match_timeout(&new_timeout);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("timeout").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "timeout event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_old, ev_new): (u64, u64) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_old, old_timeout);
    assert_eq!(ev_new, new_timeout);
}

#[test]
fn test_remove_allowed_token_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_allowed_token(&token);
    client.remove_allowed_token(&token);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("tok_rm").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "token_remove event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_token, ev_caller): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_token, token);
    assert_eq!(ev_caller, admin, "event payload must include the caller address");
}

/// Regression test for the admin audit-trail requirement: every admin
/// mutation must include the calling admin's address in its event payload,
/// not just the mutated resource.
#[test]
fn test_admin_events_include_caller_address() {
    let (env, contract_id, _oracle, _player1, _player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_allowed_token(&token);
    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("token_add").into_val(&env),
    ];
    let (_, _, data) = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics)
        .expect("token_add event not emitted");
    let (ev_token, ev_caller): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_token, token);
    assert_eq!(ev_caller, admin, "token_add event must include caller address");
}

#[test]
fn test_expire_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Use the minimum timeout and create the match at a known ledger so the
    // advance below is guaranteed to clear the expiration threshold.
    client.set_match_timeout(&MIN_MATCH_TIMEOUT_SECONDS);
    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "29f0809d"),
        &Platform::Lichess,
    );

    // Advance ledger past the configured timeout (created at ledger 100).
    env.ledger().set_sequence_number(100 + 17_280 + 1);
    client.expire_match(&id);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("expired").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match/expired event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
}

#[test]
fn test_pause_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("paused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "admin/paused event not emitted");
}

#[test]
fn test_unpause_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Pause first so unpause is valid
    client.pause(&admin);
    client.unpause(&admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("unpaused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "admin/unpaused event not emitted");
}

#[test]
fn test_transfer_admin_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin, &admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("xfer").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "admin/xfer event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_old_admin, ev_new_admin): (Address, Address) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_old_admin, admin, "old admin must match current admin");
    assert_eq!(
        ev_new_admin, new_admin,
        "new admin must match provided address"
    );
}
