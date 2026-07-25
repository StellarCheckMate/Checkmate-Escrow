use crate::tests::token_client;
use crate::types::{MatchState, Platform, Winner};
use crate::{EscrowContract, EscrowContractClient};
use oracle::{OracleContract, OracleContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke},
    token::StellarAssetClient,
    Address, Env, String, Symbol, IntoVal,
};

fn setup_multi_token_fixture() -> (
    Env,
    Address,
    OracleContractClient<'static>,
    EscrowContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle_admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    // Register Oracle Contract
    let oracle_id = env.register_contract(None, OracleContract);
    let oracle_client = OracleContractClient::new(&env, &oracle_id);
    oracle_client.initialize(&oracle_admin);

    // Register Escrow Contract
    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    escrow_client.initialize(&oracle_id, &admin);

    // Deploy two distinct tokens (e.g. USDC and XLM)
    let token_a_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_a_addr = token_a_id.address();
    let asset_a_client = StellarAssetClient::new(&env, &token_a_addr);

    let token_b_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_b_addr = token_b_id.address();
    let asset_b_client = StellarAssetClient::new(&env, &token_b_addr);

    // Mint tokens to players
    // Both need token_a for deposits (since deposit() always uses m.token)
    // Player2 also gets token_b for receiving payouts in multi-token mode
    asset_a_client.mint(&player1, &1000_0000000);
    asset_a_client.mint(&player2, &1000_0000000);
    asset_b_client.mint(&player2, &500_0000000);  // 500 = stake_amount * conversion_rate / 10^7

    // Also fund Oracle contract with token pool for swapping
    asset_a_client.mint(&oracle_id, &10000_0000000);
    asset_b_client.mint(&oracle_id, &10000_0000000);

    (
        env,
        admin,
        oracle_client,
        escrow_client,
        player1,
        player2,
        token_a_addr,
        token_b_addr,
        oracle_id,
    )
}

#[test]
fn test_create_match_with_conversion_valid_rate() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    // Set rate in oracle: 1 USDC (token_a) = 5 XLM (token_b) -> rate = 5.0 * 10^7 = 50_000_000
    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    // Create match with rate within 5% of oracle rate (e.g. 4.9 * 10^7 = 49_000_000)
    let rate = 49_000_000;
    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "valid_rate_game"),
        &Platform::Lichess,
    );

    let m = escrow_client.get_match(&match_id);
    assert_eq!(m.conversion_rate, rate);
    assert_eq!(m.token_b, Some(token_b));
}

#[test]
#[should_panic]
fn test_create_match_with_conversion_invalid_rate_high() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    // 10% high (55_000_000), should fail verification (>5%)
    let rate = 55_000_000;
    escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "invalid_rate_game"),
        &Platform::Lichess,
    );
}

#[test]
fn test_multi_token_deposits_and_refunds() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000; // 100 token_a
    let rate = 50_000_000; // 5.0
    // Expected token_b stake = 100 * 5.0 = 500 token_b
    let expected_b_stake = 500_0000000;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "deposit_refund_game"),
        &Platform::Lichess,
    );

    // Player 1 deposits token_a
    escrow_client.deposit(&match_id, &player1);
    assert_eq!(token_client(&env, &token_a).balance(&player1), 900_0000000);
    assert_eq!(token_client(&env, &token_a).balance(&escrow_client.address), stake_amount);

    // Player 2 deposits token_a (same as player1, deposit() always uses m.token)
    escrow_client.deposit(&match_id, &player2);
    assert_eq!(token_client(&env, &token_a).balance(&player2), 900_0000000); // 1000 - 100 = 900
    assert_eq!(token_client(&env, &token_a).balance(&escrow_client.address), 200_0000000); // 100 + 100

    let m = escrow_client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Active);

    // Cancel match and verify refunds in correct tokens
    // Note: Since match is active, cancel_match should fail unless we do it before active,
    // let's test refunds via cancel/expire by setting up a pending match.
    
    let match_id2 = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "cancel_game"),
        &Platform::Lichess,
    );
    escrow_client.deposit(&match_id2, &player1);
    escrow_client.deposit(&match_id2, &player2);
    // Wait, cancel_match rejects if state == Active, let's create one where only player1 deposited
    let match_id3 = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "cancel_pending_game"),
        &Platform::Lichess,
    );
    escrow_client.deposit(&match_id3, &player1);
    escrow_client.cancel_match(&match_id3, &player1);

    // Verify refund of token_a
    assert_eq!(token_client(&env, &token_a).balance(&player1), 900_0000000); // 900 - 100 + 100 = 900
}

