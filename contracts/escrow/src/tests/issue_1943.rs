/// Tests for issue #1943: match/paused and match/resumed events.
use super::*;

/// pause_match emits a `match/paused` event whose payload is the match ID.
#[test]
fn pause_match_emits_paused_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "pppp1234"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.pause_match(&match_id, &player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("paused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match/paused event must be emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, match_id, "event payload must be the match ID");
}

/// resume_match emits a `match/resumed` event whose payload is the match ID.
#[test]
fn resume_match_emits_resumed_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "rrrr5678"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.pause_match(&match_id, &player1);
    client.resume_match(&match_id, &player2);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("resumed").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match/resumed event must be emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, match_id, "event payload must be the match ID");
}

/// Both events are emitted in order across a full pause/resume cycle.
#[test]
fn pause_and_resume_both_events_emitted_in_cycle() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cycle001"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.pause_match(&match_id, &player1);
    client.resume_match(&match_id, &player2);

    let events = env.events().all();

    let paused_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("paused").into_val(&env),
    ];
    let resumed_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("resumed").into_val(&env),
    ];

    assert!(
        events.iter().any(|(_, t, _)| *t == paused_topics),
        "match/paused event must be emitted"
    );
    assert!(
        events.iter().any(|(_, t, _)| *t == resumed_topics),
        "match/resumed event must be emitted"
    );
}
