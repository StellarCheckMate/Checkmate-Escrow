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
        !snapshots.is_empty(),
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
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let history = client.get_balance_snaps_paginated(&player1, &0, &100);
    assert!(
        !history.is_empty(),
        "player balance history must have records after match completion"
    );
}

#[test]
fn test_get_escrow_balance_zero_for_uninitialized_match() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_get_escrow_balance(&9999);
    assert_eq!(
        result,
        Err(Ok(Error::MatchNotFound)),
        "escrow balance lookup for a non-existent match must report MatchNotFound"
    );
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
        !player1_history.is_empty(),
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
    client.submit_result(&match_id, &Winner::Player1, &oracle);

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

    for i in 0..3u32 {
        let game_id = format!("{:08x}", i);
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &game_id),
            &Platform::Lichess,
        );
        client.deposit(&match_id, &player1);
        client.deposit(&match_id, &player2);
        client.submit_result(&match_id, &Winner::Player1, &oracle);
    }

    let history = client.get_balance_snaps_paginated(&player1, &0, &100);
    assert!(
        history.len() >= 3,
        "player balance history must track multiple matches"
    );
}
