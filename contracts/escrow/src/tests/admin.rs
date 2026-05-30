use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};

#[test]
fn test_pause_on_uninitialized_contract_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_pause();
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_admin_pause_blocks_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "paused_game"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_admin_unpause_allows_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();
    client.unpause();

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "unpaused_game"),
        &Platform::Lichess,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_paused_contract_rejects_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game123"),
        &Platform::Lichess,
    );

    client.pause();

    let result = client.try_deposit(&id, &player1);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_deposit_blocked_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "paused_deposit_game"),
        &Platform::Lichess,
    );

    client.pause();

    let result = client.try_deposit(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "deposit must return ContractPaused when the contract is paused"
    );
}

#[test]
fn test_deposit_by_unauthorized_address_returns_unauthorized() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "unauth_deposit_game"),
        &Platform::Lichess,
    );

    let unauthorized_address = Address::generate(&env);

    let result = client.try_deposit(&id, &unauthorized_address);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_submit_result_blocked_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "paused_submit_game"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    client.pause();

    let result = client.try_submit_result(&id, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_admin_can_rotate_oracle() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let next_oracle = Address::generate(&env);
    client.update_oracle(&next_oracle);
    assert_eq!(client.get_oracle(), next_oracle);

    let attacker = Address::generate(&env);
    let rotate_to = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_oracle",
            args: (rotate_to.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    assert!(client.try_update_oracle(&rotate_to).is_err());
}

#[test]
fn test_update_oracle_rejects_self_address() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_update_oracle(&contract_id);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
}

#[test]
fn test_old_oracle_rejected_after_rotation() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_oracle = Address::generate(&env);
    client.update_oracle(&new_oracle);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "oracle_rotation"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player2).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_submit_result(&id, &Winner::Player2);
    assert!(
        matches!(result, Err(Err(_))),
        "old oracle must not be able to submit results"
    );

    env.mock_auths(&[MockAuth {
        address: &new_oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player2).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.submit_result(&id, &Winner::Player2);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
}

#[test]
fn test_non_oracle_unauthorized_even_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "paused_unauth"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    client.pause();

    let non_oracle = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_oracle,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player1).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_submit_result(&id, &Winner::Player1);
    assert!(
        matches!(
            result,
            Err(Err(_)) | Err(Ok(Error::Unauthorized)) | Err(Ok(Error::ContractPaused))
        ),
        "expected auth failure (Abort, Unauthorized, or ContractPaused) for non-oracle caller on paused contract"
    );
}

// #373 — update_oracle routes subsequent submit_result to the new oracle
#[test]
fn test_update_oracle_routes_submit_result() {
    let (env, contract_id, oracle_old, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let oracle_new = Address::generate(&env);
    client.update_oracle(&oracle_new);
    assert_eq!(client.get_oracle(), oracle_new);

    let id1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "oracle_new_match"),
        &Platform::Lichess,
    );
    client.deposit(&id1, &player1);
    client.deposit(&id1, &player2);

    env.mock_auths(&[MockAuth {
        address: &oracle_new,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id1, Winner::Player1).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.submit_result(&id1, &Winner::Player1);
    assert_eq!(client.get_match(&id1).state, MatchState::Completed);

    env.mock_all_auths();

    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &100);
    asset_client.mint(&player2, &100);
    let id2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "oracle_old_match"),
        &Platform::Lichess,
    );
    client.deposit(&id2, &player1);
    client.deposit(&id2, &player2);

    env.mock_auths(&[MockAuth {
        address: &oracle_old,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id2, Winner::Player1).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_submit_result(&id2, &Winner::Player1);
    assert!(
        matches!(result, Err(Err(_))),
        "old oracle must be rejected after rotation"
    );
}

#[test]
fn test_submit_result_from_non_oracle_returns_unauthorized() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "non_oracle_submit_game"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let non_oracle = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &non_oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player1).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_submit_result(&id, &Winner::Player1);
    assert!(
        matches!(result, Err(Err(_)) | Err(Ok(Error::Unauthorized))),
        "expected auth failure for non-oracle caller"
    );
}

#[test]
fn test_get_oracle_returns_initialized_address() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    assert_eq!(client.get_oracle(), oracle);
}

#[test]
fn test_get_oracle_returns_updated_address_after_update_oracle() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let new_oracle = Address::generate(&env);
    client.update_oracle(&new_oracle);
    assert_eq!(client.get_oracle(), new_oracle);
}

#[test]
fn test_update_oracle_rejects_non_admin_caller() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let attacker = Address::generate(&env);
    let new_oracle = Address::generate(&env);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "update_oracle",
            args: (new_oracle.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_update_oracle(&new_oracle);
    assert!(
        result.is_err(),
        "update_oracle must reject a non-admin caller"
    );
}

#[test]
fn test_transfer_admin_pause_auth() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    client.transfer_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_pause();
    assert!(
        result.is_err(),
        "old admin should be rejected from pause after transfer"
    );

    env.mock_auths(&[MockAuth {
        address: &new_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.pause();
}

#[test]
fn test_is_paused_cycle() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert!(!client.is_paused());
    client.pause();
    assert!(client.is_paused());
    client.unpause();
    assert!(!client.is_paused());
}


// #593 - propose_admin stores the pending admin and emits an event
#[test]
fn test_propose_admin_stores_pending_admin_and_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("propose").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "propose event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_pending: Address = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_pending, new_admin);
}


// #594 - accept_admin finalizes the transfer and emits an event
#[test]
fn test_accept_admin_finalizes_transfer_and_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);

    env.mock_auths(&[MockAuth {
        address: &new_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("xfer").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "xfer event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_new_admin: Address = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_new_admin, new_admin);
}


// #595 - current admin retains privileges after propose_admin and before accept_admin
#[test]
fn test_current_admin_retains_privileges_after_propose_before_accept() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.pause();
    assert!(client.is_paused());
}


// #596 - proposing a second pending admin cleanly replaces the first proposal
#[test]
fn test_second_pending_admin_replaces_first_proposal() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let pending_admin_a = Address::generate(&env);
    let pending_admin_b = Address::generate(&env);

    client.propose_admin(&pending_admin_a);
    client.propose_admin(&pending_admin_b);

    env.mock_auths(&[MockAuth {
        address: &pending_admin_a,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_accept_admin();
    assert!(result.is_err(), "pending_admin_a should not be able to accept");

    env.mock_auths(&[MockAuth {
        address: &pending_admin_b,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.accept_admin();
    assert_eq!(client.get_admin(), pending_admin_b);
}
