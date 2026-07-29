use super::*;
use soroban_sdk::testutils::{
    storage::{Instance as _, Persistent as _},
    Address as _, Ledger as _,
};

#[test]
fn test_balance_snapshot_after_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "balance_snapshot_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);

    let snapshots = client.get_match_balance_snapshots(&match_id);
    assert!(
        snapshots.len() > 0,
        "balance snapshots must be recorded after deposit"
    );
}

#[test]
fn test_player_balance_history_monotonic_increase() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &50,
        &token,
        &String::from_str(&env, "monotonic_game1"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &oracle, &1);

    let history = client.get_player_balance_snapshot_paginated(&player1, &0, &100);
    assert!(
        history.records.len() > 0,
        "player balance history must have records after match completion"
    );
}

#[test]
fn test_get_escrow_balance_zero_for_uninitialized_match() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let balance = client.get_escrow_balance(&9999);
    assert_eq!(balance, 0, "escrow balance for non-existent match must be zero");
}

#[test]
fn test_player_balance_snapshot_on_draw() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "draw_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.submit_draw(&match_id, &oracle);

    let player1_history = client.get_player_balance_snapshot_paginated(&player1, &0, &100);
    assert!(
        player1_history.records.len() > 0,
        "balance history must be recorded for draws"
    );
}

#[test]
fn test_balance_snapshot_records_correct_amounts() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stake = 150i128;
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "amount_tracking_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &oracle, &1);

    let escrow_balance = client.get_escrow_balance(&match_id);
    assert_eq!(
        escrow_balance, 0,
        "escrow balance must be zero after payout"
    );
}

#[test]
fn test_player_balance_history_with_multiple_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    for i in 0..3 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("multi_match_game_{}", i)),
            &Platform::Lichess,
        );
        client.deposit(&match_id, &player1);
        client.deposit(&match_id, &player2);
        client.submit_result(&match_id, &oracle, &1);
    }

    let history = client.get_player_balance_snapshot_paginated(&player1, &0, &100);
    assert!(
        history.records.len() >= 3,
        "player balance history must track multiple matches"
    );
}
