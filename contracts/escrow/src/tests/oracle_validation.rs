use super::*;

#[test]
fn test_submit_result_only_by_oracle() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unauthorized_actor = Address::generate(&env);
    let result = client.try_submit_result(&0, &unauthorized_actor, &1);

    assert!(
        result.is_err(),
        "submit_result must only be authorized by oracle"
    );
}

#[test]
fn test_submit_result_with_valid_winner() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_result(&0, &oracle, &1);
    assert!(result.is_ok(), "oracle can submit valid result");
}

#[test]
fn test_submit_result_with_draw() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_draw(&0, &oracle);
    assert!(result.is_ok(), "oracle can submit draw result");
}

#[test]
fn test_submit_result_on_inactive_match_rejected() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "inactive_oracle_game"),
        &Platform::Lichess,
    );

    let result = client.try_submit_result(&match_id, &oracle, &1);
    assert!(
        result.is_err(),
        "oracle result submission on inactive match must be rejected"
    );
}

#[test]
fn test_submit_result_invalid_winner_rejected() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let invalid_winner = 3i32;
    let result = client.try_submit_result(&0, &oracle, &invalid_winner);

    assert!(
        result.is_err(),
        "oracle result with invalid winner must be rejected"
    );
}

#[test]
fn test_duplicate_oracle_result_rejected() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&0, &oracle, &1);
    let result = client.try_submit_result(&0, &oracle, &2);

    assert!(
        result.is_err(),
        "duplicate oracle result submission must be rejected"
    );
}

#[test]
fn test_oracle_change_authorization() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle1 = Address::generate(&env);
    let oracle2 = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle1, &admin);
    client.set_oracle(&oracle2);

    let stored_oracle: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::Oracle).unwrap()
    });

    assert_eq!(stored_oracle, oracle2, "oracle address must be updated");
}

#[test]
fn test_oracle_can_submit_results_for_different_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "oracle_match1"),
        &Platform::Lichess,
    );
    let match2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "oracle_match2"),
        &Platform::Lichess,
    );

    client.deposit(&match1, &player1);
    client.deposit(&match1, &player2);
    client.deposit(&match2, &player1);
    client.deposit(&match2, &player2);

    let result1 = client.try_submit_result(&match1, &oracle, &1);
    let result2 = client.try_submit_result(&match2, &oracle, &2);

    assert!(result1.is_ok(), "oracle must submit result for first match");
    assert!(result2.is_ok(), "oracle must submit result for second match");
}

#[test]
fn test_get_oracle_address_returns_initial_oracle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle, &admin);

    let stored_oracle: Address = client.get_oracle_address();
    assert_eq!(stored_oracle, oracle, "get_oracle_address must return initial oracle");
}

#[test]
fn test_get_oracle_address_after_update_oracle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle1 = Address::generate(&env);
    let oracle2 = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle1, &admin);
    client.set_oracle(&oracle2);

    let stored_oracle: Address = client.get_oracle_address();
    assert_eq!(
        stored_oracle, oracle2,
        "get_oracle_address must return updated oracle"
    );
}

#[test]
fn test_get_oracle_address_uninitialized_contract() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    // Try to get oracle address before initialization
    let result = client.try_get_oracle_address();
    assert!(result.is_err(), "get_oracle_address must fail before initialization");
}
