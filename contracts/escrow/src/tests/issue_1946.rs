//! Tests for #1946 — add a get_pending_results view listing matches awaiting oracle submission.
//!
//! Verifies that:
//! - `get_pending_results` returns matches in `PendingResult` state.
//! - Pagination bounds (offset, limit) are respected.
//! - Matches that have moved out of `PendingResult` state are not returned.
//! - An offset beyond the last item returns an empty result.

#![cfg(test)]

use super::*;

/// Helper: set the global dispute period to 1000 ledgers so matches stay in
/// `PendingResult` after `submit_result`, then create/fund/submit a match.
/// Returns the match ID.
fn create_pending_result_match(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    oracle: &Address,
    game_id: &str,
) -> u64 {
    // A non-zero dispute period keeps the match in PendingResult after submit_result.
    client.set_dispute_period(&1000);
    let mid = client.create_match(
        player1,
        player2,
        &100,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&mid, player1);
    client.deposit(&mid, player2);
    client.submit_result(&mid, &Winner::Player1, oracle);
    mid
}

/// `get_pending_results` returns matches in `PendingResult` state.
#[test]
fn test_get_pending_results_returns_pending_result_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = create_pending_result_match(
        &client, &env, &player1, &player2, &token, &oracle, "pend0001",
    );

    let results = client.get_pending_results(&0, &10).unwrap();
    assert_eq!(results.len(), 1, "one match in PendingResult state");
    assert_eq!(results.get(0).unwrap().id, mid);
    assert_eq!(
        results.get(0).unwrap().state,
        MatchState::PendingResult,
        "returned match must be in PendingResult state"
    );
}

/// Pagination bounds are respected: offset skips the first N items; limit caps
/// the result count.
#[test]
fn test_get_pending_results_pagination_bounds_respected() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 5 PendingResult matches.
    let game_ids = ["pend1001", "pend1002", "pend1003", "pend1004", "pend1005"];
    let mut match_ids = Vec::new();
    for game_id in &game_ids {
        let mid = create_pending_result_match(
            &client, &env, &player1, &player2, &token, &oracle, game_id,
        );
        match_ids.push(mid);
    }

    // limit=2 returns first 2.
    let page0 = client.get_pending_results(&0, &2).unwrap();
    assert_eq!(page0.len(), 2, "first page must contain exactly 2 matches");
    assert_eq!(page0.get(0).unwrap().id, match_ids[0]);
    assert_eq!(page0.get(1).unwrap().id, match_ids[1]);

    // offset=2, limit=2 returns matches at index 2 and 3.
    let page1 = client.get_pending_results(&2, &2).unwrap();
    assert_eq!(page1.len(), 2, "second page must contain exactly 2 matches");
    assert_eq!(page1.get(0).unwrap().id, match_ids[2]);
    assert_eq!(page1.get(1).unwrap().id, match_ids[3]);

    // offset=4, limit=10 returns only the last match (partial page).
    let page2 = client.get_pending_results(&4, &10).unwrap();
    assert_eq!(page2.len(), 1, "partial last page must contain 1 match");
    assert_eq!(page2.get(0).unwrap().id, match_ids[4]);
}

/// An offset beyond the last item returns an empty result.
#[test]
fn test_get_pending_results_offset_beyond_end_returns_empty() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_pending_result_match(
        &client, &env, &player1, &player2, &token, &oracle, "pend0002",
    );

    // offset beyond the only entry returns empty.
    let empty = client.get_pending_results(&5, &10).unwrap();
    assert_eq!(
        empty.len(),
        0,
        "offset beyond last item must return empty vec"
    );
}

/// limit=0 returns an empty result regardless of how many matches exist.
#[test]
fn test_get_pending_results_limit_zero_returns_empty() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_pending_result_match(
        &client, &env, &player1, &player2, &token, &oracle, "pend0003",
    );

    let empty = client.get_pending_results(&0, &0).unwrap();
    assert_eq!(empty.len(), 0, "limit=0 must always return empty");
}

