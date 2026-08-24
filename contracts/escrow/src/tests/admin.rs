use super::*;

#[test]
fn test_pause_on_uninitialized_contract_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_pause(&Address::generate(&env));
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_admin_pause_blocks_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "fdc7c9d7"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_admin_unpause_allows_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);
    client.unpause(&admin);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "35fc8b4a"),
        &Platform::Lichess,
    );
    assert_eq!(id, 0);
}

#[test]
fn test_admin_unpause_allows_deposit_after_paused() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "0171a0c9"),
        &Platform::Lichess,
    );

    client.pause(&admin);
    let result = client.try_deposit(&id, &player1);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));

    client.unpause(&admin);
    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert!(m.player1_deposited, "deposit should succeed after unpause");
}

#[test]
fn test_admin_unpause_allows_submit_result_after_paused() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ceda6d69"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    client.pause(&admin);
    let result = client.try_submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));

    client.unpause(&admin);
    client.submit_result(&id, &Winner::Player1, &oracle);
    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Completed);
}

#[test]
fn test_paused_contract_rejects_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "d7153de4"),
        &Platform::Lichess,
    );

    client.pause(&admin);

    let result = client.try_deposit(&id, &player1);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_deposit_blocked_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "b59dc515"),
        &Platform::Lichess,
    );

    client.pause(&admin);

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
        &String::from_str(&env, "6a312768"),
        &Platform::Lichess,
    );

    let unauthorized_address = Address::generate(&env);

    let result = client.try_deposit(&id, &unauthorized_address);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_submit_result_blocked_when_paused() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cea8b158"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    client.pause(&admin);

    let result = client.try_submit_result(&id, &Winner::Player1, &oracle);
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
        &String::from_str(&env, "a5c1750c"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.mock_auths(&[MockAuth {
        address: &oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player2, oracle.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_submit_result(&id, &Winner::Player2, &oracle);
    assert!(
        matches!(result, Err(Err(_)) | Err(Ok(Error::Unauthorized))),
        "old oracle must not be able to submit results"
    );

    env.mock_auths(&[MockAuth {
        address: &new_oracle,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player2, new_oracle.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.submit_result(&id, &Winner::Player2, &new_oracle);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
}

#[test]
fn test_non_oracle_unauthorized_even_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "fea151a8"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    client.pause(&admin);

    let non_oracle = Address::generate(&env);
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &non_oracle,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id, Winner::Player1, non_oracle.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_submit_result(&id, &Winner::Player1, &non_oracle);
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
        &String::from_str(&env, "5ad728ec"),
        &Platform::Lichess,
    );
    client.deposit(&id1, &player1);
    client.deposit(&id1, &player2);

    env.mock_auths(&[MockAuth {
        address: &oracle_new,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id1, Winner::Player1, oracle_new.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.submit_result(&id1, &Winner::Player1, &oracle_new);
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
        &String::from_str(&env, "e53c88a5"),
        &Platform::Lichess,
    );
    client.deposit(&id2, &player1);
    client.deposit(&id2, &player2);

    env.mock_auths(&[MockAuth {
        address: &oracle_old,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (id2, Winner::Player1, oracle_old.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_submit_result(&id2, &Winner::Player1, &oracle_old);
    assert!(
        matches!(result, Err(Err(_)) | Err(Ok(Error::Unauthorized))),
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
        &String::from_str(&env, "272549d8"),
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
            args: (id, Winner::Player1, non_oracle.clone()).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_submit_result(&id, &Winner::Player1, &non_oracle);
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

    client.transfer_admin(&new_admin, &admin);
    assert_eq!(client.get_admin(), new_admin);

    env.mock_auths(&[MockAuth {
        address: &admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    let result = client.try_pause(&admin);
    assert!(
        result.is_err(),
        "old admin should be rejected from pause after transfer"
    );

    env.mock_auths(&[MockAuth {
        address: &new_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: (new_admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.pause(&new_admin);
}

#[test]
fn test_is_paused_cycle() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert!(!client.is_paused());
    client.pause(&admin);
    assert!(client.is_paused());
    client.unpause(&admin);
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
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
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
            args: (admin.clone(),).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.pause(&admin);
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
    assert!(
        result.is_err(),
        "pending_admin_a should not be able to accept"
    );

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

// #737 / #1160 - set_match_timeout validates minimum bound
#[test]
fn test_set_match_timeout_rejects_below_minimum() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let min_timeout = MIN_MATCH_TIMEOUT_SECONDS;
    let below_min = min_timeout - 1;

    let result = client.try_set_match_timeout(&below_min);
    assert_eq!(result, Err(Ok(Error::InvalidTimeout)));
}

// #737 / #1160 - set_match_timeout validates maximum bound
#[test]
fn test_set_match_timeout_rejects_above_maximum() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let max_timeout = MAX_MATCH_TIMEOUT_SECONDS;
    let above_max = max_timeout + 1;

    let result = client.try_set_match_timeout(&above_max);
    assert_eq!(result, Err(Ok(Error::InvalidTimeout)));
}

// #737 / #1160 - set_match_timeout accepts valid minimum value
#[test]
fn test_set_match_timeout_accepts_minimum_valid_value() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let min_timeout = MIN_MATCH_TIMEOUT_SECONDS;
    client.set_match_timeout(&min_timeout);

    let result = client.get_match_timeout();
    assert_eq!(result, min_timeout);
}

// #737 / #1160 - set_match_timeout accepts valid maximum value
#[test]
fn test_set_match_timeout_accepts_maximum_valid_value() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let max_timeout = MAX_MATCH_TIMEOUT_SECONDS;
    client.set_match_timeout(&max_timeout);

    let result = client.get_match_timeout();
    assert_eq!(result, max_timeout);
}

// #737 / #1160 - set_match_timeout requires admin authorization
#[test]
fn test_set_match_timeout_requires_admin_authorization() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let attacker = Address::generate(&env);
    let new_timeout = 172_800u64;

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_match_timeout",
            args: (new_timeout,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_match_timeout(&new_timeout);
    assert!(
        result.is_err(),
        "non-admin should not be able to set timeout"
    );
}

// #1159 - set_maximum_stake requires admin authorization
#[test]
fn test_set_maximum_stake_requires_admin_authorization() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let attacker = Address::generate(&env);
    let new_max = Some(500i128);

    env.mock_auths(&[MockAuth {
        address: &attacker,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "set_maximum_stake",
            args: (new_max,).into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_set_maximum_stake(&new_max);
    assert!(
        result.is_err(),
        "non-admin should not be able to set maximum_stake"
    );
}

// #1159 - set_maximum_stake updates ProtocolConfig.maximum_stake
#[test]
fn test_set_maximum_stake_updates_config() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_maximum_stake(&Some(500));
    assert_eq!(client.get_protocol_config().maximum_stake, Some(500));

    client.set_maximum_stake(&None);
    assert_eq!(client.get_protocol_config().maximum_stake, None);
}

// #1159 - set_maximum_stake rejects a non-positive cap
#[test]
fn test_set_maximum_stake_rejects_non_positive() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_set_maximum_stake(&Some(0));
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

// #1133 — deposit is rejected when contract is paused
#[test]
fn test_deposit_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cbaadb4a"),
        &Platform::Lichess,
    );

    client.pause(&admin);

    let result = client.try_deposit(&id, &player1);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

// #1132 — create_match is rejected when contract is paused
#[test]
fn test_create_match_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "bcdf5273"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

// #766 — two-step admin transfer: propose_admin + accept_admin happy-path
#[test]
fn test_two_step_admin_transfer() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    // Propose: current admin should not change yet
    client.propose_admin(&new_admin);
    assert_eq!(
        client.get_admin(),
        admin,
        "admin must not change before accept"
    );

    // Accept: new_admin calls accept_admin
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

    // Admin is now new_admin and PendingAdmin key is cleared
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must be new_admin after accept"
    );
    let pending: Option<PendingAdminProposal> = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::PendingAdmin)
    });
    assert!(
        pending.is_none(),
        "PendingAdmin key must be cleared after acceptance"
    );
}

// #1101 — pause blocks create_match and unpause restores it
#[test]
fn test_pause_blocks_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause(&admin);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "fdc7c9d7"),
        &Platform::Lichess,
    );
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "create_match must fail when paused"
    );

    client.unpause(&admin);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "35fc8b4a"),
        &Platform::Lichess,
    );
    assert_eq!(id, 0, "create_match must succeed after unpause");
}

// #1162 — get_contract_version returns the crate's semver string
#[test]
fn test_get_contract_version_returns_semver_string() {
    let (env, contract_id, ..) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let version = client.get_contract_version();

    assert!(!version.is_empty(), "contract version must not be empty");

    use std::string::ToString;
    let version_str = version.to_string();
    let parts: std::vec::Vec<&str> = version_str.split('.').collect();
    assert_eq!(
        parts.len(),
        3,
        "contract version must be semver-formatted (major.minor.patch): {}",
        version_str
    );
    for part in parts {
        assert!(
            part.chars().all(|c| c.is_ascii_digit()) && !part.is_empty(),
            "each semver component must be a non-empty numeric string: {}",
            version_str
        );
    }
}

// #1281 — transfer_admin clears pending admin, preventing stale nominee hijack
#[test]
fn test_stale_nominee_cannot_hijack_after_transfer_admin() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin_a) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let pending_admin_b = Address::generate(&env);
    let admin_c = Address::generate(&env);

    client.propose_admin(&pending_admin_b);
    assert_eq!(
        client.get_admin(),
        admin_a,
        "admin must not change after propose"
    );

    client.transfer_admin(&admin_c, &admin_a);
    assert_eq!(
        client.get_admin(),
        admin_c,
        "admin must be admin_c after transfer"
    );

    env.mock_auths(&[MockAuth {
        address: &pending_admin_b,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_accept_admin();
    assert!(
        result.is_err(),
        "stale pending_admin_b should not be able to accept after transfer_admin"
    );
    assert_eq!(
        client.get_admin(),
        admin_c,
        "admin must remain admin_c after failed accept attempt"
    );
}

// #1281 — legitimate two-step transfer still works when no intervening transfer_admin
#[test]
fn test_legitimate_propose_accept_flow_still_works() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.propose_admin(&new_admin);
    assert_eq!(
        client.get_admin(),
        admin,
        "admin must not change after propose"
    );

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
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must be new_admin after accept"
    );
}

// #1281 — transfer_admin with no outstanding proposal is a no-op for pending_admin
#[test]
fn test_transfer_admin_with_no_pending_proposal() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.transfer_admin(&new_admin, &admin);
    assert_eq!(
        client.get_admin(),
        new_admin,
        "admin must change to new_admin"
    );

    let pending: Option<PendingAdminProposal> = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::PendingAdmin)
    });
    assert!(
        pending.is_none(),
        "no pending admin should exist when none was proposed"
    );
}

