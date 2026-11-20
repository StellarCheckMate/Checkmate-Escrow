//! Tests for Issue #1940 — Enhancement: add a get_fee_tier_for_stake view.
//!
//! `calculate_fee_by_tier` returns the fee but not the matched tier, so UIs
//! cannot show users which tier bracket applies before they deposit.
//! `get_fee_tier_for_stake(stake)` returns the matched `FeeTier` directly,
//! using the same selection logic as `calculate_fee_by_tier`.

#![cfg(test)]
extern crate std;

use super::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_tiers(env: &Env) -> soroban_sdk::Vec<FeeTier> {
    let mut v = soroban_sdk::vec![env];
    v.push_back(FeeTier { max_stake: 100, fee_basis_points: 50 });
    v.push_back(FeeTier { max_stake: 500, fee_basis_points: 100 });
    v.push_back(FeeTier { max_stake: i128::MAX, fee_basis_points: 200 });
    v
}

// ── No tiers configured ───────────────────────────────────────────────────────

#[test]
fn test_get_fee_tier_for_stake_returns_none_when_no_tiers_configured() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(
        client.get_fee_tier_for_stake(&50),
        None,
        "must return None when no fee tiers are set"
    );
}

// ── Tier selection matches fee calculation ────────────────────────────────────

#[test]
fn test_get_fee_tier_for_stake_matches_fee_calculation_first_tier() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_fee_tiers(&make_tiers(&env));

    // stake=50 falls in the first tier (max_stake=100, bps=50).
    let tier = client
        .get_fee_tier_for_stake(&50)
        .expect("must return a tier for a configured schedule");

    assert_eq!(tier.max_stake, 100);
    assert_eq!(tier.fee_basis_points, 50);

    // The fee from calculate_fee_by_tier must match what the returned tier implies.
    // pot = 50 * 2 = 100; fee = 100 * 50 / 10_000 = 0 (integer truncation).
    let fee = client.calculate_fee_by_tier(&50).unwrap();
    let expected_fee = (50i128 * 2) * tier.fee_basis_points as i128 / 10_000;
    assert_eq!(fee, expected_fee);
}

#[test]
fn test_get_fee_tier_for_stake_matches_fee_calculation_second_tier() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_fee_tiers(&make_tiers(&env));

    // stake=250 falls in the second tier (max_stake=500, bps=100).
    let tier = client
        .get_fee_tier_for_stake(&250)
        .expect("must return a tier for stake within schedule");

    assert_eq!(tier.max_stake, 500);
    assert_eq!(tier.fee_basis_points, 100);

    let fee = client.calculate_fee_by_tier(&250).unwrap();
    let expected_fee = (250i128 * 2) * tier.fee_basis_points as i128 / 10_000;
    assert_eq!(fee, expected_fee);
}

#[test]
fn test_get_fee_tier_for_stake_uses_last_tier_as_catchall() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tiers with an explicit upper bound (no i128::MAX catch-all).
    let mut tiers = soroban_sdk::vec![&env];
    tiers.push_back(FeeTier { max_stake: 100, fee_basis_points: 50 });
    tiers.push_back(FeeTier { max_stake: 500, fee_basis_points: 100 });
    client.set_fee_tiers(&tiers);

    // stake=1_000 exceeds all explicit thresholds — last tier must be returned.
    let tier = client
        .get_fee_tier_for_stake(&1_000)
        .expect("must return the last tier as catch-all");

    assert_eq!(
        tier.max_stake, 500,
        "last tier is the catch-all for stakes exceeding all thresholds"
    );
    assert_eq!(tier.fee_basis_points, 100);

    // Verify the fee from calculate_fee_by_tier is consistent.
    let fee = client.calculate_fee_by_tier(&1_000).unwrap();
    let expected_fee = (1_000i128 * 2) * tier.fee_basis_points as i128 / 10_000;
    assert_eq!(fee, expected_fee);
}

#[test]
fn test_get_fee_tier_for_stake_at_tier_boundary_selects_lower_tier() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_fee_tiers(&make_tiers(&env));

    // stake exactly at max_stake=100 must select the first tier, not the second.
    let tier = client
        .get_fee_tier_for_stake(&100)
        .expect("stake equal to tier boundary must match that tier");

    assert_eq!(
        tier.max_stake, 100,
        "stake exactly at max_stake boundary must select that tier (inclusive)"
    );
    assert_eq!(tier.fee_basis_points, 50);
}
