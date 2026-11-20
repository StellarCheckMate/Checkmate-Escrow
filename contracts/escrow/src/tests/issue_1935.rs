/// #1935 — get_match_depositors returns the depositor addresses for a match.
use super::*;

/// Returns no addresses when neither player has deposited.
#[test]
fn test_get_match_depositors_empty_before_any_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "dep1935a"),
        &Platform::Lichess,
    );

    let depositors = client.get_match_depositors(&match_id).unwrap();
    assert_eq!(depositors.len(), 0, "no depositors before any deposit");
}

/// Returns only player1 after their deposit.
#[test]
fn test_get_match_depositors_returns_player1_after_first_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "dep1935b"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);

    let depositors = client.get_match_depositors(&match_id).unwrap();
    assert_eq!(depositors.len(), 1, "one depositor after player1 deposits");
    assert_eq!(depositors.get(0).unwrap(), player1);
}

/// Returns both players (player1 first) after both deposit.
#[test]
fn test_get_match_depositors_returns_both_players_after_full_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "dep1935c"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let depositors = client.get_match_depositors(&match_id).unwrap();
    assert_eq!(depositors.len(), 2, "two depositors after both players deposit");
    assert_eq!(depositors.get(0).unwrap(), player1, "player1 appears first");
    assert_eq!(depositors.get(1).unwrap(), player2, "player2 appears second");
}

/// Returns an error for a non-existent match ID.
#[test]
fn test_get_match_depositors_errors_for_unknown_match() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_get_match_depositors(&9999);
    assert!(result.is_err(), "should return error for unknown match ID");
}

/// Depositors list is preserved in a completed (paid out) match.
#[test]
fn test_get_match_depositors_preserved_after_match_completes() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "dep1935d"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let depositors = client.get_match_depositors(&match_id).unwrap();
    assert_eq!(depositors.len(), 2, "depositors preserved after match completes");
    assert_eq!(depositors.get(0).unwrap(), player1);
    assert_eq!(depositors.get(1).unwrap(), player2);
}
