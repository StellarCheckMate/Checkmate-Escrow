//! Tests for #964 — Referral Fee Sharing for Match Creators.
//!
//! Verifies that:
//! - `create_match_with_referrer` stores the referrer on the match.
//! - On winner payout, the referral fee is calculated as
//!   `pot * cancellation_fee_bps / 10_000 * referral_share_bps / 10_000` and
//!   transferred to the referrer address.
//! - No referral fee is paid when `cancellation_fee_basis_points` is 0.
//! - Draw payouts do not trigger a referral fee.

#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::Address as _;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Set up a match using `create_match_with_referrer`, fund it, and return all
/// relevant state.
///
/// Returns `(env, contract_id, oracle, player1, player2, token, admin, referrer, match_id)`.
fn setup_referral_match(
    game_id: &str,
    cancellation_fee_bps: u32,
    referral_share_bps: u32,
    stake: i128,
) -> (Env, Address, Address, Address, Address, Address, Address, Address, u64) {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);

    // Mint extra tokens so larger stakes work
    asset_client.mint(&player1, &10_000_000);
    asset_client.mint(&player2, &10_000_000);

    let referrer = Address::generate(&env);

    // Configure platform fee and referral share
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: cancellation_fee_bps,
        treasury: admin.clone(),
    });
    client.set_referral_share_bps(&referral_share_bps);

    let match_id = client.create_match_with_referrer(
        &player1,
        &player2,
        &stake,
        &token,
        &soroban_sdk::String::from_str(&env, game_id),
        &Platform::Lichess,
        &referrer,
    );

    // Both players deposit
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    (env, contract_id, oracle, player1, player2, token, admin, referrer, match_id)
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// The referrer address is stored on the match struct.
#[test]
fn test_create_match_with_referrer_stores_referrer() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, referrer, match_id) =
        setup_referral_match("ref_store_game", 200, 2000, 100);

    let client = EscrowContractClient::new(&env, &contract_id);
    let m = client.get_match(&match_id);

    assert_eq!(
        m.referrer,
        Some(referrer),
        "referrer should be stored on the match"
    );
}

/// Winner payout deducts a referral fee and sends it to the referrer.
///
/// With stake=10_000, pot=20_000:
///   platform_fee = 20_000 * 200 / 10_000 = 400
///   referral_fee = 400 * 2000 / 10_000 = 80
///   net winner payout = 20_000 - 80 = 19_920
#[test]
fn test_referral_fee_calculation_and_payment() {
    let stake: i128 = 10_000;
    let (env, contract_id, _oracle, player1, _player2, token, _admin, referrer, match_id) =
        setup_referral_match("ref_fee_calc_game", 200, 2000, stake);

    let client = EscrowContractClient::new(&env, &contract_id);
    let tok = token_client(&env, &token);

    let p1_before = tok.balance(&player1);
    let referrer_before = tok.balance(&referrer);

    // Oracle submits result: player1 wins
    client.submit_result(&match_id, &Winner::Player1);
    client.claim_vested_payout(&match_id, &player1);

    let pot: i128 = stake * 2;
    let platform_fee = pot * 200 / 10_000; // = 400
    let referral_fee = platform_fee * 2000 / 10_000; // = 80
    let net_payout = pot - referral_fee; // = 19_920

    assert_eq!(
        tok.balance(&player1),
        p1_before + net_payout,
        "player1 should receive pot minus referral fee"
    );
    assert_eq!(
        tok.balance(&referrer),
        referrer_before + referral_fee,
        "referrer should receive the referral fee"
    );
}

/// No referral fee is sent when cancellation_fee_basis_points is 0.
#[test]
fn test_referral_fee_no_fee_when_platform_fee_zero() {
    let stake: i128 = 10_000;
    let (env, contract_id, _oracle, player1, _player2, token, _admin, referrer, match_id) =
        setup_referral_match("ref_no_fee_game", 0, 2000, stake);

    let client = EscrowContractClient::new(&env, &contract_id);
    let tok = token_client(&env, &token);

    let referrer_before = tok.balance(&referrer);
    let p1_before = tok.balance(&player1);

    client.submit_result(&match_id, &Winner::Player1);
    client.claim_vested_payout(&match_id, &player1);

    let pot: i128 = stake * 2;
    assert_eq!(
        tok.balance(&referrer),
        referrer_before,
        "referrer should receive nothing when platform fee is 0"
    );
    assert_eq!(
        tok.balance(&player1),
        p1_before + pot,
        "player1 should receive full pot when platform fee is 0"
    );
}

/// Draw payouts return each player their full stake — no referral fee deducted.
#[test]
fn test_referral_fee_draw_no_referral_payment() {
    let stake: i128 = 10_000;
    let (env, contract_id, _oracle, player1, player2, token, _admin, referrer, match_id) =
        setup_referral_match("ref_draw_game", 200, 2000, stake);

    let client = EscrowContractClient::new(&env, &contract_id);
    let tok = token_client(&env, &token);

    let referrer_before = tok.balance(&referrer);
    let p1_before = tok.balance(&player1);
    let p2_before = tok.balance(&player2);

    client.submit_result(&match_id, &Winner::Draw);
    client.claim_vested_payout(&match_id, &player1);
    client.claim_vested_payout(&match_id, &player2);

    assert_eq!(
        tok.balance(&referrer),
        referrer_before,
        "referrer receives nothing on a draw"
    );
    assert_eq!(
        tok.balance(&player1),
        p1_before + stake,
        "player1 gets full stake back on draw"
    );
    assert_eq!(
        tok.balance(&player2),
        p2_before + stake,
        "player2 gets full stake back on draw"
    );
}

/// Default referral share is 20% (2000 bps).
#[test]
fn test_get_referral_share_bps_default() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    assert_eq!(client.get_referral_share_bps(), 2000);
}

/// `set_referral_share_bps` updates the stored value.
#[test]
fn test_set_referral_share_bps_updates_value() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_referral_share_bps(&5000);
    assert_eq!(client.get_referral_share_bps(), 5000);
}

/// `create_match` (without referrer) leaves referrer as None.
#[test]
fn test_create_match_without_referrer_has_none() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, "no_ref_game"),
        &Platform::Lichess,
    );

    let m = client.get_match(&match_id);
    assert_eq!(m.referrer, None, "standard create_match should have no referrer");
}
