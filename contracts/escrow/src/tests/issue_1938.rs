//! Tests for Issue #1938 — Enhancement: emit an event when set_maximum_stake changes.
//!
//! `set_maximum_stake` previously changed config silently, so indexers had to
//! poll `get_protocol_config` to detect stake-cap drift. The fix emits an
//! `admin/max_stake` event carrying the old and new maximum stake values.

#![cfg(test)]
extern crate std;

use super::*;

// ── Event is emitted ─────────────────────────────────────────────────────────

#[test]
fn test_set_maximum_stake_emits_event_with_old_and_new_values() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No cap initially; set a cap of 500.
    client.set_maximum_stake(&Some(500));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("max_stake").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "admin/max_stake event must be emitted");

    let (_, _, data) = matched.unwrap();
    let (old_val, new_val): (Option<i128>, Option<i128>) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    assert_eq!(
        old_val, None,
        "old maximum stake must be None (no cap was previously set)"
    );
    assert_eq!(
        new_val,
        Some(500),
        "new maximum stake must reflect the value just set"
    );
}

#[test]
fn test_set_maximum_stake_event_records_previous_cap_when_changing_cap() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Establish an initial cap so the second call has a non-None old value.
    client.set_maximum_stake(&Some(200));

    // Clear events before the second call so we isolate its event.
    let _ = env.events().all();

    client.set_maximum_stake(&Some(1_000));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("max_stake").into_val(&env),
    ];
    // Collect all max_stake events (may include the first call if events were
    // not cleared; take the last one for the second call).
    let matched: soroban_sdk::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| *topics == expected_topics)
        .collect();

    let (_, _, data) = matched.last().expect("at least one max_stake event must exist");
    let (old_val, new_val): (Option<i128>, Option<i128>) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    assert_eq!(
        old_val,
        Some(200),
        "old maximum stake must be the previously-set cap (200)"
    );
    assert_eq!(new_val, Some(1_000), "new maximum stake must be 1_000");
}

#[test]
fn test_set_maximum_stake_event_records_none_when_removing_cap() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_maximum_stake(&Some(300));
    client.set_maximum_stake(&None);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("max_stake").into_val(&env),
    ];
    let matched: soroban_sdk::Vec<_> = events
        .iter()
        .filter(|(_, topics, _)| *topics == expected_topics)
        .collect();

    let (_, _, data) = matched.last().unwrap();
    let (old_val, new_val): (Option<i128>, Option<i128>) =
        TryFromVal::try_from_val(&env, &data).unwrap();

    assert_eq!(
        old_val,
        Some(300),
        "old maximum stake must be 300 (previously set cap)"
    );
    assert_eq!(
        new_val, None,
        "new maximum stake must be None (cap was removed)"
    );
}
