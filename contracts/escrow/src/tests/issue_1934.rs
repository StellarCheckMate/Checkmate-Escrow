/// #1934 — get_pending_disputes returns open disputes with correct pagination.
use super::*;

const EVIDENCE_HASH: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn evidence(env: &Env) -> String {
    String::from_str(env, EVIDENCE_HASH)
}

/// Create a funded match, enable a dispute period, submit a result, and open a
/// dispute against it. Returns the match ID.
fn create_disputed_match(
    client: &EscrowContractClient,
    env: &Env,
    oracle: &Address,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
    set_period: bool,
) -> u64 {
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
    if set_period {
        client.set_dispute_period(&1000);
    }
    client.submit_result(&mid, &Winner::Player1, oracle);
    client.dispute_oracle_result(&mid, player2, &evidence(env));
    mid
}

/// get_pending_disputes returns all Active disputes when offset=0, limit=100.
#[test]
fn test_get_pending_disputes_returns_active_disputes() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id =
        create_disputed_match(&client, &env, &oracle, &player1, &player2, &token, "d1934a", true);

    let disputes = client.get_pending_disputes(&0, &100);
    assert_eq!(disputes.len(), 1, "one open dispute should be returned");
    assert_eq!(disputes.get(0).unwrap().match_id, match_id);
}

/// offset skips the first N active disputes.
#[test]
fn test_get_pending_disputes_offset_skips_entries() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_ids = ["pd1934b1", "pd1934b2", "pd1934b3"];
    for (i, game_id) in game_ids.iter().enumerate() {
        create_disputed_match(
            &client,
            &env,
            &oracle,
            &player1,
            &player2,
            &token,
            game_id,
            i == 0, // set_period only once; subsequent calls reuse existing setting
        );
    }

    // Skip first 1, get up to 10 → should return 2.
    let page = client.get_pending_disputes(&1, &10);
    assert_eq!(page.len(), 2, "offset=1 should skip the first dispute");

    // Skip all 3 → empty.
    let empty = client.get_pending_disputes(&3, &10);
    assert_eq!(empty.len(), 0, "offset beyond total should return empty");
}

/// limit caps the number of results returned.
#[test]
fn test_get_pending_disputes_limit_caps_results() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_ids = ["pd1934c1", "pd1934c2", "pd1934c3", "pd1934c4"];
    for (i, game_id) in game_ids.iter().enumerate() {
        create_disputed_match(
            &client,
            &env,
            &oracle,
            &player1,
            &player2,
            &token,
            game_id,
            i == 0,
        );
    }

    let page = client.get_pending_disputes(&0, &2);
    assert_eq!(page.len(), 2, "limit=2 should return only 2 disputes");
}

/// limit is silently clamped to 100.
#[test]
fn test_get_pending_disputes_limit_clamped_to_100() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No disputes; just verify that a huge limit doesn't panic.
    let result = client.get_pending_disputes(&0, &u32::MAX);
    assert_eq!(result.len(), 0);
}

/// An empty dispute history returns an empty vec regardless of offset/limit.
#[test]
fn test_get_pending_disputes_empty_when_no_disputes() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.get_pending_disputes(&0, &10);
    assert_eq!(result.len(), 0);
}
