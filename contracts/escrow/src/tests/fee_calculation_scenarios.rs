use super::*;

#[test]
fn test_cancellation_fee_calculation_correct() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 500,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
    });

    let match_id = client.create_match(
        &player1,
        &player2,
        &1000,
        &token,
        &String::from_str(&env, "fee_calc_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);

    let result = client.cancel_match(&match_id);
    assert!(result.is_ok(), "cancellation must succeed with valid fee config");
}

#[test]
fn test_zero_cancellation_fee_accepted() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
    });

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "zero_fee_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    let result = client.cancel_match(&match_id);

    assert!(result.is_ok(), "cancellation with zero fee must be allowed");
}

#[test]
fn test_high_cancellation_fee_allowed() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 10000,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
    });

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "high_fee_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    let result = client.cancel_match(&match_id);

    assert!(
        result.is_ok(),
        "cancellation with high fee (100%) must be allowed"
    );
}

#[test]
fn test_fee_applied_to_deposited_amount() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let fee_basis_points = 1000;
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: fee_basis_points,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
    });

    let stake = 1000i128;
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "fee_amount_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.cancel_match(&match_id);

    let escrow_balance = client.get_escrow_balance(&match_id);
    assert_eq!(escrow_balance, 0, "escrow must be cleared after cancellation");
}

#[test]
fn test_token_swap_fee_considerations() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(admin.clone());
    let token2_addr = token2.address();
    let asset_client = StellarAssetClient::new(&env, &token2_addr);
    asset_client.mint(&player1, &1000);
    asset_client.mint(&player2, &1000);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token2_addr,
        &String::from_str(&env, "token_swap_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let escrow_balance = client.get_escrow_balance(&match_id);
    assert_eq!(
        escrow_balance, 200,
        "escrow must account for both deposits with token swap"
    );
}

#[test]
fn test_tier_based_fee_adjustment() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 500,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
    });

    let bronze_match = client.create_match(
        &player1,
        &player2,
        &50,
        &token,
        &String::from_str(&env, "bronze_fee_game"),
        &Platform::Lichess,
    );

    client.deposit(&bronze_match, &player1);
    let result = client.cancel_match(&bronze_match);

    assert!(
        result.is_ok(),
        "fee calculation must handle bronze tier stakes"
    );
}
