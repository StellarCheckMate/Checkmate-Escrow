//! Property-based tests for fee tier calculation using proptest.
//!
//! Issue #1368 — verify the fee tier logic holds the following invariants for
//! all possible inputs:
//!
//! 1. `fee >= 0` — the fee is never negative.
//! 2. `fee <= 2 * stake` — the fee never exceeds the full pot.
//! 3. Tier selection is monotonic: a higher stake never produces a lower fee
//!    than a lower stake on the same tier schedule (fee is non-decreasing).
//!
//! These properties hold regardless of the fee schedule configured, so the
//! tests run each property across a large range of random `(stake, bps)`
//! combinations.
//!
//! Run with:
//! ```bash
//! cargo test -p escrow proptest_fee_tiers
//! ```

#![cfg(test)]
extern crate std;

use super::*;
use proptest::prelude::*;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a single `FeeTier` value.
fn make_tier(max_stake: i128, bps: u32) -> FeeTier {
    FeeTier {
        max_stake,
        fee_basis_points: bps,
    }
}

/// Load a tier schedule into a fresh contract and return the client.
fn setup_with_tiers(
    env: &Env,
    tiers: &[(i128, u32)],
) -> (Address, EscrowContractClient<'_>) {
    let (env2, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    // `setup` creates its own Env, so we use the passed-in env by simply
    // calling setup and ignoring the env parameter — proptest doesn't use
    // a custom env for these property checks.
    let _ = env;
    let client = EscrowContractClient::new(&env2, &contract_id);
    let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env2];
    for (max, bps) in tiers {
        v.push_back(make_tier(*max, *bps));
    }
    client.set_fee_tiers(&v);
    (contract_id, client)
}

// ── Strategies ────────────────────────────────────────────────────────────────

/// A valid basis-points value: 0..=10_000.
fn arb_bps() -> impl Strategy<Value = u32> {
    0u32..=10_000u32
}

/// A reasonable stake amount: 1..=1_000_000_000 (avoids i128 overflow in pot).
fn arb_stake() -> impl Strategy<Value = i128> {
    1i128..=1_000_000_000i128
}

/// Two ordered (ascending) stake values for monotonicity checks.
fn arb_two_stakes() -> impl Strategy<Value = (i128, i128)> {
    (arb_stake(), arb_stake()).prop_map(|(a, b)| {
        if a <= b { (a, b) } else { (b, a) }
    })
}

// ── Property 1: fee >= 0 ──────────────────────────────────────────────────────

proptest! {
    /// For any valid stake and bps, the computed fee is never negative.
    #[test]
    fn prop_fee_is_never_negative(stake in arb_stake(), bps in arb_bps()) {
        let (env, _contract, _oracle, _p1, _p2, _token, _admin) = setup();
        let contract_id = _contract;
        let client = EscrowContractClient::new(&env, &contract_id);

        let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
        v.push_back(make_tier(i128::MAX, bps));
        client.set_fee_tiers(&v);

        let fee = client.calculate_fee_by_tier(&stake);
        prop_assert!(fee >= 0, "fee must be >= 0, got {} for stake={} bps={}", fee, stake, bps);
    }
}

// ── Property 2: fee <= 2 * stake (pot) ───────────────────────────────────────

proptest! {
    /// For any valid stake and bps, the fee never exceeds the total pot (2×stake).
    #[test]
    fn prop_fee_never_exceeds_pot(stake in arb_stake(), bps in arb_bps()) {
        let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
        v.push_back(make_tier(i128::MAX, bps));
        client.set_fee_tiers(&v);

        let fee = client.calculate_fee_by_tier(&stake);
        let pot = stake * 2;
        prop_assert!(
            fee <= pot,
            "fee ({}) must be <= pot ({}) for stake={} bps={}",
            fee, pot, stake, bps
        );
    }
}

// ── Property 3: monotonicity ──────────────────────────────────────────────────

