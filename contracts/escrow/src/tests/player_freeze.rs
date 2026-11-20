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

    // The freeze event payload is now `(player, admin)` (issue #1917), so the
    // data decodes as a 2-tuple rather than a bare address.
    let (_, _, data) = matched.unwrap();
    let (ev_player, _ev_admin): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_player, player1);
}

/// Issue #1917: the freeze event must carry the caller admin address so every
/// freeze is attributable for audit, not just the frozen player.
#[test]
fn test_admin_freeze_player_event_carries_caller_admin() {
    let (env, contract_id, _oracle, player1, _p2, _token, admin) = setup();
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
    let (ev_player, ev_admin): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_player, player1, "freeze event must carry the frozen player");
    assert_eq!(
        ev_admin, admin,
        "freeze event must carry the caller admin address so freezes are attributable"
    );
}

#[test]
fn test_admin_freeze_player_appears_in_get_frozen_players() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "fraud"));

    let list = client.get_frozen_players(&0, &100);
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

    let list = client.get_frozen_players(&0, &100);
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

    let list = client.get_frozen_players(&0, &100);
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

    let list = client.get_frozen_players(&0, &100);
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

    let result = client.try_admin_list_frozen_players(&_admin, &0, &100);
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

    let list = client.admin_list_frozen_players(&admin, &0, &100);
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

    client.register_bracket(&admin, &1, &admin, &soroban_sdk::vec![&env]);
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

    client.register_bracket(&admin, &1, &admin, &soroban_sdk::vec![&env]);
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

// ── create_match frozen-player guard ─────────────────────────────────────────

#[test]
fn test_create_match_rejects_frozen_player1() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "cheating"));

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player1 must not be able to create a match"
    );
}

#[test]
fn test_create_match_rejects_frozen_player2() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player2, &reason(&env, "cheating"));

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "a frozen player must not be named as opponent"
    );
}

// ── deposit frozen-player guard ──────────────────────────────────────────────

#[test]
fn test_deposit_rejects_frozen_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );

    client.admin_freeze_player(&player1, &reason(&env, "cheating"));

    let result = client.try_deposit(&match_id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player must not be able to deposit"
    );
}

// ── cancel_match allowed for frozen player ───────────────────────────────────

#[test]
fn test_cancel_match_allowed_for_frozen_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );

    client.admin_freeze_player(&player1, &reason(&env, "cheating"));

    client.cancel_match(&match_id, &player1);

    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::Cancelled,
        "frozen player must still be able to cancel their own pending match"
    );
}

// ── claim_vested_payout allowed for frozen player ────────────────────────────

#[test]
fn test_claim_vested_payout_allowed_for_frozen_player() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    client.admin_freeze_player(&player1, &reason(&env, "cheating"));

    // Freezing must not block a player from claiming an already-vested payout.
    client.claim_vested_payout(&match_id, &player1);
}

// ── admin_unfreeze_player restores all permissions ───────────────────────────

#[test]
fn test_admin_unfreeze_player_restores_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "temp"));
    client.admin_unfreeze_player(&player1);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );
    assert!(
        result.is_ok(),
        "unfrozen player must be able to create a match again"
    );
}

#[test]
fn test_admin_unfreeze_player_restores_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );

    client.admin_freeze_player(&player1, &reason(&env, "temp"));
    client.admin_unfreeze_player(&player1);

    let result = client.try_deposit(&match_id, &player1);
    assert!(
        result.is_ok(),
        "unfrozen player must be able to deposit again"
    );
}

#[test]
fn test_admin_unfreeze_player_restores_cancel_and_claim() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Cancel path: freeze then unfreeze, then cancel must succeed.
    let cancel_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );
    client.admin_freeze_player(&player1, &reason(&env, "temp"));
    client.admin_unfreeze_player(&player1);
    client.cancel_match(&cancel_id, &player1);
    assert_eq!(client.get_match(&cancel_id).state, MatchState::Cancelled);

    // Claim path: freeze then unfreeze, then claim must succeed.
    let claim_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "match"),
        &Platform::Lichess,
    );
    client.deposit(&claim_id, &player1);
    client.deposit(&claim_id, &player2);
    client.admin_freeze_player(&player1, &reason(&env, "temp"));
    client.admin_unfreeze_player(&player1);
    client.claim_vested_payout(&claim_id, &player1);
}

// ── Issue #1764: get_frozen_players pagination / size limit ──────────────────

/// Freezing more than 100 players and paginating through them must return the
/// correct subset on each page, and the clamped limit of 100 must prevent any
/// single call from returning more than 100 entries regardless of how large the
/// full list is.
#[test]
fn test_1764_get_frozen_players_pagination_large_list() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    const TOTAL: u32 = 150;

    // Freeze 150 unique players.
    let mut players = soroban_sdk::Vec::new(&env);
    for _ in 0..TOTAL {
        let p = Address::generate(&env);
        client.admin_freeze_player(&p, &reason(&env, "bulk-test"));
        players.push_back(p);
    }

    // Requesting with limit=100 should return exactly 100 from offset 0.
    let page_0 = client.get_frozen_players(&0, &100);
    assert_eq!(
        page_0.len(),
        100,
        "first page with limit=100 must return exactly 100 entries"
    );

    // Requesting the second page (offset=100, limit=100) should return the
    // remaining 50 entries.
    let page_1 = client.get_frozen_players(&100, &100);
    assert_eq!(
        page_1.len(),
        50,
        "second page (offset=100, limit=100) must return the remaining 50 entries"
    );

    // Limit clamping: requesting limit=200 is silently capped to 100.
    let clamped = client.get_frozen_players(&0, &200);
    assert_eq!(
        clamped.len(),
        100,
        "limit above 100 must be silently clamped to 100"
    );

    // Pages are non-overlapping: no address appears in both pages.
    let p0_set: std::vec::Vec<Address> = page_0.iter().collect();
    for addr in page_1.iter() {
        assert!(
            !p0_set.contains(&addr),
            "pages must not overlap: address {addr:?} appears in both page 0 and page 1"
        );
    }

    // Total across two full pages equals TOTAL (150).
    assert_eq!(
        page_0.len() + page_1.len(),
        TOTAL,
        "pages must cover all frozen players without gaps or overlaps"
    );
}

/// An empty offset beyond the list length must return an empty Vec.
#[test]
fn test_1764_get_frozen_players_offset_beyond_end() {
    let (env, contract_id, _oracle, player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "test"));

    // Offset=10 with only 1 player — nothing to return.
    let page = client.get_frozen_players(&10, &100);
    assert_eq!(
        page.len(),
        0,
        "offset beyond list length must return an empty page"
    );
}

/// With zero players frozen, every page must be empty.
#[test]
fn test_1764_get_frozen_players_empty_list() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let page = client.get_frozen_players(&0, &100);
    assert_eq!(
        page.len(),
        0,
        "get_frozen_players must return empty Vec when no player is frozen"
    );
}
