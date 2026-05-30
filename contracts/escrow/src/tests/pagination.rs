use super::*;

/// Test #579: player history index excludes unrelated matches for other players
#[test]
fn test_player_history_index_excludes_unrelated_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);

    // Mint tokens for player3 and player4
    let token_client = TokenClient::new(&env, &token);
    token_client.mint(&player3, &1000);
    token_client.mint(&player4, &1000);

    // Create matches for player1 and player2
    let match_id_1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_1"),
        &Platform::Lichess,
    );

    let match_id_2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_2"),
        &Platform::Lichess,
    );

    // Create matches for player3 and player4
    let match_id_3 = client.create_match(
        &player3,
        &player4,
        &100,
        &token,
        &String::from_str(&env, "game_3"),
        &Platform::Lichess,
    );

    let match_id_4 = client.create_match(
        &player3,
        &player4,
        &100,
        &token,
        &String::from_str(&env, "game_4"),
        &Platform::Lichess,
    );

    // Assert player1 only receives their own match IDs
    let player1_matches = client.get_player_matches(&player1);
    assert_eq!(player1_matches.len(), 2);
    assert_eq!(player1_matches.get(0).unwrap(), match_id_1);
    assert_eq!(player1_matches.get(1).unwrap(), match_id_2);

    // Assert player2 only receives their own match IDs
    let player2_matches = client.get_player_matches(&player2);
    assert_eq!(player2_matches.len(), 2);
    assert_eq!(player2_matches.get(0).unwrap(), match_id_1);
    assert_eq!(player2_matches.get(1).unwrap(), match_id_2);

    // Assert player3 only receives their own match IDs
    let player3_matches = client.get_player_matches(&player3);
    assert_eq!(player3_matches.len(), 2);
    assert_eq!(player3_matches.get(0).unwrap(), match_id_3);
    assert_eq!(player3_matches.get(1).unwrap(), match_id_4);

    // Assert player4 only receives their own match IDs
    let player4_matches = client.get_player_matches(&player4);
    assert_eq!(player4_matches.len(), 2);
    assert_eq!(player4_matches.get(0).unwrap(), match_id_3);
    assert_eq!(player4_matches.get(1).unwrap(), match_id_4);
}

/// Test #578: get_player_matches preserves insertion order
#[test]
fn test_get_player_matches_preserves_insertion_order() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create multiple matches for the same player
    let match_id_1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_1"),
        &Platform::Lichess,
    );

    let match_id_2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_2"),
        &Platform::Lichess,
    );

    let match_id_3 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_3"),
        &Platform::Lichess,
    );

    let match_id_4 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_4"),
        &Platform::Lichess,
    );

    // Assert returned IDs are in expected order
    let player1_matches = client.get_player_matches(&player1);
    assert_eq!(player1_matches.len(), 4);
    assert_eq!(player1_matches.get(0).unwrap(), match_id_1);
    assert_eq!(player1_matches.get(1).unwrap(), match_id_2);
    assert_eq!(player1_matches.get(2).unwrap(), match_id_3);
    assert_eq!(player1_matches.get(3).unwrap(), match_id_4);
}

/// Test #577: get_match_count increments correctly
#[test]
fn test_get_match_count_increments_correctly() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Initial count should be 0
    let count = client.get_match_count();
    assert_eq!(count, 0);

    // Create first match
    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_1"),
        &Platform::Lichess,
    );
    let count = client.get_match_count();
    assert_eq!(count, 1);

    // Create second match
    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_2"),
        &Platform::Lichess,
    );
    let count = client.get_match_count();
    assert_eq!(count, 2);

    // Create third match
    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_3"),
        &Platform::Lichess,
    );
    let count = client.get_match_count();
    assert_eq!(count, 3);

    // Create fourth match
    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_4"),
        &Platform::Lichess,
    );
    let count = client.get_match_count();
    assert_eq!(count, 4);
}
