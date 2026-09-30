use super::*;
use soroban_sdk::testutils::{
    storage::{Instance as _, Persistent as _},
    Ledger as _,
};

// ── Instance TTL coverage for the remaining entry points ───────────────────

/// Read the instance-entry TTL (all instance keys share one TTL) from inside
/// the contract's storage scope.
fn instance_ttl(env: &Env, contract_id: &Address) -> u32 {
    env.as_contract(contract_id, || env.storage().instance().get_ttl())
}

/// `extend_instance_ttl` only refreshes the instance entry once its remaining
/// TTL has dropped to `MATCH_TTL_LEDGERS / 2` or less, so age the ledger past
/// that threshold before the measured call.
///
/// The token contract never extends its own instance storage, so its instance
/// entry is topped up first — otherwise ageing would archive it and every
/// transfer would fail.
fn age_instance_ttl(env: &Env, token: &Address) {
    env.as_contract(token, || {
        env.storage()
            .instance()
            .extend_ttl(crate::MATCH_TTL_LEDGERS, crate::MATCH_TTL_LEDGERS);
    });
    env.ledger()
        .set_sequence_number(env.ledger().sequence() + crate::MATCH_TTL_LEDGERS / 2 + 1);
}

/// Push a dispute's voting deadline past the current ledger. The voting window
/// (`VOTING_PERIOD_LEDGERS`) is shorter than the ageing applied by
/// [`age_instance_ttl`], so the deadline has to move for a vote to be cast.
fn push_voting_deadline(env: &Env, contract_id: &Address, dispute_id: u64) {
    env.as_contract(contract_id, || {
        let key = DataKey::Dispute(dispute_id);
        let mut dispute: Dispute = env.storage().persistent().get(&key).unwrap();
        dispute.voting_deadline = env.ledger().sequence() + 1_000;
        env.storage().persistent().set(&key, &dispute);
    });
}

/// Assert that `f` restored the full instance TTL window. The instance entry is
/// expected to have aged below the refresh threshold beforehand, otherwise the
/// call could leave it untouched and still pass.
fn assert_instance_ttl_refreshed<F: FnOnce()>(env: &Env, contract_id: &Address, what: &str, f: F) {
    let before = instance_ttl(env, contract_id);
    assert!(
        before < crate::MATCH_TTL_LEDGERS / 2,
        "instance TTL must age below the refresh threshold before {what}: ttl={before}"
    );
    f();
    let after = instance_ttl(env, contract_id);
    assert_eq!(
        after,
        crate::MATCH_TTL_LEDGERS,
        "{what} must restore the full instance TTL window: before={before} after={after}"
    );
}

/// Age the instance entry, then assert that `f` restored the full window.
fn assert_instance_ttl_extended<F: FnOnce()>(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    what: &str,
    f: F,
) {
    age_instance_ttl(env, token);
    assert_instance_ttl_refreshed(env, contract_id, what, f);
}

#[test]
fn test_instance_ttl_extended_on_get_protocol_config() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_instance_ttl_extended(&env, &contract_id, &token, "get_protocol_config", || {
        let _ = client.get_protocol_config();
    });
}

#[test]
fn test_instance_ttl_extended_on_add_allowed_token() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let extra = Address::generate(&env);

    assert_instance_ttl_extended(&env, &contract_id, &token, "add_allowed_token", || {
        client.add_allowed_token(&extra);
    });
}

#[test]
fn test_instance_ttl_extended_on_remove_allowed_token() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.add_allowed_token(&token);

    assert_instance_ttl_extended(&env, &contract_id, &token, "remove_allowed_token", || {
        client.remove_allowed_token(&token);
    });
}

#[test]
fn test_instance_ttl_extended_on_set_match_timeout() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_instance_ttl_extended(&env, &contract_id, &token, "set_match_timeout", || {
        client.set_match_timeout(&MIN_MATCH_TIMEOUT_SECONDS);
    });
}

#[test]
fn test_instance_ttl_extended_on_set_maximum_stake() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_instance_ttl_extended(&env, &contract_id, &token, "set_maximum_stake", || {
        client.set_maximum_stake(&Some(10_000i128));
    });
}

#[test]
fn test_instance_ttl_extended_on_set_dispute_period() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_instance_ttl_extended(&env, &contract_id, &token, "set_dispute_period", || {
        client.set_dispute_period(&200u32);
    });
}