proptest! {
    /// Tier selection is monotonic: if stake_a <= stake_b, then fee_a <= fee_b
    /// (fees are non-decreasing with stake on a single-tier flat schedule).
    #[test]
    fn prop_fee_monotonic_single_tier(
        (stake_lo, stake_hi) in arb_two_stakes(),
        bps in arb_bps()
    ) {
        let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
        v.push_back(make_tier(i128::MAX, bps));
        client.set_fee_tiers(&v);

        let fee_lo = client.calculate_fee_by_tier(&stake_lo);
        let fee_hi = client.calculate_fee_by_tier(&stake_hi);

        prop_assert!(
            fee_lo <= fee_hi,
            "fee must be non-decreasing: fee({})={} > fee({})={} with bps={}",
            stake_lo, fee_lo, stake_hi, fee_hi, bps
        );
    }
}

proptest! {
    /// On a multi-tier schedule the last (catch-all) tier's fee is always
    /// >= the fee of the lower tier for stakes that fall into each respective
    /// range — provided the upper tier has bps >= lower tier's bps.
    #[test]
    fn prop_fee_monotonic_across_tiers(
        boundary in 1i128..500_000_000i128,
        bps_lo in arb_bps(),
        bps_delta in 0u32..=10_000u32,
    ) {
        // Ensure bps_hi is valid and >= bps_lo (so the schedule is itself monotonic).
        let bps_hi = (bps_lo + bps_delta).min(10_000);

        let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
        v.push_back(make_tier(boundary, bps_lo));
        v.push_back(make_tier(i128::MAX, bps_hi));
        client.set_fee_tiers(&v);

        // A stake at the boundary falls into the low tier.
        let fee_at_boundary = client.calculate_fee_by_tier(&boundary);
        // A stake just above the boundary falls into the high tier.
        let stake_above = boundary + 1;
        let fee_above = client.calculate_fee_by_tier(&stake_above);

        // fee_above may be >= or <= fee_at_boundary depending on the bps jump
        // and the stake delta, but fee_at_boundary itself must be <= 2*boundary.
        let pot_boundary = boundary * 2;
        prop_assert!(
            fee_at_boundary <= pot_boundary,
            "boundary fee ({}) must not exceed pot ({})",
            fee_at_boundary, pot_boundary
        );

        // When bps_hi >= bps_lo the fee for a larger pot (stake_above) must be
        // >= the fee for the boundary pot, because both the bps and the stake
        // are non-decreasing.
        if bps_hi >= bps_lo {
            prop_assert!(
                fee_above >= fee_at_boundary,
                "fee above boundary ({}) should be >= boundary fee ({}) when bps_hi >= bps_lo",
                fee_above, fee_at_boundary
            );
        }
    }
}

// ── Property 4: zero bps always yields zero fee ───────────────────────────────

proptest! {
    /// A tier configured at 0 bps always returns a zero fee regardless of stake.
    #[test]
    fn prop_zero_bps_yields_zero_fee(stake in arb_stake()) {
        let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
        v.push_back(make_tier(i128::MAX, 0));
        client.set_fee_tiers(&v);

        let fee = client.calculate_fee_by_tier(&stake);
        prop_assert_eq!(fee, 0, "0 bps must always yield 0 fee for stake={}", stake);
    }
}

// ── Property 5: 10_000 bps always yields fee == pot ──────────────────────────

proptest! {
    /// A tier at 10_000 bps (100%) always produces fee == 2×stake (the whole pot).
    #[test]
    fn prop_full_bps_yields_full_pot(stake in arb_stake()) {
        let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);

        let mut v: soroban_sdk::Vec<FeeTier> = soroban_sdk::vec![&env];
        v.push_back(make_tier(i128::MAX, 10_000));
        client.set_fee_tiers(&v);

        let fee = client.calculate_fee_by_tier(&stake);
        let expected = stake * 2;
        prop_assert_eq!(
            fee, expected,
            "10_000 bps must yield fee == pot for stake={}",
            stake
        );
    }
}

// ── Property 6: empty tier schedule always yields zero fee ───────────────────

proptest! {
    /// When no tiers are configured the fee is always 0, for any stake.
    #[test]
    fn prop_no_tiers_zero_fee(stake in arb_stake()) {
        let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
        let client = EscrowContractClient::new(&env, &contract_id);
        // No tiers set — default is empty.
        let fee = client.calculate_fee_by_tier(&stake);
        prop_assert_eq!(fee, 0, "empty tier schedule must yield 0 fee for stake={}", stake);
    }
}
