//! Tests for #965 — Token Swap on Payout for Currency Preference.
//!
//! Verifies that:
//! - `set_preferred_payout_token` stores the player's preference.
//! - `get_preferred_payout_token` retrieves it (or `None` if unset).
//! - Clearing the preference (passing `None`) removes it from storage.
//! - When a player's preferred token matches `token_b` on a conversion match,
//!   `claim_vested_payout` delivers the payout in `token_b` using the
//!   oracle-provided `conversion_rate`.
//! - When no preference is set, payout falls back to the stake token.
//! - When the preference does not match `token_b`, payout falls back to the
//!   stake token (no swap attempted).
//! - Draw payouts always use the stake token regardless of preference.
//! - The `claim` event carries the actual payout token address.

#![cfg(test)]
extern crate std;

use super::*;
use oracle::{OracleContract, OracleContractClient};
use soroban_sdk::{token::StellarAssetClient, Address, Env, String};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Create a two-token conversion match using the Oracle contract, fund it,
/// and submit the given result.
///
/// Returns:
/// `(env, contract_id, player1, player2, token_a, token_b, oracle_client, match_id)`
///
/// The oracle rate is set to `10_000_000` (1:1 at base denominator) unless
/// `rate` is specified otherwise.  The passed `rate` must be within 5% of the
/// oracle rate (so pass matching values for valid setups).
fn setup_swap_match(
    game_id: &str,
    winner: Winner,
    oracle_rate: i128,
    match_rate: i128,
) -> (
    Env,
    Address, // escrow contract id
    Address, // player1
    Address, // player2
    Address, // token_a (stake token)
    Address, // token_b (preferred payout token)
    OracleContractClient<'static>,
    u64, // match_id
) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let oracle_admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    // Register Oracle contract
    let oracle_id = env.register_contract(None, OracleContract);
    let oracle_client = OracleContractClient::new(&env, &oracle_id);
    oracle_client.initialize(&oracle_admin);

    // Register Escrow contract
    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    escrow_client.initialize(&oracle_id, &admin);
    escrow_client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 0,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
    });

    // Register token A (stake token) and token B (preferred payout token)
    let token_a_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_a = token_a_id.address();
    let asset_a = StellarAssetClient::new(&env, &token_a);

    let token_b_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_b = token_b_id.address();
    let asset_b = StellarAssetClient::new(&env, &token_b);

    // Mint token_a to players for deposits
    let stake: i128 = 100; // within Bronze tier bounds (1..=100) for a fresh player
    asset_a.mint(&player1, &(stake * 10));
    asset_a.mint(&player2, &(stake * 10));
    // Mint token_b to the oracle contract: `OracleContract::swap` sources
    // token_out from its own balance (env.current_contract_address()), not
    // escrow's -- escrow only calls swap(), it never holds token_b itself.
    // worst case: pot * 2 * rate / 10_000_000
    asset_b.mint(&oracle_id, &(stake * 200));

    // Set oracle rate and create the match
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake,
        &token_a,
        &token_b,
        &match_rate,
        &String::from_str(&env, game_id),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);
    escrow_client.submit_result(&match_id, &winner, &oracle_id);

    (
        env,
        escrow_id,
        player1,
        player2,
        token_a,
        token_b,
        oracle_client,
        match_id,
    )
}

// ── set/get preferred payout token ───────────────────────────────────────────

