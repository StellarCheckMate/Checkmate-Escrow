use super::*;

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
