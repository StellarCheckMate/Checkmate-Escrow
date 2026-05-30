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
