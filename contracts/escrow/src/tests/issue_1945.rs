//! Tests for #1945 — add a get_referral_volume view.
//!
//! Verifies that:
//! - `get_referral_volume` returns 0 for a referrer with no attributed matches.
//! - After a winner payout on a match with a referrer, the volume equals the full pot (stake × 2).
//! - Volume accumulates correctly across multiple matches for the same referrer.
//! - Draw payouts do not contribute to referral volume.

#![cfg(test)]

use super::*;

/// Helper: set up a match with a referrer, both players deposit, and return
/// `(env, contract_id, oracle, player1, player2, token, referrer, match_id)`.
fn setup_referral(
    game_id: &str,
    stake: i128,
    cancellation_fee_bps: u32,
    referral_share_bps: u32,
) -> (Env, Address, Address, Address, Address, Address, Address, u64) {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Mint extra so larger stakes work.
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &10_000_000);
    asset_client.mint(&player2, &10_000_000);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: cancellation_fee_bps,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: crate::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: crate::DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
        dispute_bond_tier_schedule: soroban_sdk::vec![&env],
    });
    client.set_referral_share_bps(&referral_share_bps);

    let referrer = Address::generate(&env);

    let match_id = client.create_match_with_referrer(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, game_id),
        &Platform::Lichess,
        &referrer,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    (env, contract_id, oracle, player1, player2, token, referrer, match_id)
}

/// A referrer with no attributed matches returns volume 0.
#[test]
fn test_get_referral_volume_returns_zero_for_unattributed_referrer() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stranger = Address::generate(&env);
    assert_eq!(
        client.get_referral_volume(&stranger),
        0,
        "referrer with no matches must have volume 0"
    );
}

/// After one winner payout, the referral volume equals the full pot (stake × 2).
/// This verifies that the volume matches the referral event (stake × 2 attributed per match).
#[test]
fn test_get_referral_volume_matches_referral_events_after_one_match() {
    let stake: i128 = 100;
    // cancellation_fee_bps = 200 (2 %), referral_share_bps = 2000 (20%)
    // → referral_fee = (200 * 200 / 10_000) * 2000 / 10_000 = 4 * 0.2 = 0.8 → 0
    // Use larger bps so the fee is non-zero: 1000 bps (10 %), 5000 (50 %)
    // referral_fee = (200 * 1000 / 10_000) * 5000 / 10_000 = 20 * 0.5 = 10
    let (env, contract_id, oracle, player1, _p2, _token, referrer, match_id) =
        setup_referral("ab12cd34", stake, 1000, 5000);

    let client = EscrowContractClient::new(&env, &contract_id);

    // Before payout: volume is 0.
    assert_eq!(client.get_referral_volume(&referrer), 0);

    // Player1 wins → referral fee is paid → volume is recorded.
    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    let pot = stake * 2; // 200
    assert_eq!(
        client.get_referral_volume(&referrer),
        pot,
        "volume after one winner payout must equal the full pot (stake × 2)"
    );
}

/// Volume accumulates across multiple matches referencing the same referrer.
#[test]
fn test_get_referral_volume_accumulates_across_multiple_matches() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &10_000_000);
    asset_client.mint(&player2, &10_000_000);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 1000, // 10 %
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: crate::DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: crate::DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: None,
        dispute_bond_tier_schedule: soroban_sdk::vec![&env],
    });
    client.set_referral_share_bps(&5000); // 50 %

    let referrer = Address::generate(&env);

    // Create 3 matches all referencing the same referrer.
    let stake: i128 = 100;
    let game_ids = ["aaaa1111", "bbbb2222", "cccc3333"];
    let mut match_ids = Vec::new();

    for game_id in &game_ids {
        let mid = client.create_match_with_referrer(
            &player1,
            &player2,
            &stake,
            &token,
            &String::from_str(&env, game_id),
            &Platform::Lichess,
            &referrer,
        );
        client.deposit(&mid, &player1);
        client.deposit(&mid, &player2);
        match_ids.push(mid);
    }

    // Settle all three matches — player1 wins each.
    for mid in &match_ids {
        client.submit_result(mid, &Winner::Player1, &oracle);
        client.claim_vested_payout(mid, &player1);
    }

    let expected_volume = stake * 2 * game_ids.len() as i128; // 600
    assert_eq!(
        client.get_referral_volume(&referrer),
        expected_volume,
        "volume must accumulate across all three matches"
    );
}

/// Draw payouts do not contribute to referral volume because no referral fee is paid.
#[test]
fn test_get_referral_volume_unchanged_after_draw() {
    let stake: i128 = 100;
    let (env, contract_id, oracle, player1, player2, _token, referrer, match_id) =
        setup_referral("draw0001", stake, 1000, 5000);

    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&match_id, &Winner::Draw, &oracle);
    client.claim_vested_payout(&match_id, &player1);
    client.claim_vested_payout(&match_id, &player2);

    assert_eq!(
        client.get_referral_volume(&referrer),
        0,
        "draw payout must not contribute to referral volume"
    );
}