#[test]
fn test_instance_ttl_extended_on_accept_admin() {
    let (env, contract_id, _oracle, _player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);

    assert_instance_ttl_extended(&env, &contract_id, &token, "accept_admin", || {
        client.accept_admin();
    });
}

#[test]
fn test_instance_ttl_extended_on_submit_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tlres001"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    assert_instance_ttl_extended(&env, &contract_id, &token, "submit_result", || {
        client.submit_result(&id, &Winner::Player1, &oracle, &None);
    });
}

#[test]
fn test_instance_ttl_extended_on_submit_result_batch() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mut ids = soroban_sdk::Vec::new(&env);
    for game in ["tlbtc001", "tlbtc002"].iter() {
        let id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, game),
            &Platform::Lichess,
        );
        client.deposit(&id, &player1);
        client.deposit(&id, &player2);
        ids.push_back((id, Winner::Player1));
    }

    assert_instance_ttl_extended(&env, &contract_id, &token, "submit_result_batch", || {
        let outcomes = client.submit_result_batch(&ids, &oracle);
        assert!(outcomes.iter().all(|o| o.is_none()));
    });
}

#[test]
fn test_instance_ttl_extended_on_submit_draw() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tldrw001"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    assert_instance_ttl_extended(&env, &contract_id, &token, "submit_draw", || {
        client.submit_draw(&id, &oracle, &None);
    });
}

#[test]
fn test_instance_ttl_extended_on_finalize_match() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(100);
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tlfin001"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    env.ledger().set_sequence_number(1_000);
    client.submit_result(&id, &Winner::Player1, &oracle, &None);

    assert_instance_ttl_extended(&env, &contract_id, &token, "finalize_match", || {
        client.finalize_match(&id);
    });
}

#[test]
fn test_instance_ttl_extended_on_dispute_oracle_result() {
    // Wide enough that the dispute window is still open after the ledger has
    // been aged past the instance-TTL refresh threshold.
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(1_000_000);
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tldsp001"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    env.ledger().set_sequence_number(1_000);
    client.submit_result(&id, &Winner::Player1, &oracle, &None);

    assert_instance_ttl_extended(&env, &contract_id, &token, "dispute_oracle_result", || {
        client.dispute_oracle_result(&id, &player2, &String::from_str(&env, "ev1d3nc3"));
    });
}

#[test]
fn test_instance_ttl_extended_on_vote_on_dispute() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tldvt001"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    env.ledger().set_sequence_number(1_000);
    client.submit_result(&id, &Winner::Player1, &oracle, &None);
    let dispute_id =
        client.dispute_oracle_result(&id, &player2, &String::from_str(&env, "ev1d3nc3"));

    // Age the instance entry first, then widen the voting window so the vote is
    // still castable at the aged ledger.
    age_instance_ttl(&env, &token);
    push_voting_deadline(&env, &contract_id, dispute_id);

    assert_instance_ttl_refreshed(&env, &contract_id, "vote_on_dispute", || {
        client.vote_on_dispute(&dispute_id, &player1, &false);
    });
}

#[test]
fn test_instance_ttl_extended_on_resolve_dispute_by_vote() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "tldrs001"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    env.ledger().set_sequence_number(1_000);
    client.submit_result(&id, &Winner::Player1, &oracle, &None);
    let dispute_id =
        client.dispute_oracle_result(&id, &player2, &String::from_str(&env, "ev1d3nc3"));
    client.vote_on_dispute(&dispute_id, &player1, &false);
    client.vote_on_dispute(&dispute_id, &player2, &true);

    // Move past the voting deadline so resolution is permitted.
    let deadline = client.get_dispute(&dispute_id).voting_deadline;
    env.ledger().set_sequence_number(deadline + 1);

    assert_instance_ttl_extended(
        &env,
        &contract_id,
        &token,
        "resolve_dispute_by_vote",
        || {
            client.resolve_dispute_by_vote(&dispute_id);
        },
    );
}

