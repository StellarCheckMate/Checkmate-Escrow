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

// ── get_frozen_players ────────────────────────────────────────────────────────

#[test]
fn test_get_frozen_players_empty_by_default() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_frozen_players().len(), 0);
}

// ── create_match enforcement ──────────────────────────────────────────────────

#[test]
fn test_create_match_blocked_when_player1_frozen() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "f51a31c0"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player1 must be rejected by create_match"
    );
}

#[test]
fn test_create_match_blocked_when_player2_frozen() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player2, &reason(&env, "bad actor"));

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "d2a90f47"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "a frozen player2 must be rejected by create_match"
    );
}

#[test]
fn test_create_match_allowed_after_unfreeze() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "temp"));
    client.admin_unfreeze_player(&player1);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "8b1c4a2e"),
        &Platform::Lichess,
    );
    assert!(
        result.is_ok(),
        "player removed from freeze must be able to create a match"
    );
}

#[test]
fn test_create_match_with_referrer_blocked_when_player_frozen() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let referrer = Address::generate(&env);

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    let result = client.try_create_match_with_referrer(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "0f27c9b4"),
        &Platform::Lichess,
        &referrer,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player must be rejected by create_match_with_referrer"
    );
}

#[test]
fn test_create_match_with_conversion_blocked_when_player_frozen() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_b = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    // The freeze check fires before the oracle rate fetch, so no oracle
    // contract/rate setup is needed here — the call must never reach it.
    let result = client.try_create_match_with_conversion(
        &player1,
        &player2,
        &100,
        &token,
        &token_b,
        &50_000_000,
        &String::from_str(&env, "c4b8e21d"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player must be rejected by create_match_with_conversion"
    );
}

// ── deposit enforcement ───────────────────────────────────────────────────────

#[test]
fn test_deposit_blocked_when_player_frozen() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "71e0c3a8"),
        &Platform::Lichess,
    );

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    let result = client.try_deposit(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "frozen player must not be able to deposit"
    );
}

#[test]
fn test_deposit_allowed_after_unfreeze() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "9a4d6f15"),
        &Platform::Lichess,
    );

    client.admin_freeze_player(&player1, &reason(&env, "temp"));
    client.admin_unfreeze_player(&player1);

    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert!(
        m.player1_deposited,
        "deposit must succeed after the player is unfrozen"
    );
}

// ── Freeze does not disrupt existing matches (fund safety) ───────────────────

#[test]
fn test_freeze_does_not_block_active_match_settlement() {
    let (env, contract_id, oracle, player1, player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    // Oracle can still settle the already-funded match...
    client.submit_result(&match_id, &Winner::Draw, &oracle);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);

    // ...and the frozen player can still claim their refund (fund recovery is
    // deliberately not blocked by a freeze).
    client.claim_vested_payout(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).winner, Winner::Draw);
    assert!(
        client.get_match(&match_id).player1_claimed,
        "frozen player must still be able to claim their own funds"
    );
}

#[test]
fn test_frozen_player_can_still_cancel_pending_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "3c6e1b78"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    // The frozen player can still cancel and recover their own stake.
    client.cancel_match(&id, &player1);
    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Cancelled);
    assert_eq!(
        TokenClient::new(&env, &token).balance(&player1),
        1000,
        "frozen player must recover their deposited stake via cancel"
    );
}

#[test]
fn test_unfrozen_opponent_unaffected_by_freeze() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.admin_freeze_player(&player1, &reason(&env, "bad actor"));

    // player2 (not frozen) can still create and fund matches with a third party.
    let player3 = Address::generate(&env);
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player3, &1000);

    let id = client.create_match(
        &player2,
        &player3,
        &100,
        &token,
        &String::from_str(&env, "a18d4e62"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player2);
    client.deposit(&id, &player3);
    assert_eq!(client.get_match(&id).state, MatchState::Active);
}
