use super::*;
use soroban_sdk::testutils::{
    storage::{Instance as _, Persistent as _},
    Address as _, Ledger as _,
};

#[test]
fn test_ttl_extended_on_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ttl_game1"),
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
    let game_id = String::from_str(&env, "reserved_game_ttl");

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &game_id,
        &Platform::Lichess,
    );

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::GameId(game_id.clone()))
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
        &String::from_str(&env, "ttl_game2"),
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
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ttl_active_append_remove_1"),
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

    let _match2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ttl_active_append_remove_2"),
        &Platform::Lichess,
    );

    let ttl_after_append = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::ActiveMatches)
    });
    assert_eq!(ttl_after_append, crate::MATCH_TTL_LEDGERS);

    client.deposit(&match1, &player1);
    client.deposit(&match1, &player2);

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

    client.submit_result(&match1, &Winner::Player1);

    let ttl_after_removal = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::ActiveMatches)
    });
    assert_eq!(ttl_after_removal, crate::MATCH_TTL_LEDGERS);
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
        &String::from_str(&env, "ttl_active_read"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.ledger().set_sequence_number(env.ledger().sequence() + 1000);

    let active_matches = client.get_active_matches();
    assert_eq!(active_matches.len(), 1);
    assert_eq!(active_matches.get(0).unwrap().id, id);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::ActiveMatches)
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_ttl_extended_on_submit_result() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ttl_game3"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player2);

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
        &String::from_str(&env, "ttl_game4"),
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
        &String::from_str(&env, "ttl_is_funded"),
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
        &String::from_str(&env, "ttl_balance_game"),
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
        &String::from_str(&env, "ttl_read_test"),
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
        &String::from_str(&env, "ttl_get_match"),
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
        &String::from_str(&env, "ttl_index_append_1"),
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
        &String::from_str(&env, "ttl_index_append_2"),
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
        &String::from_str(&env, "ttl_index_read"),
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
        &String::from_str(&env, "ttl_getter_test"),
        &Platform::Lichess,
    );

    let ttl_after = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::PlayerMatches(player1.clone()))
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
        env.storage().persistent().get_ttl(&DataKey::PlayerMatches(player1.clone()))
    });
    assert!(ttl_decreased < ttl_after, "TTL should decrease after ledger advancement");
    assert!(ttl_decreased >= ttl_after - 1000, "TTL should be approximately 1000 less");

    client.get_player_matches(&player1);
    let ttl_refreshed = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::PlayerMatches(player1.clone()))
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
