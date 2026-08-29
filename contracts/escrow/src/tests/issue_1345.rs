//! Tests for issue #1345: emit escrow/stablecoin_mode_changed event on toggle
use super::*;

fn make_config(env: &Env, admin: &Address, stablecoin_only_mode: bool) -> ProtocolConfig {
    ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
        dispute_bond_tier_schedule: soroban_sdk::vec![env],
    }
}

#[test]
fn test_stablecoin_mode_event_emitted_on_toggle() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Toggle stablecoin_only_mode from false → true
    client.set_protocol_config(&make_config(&env, &admin, true));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "escrow").into_val(&env),
        Symbol::new(&env, "stablecoin_mode").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "escrow/stablecoin_mode event must be emitted on toggle");

    // Verify payload is the new boolean value (true)
    let (_, _, data) = matched.unwrap();
    let emitted_val = bool::try_from_val(&env, &data).unwrap();
    assert!(emitted_val, "event payload must be the new stablecoin_only_mode value");
}

#[test]
fn test_stablecoin_mode_event_not_emitted_when_unchanged() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Set same value (false → false) — no event expected
    client.set_protocol_config(&make_config(&env, &admin, false));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "escrow").into_val(&env),
        Symbol::new(&env, "stablecoin_mode").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_none(), "no event must be emitted when stablecoin_only_mode is unchanged");
}

#[test]
fn test_stablecoin_mode_event_emitted_on_toggle_back() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_protocol_config(&make_config(&env, &admin, true));
    client.set_protocol_config(&make_config(&env, &admin, false));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "escrow").into_val(&env),
        Symbol::new(&env, "stablecoin_mode").into_val(&env),
    ];
    let count = events
        .iter()
        .filter(|(_, topics, _)| *topics == expected_topics)
        .count();
    assert_eq!(count, 2, "event must be emitted for each toggle");
}
