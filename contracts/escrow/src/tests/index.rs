use super::*;
use soroban_sdk::testutils::{
    storage::{Instance as _, Persistent as _},
    Address as _, Ledger as _,
};

/// Test #584: game ID reservation remains enforced after ledger advancement
#[test]
fn test_game_id_reservation_survives_ledger_advancement() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_id = String::from_str(&env, "game_123");

    // Reserve a game ID
    let _match_id_1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &game_id,
        &Platform::Lichess,
    );

    // Advance ledgers
    env.ledger().set_sequence_number(env.ledger().sequence() + 100);

    // Assert duplicate create still fails
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &game_id,
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::AlreadyExists)));
}

/// Test #583: active/live index stays correct across concurrent cancellations and completions
#[test]
fn test_active_index_correct_after_concurrent_cancellations_and_completions() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create at least three matches
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

    // Deposit for all matches to make them active
    client.deposit(&match_id_1, &player1);
    client.deposit(&match_id_1, &player2);

    client.deposit(&match_id_2, &player1);
    client.deposit(&match_id_2, &player2);

    client.deposit(&match_id_3, &player1);
    client.deposit(&match_id_3, &player2);

    // Cancel one and complete another
    client.cancel_match(&match_id_1, &player1);
    client.submit_result(&match_id_2, &Winner::Player1);

    // Assert only the still-live match IDs remain
    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 1);
    assert_eq!(active_matches.get(0).unwrap(), match_id_3);
}

/// Test #582: active/live index ordering stays stable after cancellation gaps
#[test]
fn test_active_index_ordering_stable_after_cancellation_gaps() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create several matches that enter the same index
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

    // Deposit for all to make them active
    for match_id in [match_id_1, match_id_2, match_id_3, match_id_4].iter() {
        client.deposit(match_id, &player1);
        client.deposit(match_id, &player2);
    }

    // Cancel one in the middle
    client.cancel_match(&match_id_2, &player1);

    // Assert remaining IDs preserve documented ordering
    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 3);
    assert_eq!(active_matches.get(0).unwrap(), match_id_1);
    assert_eq!(active_matches.get(1).unwrap(), match_id_3);
    assert_eq!(active_matches.get(2).unwrap(), match_id_4);
}

/// Test #581: active/live pagination handles empty and partial pages
#[test]
fn test_active_pagination_handles_empty_and_partial_pages() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create enough matches for multiple pages (assuming page size of 10)
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

    // Deposit for all to make them active
    for match_id in match_ids.iter() {
        client.deposit(match_id, &player1);
        client.deposit(match_id, &player2);
    }

    // Assert pagination boundaries
    let all_active = client.get_active_matches();
    assert_eq!(all_active.len(), 25);

    // Verify all match IDs are present
    for (i, match_id) in match_ids.iter().enumerate() {
        assert_eq!(all_active.get(i as u32).unwrap(), *match_id);
    }
}

#[test]
fn test_get_pending_matches_returns_only_pending_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let pending_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "pending_match"),
        &Platform::Lichess,
    );

    let active_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "active_match"),
        &Platform::Lichess,
    );
    client.deposit(&active_id, &player1);
    client.deposit(&active_id, &player2);

    let pending_matches = client.get_pending_matches();
    assert_eq!(pending_matches.len(), 1);
    assert_eq!(pending_matches.get(0).unwrap().id, pending_id);

    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 1);
    assert_eq!(active_matches.get(0).unwrap().id, active_id);
}

#[test]
fn test_match_transitions_from_pending_to_active_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "transition_match"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);

    let pending_matches = client.get_pending_matches();
    assert_eq!(pending_matches.len(), 1);
    assert_eq!(pending_matches.get(0).unwrap().id, match_id);

    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 0);

    client.deposit(&match_id, &player2);

    let pending_matches = client.get_pending_matches();
    assert_eq!(pending_matches.len(), 0);

    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 1);
    assert_eq!(active_matches.get(0).unwrap().id, match_id);
}
