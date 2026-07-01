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
    asset_a_client.mint(&player1, &1000_0000000);
    asset_b_client.mint(&player2, &1000_0000000);

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

    // Player 2 deposits token_b
    escrow_client.deposit(&match_id, &player2);
    assert_eq!(token_client(&env, &token_b).balance(&player2), 500_0000000);
    assert_eq!(token_client(&env, &token_b).balance(&escrow_client.address), expected_b_stake);

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

    // Oracle submits result: Player 1 wins
    oracle_client.submit_result(&match_id, &String::from_str(&env, "p1_win_game"), &oracle::types::Platform::Lichess, &oracle::types::Winner::Player1);

    // Payout should convert Player 2's token_b stake back to Player 1's preferred token_a.
    // Player 1 should receive total 200 token_a.
    assert_eq!(token_client(&env, &token_a).balance(&player1), 1100_0000000); // 900 + 200 = 1100
    assert_eq!(token_client(&env, &token_b).balance(&player2), 500_0000000); // 500

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

    // Oracle submits result: Player 2 wins
    oracle_client.submit_result(&match_id, &String::from_str(&env, "p2_win_game"), &oracle::types::Platform::Lichess, &oracle::types::Winner::Player2);

    // Payout should convert Player 1's token_a stake back to Player 2's preferred token_b.
    // Player 2 should receive total 1000 token_b.
    assert_eq!(token_client(&env, &token_a).balance(&player1), 900_0000000); // 900
    assert_eq!(token_client(&env, &token_b).balance(&player2), 1500_0000000); // 500 + 1000 = 1500

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

    // Oracle submits result: Draw
    oracle_client.submit_result(&match_id, &String::from_str(&env, "draw_game"), &oracle::types::Platform::Lichess, &oracle::types::Winner::Draw);

    // Refund player 1 their token_a stake, and player 2 their token_b stake
    assert_eq!(token_client(&env, &token_a).balance(&player1), 1000_0000000); // 900 + 100 = 1000
    assert_eq!(token_client(&env, &token_b).balance(&player2), 1000_0000000); // 500 + 500 = 1000

    assert_eq!(token_client(&env, &token_a).balance(&escrow_client.address), 0);
    assert_eq!(token_client(&env, &token_b).balance(&escrow_client.address), 0);
}
