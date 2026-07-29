use super::*;

#[test]
fn test_create_match_with_zero_stake_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &0,
        &token,
        &String::from_str(&env, "zero_stake_game"),
        &Platform::Lichess,
    );

    assert!(result.is_err(), "match creation with zero stake must be rejected");
}

#[test]
fn test_create_match_with_same_player_rejected() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player1,
        &100,
        &token,
        &String::from_str(&env, "same_player_game"),
        &Platform::Lichess,
    );

    assert!(
        result.is_err(),
        "match creation with same player must be rejected"
    );
}

#[test]
fn test_create_match_with_excessive_stake_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let excessive_stake = i128::MAX;

    let result = client.try_create_match(
        &player1,
        &player2,
        &excessive_stake,
        &token,
        &String::from_str(&env, "excessive_stake_game"),
        &Platform::Lichess,
    );

    assert!(
        result.is_err(),
        "match creation with excessive stake must be rejected"
    );
}

#[test]
fn test_create_match_with_empty_game_id_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, ""),
        &Platform::Lichess,
    );

    assert!(
        result.is_err(),
        "match creation with empty game_id must be rejected"
    );
}

#[test]
fn test_deposit_insufficient_balance_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let asset_client = StellarAssetClient::new(&env, &token);
    let broke_player = Address::generate(&env);
    asset_client.mint(&broke_player, &5);

    let match_id = client.create_match(
        &broke_player,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "insufficient_balance_game"),
        &Platform::Lichess,
    );

    let result = client.try_deposit(&match_id, &broke_player);
    assert!(
        result.is_err(),
        "deposit with insufficient balance must be rejected"
    );
}

#[test]
fn test_deposit_on_completed_match_rejected() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&0, &oracle, &1);

    let result = client.try_deposit(&0, &player1);
    assert!(
        result.is_err(),
        "deposit on completed match must be rejected"
    );
}

#[test]
fn test_multiple_deposits_from_same_player_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "duplicate_deposit_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    let result = client.try_deposit(&match_id, &player1);

    assert!(
        result.is_err(),
        "duplicate deposit from same player must be rejected"
    );
}
