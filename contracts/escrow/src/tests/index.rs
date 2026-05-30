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