#[test]
fn test_ttl_extended_on_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "0cf4aed7"),
        &Platform::Lichess,
    );

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_game_id_ttl_extended_on_match_reservation() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let game_id = String::from_str(&env, "3db705b0");

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &game_id,
        &Platform::Lichess,
    );

    let ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::GameId(game_id.clone()))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_ttl_extended_on_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "badc5086"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_active_matches_ttl_refreshed_on_append_and_removal() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "3ddc215a"),
        &Platform::Lichess,
    );

    let _match2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "d6539cbe"),
        &Platform::Lichess,
    );

    // Activate match1 so its per-player ActiveMatch index entry is written.
    client.deposit(&match1, &player1);
    client.deposit(&match1, &player2);

    // TTL should be set after append (activation writes the indexed key).
    let ttl_after_append = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::ActiveMatch(player1.clone(), match1))
    });
    assert_eq!(ttl_after_append, crate::MATCH_TTL_LEDGERS);

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.submit_result(&match1, &Winner::Player1, &oracle);

    // Completion removes the match from the active index entirely.
    let still_active = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .has(&DataKey::ActiveMatch(player1.clone(), match1))
    });
    assert!(!still_active);
}

#[test]
fn test_active_matches_read_extends_ttl_after_ledger_advancement() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "95c78e0c"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 1000);

    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 1);
    assert_eq!(active_matches.get(0).unwrap().id, id);
}

#[test]
fn test_ttl_extended_on_submit_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "b615c463"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player2, &oracle);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_ttl_extended_on_cancel() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "07b1d378"),
        &Platform::Lichess,
    );

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.cancel_match(&id, &player1);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_is_funded_extends_ttl() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "fb36b5c0"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.is_funded(&id);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_ttl_extended_on_get_escrow_balance() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "3a2ad9a0"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);

    let ttl_before = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });

    let _balance = client.get_escrow_balance(&id);

    let ttl_after = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });

    assert!(
        ttl_after >= ttl_before,
        "TTL should be extended after get_escrow_balance"
    );
}

#[test]
fn test_get_match_extends_ttl_on_read() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ac157bb4"),
        &Platform::Lichess,
    );

    client.get_match(&id);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, MATCH_TTL_LEDGERS);
}

#[test]
fn test_get_match_resets_ttl_after_ledger_advance() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "3a93aa2a"),
        &Platform::Lichess,
    );

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.get_match(&id);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_player_match_index_ttl_refreshes_on_append() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "377c9285"),
        &Platform::Lichess,
    );

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "12bf42e3"),
        &Platform::Lichess,
    );

    let ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::PlayerMatches(player1.clone()))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_player_match_index_ttl_refreshes_on_read() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "a9abc8a8"),
        &Platform::Lichess,
    );

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.get_player_matches(&player1);

    let ttl = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::PlayerMatches(player1.clone()))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_get_player_matches_ttl_returns_correct_value() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Before any matches, key doesn't exist
    let ttl_before = env.as_contract(&contract_id, || {
        let key = DataKey::PlayerMatches(player1.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().get_ttl(&key)
        } else {
            0u32
        }
    });
    assert_eq!(ttl_before, 0);

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "888cbb28"),
        &Platform::Lichess,
    );

    let ttl_after = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::PlayerMatches(player1.clone()))
    });
    assert_eq!(ttl_after, crate::MATCH_TTL_LEDGERS);

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    let ttl_decreased = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::PlayerMatches(player1.clone()))
    });
    assert!(
        ttl_decreased < ttl_after,
        "TTL should decrease after ledger advancement"
    );
    assert!(
        ttl_decreased >= ttl_after - 1000,
        "TTL should be approximately 1000 less"
    );

    client.get_player_matches(&player1);
    let ttl_refreshed = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .get_ttl(&DataKey::PlayerMatches(player1.clone()))
    });
    assert_eq!(ttl_refreshed, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_get_player_matches_ttl_for_nonexistent_player() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let random_player = Address::generate(&env);
    let ttl = env.as_contract(&contract_id, || {
        let key = DataKey::PlayerMatches(random_player.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().get_ttl(&key)
        } else {
            0u32
        }
    });
    assert_eq!(ttl, 0, "TTL should be 0 for player with no match history");
}

#[test]
fn test_submit_result_extends_match_ttl() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "1edf395a"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.ledger().set(soroban_sdk::testutils::LedgerInfo {
        sequence_number: env.ledger().sequence() + 1000,
        timestamp: env.ledger().timestamp() + 5000,
        protocol_version: 22,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: crate::MATCH_TTL_LEDGERS + 2000,
    });

    client.submit_result(&id, &Winner::Player1, &oracle);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}