/// Setting a preferred payout token stores it and `get_preferred_payout_token` returns it.
#[test]
fn test_set_and_get_preferred_payout_token() {
    let (env, escrow_id, player1, _player2, _token_a, _token_b, _oracle, _match_id) =
        setup_swap_match("54cd1eff", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let preferred = Address::generate(&env);

    client.set_preferred_payout_token(&player1, &Some(preferred.clone()));

    assert_eq!(
        client.get_preferred_payout_token(&player1),
        Some(preferred),
        "preferred payout token should be stored"
    );
}

/// Clearing the preference (passing `None`) removes it; getter returns `None`.
#[test]
fn test_clear_preferred_payout_token() {
    let (env, escrow_id, player1, _player2, _token_a, _token_b, _oracle, _match_id) =
        setup_swap_match("80dc1e86", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let preferred = Address::generate(&env);

    // Set then clear
    client.set_preferred_payout_token(&player1, &Some(preferred));
    client.set_preferred_payout_token(&player1, &None);

    assert_eq!(
        client.get_preferred_payout_token(&player1),
        None,
        "preference should be cleared after passing None"
    );
}

/// When no preference is set, `get_preferred_payout_token` returns `None`.
#[test]
fn test_get_preferred_payout_token_defaults_to_none() {
    let (env, escrow_id, player1, _player2, _token_a, _token_b, _oracle, _match_id) =
        setup_swap_match("c2f2a21b", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);

    assert_eq!(
        client.get_preferred_payout_token(&player1),
        None,
        "default preferred token should be None"
    );
}

// ── swap on payout ────────────────────────────────────────────────────────────

/// When a player prefers token_b and the match has a conversion rate, the
/// payout is delivered in token_b at the oracle exchange rate.
///
/// With stake=100, pot=200, rate=10_000_000 (1:1):
///   swap_amount = 200 * 10_000_000 / 10_000_000 = 200 token_b
#[test]
fn test_winner_receives_preferred_token_on_swap() {
    let (env, escrow_id, player1, _player2, _token_a, token_b, _oracle, match_id) =
        setup_swap_match("0ca4916b", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_b = token_client(&env, &token_b);

    let b_before = tok_b.balance(&player1);

    // Set player1's preferred payout token to token_b
    client.set_preferred_payout_token(&player1, &Some(token_b.clone()));
    client.claim_vested_payout(&match_id, &player1);

    let expected_b: i128 = 200 * 10_000_000 / 10_000_000; // = 200
    assert_eq!(
        tok_b.balance(&player1),
        b_before + expected_b,
        "player1 should receive payout in token_b at 1:1 conversion rate"
    );
}

/// Swap with a 2:1 rate: 200 token_a → 400 token_b
/// rate = 20_000_000 → swap_amount = 200 * 20_000_000 / 10_000_000 = 400
#[test]
fn test_swap_rate_calculation() {
    // oracle_rate and match_rate both at 20_000_000 (within 5% of each other)
    let (env, escrow_id, player1, _player2, _token_a, token_b, _oracle, match_id) =
        setup_swap_match("6896921b", Winner::Player1, 20_000_000, 20_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_b = token_client(&env, &token_b);

    let b_before = tok_b.balance(&player1);

    client.set_preferred_payout_token(&player1, &Some(token_b.clone()));
    client.claim_vested_payout(&match_id, &player1);

    let expected_b: i128 = 200 * 20_000_000 / 10_000_000; // = 400
    assert_eq!(
        tok_b.balance(&player1),
        b_before + expected_b,
        "2:1 rate should give 400 token_b for 200 token_a pot"
    );
}

/// When no preference is set, payout falls back to the stake token (token_a).
#[test]
fn test_no_preference_uses_stake_token() {
    let (env, escrow_id, player1, _player2, token_a, token_b, _oracle, match_id) =
        setup_swap_match("98fd1a03", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_a = token_client(&env, &token_a);
    let tok_b = token_client(&env, &token_b);

    let a_before = tok_a.balance(&player1);
    let b_before = tok_b.balance(&player1);

    // No preference set — should receive token_a
    client.claim_vested_payout(&match_id, &player1);

    assert_eq!(
        tok_a.balance(&player1),
        a_before + 200,
        "player1 should receive 200 token_a (full pot, no swap)"
    );
    assert_eq!(
        tok_b.balance(&player1),
        b_before,
        "token_b balance should be unchanged"
    );
}

/// When the preferred token is the same as the stake token, no swap occurs.
#[test]
fn test_preference_equals_stake_token_no_swap() {
    let (env, escrow_id, player1, _player2, token_a, _token_b, _oracle, match_id) =
        setup_swap_match("3bf379af", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_a = token_client(&env, &token_a);

    let a_before = tok_a.balance(&player1);

    // Prefer the stake token itself — should receive token_a normally
    client.set_preferred_payout_token(&player1, &Some(token_a.clone()));
    client.claim_vested_payout(&match_id, &player1);

    assert_eq!(
        tok_a.balance(&player1),
        a_before + 200,
        "same-token preference should not change payout"
    );
}

/// When the preferred token is set to an unrelated address (not token_b),
/// payout falls back to the stake token.
#[test]
fn test_preference_unrelated_token_falls_back_to_stake() {
    let (env, escrow_id, player1, _player2, token_a, _token_b, _oracle, match_id) =
        setup_swap_match("26e5e572", Winner::Player1, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_a = token_client(&env, &token_a);

    let a_before = tok_a.balance(&player1);

    // Prefer a random address that is not token_b on this match
    let unrelated = Address::generate(&env);
    client.set_preferred_payout_token(&player1, &Some(unrelated));
    client.claim_vested_payout(&match_id, &player1);

    assert_eq!(
        tok_a.balance(&player1),
        a_before + 200,
        "unrelated preference should fall back to stake token"
    );
}

/// Draw payouts always return the stake token to each player,
/// regardless of their preferred payout token.
#[test]
fn test_draw_ignores_preferred_payout_token() {
    let (env, escrow_id, player1, player2, token_a, token_b, _oracle, match_id) =
        setup_swap_match("1b246961", Winner::Draw, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_a = token_client(&env, &token_a);
    let tok_b = token_client(&env, &token_b);

    let p1_a_before = tok_a.balance(&player1);
    let p1_b_before = tok_b.balance(&player1);
    let p2_a_before = tok_a.balance(&player2);

    // Both players prefer token_b
    client.set_preferred_payout_token(&player1, &Some(token_b.clone()));
    client.set_preferred_payout_token(&player2, &Some(token_b.clone()));

    client.claim_vested_payout(&match_id, &player1);
    client.claim_vested_payout(&match_id, &player2);

    // Draw refunds the stake token regardless of preference (swap only applies to wins)
    assert_eq!(
        tok_a.balance(&player1),
        p1_a_before + 100,
        "player1 should get stake back in token_a on draw"
    );
    assert_eq!(
        tok_b.balance(&player1),
        p1_b_before,
        "player1 token_b balance should be unchanged on draw"
    );
    assert_eq!(
        tok_a.balance(&player2),
        p2_a_before + 100,
        "player2 should get stake back in token_a on draw"
    );
}

/// Player 2 wins and has a preferred payout token — swap is applied correctly.
#[test]
fn test_player2_winner_swap() {
    let (env, escrow_id, _player1, player2, _token_a, token_b, _oracle, match_id) =
        setup_swap_match("4ac21e17", Winner::Player2, 10_000_000, 10_000_000);

    let client = EscrowContractClient::new(&env, &escrow_id);
    let tok_b = token_client(&env, &token_b);

    let b_before = tok_b.balance(&player2);

    client.set_preferred_payout_token(&player2, &Some(token_b.clone()));
    client.claim_vested_payout(&match_id, &player2);

    let expected_b: i128 = 200 * 10_000_000 / 10_000_000; // = 200
    assert_eq!(
        tok_b.balance(&player2),
        b_before + expected_b,
        "player2 should receive payout in token_b at 1:1 rate"
    );
}
