use super::*;

/// Test #580: player-match pagination handles empty and partial pages
#[test]
fn test_player_match_pagination_handles_empty_and_partial_pages() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 25 matches for player1
    let mut match_ids = Vec::new();
    for i in 0..25 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("game_{}", i)),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    // Query player1's matches with paginated API
    let player1_page_0 = client.get_player_matches_paginated(&player1, &0, &5);
    assert_eq!(player1_page_0.len(), 5);
    for (i, match_id) in player1_page_0.iter().enumerate() {
        assert_eq!(*match_id, match_ids[i]);
    }

    let player1_page_1 = client.get_player_matches_paginated(&player1, &5, &10);
    assert_eq!(player1_page_1.len(), 10);
    for (i, match_id) in player1_page_1.iter().enumerate() {
        assert_eq!(*match_id, match_ids[5 + i]);
    }

    let player1_page_2 = client.get_player_matches_paginated(&player1, &20, &10);
    assert_eq!(player1_page_2.len(), 5);
    for (i, match_id) in player1_page_2.iter().enumerate() {
        assert_eq!(*match_id, match_ids[20 + i]);
    }

    let player1_page_3 = client.get_player_matches_paginated(&player1, &25, &10);
    assert_eq!(player1_page_3.len(), 0);

    // Verify the existing getter still returns the full list for compatibility.
    let player1_matches = client.get_player_matches(&player1);
    assert_eq!(player1_matches.len(), 25);

    // Query player2's matches (should have 25 as well)
    let player2_matches = client.get_player_matches(&player2);
    assert_eq!(player2_matches.len(), 25);

    // Query a player with no matches
    let player3 = Address::generate(&env);
    let player3_matches = client.get_player_matches(&player3);
    assert_eq!(player3_matches.len(), 0);
}

/// Test #581: player match pagination returns empty page for zero limit and offset beyond end
#[test]
fn test_player_match_pagination_zero_limit_and_offset_beyond_end() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 10 matches for player1
    let mut match_ids = Vec::new();
    for i in 0..10 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("game_{}", i)),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    let zero_limit = client.get_player_matches_paginated(&player1, &0, &0);
    assert_eq!(zero_limit.len(), 0);

    let beyond_offset = client.get_player_matches_paginated(&player1, &15, &5);
    assert_eq!(beyond_offset.len(), 0);

    let partial_page = client.get_player_matches_paginated(&player1, &8, &5);
    assert_eq!(partial_page.len(), 2);
    assert_eq!(partial_page.get(0).unwrap(), match_ids[8]);
    assert_eq!(partial_page.get(1).unwrap(), match_ids[9]);
}

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
