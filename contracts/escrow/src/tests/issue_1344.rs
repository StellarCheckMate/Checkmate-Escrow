//! Tests for issue #1344: get_dispute_details(match_id)
use super::*;

#[test]
fn test_get_dispute_details_returns_dispute_for_match() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Enable dispute period so result goes to PendingResult
    client.set_dispute_period(&100);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "aa1b2c3d"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // Give disputer enough bond balance
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player2, &10);

    let dispute_id = client.dispute_oracle_result(
        &match_id,
        &player2,
        &String::from_str(&env, "evidence_hash_here"),
    );

    // get_dispute_details should return the same dispute as get_dispute
    let by_match = client.get_dispute_details(&match_id);
    let by_id = client.get_dispute(&dispute_id);

    assert_eq!(by_match.id, by_id.id);
    assert_eq!(by_match.match_id, match_id);
}

#[test]
fn test_get_dispute_details_returns_error_when_no_dispute() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "bb2c3d4e"),
        &Platform::Lichess,
    );

    let result = client.try_get_dispute_details(&match_id);
    assert!(
        matches!(result, Err(Ok(Error::DisputeNotFound))),
        "expected DisputeNotFound, got: {:?}",
        result
    );
}
