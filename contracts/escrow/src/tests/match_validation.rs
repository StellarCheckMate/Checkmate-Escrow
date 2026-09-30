use super::*;

#[derive(Clone, Copy)]
enum CreationVariant {
    Standard,
    Tournament,
    Conversion,
    Referrer,
}

#[derive(Clone, Copy)]
enum InvalidCreation {
    Paused,
    FrozenPlayer1,
    FrozenPlayer2,
    BlacklistedToken,
    TokenNotAllowlisted,
    NonStablecoin,
    BelowMinimum,
    AboveMaximum,
    InsufficientTier,
    InvalidGameId,
    SamePlayer,
    ContractAsOpponent,
    DuplicateGameId,
}

fn try_create_variant(
    client: &EscrowContractClient,
    env: &Env,
    variant: CreationVariant,
    player1: &Address,
    player2: &Address,
    stake: i128,
    token: &Address,
    game_id: &String,
) -> bool {
    match variant {
        CreationVariant::Standard => client
            .try_create_match(player1, player2, &stake, token, game_id, &Platform::Lichess)
            .is_err(),
        CreationVariant::Tournament => client
            .try_create_match_tournament(
                &1,
                &2,
                player1,
                player2,
                &stake,
                token,
                game_id,
                &Platform::Lichess,
            )
            .is_err(),
        CreationVariant::Conversion => client
            .try_create_match_with_conversion(
                player1,
                player2,
                &stake,
                token,
                token,
                &10_000_000,
                game_id,
                &Platform::Lichess,
            )
            .is_err(),
        CreationVariant::Referrer => {
            let referrer = Address::generate(env);
            client
                .try_create_match_with_referrer(
                    player1,
                    player2,
                    &stake,
                    token,
                    game_id,
                    &Platform::Lichess,
                    &referrer,
                )
                .is_err()
        }
    }
}

#[test]
fn test_all_creation_variants_share_validation_rules() {
    let variants = [
        CreationVariant::Standard,
        CreationVariant::Tournament,
        CreationVariant::Conversion,
        CreationVariant::Referrer,
    ];
    let invalid_cases = [
        InvalidCreation::Paused,
        InvalidCreation::FrozenPlayer1,
        InvalidCreation::FrozenPlayer2,
        InvalidCreation::BlacklistedToken,
        InvalidCreation::TokenNotAllowlisted,
        InvalidCreation::NonStablecoin,
        InvalidCreation::BelowMinimum,
        InvalidCreation::AboveMaximum,
        InvalidCreation::InsufficientTier,
        InvalidCreation::InvalidGameId,
        InvalidCreation::SamePlayer,
        InvalidCreation::ContractAsOpponent,
        InvalidCreation::DuplicateGameId,
    ];

    for variant in variants {
        for invalid_case in invalid_cases {
            let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
            let client = EscrowContractClient::new(&env, &contract_id);
            client.register_bracket(&admin, &1, &admin);

            let mut stake = 100;
            let mut game_id = String::from_str(&env, "abcd1234");
            let mut opponent = player2.clone();
            match invalid_case {
                InvalidCreation::Paused => {
                    client.pause(&admin);
                }
                InvalidCreation::FrozenPlayer1 => {
                    client.admin_freeze_player(&player1, &String::from_str(&env, "frozen"));
                }
                InvalidCreation::FrozenPlayer2 => {
                    client.admin_freeze_player(&player2, &String::from_str(&env, "frozen"));
                }
                InvalidCreation::BlacklistedToken => {
                    client.add_token_to_blacklist(&token, &String::from_str(&env, "blocked"));
                }
                InvalidCreation::TokenNotAllowlisted => {
                    client.add_allowed_token(&Address::generate(&env));
                }
                InvalidCreation::NonStablecoin => {
                    let mut config = client.get_protocol_config().unwrap();
                    config.stablecoin_only_mode = true;
                    client.set_protocol_config(&config).unwrap();
                }
                InvalidCreation::BelowMinimum => stake = 0,
                InvalidCreation::AboveMaximum => {
                    let mut config = client.get_protocol_config().unwrap();
                    config.maximum_stake = Some(50);
                    client.set_protocol_config(&config).unwrap();
                }
                InvalidCreation::InsufficientTier => stake = 101,
                InvalidCreation::InvalidGameId => game_id = String::from_str(&env, "bad!"),
                InvalidCreation::SamePlayer => opponent = player1.clone(),
                InvalidCreation::ContractAsOpponent => opponent = contract_id.clone(),
                InvalidCreation::DuplicateGameId => {
                    client.create_match(
                        &player1,
                        &player2,
                        &100,
                        &token,
                        &game_id,
                        &Platform::Lichess,
                    );
                }
            }

            assert!(
                try_create_variant(
                    &client,
                    &env,
                    variant,
                    &player1,
                    &opponent,
                    stake,
                    &token,
                    &game_id,
                ),
                "creation variant accepted an invalid case"
            );
        }
    }
}

#[test]
fn test_tournament_registration_and_round_persistence() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.register_bracket(&admin, &9, &admin);

    let match_id = client.create_match_tournament(
        &9,
        &4,
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "round123"),
        &Platform::Lichess,
    );

    let created_match = client.get_match(&match_id).unwrap();
    assert_eq!(created_match.bracket_id, Some(9));
    assert_eq!(created_match.round, Some(4));
}

#[test]
fn test_tournament_match_requires_registered_bracket() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match_tournament(
        &999,
        &1,
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "round999"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_tournament_match_requires_organizer_authorization() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.register_bracket(&admin, &9, &admin);
    let game_id = String::from_str(&env, "auth1234");
    env.mock_auths(&[MockAuth {
        address: &player1,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "create_match_tournament",
            args: (
                9u64,
                4u32,
                player1.clone(),
                player2.clone(),
                100i128,
                token.clone(),
                game_id.clone(),
                Platform::Lichess,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_create_match_tournament(
        &9,
        &4,
        &player1,
        &player2,
        &100,
        &token,
        &game_id,
        &Platform::Lichess,
    );
    assert!(result.is_err(), "registered organizer must authorize the match");
}

#[test]
fn test_create_match_with_zero_stake_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &0,
        &token,
        &String::from_str(&env, "c12c3c42"),
        &Platform::Lichess,
    );

    assert!(
        result.is_err(),
        "match creation with zero stake must be rejected"
    );
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
        &String::from_str(&env, "6bff5692"),
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
        &String::from_str(&env, "98f83834"),
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
fn test_create_match_tournament_with_frozen_player_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.register_bracket(&admin, &7, &admin);
    client.admin_freeze_player(&player1, &String::from_str(&env, "frozen"));

    let result = client.try_create_match_tournament(
        &7,
        &1,
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "a1b2c3d4"),
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player must not be able to create a tournament match"
    );
}

#[test]
fn test_deposit_insufficient_balance_rejected() {
    let (env, contract_id, _oracle, _player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let asset_client = StellarAssetClient::new(&env, &token);
    let broke_player = Address::generate(&env);
    asset_client.mint(&broke_player, &5);

    let match_id = client.create_match(
        &broke_player,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ae5352c2"),
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
    let (env, contract_id, oracle, player1, _player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.submit_result(&match_id, &Winner::Player1, &oracle);

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
        &String::from_str(&env, "ff109d4b"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    let result = client.try_deposit(&match_id, &player1);

    assert!(
        result.is_err(),
        "duplicate deposit from same player must be rejected"
    );
}
