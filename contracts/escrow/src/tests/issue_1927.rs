//! Tests for issue #1927: `update_heartbeat` and `heartbeat_match` share one
//! internal implementation and must update the same liveness field.
//!
//! Both public entrypoints are thin wrappers over the single internal
//! `apply_heartbeat` implementation. The assertions below verify that the two
//! entrypoints write the same `Match::last_heartbeat` field — regardless of
//! which wrapper is used or which player invokes it — so they cannot drift.
use super::*;

/// Create and fully fund a match so it lands in `Active` (the only state that
/// accepts a heartbeat) and return its id.
fn active_match(
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

#[test]
fn test_both_heartbeat_entrypoints_update_same_liveness_field() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Two identical Active matches.
    let match_a = active_match(&client, &env, &player1, &player2, &token, "aabb1122");
    let match_b = active_match(&client, &env, &player1, &player2, &token, "bbcc3344");

    // Heartbeat match_a via `update_heartbeat` and match_b via `heartbeat_match`
    // from opposite players at the same ledger timestamp.
    env.ledger().with_mut(|l| l.timestamp = 12_345);
    client.update_heartbeat(&match_a, &player1);
    client.heartbeat_match(&match_b, &player2);

    let ma = client.get_match(&match_a);
    let mb = client.get_match(&match_b);
    assert_eq!(ma.last_heartbeat, 12_345, "update_heartbeat must set last_heartbeat");
    assert_eq!(mb.last_heartbeat, 12_345, "heartbeat_match must set last_heartbeat");
    assert_eq!(
        ma.last_heartbeat, mb.last_heartbeat,
        "both entrypoints must write the same liveness field"
    );

    // Drive each match through the *other* entrypoint to prove the field is
    // shared regardless of which wrapper is used.
    env.ledger().with_mut(|l| l.timestamp = 67_890);
    client.heartbeat_match(&match_a, &player2);
    client.update_heartbeat(&match_b, &player1);

    let ma = client.get_match(&match_a);
    let mb = client.get_match(&match_b);
    assert_eq!(ma.last_heartbeat, 67_890);
    assert_eq!(mb.last_heartbeat, 67_890);
    assert_eq!(ma.last_heartbeat, mb.last_heartbeat);
}

#[test]
fn test_both_heartbeat_entrypoints_emit_same_event_and_reject_non_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = active_match(&client, &env, &player1, &player2, &token, "ccdd5566");
    let stranger = Address::generate(&env);

    // Both entrypoints must reject a non-player identically (same underlying
    // implementation), otherwise one wrapper could drift to accept strangers.
    let via_update = client.try_update_heartbeat(&match_id, &stranger);
    let via_match = client.try_heartbeat_match(&match_id, &stranger);
    assert!(via_update.is_err(), "update_heartbeat must reject non-player");
    assert!(via_match.is_err(), "heartbeat_match must reject non-player");

    // Both entrypoints must emit the same ("match", "heartbeat") event.
    env.ledger().with_mut(|l| l.timestamp = 9_999);
    client.update_heartbeat(&match_id, &player1);
    client.heartbeat_match(&match_id, &player2);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("heartbeat").into_val(&env),
    ];
    let heartbeats: usize = events
        .iter()
        .filter(|(_, topics, _)| *topics == expected_topics)
        .count();
    assert_eq!(heartbeats, 2, "both entrypoints must emit the heartbeat event");
}