#[test]
fn test_multi_token_payout_player1_wins() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000;
    let rate = 50_000_000;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "p1_win_game"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Submit result: Player 1 wins
    escrow_client.submit_result(&match_id, &Winner::Player1);

    // Player 1 wins and receives the pot in token_a. Player 2 receives nothing (loser).
    // Player 1: 1000 starting - 100 deposit + 200 pot = 1100 token_a
    assert_eq!(token_client(&env, &token_a).balance(&player1), 1100_0000000);
    // Player 2: 500 starting token_b, unchanged (no payout on loss)
    assert_eq!(token_client(&env, &token_b).balance(&player2), 500_0000000);

    // Escrow contract should have 0 balances for this match
    assert_eq!(token_client(&env, &token_a).balance(&escrow_client.address), 0);
    assert_eq!(token_client(&env, &token_b).balance(&escrow_client.address), 0);
}

#[test]
fn test_multi_token_payout_player2_wins() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000;
    let rate = 50_000_000;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "p2_win_game"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Submit result: Player 2 wins
    escrow_client.submit_result(&match_id, &Winner::Player2);

    // Player 2 wins and receives the pot (200 token_a) converted to token_b.
    // Pot in token_b: 200 * 50_000_000 / 10_000_000 = 1000 token_b
    assert_eq!(token_client(&env, &token_a).balance(&player1), 900_0000000);
    // Player 2: 500 starting + 1000 payout = 1500 token_b
    assert_eq!(token_client(&env, &token_b).balance(&player2), 1500_0000000);

    assert_eq!(token_client(&env, &token_a).balance(&escrow_client.address), 0);
    assert_eq!(token_client(&env, &token_b).balance(&escrow_client.address), 0);
}

#[test]
fn test_multi_token_payout_draw() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000;
    let rate = 50_000_000;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "draw_game"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Submit result: Draw
    escrow_client.submit_result(&match_id, &Winner::Draw);

    // Refund player 1 their token_a stake, and player 2 their token_b stake
    assert_eq!(token_client(&env, &token_a).balance(&player1), 1000_0000000); // 900 + 100 = 1000
    assert_eq!(token_client(&env, &token_b).balance(&player2), 1000_0000000); // 500 + 500 = 1000

    assert_eq!(token_client(&env, &token_a).balance(&escrow_client.address), 0);
    assert_eq!(token_client(&env, &token_b).balance(&escrow_client.address), 0);
}

#[test]
fn test_create_match_with_conversion_rate_boundary_low() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    // Exactly 5% low: 50 * 0.95 = 47.5
    let rate = 47_500_000;
    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "boundary_low_game"),
        &Platform::Lichess,
    );

    let m = escrow_client.get_match(&match_id);
    assert_eq!(m.conversion_rate, rate);
    assert_eq!(m.token_b, Some(token_b));
}

#[test]
fn test_create_match_with_conversion_rate_boundary_high() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    // Exactly 5% high: 50 * 1.05 = 52.5
    let rate = 52_500_000;
    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "boundary_high_game"),
        &Platform::Lichess,
    );

    let m = escrow_client.get_match(&match_id);
    assert_eq!(m.conversion_rate, rate);
    assert_eq!(m.token_b, Some(token_b));
}

#[test]
#[should_panic]
fn test_create_match_with_conversion_rate_below_boundary() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    // Just below 5% low: 50 * 0.949 = 47.45
    let rate = 47_450_000;
    escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "below_boundary_game"),
        &Platform::Lichess,
    );
}

