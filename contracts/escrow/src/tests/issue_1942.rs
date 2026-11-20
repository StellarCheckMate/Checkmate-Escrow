/// Tests for issue #1942: get_match_dispute view.
use super::*;

/// When no dispute has been raised, the view returns None.
#[test]
fn get_match_dispute_returns_none_when_no_dispute() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(1000);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "aabb1122"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // PendingResult state but no dispute raised yet.
    assert!(client.get_match_dispute(&match_id).is_none());
}

/// Once a dispute is raised, the view returns the expected dispute record.
#[test]
fn get_match_dispute_returns_dispute_after_it_is_raised() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(1000);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ccdd3344"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // player2 raises a dispute.
    let evidence = String::from_str(
        &env,
        "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
    );
    let dispute_id = client.dispute_oracle_result(&match_id, &player2, &evidence);

    let maybe_dispute = client.get_match_dispute(&match_id);
    assert!(maybe_dispute.is_some(), "expected a dispute to be returned");

    let dispute = maybe_dispute.unwrap();
    assert_eq!(dispute.id, dispute_id, "dispute id must match");
    assert_eq!(dispute.match_id, match_id, "dispute.match_id must match");
    assert_eq!(dispute.disputer, player2, "disputer must be player2");
    assert_eq!(
        dispute.state,
        DisputeState::Active,
        "newly raised dispute must be Active"
    );
}

/// The view returns None for a match that does not exist.
#[test]
fn get_match_dispute_returns_none_for_nonexistent_match() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert!(client.get_match_dispute(&99999).is_none());
}
