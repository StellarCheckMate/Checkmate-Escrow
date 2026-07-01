#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::Address as _;

#[test]
fn test_cancellation_fee_deduction() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = token_client(&env, &token);

    // Initial balances
    assert_eq!(token_client.balance(&player1), 1000);

    let match_id = helpers::create_default_match(&client, &env, &player1, &player2, &token, "test_fee_game");

    // Player 1 deposits
    client.deposit(&match_id, &player1);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&contract_id), 100);

    // Default fee is 1%. Stake is 100. Fee should be 1.
    // Player 1 cancels
    client.cancel_match(&match_id, &player1);

    // Contract should refund 99 to Player 1, and send 1 to admin (default treasury)
    assert_eq!(token_client.balance(&player1), 999);
    assert_eq!(token_client.balance(&admin), 1); // Admin starts with 0
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_cancellation_no_fee_if_no_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = token_client(&env, &token);

    let match_id = helpers::create_default_match(&client, &env, &player1, &player2, &token, "test_no_fee_game");

    // Neither player deposits
    // Player 1 cancels
    client.cancel_match(&match_id, &player1);

    // Player 1 should not be charged since they didn't deposit
    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&admin), 0);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_cancellation_fee_with_custom_config() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = token_client(&env, &token);

    let new_treasury = Address::generate(&env);
    
    // Admin sets new config: 5% fee
    let config = types::ProtocolConfig {
        cancellation_fee_basis_points: 500, // 5%
        treasury: new_treasury.clone(),
    };
    
    env.mock_all_auths();
    client.set_protocol_config(&config);

    let match_id = helpers::create_default_match(&client, &env, &player1, &player2, &token, "test_custom_fee_game");

    // Player 2 deposits
    client.deposit(&match_id, &player2);

    // Player 1 cancels
    client.cancel_match(&match_id, &player1);

    // Player 2 should be refunded 95. Treasury gets 5.
    assert_eq!(token_client.balance(&player2), 995);
    assert_eq!(token_client.balance(&new_treasury), 5);
    assert_eq!(token_client.balance(&contract_id), 0);
}
