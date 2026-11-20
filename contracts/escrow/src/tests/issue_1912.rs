//! Tests for Issue #1912 — Security: dispute_and_rollback_match does not emit
//! an event naming the rollback initiator.
//!
//! Without the initiator in the event payload, rollbacks are not attributable
//! for audit. The fix emits a `("match", "rollback")` event with
//! `(match_id, disputer, reason)` once both players have consented and the
//! actual rollback executes.

use super::*;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create an active match (both players deposited) and return the match id.
fn create_active_match(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> u64 {
    let match_id = client.create_match(
        player1,
        player2,
        &100,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&match_id, player1);
    client.deposit(&match_id, player2);
    match_id
}

fn rollback_topics(env: &Env) -> soroban_sdk::Vec<soroban_sdk::Val> {
    vec![
        env,
        Symbol::new(env, "match").into_val(env),
        symbol_short!("rollback").into_val(env),
    ]
}

// ── Event-emission tests ──────────────────────────────────────────────────────

/// When both players consent to a rollback, a `("match", "rollback")` event
/// must be emitted and its payload must include the final caller (the second
/// voter, since they trigger the actual refund).
#[test]
fn test_dispute_and_rollback_emits_event_with_initiator() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_active_match(&client, &env, &player1, &player2, &token, "1912aa01");
    let reason = String::from_str(&env, "opponent disconnected");

    // Player1 votes first — no rollback yet, only a partial-vote event.
    client.dispute_and_rollback_match(&match_id, &player1, &reason);

    // Player2 consents — this triggers the actual rollback and the rollback event.
    client.dispute_and_rollback_match(&match_id, &player2, &reason);

    // Check that a "match"/"rollback" event was published.
    let expected_topics = rollback_topics(&env);
    let events = env.events().all();
    let rollback_event_found = events.iter().any(|(_, topics, _)| *topics == expected_topics);

    assert!(
        rollback_event_found,
        "a (\"match\", \"rollback\") event must be emitted when both players consent to rollback"
    );
}

/// The rollback event data must contain the match_id, the second voter
/// (disputer who finalises the rollback), and the reason string.
#[test]
fn test_dispute_and_rollback_event_contains_match_id_and_disputer() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_active_match(&client, &env, &player1, &player2, &token, "1912bb01");
    let reason = String::from_str(&env, "claim disconnect");

    // Player1 votes first.
    client.dispute_and_rollback_match(&match_id, &player1, &reason);

    // Player2 finalises the rollback.
    client.dispute_and_rollback_match(&match_id, &player2, &reason);

    // Find the rollback event and verify its payload.
    let expected_topics = rollback_topics(&env);
    let events = env.events().all();
    let rollback_event = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);

    assert!(
        rollback_event.is_some(),
        "rollback event must be emitted after both players consent"
    );

    // Decode the event data: (match_id: u64, disputer: Address, reason: String)
    let (_, _, data_val) = rollback_event.unwrap();
    let data: (u64, Address, String) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data_val)
            .expect("rollback event data must decode to (u64, Address, String)");

    assert_eq!(data.0, match_id, "event match_id must match the rolled-back match");
    assert_eq!(
        data.1, player2,
        "event disputer must be player2 — the caller who finalised the rollback"
    );
}

/// Partial rollback vote (only one player) must NOT emit the final rollback
/// event — only an intermediate `("match", "rb_vote")` event.
#[test]
fn test_dispute_and_rollback_partial_vote_does_not_emit_rollback_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_active_match(&client, &env, &player1, &player2, &token, "1912cc01");
    let reason = String::from_str(&env, "partial vote");

    // Only player1 votes — no rollback should happen yet.
    client.dispute_and_rollback_match(&match_id, &player1, &reason);

    let expected_topics = rollback_topics(&env);
    let events = env.events().all();
    let rollback_event_found = events.iter().any(|(_, topics, _)| *topics == expected_topics);

    assert!(
        !rollback_event_found,
        "rollback event must NOT be emitted after only one player votes"
    );
}