// #1281 — accept_admin fails when no proposal is pending
#[test]
fn test_accept_admin_fails_when_no_proposal_pending() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let random_address = Address::generate(&env);
    env.mock_auths(&[MockAuth {
        address: &random_address,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_accept_admin();
    assert!(
        result.is_err(),
        "accept_admin must fail when no proposal is pending"
    );
}

// #1281 — accept_admin fails when called by wrong address
#[test]
fn test_accept_admin_fails_when_called_by_wrong_address() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let pending_admin = Address::generate(&env);
    let wrong_address = Address::generate(&env);

    client.propose_admin(&pending_admin);

    env.mock_auths(&[MockAuth {
        address: &wrong_address,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_accept_admin();
    assert!(
        result.is_err(),
        "accept_admin must fail when called by wrong address"
    );
}

// #1281 — accept_admin fails when proposer is no longer the current admin
#[test]
fn test_accept_admin_fails_when_proposer_changed() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin_a) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let pending_admin_b = Address::generate(&env);
    let admin_c = Address::generate(&env);

    client.propose_admin(&pending_admin_b);
    assert_eq!(
        client.get_admin(),
        admin_a,
        "initial admin should be admin_a"
    );

    let proposal: PendingAdminProposal = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap()
    });
    assert_eq!(
        proposal.proposer, admin_a,
        "proposer should be the initial admin"
    );

    client.transfer_admin(&admin_c, &admin_a);
    assert_eq!(client.get_admin(), admin_c, "admin should now be admin_c");

    env.mock_auths(&[MockAuth {
        address: &pending_admin_b,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "accept_admin",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_accept_admin();
    assert!(
        result.is_err(),
        "accept_admin must fail because proposer (admin_a) is not the current admin (admin_c)"
    );
}
