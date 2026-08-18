use super::*;

fn complete_match_with_stake(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
    stake: i128,
) {
    let match_id = client.create_match(
        player1,
        player2,
        &stake,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&match_id, player1);
    client.deposit(&match_id, player2);
    client.submit_result(&match_id, &Winner::Player1);
}

#[test]
fn test_tier_progression_tracks_completed_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset_client = StellarAssetClient::new(&env, &token);

    asset_client.mint(&player1, &10_000);
    asset_client.mint(&player2, &10_000);

    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Bronze);
    assert_eq!(client.min_tier_stake(&PlayerTier::Bronze), 1);
    assert_eq!(client.max_tier_stake(&PlayerTier::Bronze), 100);
    assert_eq!(client.min_tier_stake(&PlayerTier::Silver), 101);
    assert_eq!(client.max_tier_stake(&PlayerTier::Silver), 500);
    assert_eq!(client.min_tier_stake(&PlayerTier::Gold), 501);
    assert_eq!(client.max_tier_stake(&PlayerTier::Gold), 1_000);
    assert_eq!(client.min_tier_stake(&PlayerTier::Platinum), 1_001);
    assert_eq!(client.max_tier_stake(&PlayerTier::Platinum), i128::MAX);

    for i in 0..3 {
        complete_match_with_stake(
            &client,
            &env,
            &player1,
            &player2,
            &token,
            &format!("{:08x}", i),
            100,
        );
    }
    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Silver);

    for i in 3..6 {
        complete_match_with_stake(
            &client,
            &env,
            &player1,
            &player2,
            &token,
            &format!("{:08x}", i),
            500,
        );
    }
    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Gold);

    for i in 6..10 {
        complete_match_with_stake(
            &client,
            &env,
            &player1,
            &player2,
            &token,
            &format!("{:08x}", i),
            1_000,
        );
    }
    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Platinum);
}

#[test]
fn test_create_match_enforces_tier_stake_caps() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &101,
        &token,
        &String::from_str(&env, "9519079c"),
        &Platform::Lichess,
    );

    assert_eq!(result, Err(Ok(Error::TierStakeNotAllowed)));
}

#[test]
fn test_deposit_rechecks_current_player_tier() {
    let (env, contract_id, _oracle, player1, player2, player3, _player4, token, _admin) =
        setup_with_four_players();
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset_client = StellarAssetClient::new(&env, &token);

    asset_client.mint(&player1, &10_000);
    asset_client.mint(&player2, &10_000);
    asset_client.mint(&player3, &10_000);

    let pending_match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "0813206a"),
        &Platform::Lichess,
    );

    for i in 0..3 {
        complete_match_with_stake(
            &client,
            &env,
            &player1,
            &player3,
            &token,
            &format!("{:08x}", i),
            100,
        );
    }

    assert_eq!(client.tier_from_match_count(&player1), PlayerTier::Silver);

    let result = client.try_deposit(&pending_match_id, &player1);
    assert_eq!(result, Err(Ok(Error::TierStakeNotAllowed)));
}
