use super::*;

#[test]
fn test_create_match_zero_stake() {
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

    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

// Issue #1156: `platform` is a typed enum (`Platform::Lichess` /
// `Platform::ChessDotCom`) — the ABI rejects any other discriminant before
// the call reaches contract code, so both accepted variants must succeed.
#[test]
fn test_create_match_accepts_both_platform_variants() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let lichess_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "abcd1234"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&lichess_id).platform, Platform::Lichess);

    let chess_com_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "1234567"),
        &Platform::ChessDotCom,
    );
    assert_eq!(
        client.get_match(&chess_com_id).platform,
        Platform::ChessDotCom
    );
}

// Issue #1157: `game_id` must match the format expected for its `platform`.
fn setup_for_platform_and_game_id() -> (Env, Address, Address, Address, Address) {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    (env, contract_id, player1, player2, token)
}

#[test]
fn test_create_match_valid_lichess_game_id() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "abcd1234"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&id).game_id, String::from_str(&env, "abcd1234"));
}

#[test]
fn test_create_match_invalid_lichess_game_id_wrong_length() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "abcd12345"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

#[test]
fn test_create_match_invalid_lichess_game_id_non_alphanumeric() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "abcd-123"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

#[test]
fn test_create_match_valid_chess_com_game_id() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "123456789"),
        &Platform::ChessDotCom,
    );
    assert_eq!(
        client.get_match(&id).game_id,
        String::from_str(&env, "123456789")
    );
}

#[test]
fn test_create_match_invalid_chess_com_game_id_too_short() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "123456"),
        &Platform::ChessDotCom,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

#[test]
fn test_create_match_invalid_chess_com_game_id_non_numeric() {
    let (env, contract_id, player1, player2, token, ..) = setup_for_platform_and_game_id();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "12a4567"),
        &Platform::ChessDotCom,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

// Issue #1158: `minimum_stake` is enforced in `create_match`, and only the
// admin can change it via `set_minimum_stake`.
#[test]
fn test_create_match_below_minimum_stake_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_minimum_stake(&50);

    let result = client.try_create_match(
        &player1,
        &player2,
        &10,
        &token,
        &String::from_str(&env, "low_stake_game"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));

    // Raising the minimum doesn't affect stakes that meet it.
    let id = client.create_match(
        &player1,
        &player2,
        &50,
        &token,
        &String::from_str(&env, "min_stake_game"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&id).stake_amount, 50);
    let _ = admin;
}

#[test]
fn test_set_minimum_stake_updates_protocol_config() {
    let (env, contract_id, ..) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_protocol_config().minimum_stake, 1);
    client.set_minimum_stake(&25);
    assert_eq!(client.get_protocol_config().minimum_stake, 25);
}

#[test]
fn test_set_minimum_stake_rejects_negative() {
    let (env, contract_id, ..) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_set_minimum_stake(&-1);
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}
