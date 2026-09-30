#![cfg(test)]
extern crate std;

use super::*;

// ── Helper ────────────────────────────────────────────────────────────────────

fn reason(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn freeze_reason_stored(env: &Env, contract_id: &Address, player: &Address) -> Option<String> {
    env.as_contract(contract_id, || {
        env.storage()
            .instance()
            .get(&PlayerFreezeKey::FrozenPlayer(player.clone()))
    })
}

// ── is_player_frozen ──────────────────────────────────────────────────────────

#[test]
fn test_freeze_unknown_player_not_frozen() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown = Address::generate(&env);
    assert!(
        !client.is_player_frozen(&unknown),
        "unknown player must not be frozen"
    );
}

// ── admin_freeze_player ───────────────────────────────────────────────────────

#[test]
fn test_admin_freeze_player_requires_admin_auth() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.set_auths(&[]);
    let result = client.try_admin_freeze_player(&player1, &reason(&env, "cheating"));
    assert!(result.is_err(), "non-admin freeze must be rejected");
}

#[test]
fn test_admin_freeze_player_on_uninitialized_contract_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_admin_freeze_player(&Address::generate(&env), &reason(&env, "x"));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_admin_freeze_player_marks_player_frozen() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "stalling matches"));
    assert!(
        client.is_player_frozen(&player1),
        "player must be frozen after admin_freeze_player"
    );

    let stored = freeze_reason_stored(&env, &contract_id, &player1);
    assert_eq!(
        stored.as_deref(),
        Some("stalling matches"),
        "freeze reason must be stored on-chain for auditability"
    );
}

#[test]
fn test_admin_freeze_player_emits_event() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "cheating"));

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("freeze").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "freeze event must be emitted");

    let (_, _, data) = matched.unwrap();
    let ev_player: Address = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_player, player1);
}

#[test]
fn test_admin_freeze_player_appears_in_get_frozen_players() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "fraud"));

    let list = client.get_frozen_players();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), player1);
}

#[test]
fn test_admin_freeze_multiple_players() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let player3 = Address::generate(&env);

    client.admin_freeze_player(&player1, &reason(&env, "r1"));
    client.admin_freeze_player(&player2, &reason(&env, "r2"));
    client.admin_freeze_player(&player3, &reason(&env, "r3"));

    let list = client.get_frozen_players();
    assert_eq!(list.len(), 3);
    assert!(client.is_player_frozen(&player1));
    assert!(client.is_player_frozen(&player2));
    assert!(client.is_player_frozen(&player3));
}

#[test]
fn test_admin_freeze_player_idempotent_no_duplicate_in_list() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "first"));
    client.admin_freeze_player(&player1, &reason(&env, "updated reason"));

    let list = client.get_frozen_players();
    assert_eq!(
        list.len(),
        1,
        "re-freezing an already-frozen player must not duplicate the list entry"
    );
    assert!(client.is_player_frozen(&player1));
}

// ── admin_unfreeze_player ─────────────────────────────────────────────────────

#[test]
fn test_admin_unfreeze_player_requires_admin_auth() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "scam"));

    env.set_auths(&[]);
    let result = client.try_admin_unfreeze_player(&player1);
    assert!(result.is_err(), "non-admin unfreeze must be rejected");
}

#[test]
fn test_admin_unfreeze_player_unmarks() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "temp block"));
    assert!(client.is_player_frozen(&player1));

    client.admin_unfreeze_player(&player1);
    assert!(
        !client.is_player_frozen(&player1),
        "player must no longer be frozen after admin_unfreeze_player"
    );
    assert!(freeze_reason_stored(&env, &contract_id, &player1).is_none());
}

#[test]
fn test_admin_unfreeze_player_emits_event() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "scam"));
    client.admin_unfreeze_player(&player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("unfreeze").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "unfreeze event must be emitted");
}

#[test]
fn test_admin_unfreeze_player_removes_from_list() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "a"));
    client.admin_freeze_player(&player2, &reason(&env, "b"));
    client.admin_unfreeze_player(&player1);

    let list = client.get_frozen_players();
    assert_eq!(list.len(), 1);
    assert_eq!(list.get(0).unwrap(), player2);
    assert!(!client.is_player_frozen(&player1));
}

// ── admin_list_frozen_players ──────────────────────────────────────────────────

#[test]
fn test_admin_list_frozen_players_requires_admin_auth() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "audit"));
    env.set_auths(&[]);

    let result = client.try_admin_list_frozen_players(&_admin);
    assert!(result.is_err(), "frozen-player audit must require admin auth");
}

#[test]
fn test_admin_list_frozen_players_returns_current_ordered_list() {
    let (env, contract_id, _oracle, player1, player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let player3 = Address::generate(&env);

    client.admin_freeze_player(&player1, &reason(&env, "r1"));
    client.admin_freeze_player(&player2, &reason(&env, "r2"));
    client.admin_freeze_player(&player3, &reason(&env, "r3"));

    let list = client.admin_list_frozen_players(&admin);
    assert_eq!(list.len(), 3);
    assert_eq!(list.get(0).unwrap(), player1);
    assert_eq!(list.get(1).unwrap(), player2);
    assert_eq!(list.get(2).unwrap(), player3);
}

// ── create_match_tournament frozen-player guard ───────────────────────────────

#[test]
fn test_create_match_tournament_rejects_frozen_player1() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.register_bracket(&admin, &1, &admin);
    client.admin_freeze_player(&player1, &reason(&env, "cheating"));

    let result = client.try_create_match_tournament(
        &1,
        &1,
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tournament"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_create_match_tournament_rejects_frozen_player2() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.register_bracket(&admin, &1, &admin);
    client.admin_freeze_player(&player2, &reason(&env, "cheating"));

    let result = client.try_create_match_tournament(
        &1,
        &1,
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tournament"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}