#[test]
fn test_multi_token_payout_fund_conservation() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, escrow_id) =
        setup_multi_token_fixture();

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000;
    let rate = 50_000_000;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "fund_conservation_game"),
        &Platform::Lichess,
    );

    // Record initial player balances
    let player1_initial_a = token_client(&env, &token_a).balance(&player1);
    let player2_initial_a = token_client(&env, &token_a).balance(&player2);
    let player2_initial_b = token_client(&env, &token_b).balance(&player2);
    let escrow_initial_a = token_client(&env, &token_a).balance(&escrow_id);
    let escrow_initial_b = token_client(&env, &token_b).balance(&escrow_id);

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Submit result: Player 1 wins
    escrow_client.submit_result(&match_id, &Winner::Player1);

    // Verify fund conservation: total token_a and token_b balances unchanged
    let player1_final_a = token_client(&env, &token_a).balance(&player1);
    let player2_final_a = token_client(&env, &token_a).balance(&player2);
    let player2_final_b = token_client(&env, &token_b).balance(&player2);
    let escrow_final_a = token_client(&env, &token_a).balance(&escrow_id);
    let escrow_final_b = token_client(&env, &token_b).balance(&escrow_id);

    // Total token_a should be conserved
    assert_eq!(
        player1_initial_a + player2_initial_a + escrow_initial_a,
        player1_final_a + player2_final_a + escrow_final_a
    );

    // Total token_b should be conserved
    assert_eq!(
        player2_initial_b + escrow_initial_b,
        player2_final_b + escrow_final_b
    );

    // Escrow balances should be 0 after payout
    assert_eq!(escrow_final_a, 0);
    assert_eq!(escrow_final_b, 0);
}

#[test]
#[should_panic]
fn test_multi_token_payout_rejects_stale_rate() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    // Set initial ledger sequence to 100
    env.ledger().set_sequence_number(100);

    let oracle_rate = 50_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000;
    let rate = 50_000_000;

    let match_id = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "stale_rate_game"),
        &Platform::Lichess,
    );

    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Advance ledger beyond staleness threshold (1000 ledgers)
    // Rate was set at ledger 100, so at ledger 1101+ it's stale
    env.ledger().set_sequence_number(1101);

    // Submit result with stale rate — should panic due to ConversionRateStalePriceSource error
    escrow_client.submit_result(&match_id, &Winner::Player1);
}

#[test]
#[should_panic]
fn test_create_match_with_conversion_missing_oracle_rate() {
    let (env, _admin, _oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    // Do NOT set rate in oracle — this should cause create_match_with_conversion to fail
    // when it tries to fetch the oracle rate
    let rate = 50_000_000;
    escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &100_0000000,
        &token_a,
        &token_b,
        &rate,
        &String::from_str(&env, "missing_rate_game"),
        &Platform::Lichess,
    );
}

#[test]
fn test_multi_token_with_valid_rate_at_boundary() {
    let (env, _admin, oracle_client, escrow_client, player1, player2, token_a, token_b, _oracle_id) =
        setup_multi_token_fixture();

    let oracle_rate = 1_000_000;
    oracle_client.set_rate(&token_a, &token_b, &oracle_rate);

    let stake_amount = 100_0000000;

    // Test rate exactly at lower boundary: 1_000_000 * 0.95 = 950_000
    let rate_lower = 950_000;
    let match_id_lower = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate_lower,
        &String::from_str(&env, "boundary_lower_game"),
        &Platform::Lichess,
    );
    let m_lower = escrow_client.get_match(&match_id_lower);
    assert_eq!(m_lower.conversion_rate, rate_lower);

    // Test rate exactly at upper boundary: 1_000_000 * 1.05 = 1_050_000
    let rate_upper = 1_050_000;
    let match_id_upper = escrow_client.create_match_with_conversion(
        &player1,
        &player2,
        &stake_amount,
        &token_a,
        &token_b,
        &rate_upper,
        &String::from_str(&env, "boundary_upper_game"),
        &Platform::Lichess,
    );
    let m_upper = escrow_client.get_match(&match_id_upper);
    assert_eq!(m_upper.conversion_rate, rate_upper);
}
