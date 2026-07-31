#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::Address as _;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn tier(env: &Env, max_stake: i128, bps: u32) -> FeeTier {
    FeeTier {
        max_stake,
        fee_basis_points: bps,
    }
}

fn tiers_vec(env: &Env, items: &[(i128, u32)]) -> soroban_sdk::Vec<FeeTier> {
    let mut v = soroban_sdk::vec![env];
    for (max, bps) in items {
        v.push_back(tier(env, *max, *bps));
    }
    v
}

// ── get_fee_tiers default ─────────────────────────────────────────────────────

#[test]
fn test_get_fee_tiers_empty_by_default() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_fee_tiers().len(), 0, "no fee tiers configured by default");
}

// ── set_fee_tiers ─────────────────────────────────────────────────────────────

#[test]
fn test_set_fee_tiers_requires_admin_auth() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.set_auths(&[]);
    let t = tiers_vec(&env, &[(100, 50)]);
    let result = client.try_set_fee_tiers(&t);
    assert!(result.is_err(), "non-admin must be rejected");
}

#[test]
fn test_set_fee_tiers_stores_and_retrieves_tiers() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let t = tiers_vec(&env, &[(100, 50), (500, 100), (i128::MAX, 200)]);
    client.set_fee_tiers(&t);

    let stored = client.get_fee_tiers();
    assert_eq!(stored.len(), 3);
    assert_eq!(stored.get(0).unwrap().max_stake, 100);
    assert_eq!(stored.get(0).unwrap().fee_basis_points, 50);
    assert_eq!(stored.get(1).unwrap().max_stake, 500);
    assert_eq!(stored.get(2).unwrap().fee_basis_points, 200);
}

#[test]
fn test_set_fee_tiers_rejects_non_ascending_order() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Descending order — must be rejected.
    let t = tiers_vec(&env, &[(500, 100), (100, 50)]);
    let result = client.try_set_fee_tiers(&t);
    assert!(result.is_err(), "non-ascending tiers must be rejected");
}

#[test]
fn test_set_fee_tiers_rejects_duplicate_max_stake() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let t = tiers_vec(&env, &[(100, 50), (100, 100)]);
    let result = client.try_set_fee_tiers(&t);
    assert!(result.is_err(), "duplicate max_stake must be rejected");
}

#[test]
fn test_set_fee_tiers_empty_clears_schedule() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Set then clear.
    let t = tiers_vec(&env, &[(100, 50)]);
    client.set_fee_tiers(&t);
    let empty: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
    client.set_fee_tiers(&empty);

    assert_eq!(client.get_fee_tiers().len(), 0, "tiers must be cleared");
}

#[test]
fn test_set_fee_tiers_emits_event() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let t = tiers_vec(&env, &[(100, 50)]);
    client.set_fee_tiers(&t);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        Symbol::new(&env, "fee_tiers_set").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "fee_tiers_set event must be emitted");
}

// ── calculate_fee_by_tier ─────────────────────────────────────────────────────

#[test]
fn test_fee_zero_when_no_tiers_configured() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.calculate_fee_by_tier(&100), 0, "no tiers → no fee");
}

#[test]
fn test_fee_first_tier_applies_for_small_stake() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tier 1: stake ≤ 100 → 50 bps  (0.5%)
    // Tier 2: stake ≤ MAX → 200 bps (2.0%)
    let t = tiers_vec(&env, &[(100, 50), (i128::MAX, 200)]);
    client.set_fee_tiers(&t);

    // stake=50 → pot=100 → fee = 100 * 50 / 10_000 = 0 (rounds down)
    assert_eq!(client.calculate_fee_by_tier(&50), 0);

    // stake=100 → pot=200 → fee = 200 * 50 / 10_000 = 1
    assert_eq!(client.calculate_fee_by_tier(&100), 1);
}

#[test]
fn test_fee_second_tier_applies_for_larger_stake() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tier 1: ≤ 100 → 50 bps
    // Tier 2: ≤ MAX → 200 bps
    let t = tiers_vec(&env, &[(100, 50), (i128::MAX, 200)]);
    client.set_fee_tiers(&t);

    // stake=101 → pot=202 → fee = 202 * 200 / 10_000 = 4
    assert_eq!(client.calculate_fee_by_tier(&101), 4);
}

#[test]
fn test_fee_boundary_inclusive_on_max_stake() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tier 1: stake ≤ 500 → 100 bps (1%)
    // Tier 2: stake ≤ MAX → 300 bps (3%)
    let t = tiers_vec(&env, &[(500, 100), (i128::MAX, 300)]);
    client.set_fee_tiers(&t);

    // stake=500 is inclusive in Tier 1 → pot=1000 → fee = 1000 * 100 / 10_000 = 10
    assert_eq!(client.calculate_fee_by_tier(&500), 10);

    // stake=501 falls into Tier 2 → pot=1002 → fee = 1002 * 300 / 10_000 = 30
    assert_eq!(client.calculate_fee_by_tier(&501), 30);
}

#[test]
fn test_fee_fallback_to_last_tier_when_stake_exceeds_all_explicit_tiers() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No open-ended final tier — last tier max_stake = 100.
    let t = tiers_vec(&env, &[(100, 50)]);
    client.set_fee_tiers(&t);

    // stake=999 exceeds the only tier → falls back to last → pot=1998 → fee=9
    assert_eq!(client.calculate_fee_by_tier(&999), 9);
}

#[test]
fn test_fee_three_tier_schedule() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Low:    ≤ 100  → 0 bps  (free for small games)
    // Mid:    ≤ 1000 → 100 bps (1%)
    // High:   ≤ MAX  → 200 bps (2%)
    let t = tiers_vec(&env, &[(100, 0), (1000, 100), (i128::MAX, 200)]);
    client.set_fee_tiers(&t);

    // stake=50  → pot=100  → 0 fee
    assert_eq!(client.calculate_fee_by_tier(&50), 0);

    // stake=100 → pot=200  → 0 fee (inclusive low tier)
    assert_eq!(client.calculate_fee_by_tier(&100), 0);

    // stake=101 → pot=202  → fee=2 (1%)
    assert_eq!(client.calculate_fee_by_tier(&101), 2);

    // stake=1000 → pot=2000 → fee=20 (1%)
    assert_eq!(client.calculate_fee_by_tier(&1000), 20);

    // stake=1001 → pot=2002 → fee=40 (2%)
    assert_eq!(client.calculate_fee_by_tier(&1001), 40);
}

#[test]
fn test_fee_single_tier_applies_universally() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Flat 1% for everything.
    let t = tiers_vec(&env, &[(i128::MAX, 100)]);
    client.set_fee_tiers(&t);

    // stake=100 → pot=200 → fee=2
    assert_eq!(client.calculate_fee_by_tier(&100), 2);
    // stake=10_000 → pot=20_000 → fee=200
    assert_eq!(client.calculate_fee_by_tier(&10_000), 200);
}

#[test]
fn test_fee_update_takes_effect_immediately() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Initially 1% flat.
    let t1 = tiers_vec(&env, &[(i128::MAX, 100)]);
    client.set_fee_tiers(&t1);
    assert_eq!(client.calculate_fee_by_tier(&1000), 20);

    // Update to 2% flat.
    let t2 = tiers_vec(&env, &[(i128::MAX, 200)]);
    client.set_fee_tiers(&t2);
    assert_eq!(client.calculate_fee_by_tier(&1000), 40);
}
