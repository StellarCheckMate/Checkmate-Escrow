use super::*;

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
