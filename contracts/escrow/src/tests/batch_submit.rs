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
        &String::from_str(&env, "885b6556"),
        &Platform::Lichess,
    );
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "8372756a"),
        &Platform::Lichess,
    );

    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);
    client.deposit(&match_b, &player1);
    client.deposit(&match_b, &player2);

    let oracle = client.get_oracle();
    let batch = soroban_sdk::vec![&env, (match_a, Winner::Player1), (match_b, Winner::Player2),];

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
        &String::from_str(&env, "7e081169"),
        &Platform::Lichess,
    );
    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);

    // match_b only has one deposit, so it's still Pending — submit_result
    // fails with NotFunded (not yet fully funded).
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "8b4685e8"),
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
    assert_eq!(outcomes.get(1).unwrap(), Some(Error::NotFunded));
    assert_eq!(outcomes.get(2).unwrap(), Some(Error::MatchNotFound));

    // The successful match settled, and the failing matches were left untouched.
    assert_eq!(client.get_match(&match_a).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_b).state, MatchState::Pending);
}

// #1308 — NotFunded must be reported per-entry for every unfunded match in a
// batch, not just skipped or mislabeled as InvalidState.
#[test]
fn test_submit_result_batch_mixed_funded_and_unfunded_reports_not_funded() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Fully funded — should settle successfully.
    let funded = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_funded1"),
        &Platform::Lichess,
    );
    client.deposit(&funded, &player1);
    client.deposit(&funded, &player2);

    // No deposits at all — still Pending.
    let unfunded_none = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_unfunded_none"),
        &Platform::Lichess,
    );

    // Only one side deposited — still Pending.
    let unfunded_partial = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_unfunded_partial"),
        &Platform::Lichess,
    );
    client.deposit(&unfunded_partial, &player1);

    let oracle = client.get_oracle();
    let batch = soroban_sdk::vec![
        &env,
        (funded, Winner::Player1),
        (unfunded_none, Winner::Player1),
        (unfunded_partial, Winner::Player2),
    ];

    let outcomes = client.submit_result_batch(&batch, &oracle);

    assert_eq!(outcomes.len(), 3);
    assert_eq!(outcomes.get(0).unwrap(), None);
    assert_eq!(outcomes.get(1).unwrap(), Some(Error::NotFunded));
    assert_eq!(outcomes.get(2).unwrap(), Some(Error::NotFunded));

    assert_eq!(client.get_match(&funded).state, MatchState::Completed);
    assert_eq!(client.get_match(&unfunded_none).state, MatchState::Pending);
    assert_eq!(
        client.get_match(&unfunded_partial).state,
        MatchState::Pending
    );
}

// #1744 — batch entries are processed independently: a failure in the middle
// entry must not block the valid entries before or after it.
#[test]
fn test_submit_result_batch_mixed_valid_invalid_valid() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Entry 0 — fully funded, will settle successfully.
    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "mixed_valid_0"),
        &Platform::Lichess,
    );
    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);

    // Entry 1 — only one side deposited, so it's still Pending and
    // submit_result fails with NotFunded.
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "mixed_invalid_1"),
        &Platform::Lichess,
    );
    client.deposit(&match_b, &player1);

    // Entry 2 — fully funded, will settle successfully.
    let match_c = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "mixed_valid_2"),
        &Platform::Lichess,
    );
    client.deposit(&match_c, &player1);
    client.deposit(&match_c, &player2);

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
    assert_eq!(outcomes.get(1).unwrap(), Some(Error::NotFunded));
    assert_eq!(outcomes.get(2).unwrap(), None);

    // Valid entries settled despite the failure in the middle.
    assert_eq!(client.get_match(&match_a).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_a).winner, Winner::Player1);
    assert_eq!(client.get_match(&match_c).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_c).winner, Winner::Draw);

    // The invalid entry was left untouched.
    assert_eq!(client.get_match(&match_b).state, MatchState::Pending);
}

#[test]
fn test_submit_result_batch_already_confirmed_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "a1b2c3d4"),
        &Platform::Lichess,
    );
    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);

    let oracle = client.get_oracle();

    // First submission settles the match.
    let first = client.submit_result_batch(
        &soroban_sdk::vec![&env, (match_a, Winner::Player1)],
        &oracle,
    );
    assert_eq!(first.get(0).unwrap(), None);
    assert_eq!(client.get_match(&match_a).state, MatchState::Completed);

    // Re-submitting a result for the same (already-settled) match must be
    // reported as OracleAlreadyConfirmed rather than a generic InvalidState.
    let second = client.submit_result_batch(
        &soroban_sdk::vec![&env, (match_a, Winner::Player1)],
        &oracle,
    );
    assert_eq!(second.get(0).unwrap(), Some(Error::OracleAlreadyConfirmed));
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

// #1529 — submit_result_batch must authorize against the effective oracle so
// that a temporary oracle rotation is honoured, matching submit_result and
// submit_draw. During an active temporary rotation the temporary oracle can
// batch-submit while the rotated-out oracle cannot.
#[test]
fn test_submit_result_batch_honours_temporary_oracle_rotation() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "rot_batch_a"),
        &Platform::Lichess,
    );
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "rot_batch_b"),
        &Platform::Lichess,
    );

    client.deposit(&match_a, &player1);
    client.deposit(&match_a, &player2);
    client.deposit(&match_b, &player1);
    client.deposit(&match_b, &player2);

    let original_oracle = client.get_oracle();
    let temporary_oracle = Address::generate(&env);

    // Activate a temporary oracle rotation.
    client.rotate_oracle_temporary(&admin, &temporary_oracle);

    // The temporary oracle is now the effective oracle and must be able to
    // batch-submit results.
    let batch = soroban_sdk::vec![&env, (match_a, Winner::Player1), (match_b, Winner::Player2)];
    let outcomes = client.submit_result_batch(&batch, &temporary_oracle);

    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes.get(0).unwrap(), None);
    assert_eq!(outcomes.get(1).unwrap(), None);

    assert_eq!(client.get_match(&match_a).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_a).winner, Winner::Player1);
    assert_eq!(client.get_match(&match_b).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_b).winner, Winner::Player2);

    // The rotated-out oracle must no longer be able to batch-submit.
    let match_c = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "rot_batch_c"),
        &Platform::Lichess,
    );
    client.deposit(&match_c, &player1);
    client.deposit(&match_c, &player2);

    let stale_batch = soroban_sdk::vec![&env, (match_c, Winner::Player1)];
    let stale_result = client.try_submit_result_batch(&stale_batch, &original_oracle);
    assert!(stale_result.is_err());
}
