//! Tests for issue #1343: update_heartbeat / on-chain heartbeat for match liveness
use super::*;

#[test]
fn test_update_heartbeat_refreshes_timestamp() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ab1c2d3e"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    // Advance ledger time
    env.ledger().with_mut(|l| l.timestamp = 10_000);

    client.update_heartbeat(&match_id, &player1);

    let m = client.get_match(&match_id);
    assert_eq!(m.last_heartbeat, 10_000);
}

#[test]
fn test_update_heartbeat_rejects_non_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cd3e4f5a"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let stranger = Address::generate(&env);
    let result = client.try_update_heartbeat(&match_id, &stranger);
    assert!(result.is_err(), "non-player must not be able to heartbeat");
}

#[test]
fn test_rollback_rejected_within_heartbeat_window() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ef5a6b7c"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    // Advance time to just before the rollback window expires
    env.ledger().with_mut(|l| l.timestamp = 1_000);
    client.update_heartbeat(&match_id, &player2);

    // Advance time within the 24h window from the heartbeat
    env.ledger().with_mut(|l| l.timestamp = 1_000 + ROLLBACK_WINDOW_SECONDS - 1);

    // Rollback should succeed (within window)
    let result = client.try_dispute_and_rollback_match(
        &match_id,
        &player1,
        &String::from_str(&env, "disconnect"),
    );
    assert!(result.is_ok(), "rollback must succeed within heartbeat window");
}
