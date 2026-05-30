use super::*;

#[test]
fn test_is_token_allowed_returns_false_for_unknown_tokens() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown_token = Address::generate(&env);
    let result = client.is_token_allowed(&unknown_token);
    assert!(!result, "unknown token should not be allowed");
}

#[test]
fn test_add_allowed_token_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_allowed_token(&token);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        soroban_sdk::symbol_short!("token_add").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "token_add event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_token: Address = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_token, token);
}

#[test]
fn test_removed_tokens_can_no_longer_be_used_for_new_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_allowed_token(&token);
    client.remove_allowed_token(&token);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "removed_token_game"),
        &Platform::Lichess,
    );
    assert!(
        result.is_err(),
        "create_match should reject removed token"
    );
}

#[test]
fn test_multiple_approved_tokens_can_coexist_after_allowlist_enforcement_is_enabled() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token2_id = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token2_addr = token2_id.address();
    let asset_client2 = StellarAssetClient::new(&env, &token2_addr);
    asset_client2.mint(&player1, &1000);
    asset_client2.mint(&player2, &1000);

    client.add_allowed_token(&token);
    client.add_allowed_token(&token2_addr);

    let id1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_token1"),
        &Platform::Lichess,
    );
    assert_eq!(id1, 0, "first match with token1 should succeed");

    let id2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token2_addr,
        &String::from_str(&env, "game_token2"),
        &Platform::Lichess,
    );
    assert_eq!(id2, 1, "second match with token2 should succeed");

    let unknown_token = Address::generate(&env);
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &unknown_token,
        &String::from_str(&env, "game_unknown"),
        &Platform::Lichess,
    );
    assert!(
        result.is_err(),
        "create_match should reject unknown token when allowlist is enforced"
    );
}
