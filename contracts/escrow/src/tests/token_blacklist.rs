#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::testutils::Address as _;

// ── Helper ────────────────────────────────────────────────────────────────────

fn reason(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

// ── is_token_blacklisted ──────────────────────────────────────────────────────

#[test]
fn test_blacklist_unknown_token_not_blacklisted() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    assert!(
        !client.is_token_blacklisted(&unknown),
        "unknown token must not be blacklisted"
    );
}

// ── add_token_to_blacklist ────────────────────────────────────────────────────

#[test]
fn test_add_token_to_blacklist_requires_admin_auth() {
    let (env, contract_id, _oracle, player1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Only the admin key is in mock_all_auths; a plain player call must fail.
    env.set_auths(&[]);
    let result = client.try_add_token_to_blacklist(&token, &reason(&env, "scam"));
    assert!(result.is_err(), "non-admin must be rejected");
}

#[test]
fn test_add_token_to_blacklist_marks_token_as_blacklisted() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "rug pull"));
    assert!(
        client.is_token_blacklisted(&token),
        "token must be blacklisted after add"
    );
}

#[test]
fn test_add_token_to_blacklist_emits_event() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "scam"));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        Symbol::new(&env, "tok_blacklist").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "tok_blacklist event must be emitted");

    let (_, _, data) = matched.unwrap();
    let ev_token: Address = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_token, token);
}

#[test]
fn test_add_token_to_blacklist_appears_in_get_blacklist() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "fraud"));

    let list = client.get_blacklist();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), token);
}

#[test]
fn test_add_multiple_tokens_to_blacklist() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();
    let token3 = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();

    client.add_token_to_blacklist(&token, &reason(&env, "reason1"));
    client.add_token_to_blacklist(&token2, &reason(&env, "reason2"));
    client.add_token_to_blacklist(&token3, &reason(&env, "reason3"));

    let list = client.get_blacklist();
    assert_eq!(list.len(), 3);
    assert!(client.is_token_blacklisted(&token));
    assert!(client.is_token_blacklisted(&token2));
    assert!(client.is_token_blacklisted(&token3));
}

// ── remove_token_from_blacklist ───────────────────────────────────────────────

#[test]
fn test_remove_token_from_blacklist_requires_admin_auth() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "scam"));

    env.set_auths(&[]);
    let result = client.try_remove_token_from_blacklist(&token);
    assert!(result.is_err(), "non-admin removal must be rejected");
}

#[test]
fn test_remove_token_from_blacklist_unmarks_token() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "temp block"));
    assert!(client.is_token_blacklisted(&token));

    client.remove_token_from_blacklist(&token);
    assert!(
        !client.is_token_blacklisted(&token),
        "token must no longer be blacklisted"
    );
}

#[test]
fn test_remove_token_from_blacklist_emits_event() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "scam"));
    client.remove_token_from_blacklist(&token);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        Symbol::new(&env, "tok_unblacklist").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "tok_unblacklist event must be emitted");
}

#[test]
fn test_remove_token_removes_from_get_blacklist() {
    let (env, contract_id, _oracle, _p1, _p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(Address::generate(&env)).address();

    client.add_token_to_blacklist(&token, &reason(&env, "a"));
    client.add_token_to_blacklist(&token2, &reason(&env, "b"));
    client.remove_token_from_blacklist(&token);

    let list = client.get_blacklist();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), token2);
    assert!(!client.is_token_blacklisted(&token));
}

// ── get_blacklist ─────────────────────────────────────────────────────────────

#[test]
fn test_get_blacklist_empty_by_default() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_blacklist().len(), 0);
}

// ── create_match enforcement ──────────────────────────────────────────────────

#[test]
fn test_create_match_rejects_blacklisted_token() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "known scam"));

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "blacklist_game1"),
        &Platform::Lichess,
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        Error::TokenBlacklisted,
        "blacklisted token must be rejected by create_match"
    );
}

#[test]
fn test_create_match_allows_token_after_removal_from_blacklist() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.add_token_to_blacklist(&token, &reason(&env, "temp"));
    client.remove_token_from_blacklist(&token);

    // Should succeed now.
    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "blacklist_game2"),
        &Platform::Lichess,
    );
    assert!(result.is_ok(), "token removed from blacklist must be usable");
}

#[test]
fn test_blacklist_takes_precedence_over_allowlist() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Enable allowlist and add token — then also blacklist it.
    client.add_allowed_token(&token);
    client.add_token_to_blacklist(&token, &reason(&env, "fraudulent despite allowlist"));

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "blacklist_game3"),
        &Platform::Lichess,
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        Error::TokenBlacklisted,
        "blacklist must override allowlist"
    );
}
