use super::*;

// #1166 — submit_result_batch settles multiple matches in one call

#[test]
fn test_submit_result_batch_all_success() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_all_success_a"),
        &Platform::Lichess,
    );
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_all_success_b"),
        &Platform::Lichess,
    );

    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);
    client.deposit(&match_b, &player1);
    client.deposit(&match_b, &player2);

    let oracle = client.get_oracle();
    let batch = soroban_sdk::vec![
        &env,
        (match_a, Winner::Player1),
        (match_b, Winner::Player2),
    ];

    let outcomes = client.submit_result_batch(&batch, &oracle);

    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes.get(0).unwrap(), None);
    assert_eq!(outcomes.get(1).unwrap(), None);

    assert_eq!(client.get_match(&match_a).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_a).winner, Winner::Player1);
    assert_eq!(client.get_match(&match_b).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_b).winner, Winner::Player2);
}

#[test]
fn test_submit_result_batch_partial_failure() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // match_a is fully funded and will succeed.
    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_partial_ok"),
        &Platform::Lichess,
    );
    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);

    // match_b only has one deposit, so it's still Pending — submit_result
    // will fail with InvalidState (not yet Active).
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_partial_not_funded"),
        &Platform::Lichess,
    );
    client.deposit(&match_b, &player1);

    // match_c does not exist at all, so submit_result will fail with MatchNotFound.
    let match_c = client.get_match_count() + 999;

    let oracle = client.get_oracle();
    let batch = soroban_sdk::vec![
        &env,
        (match_a, Winner::Player1),
        (match_b, Winner::Player2),
        (match_c, Winner::Draw),
    ];

    let outcomes = client.submit_result_batch(&batch, &oracle);

    assert_eq!(outcomes.len(), 3);
    assert_eq!(outcomes.get(0).unwrap(), None);
    assert_eq!(outcomes.get(1).unwrap(), Some(Error::InvalidState));
    assert_eq!(outcomes.get(2).unwrap(), Some(Error::MatchNotFound));

    // The successful match settled, and the failing matches were left untouched.
    assert_eq!(client.get_match(&match_a).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_b).state, MatchState::Pending);
}

#[test]
fn test_submit_result_batch_empty() {
    let (env, contract_id, _oracle, ..) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let oracle = client.get_oracle();
    let batch: soroban_sdk::Vec<(u64, Winner)> = soroban_sdk::vec![&env];

    let outcomes = client.submit_result_batch(&batch, &oracle);

    assert_eq!(outcomes.len(), 0);
}