/// A match that has been finalised (Completed) is no longer returned.
#[test]
fn test_get_pending_results_excludes_finalized_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No dispute period → submit_result completes the match immediately.
    client.set_dispute_period(&0);
    let mid = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "pend0004"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);

    // The match should be Completed, not PendingResult.
    let m = client.get_match(&mid);
    assert_eq!(m.state, MatchState::Completed);

    let results = client.get_pending_results(&0, &10).unwrap();
    assert!(
        !results.iter().any(|r| r.id == mid),
        "completed match must not appear in get_pending_results"
    );
}


/// `get_pending_results` returns matches in `PendingResult` state.
#[test]
fn test_get_pending_results_returns_pending_result_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = create_pending_result_match(
        &client, &env, &player1, &player2, &token, &oracle, "pend0001",
    );

    let results = client.get_pending_results(&0, &10).unwrap();
    assert_eq!(results.len(), 1, "one match in PendingResult state");
    assert_eq!(results.get(0).unwrap().id, mid);
    assert_eq!(
        results.get(0).unwrap().state,
        MatchState::PendingResult,
        "returned match must be in PendingResult state"
    );
}

/// Pagination bounds are respected: offset skips the first N items; limit caps
/// the result count.
#[test]
fn test_get_pending_results_pagination_bounds_respected() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 5 PendingResult matches.
    let game_ids = ["pr000000", "pr000001", "pr000002", "pr000003", "pr000004"];
    let mut match_ids = Vec::new();
    for game_id in &game_ids {
        let mid = create_pending_result_match(
            &client, &env, &player1, &player2, &token, &oracle, game_id,
        );
        match_ids.push(mid);
    }

    // limit=2 returns first 2.
    let page0 = client.get_pending_results(&0, &2).unwrap();
    assert_eq!(page0.len(), 2);
    assert_eq!(page0.get(0).unwrap().id, match_ids[0]);
    assert_eq!(page0.get(1).unwrap().id, match_ids[1]);

    // offset=2, limit=2 returns matches 2 and 3.
    let page1 = client.get_pending_results(&2, &2).unwrap();
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, match_ids[2]);
    assert_eq!(page1.get(1).unwrap().id, match_ids[3]);

    // offset=4, limit=10 returns only the last match (partial page).
    let page2 = client.get_pending_results(&4, &10).unwrap();
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().id, match_ids[4]);
}

/// An offset beyond the last item returns an empty result.
#[test]
fn test_get_pending_results_offset_beyond_end_returns_empty() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_pending_result_match(
        &client, &env, &player1, &player2, &token, &oracle, "pend0002",
    );

    // offset beyond the only entry.
    let empty = client.get_pending_results(&5, &10).unwrap();
    assert_eq!(
        empty.len(),
        0,
        "offset beyond last item must return empty vec"
    );
}

/// limit=0 returns an empty result regardless of how many matches exist.
#[test]
fn test_get_pending_results_limit_zero_returns_empty() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_pending_result_match(
        &client, &env, &player1, &player2, &token, &oracle, "pend0003",
    );

    let empty = client.get_pending_results(&0, &0).unwrap();
    assert_eq!(empty.len(), 0, "limit=0 must always return empty");
}

/// A match that has been finalised (Completed) is no longer returned.
#[test]
fn test_get_pending_results_excludes_finalized_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No dispute period → submit_result completes the match immediately.
    client.set_dispute_period(&0);
    let mid = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "pend0004"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);
    client.submit_result(&mid, &Winner::Player1, &oracle);

    // The match should be Completed, not PendingResult.
    let m = client.get_match(&mid);
    assert_eq!(m.state, MatchState::Completed);

    let results = client.get_pending_results(&0, &10).unwrap();
    assert!(
        !results.iter().any(|r| r.id == mid),
        "completed match must not appear in get_pending_results"
    );
}
