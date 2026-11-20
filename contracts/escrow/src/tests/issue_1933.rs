/// #1933 — submit_draw emits a dedicated match/draw event naming the outcome.
use super::*;

/// Helper: create a match, have both players deposit, and return the match ID.
fn funded_match(
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

/// submit_draw must emit a `match/draw` event whose payload names the match ID
/// and `Winner::Draw` as the outcome.
#[test]
fn test_submit_draw_emits_draw_event() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = funded_match(&client, &env, &player1, &player2, &token, "draw1933a");

    client.submit_draw(&match_id, &oracle, &None);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "draw").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match/draw event not emitted by submit_draw");

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_winner): (u64, Winner) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id, "event match_id must match the settled match");
    assert_eq!(ev_winner, Winner::Draw, "event winner must be Winner::Draw");
}

/// submit_draw also emits the existing match/completed event (unchanged
/// behaviour) in addition to the new match/draw event.
#[test]
fn test_submit_draw_still_emits_completed_event() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = funded_match(&client, &env, &player1, &player2, &token, "draw1933b");

    client.submit_draw(&match_id, &oracle, &None);

    let events = env.events().all();
    let completed_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "completed").into_val(&env),
    ];
    let completed = events
        .iter()
        .find(|(_, topics, _)| *topics == completed_topics);
    assert!(
        completed.is_some(),
        "match/completed event must still be emitted by submit_draw"
    );
}

/// submit_result with Winner::Player1 must NOT emit a match/draw event.
#[test]
fn test_submit_result_winner_does_not_emit_draw_event() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = funded_match(&client, &env, &player1, &player2, &token, "draw1933c");
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let events = env.events().all();
    let draw_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        Symbol::new(&env, "draw").into_val(&env),
    ];
    let draw_event = events
        .iter()
        .find(|(_, topics, _)| *topics == draw_topics);
    assert!(
        draw_event.is_none(),
        "match/draw event must not be emitted for a non-draw result"
    );
}
