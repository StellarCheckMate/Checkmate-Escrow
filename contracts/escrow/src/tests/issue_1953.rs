//! Tests for Issue #1953 — set_fee_tiers rejects tiers whose stake ranges overlap
//!
//! The overlapping-range guard in `set_fee_tiers` requires that each tier's
//! `max_stake` is strictly greater than the previous tier's `max_stake`.
//! Duplicate or out-of-order values create overlapping ranges and must be
//! rejected.

use super::*;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_tiers(env: &Env, items: &[(i128, u32)]) -> soroban_sdk::Vec<FeeTier> {
    let mut v = soroban_sdk::vec![env];
    for (max_stake, bps) in items {
        v.push_back(FeeTier {
            max_stake: *max_stake,
            fee_basis_points: *bps,
        });
    }
    v
}

// ── Overlapping ranges are rejected ──────────────────────────────────────────

/// Two tiers with the same `max_stake` produce overlapping ranges and must be
/// rejected with `InvalidAmount`.
#[test]
fn test_set_fee_tiers_rejects_tiers_with_duplicate_max_stake() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Both tiers share max_stake = 500 — their ranges [0, 500] overlap.
    let tiers = make_tiers(&env, &[(500, 100), (500, 200)]);
    let result = client.try_set_fee_tiers(&tiers);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "duplicate max_stake values must be rejected as overlapping ranges"
    );
}

/// Descending `max_stake` values mean the second tier's range starts below the
/// first tier's ceiling, creating an overlap. Must be rejected.
#[test]
fn test_set_fee_tiers_rejects_tiers_with_descending_max_stake() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tier 1 max_stake = 1000, Tier 2 max_stake = 500 — descending, hence
    // overlapping ranges [0, 1000] and [0, 500].
    let tiers = make_tiers(&env, &[(1000, 100), (500, 200)]);
    let result = client.try_set_fee_tiers(&tiers);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "descending max_stake values must be rejected as overlapping ranges"
    );
}

/// Three tiers where the third overlaps the second (the first two are
/// ascending, but the third drops back) must be rejected.
#[test]
fn test_set_fee_tiers_rejects_partial_overlap_in_three_tier_schedule() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Tier 1: 100, Tier 2: 500, Tier 3: 300 — third tier overlaps the second.
    let tiers = make_tiers(&env, &[(100, 50), (500, 100), (300, 150)]);
    let result = client.try_set_fee_tiers(&tiers);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "a later tier with a lower max_stake must be rejected"
    );
}

// ── Sorted, non-overlapping tiers are accepted ────────────────────────────────

/// Strictly ascending `max_stake` values produce non-overlapping ranges and
/// must be accepted.
#[test]
fn test_set_fee_tiers_accepts_strictly_ascending_tiers() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Three tiers with strictly increasing max_stake — no overlaps.
    let tiers = make_tiers(&env, &[(100, 50), (500, 100), (i128::MAX, 200)]);
    let result = client.try_set_fee_tiers(&tiers);
    assert!(
        result.is_ok(),
        "strictly ascending tiers must be accepted: {:?}",
        result
    );

    let stored = client.get_fee_tiers();
    assert_eq!(stored.len(), 3, "three tiers must be stored");
    assert_eq!(stored.get(0).unwrap().max_stake, 100);
    assert_eq!(stored.get(1).unwrap().max_stake, 500);
    assert_eq!(stored.get(2).unwrap().max_stake, i128::MAX);
}

/// A single-tier schedule (by definition non-overlapping) must be accepted.
#[test]
fn test_set_fee_tiers_accepts_single_tier_schedule() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let tiers = make_tiers(&env, &[(i128::MAX, 100)]);
    let result = client.try_set_fee_tiers(&tiers);
    assert!(
        result.is_ok(),
        "a single-tier schedule must be accepted: {:?}",
        result
    );
}

/// Two tiers with max_stake values that differ by exactly 1 are strictly
/// ascending and must be accepted (boundary condition).
#[test]
fn test_set_fee_tiers_accepts_adjacent_max_stake_boundary() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 999 and 1000 differ by 1 — strictly ascending, no overlap.
    let tiers = make_tiers(&env, &[(999, 50), (1000, 100)]);
    let result = client.try_set_fee_tiers(&tiers);
    assert!(
        result.is_ok(),
        "adjacent max_stake values (differ by 1) must be accepted: {:?}",
        result
    );
}
