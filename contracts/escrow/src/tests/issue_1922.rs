//! Tests for issue #1922 — `pause_match` and `resume_match` event emission.
//!
//! Verifies that both functions emit their respective `match/paused` and
//! `match/resumed` events, and that the event payload includes both the
//! match ID and the caller address so off-chain indexers can track the full
//! pause/resume history.
#![cfg(test)]
extern crate std;

use super::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Create a funded (Active) match and return `(match_id, player1, player2)`.
fn setup_active_match(
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

// ── #1922: pause_match emits match/paused event ───────────────────────────────

/// `pause_match` must emit a `match/paused` event whose payload contains the
/// match ID and the caller address.
#[test]
fn test_pause_match_emits_paused_event_with_caller() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = setup_active_match(&client, &env, &player1, &player2, &token, "aa11bb22");

    client.pause_match(&match_id, &player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("paused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);

    assert!(matched.is_some(), "match/paused event must be emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_caller): (u64, Address) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id, "event match_id must match");
    assert_eq!(ev_caller, player1, "event caller must be player1 who paused");
}

/// Either player can pause; the event payload must reflect the actual caller.
#[test]
fn test_pause_match_emitted_by_player2_has_correct_caller() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = setup_active_match(&client, &env, &player1, &player2, &token, "cc33dd44");

    client.pause_match(&match_id, &player2);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("paused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);

    assert!(matched.is_some(), "match/paused event must be emitted when player2 pauses");

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_caller): (u64, Address) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id);
    assert_eq!(ev_caller, player2, "event caller must be player2 who paused");
}

// ── #1922: resume_match emits match/resumed event ────────────────────────────

/// `resume_match` must emit a `match/resumed` event whose payload contains the
/// match ID and the caller address.
#[test]
fn test_resume_match_emits_resumed_event_with_caller() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = setup_active_match(&client, &env, &player1, &player2, &token, "ee55ff66");

    client.pause_match(&match_id, &player1);
    client.resume_match(&match_id, &player2);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("resumed").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);

    assert!(matched.is_some(), "match/resumed event must be emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_match_id, ev_caller): (u64, Address) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_match_id, match_id, "event match_id must match");
    assert_eq!(ev_caller, player2, "event caller must be player2 who resumed");
}

/// A full pause-resume cycle must emit both events in order with correct callers.
#[test]
fn test_pause_then_resume_emits_both_events_in_order() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = setup_active_match(&client, &env, &player1, &player2, &token, "1a2b3c4d");

    client.pause_match(&match_id, &player1);
    client.resume_match(&match_id, &player2);

    let events = env.events().all();

    // Check paused event.
    let paused_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("paused").into_val(&env),
    ];
    let paused_ev = events
        .iter()
        .find(|(_, topics, _)| *topics == paused_topics);
    assert!(paused_ev.is_some(), "match/paused event must be present");
    let (_, _, pdata) = paused_ev.unwrap();
    let (pid, pcaller): (u64, Address) = TryFromVal::try_from_val(&env, &pdata).unwrap();
    assert_eq!(pid, match_id);
    assert_eq!(pcaller, player1);

    // Check resumed event.
    let resumed_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("resumed").into_val(&env),
    ];
    let resumed_ev = events
        .iter()
        .find(|(_, topics, _)| *topics == resumed_topics);
    assert!(resumed_ev.is_some(), "match/resumed event must be present");
    let (_, _, rdata) = resumed_ev.unwrap();
    let (rid, rcaller): (u64, Address) = TryFromVal::try_from_val(&env, &rdata).unwrap();
    assert_eq!(rid, match_id);
    assert_eq!(rcaller, player2);
}
