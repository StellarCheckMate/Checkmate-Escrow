#![cfg(test)]
extern crate std;

use super::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn tier(_env: &Env, max_stake: i128, bps: u32) -> FeeTier {
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

/// Realistic 7-decimal token amounts expressed in base units (stroops).
/// 1 XLM = 10_000_000 stroops.
const ONE_XLM: i128 = 10_000_000;

// ── get_fee_tiers default ─────────────────────────────────────────────────────

#[test]
fn test_get_fee_tiers_empty_by_default() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(
        client.get_fee_tiers().len(),
        0,
        "no fee tiers configured by default"
    );
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
fn test_set_fee_tiers_rejects_fee_basis_points_over_10000() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 10_001 bps (> 100%) must be rejected — a tier fee can never exceed
    // the total pot.
    let t = tiers_vec(&env, &[(i128::MAX, 10_001)]);
    let result = client.try_set_fee_tiers(&t);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "fee_basis_points > 10_000 must be rejected"
    );

    // Exactly 10_000 bps (100%) is the allowed boundary.
    let t_ok = tiers_vec(&env, &[(i128::MAX, 10_000)]);
    assert!(client.try_set_fee_tiers(&t_ok).is_ok());
}

#[test]
fn test_set_fee_tiers_rejects_more_than_max_tiers() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Build 21 ascending tiers — one over the 20-tier cap.
    let mut items: std::vec::Vec<(i128, u32)> = std::vec::Vec::new();
    for i in 0..21i128 {
        items.push(((i + 1) * 100, 50));
    }
    let t = tiers_vec(&env, &items);
    let result = client.try_set_fee_tiers(&t);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "more than 20 fee tiers must be rejected"
    );

    // Exactly 20 tiers is the allowed boundary.
    let mut ok_items: std::vec::Vec<(i128, u32)> = std::vec::Vec::new();
    for i in 0..20i128 {
        ok_items.push(((i + 1) * 100, 50));
    }
    let t_ok = tiers_vec(&env, &ok_items);
    assert!(client.try_set_fee_tiers(&t_ok).is_ok());
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
    let (_, _, data) = matched.expect("fee_tiers_set event must be emitted");
    let count: u32 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(
        count, t.len(),
        "fee_tiers_set event must carry the number of tiers set"
    );
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

    // stake=101 → pot=202  → 202 * 100 / 10_000 = 2
    assert_eq!(client.calculate_fee_by_tier(&101), 2);

    // stake=1000 → pot=2000 → 2000 * 100 / 10_000 = 20
    assert_eq!(client.calculate_fee_by_tier(&1000), 20);

    // stake=1001 → pot=2002 → 2002 * 200 / 10_000 = 40
    assert_eq!(client.calculate_fee_by_tier(&1001), 40);
}

#[test]
fn test_fee_uses_large_realistic_amounts() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tier 1: ≤ 100 XLM → 100 bps (1%)
    // Tier 2: ≤ MAX     → 250 bps (2.5%)
    let t = tiers_vec(&env, &[(100 * ONE_XLM, 100), (i128::MAX, 250)]);
    client.set_fee_tiers(&t);

    // stake=100 XLM → pot=200 XLM → fee = 200 XLM * 1% = 2 XLM
    assert_eq!(client.calculate_fee_by_tier(&(100 * ONE_XLM)), 2 * ONE_XLM);

    // stake=200 XLM → pot=400 XLM → fee = 400 XLM * 2.5% = 10 XLM
    assert_eq!(client.calculate_fee_by_tier(&(200 * ONE_XLM)), 10 * ONE_XLM);
}
