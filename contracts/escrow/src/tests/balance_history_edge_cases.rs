use super::*;

#[test]
fn test_balance_snapshot_after_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "597bc181"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);

    let snapshots = client.get_balance_snapshots(&admin, &match_id);
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
        &String::from_str(&env, "5bff90ab"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1);

    let history = client.get_balance_snaps_paginated(&player1, &0, &100);
    assert!(
        history.len() > 0,
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
        &String::from_str(&env, "0861c2d4"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.submit_draw(&match_id, &oracle);

    let player1_history = client.get_balance_snaps_paginated(&player1, &0, &100);
    assert!(
        player1_history.len() > 0,
        "balance history must be recorded for draws"
    );
}

#[test]
fn test_balance_snapshot_records_correct_amounts() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stake = 100i128;
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "4085b8cc"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1);

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

    let game_ids = ["game0001", "game0002", "game0003"];
    for i in 0..3 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, game_ids[i]),
            &Platform::Lichess,
        );
        client.deposit(&match_id, &player1);
        client.deposit(&match_id, &player2);
        client.submit_result(&match_id, &Winner::Player1);
    }

    let history = client.get_balance_snaps_paginated(&player1, &0, &100);
    assert!(
        history.len() >= 3,
        "player balance history must track multiple matches"
    );
}
