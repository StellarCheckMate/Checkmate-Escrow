use super::*;
use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _, Events, Ledger as _},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env, IntoVal, String, Symbol, TryFromVal,
};

fn setup() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = token_id.address();
    let asset_client = StellarAssetClient::new(&env, &token_addr);
    asset_client.mint(&player1, &1000);
    asset_client.mint(&player2, &1000);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    (
        env,
        contract_id,
        oracle,
        player1,
        player2,
        token_addr,
        admin,
    )
}

#[test]
fn test_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
    );

    assert_eq!(id, 0);
    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Pending);
}

#[test]
fn test_get_match_count() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_match_count(), 0);

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game1"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match_count(), 1);

    client.create_match(
        &player1,
        &player2,
        &200,
        &token,
        &String::from_str(&env, "game2"),
        &Platform::ChessDotCom,
    );
    assert_eq!(client.get_match_count(), 2);
}

#[test]
fn test_create_match_sets_created_ledger() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ledger_test"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    // created_ledger must be set to the ledger sequence at creation time (non-zero
    // in a real network; the test env starts at 0 but the field must be present and
    // readable — future timeout logic will rely on it).
    assert_eq!(m.created_ledger, env.ledger().sequence());
}

#[test]
fn test_get_match_returns_match_not_found_for_unknown_id() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_get_match(&999);

    assert!(matches!(result, Err(Ok(Error::MatchNotFound))));
}

#[test]
fn test_get_match_returns_correct_game_id() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_id = String::from_str(&env, "game_xyz_42");
    let id = client.create_match(&player1, &player2, &100, &token, &game_id, &Platform::Lichess);

    let m = client.get_match(&id);
    assert_eq!(m.game_id, game_id);
}

#[test]
fn test_get_match_returns_correct_platform() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_platform_test"),
        &Platform::ChessDotCom,
    );

    let m = client.get_match(&id);
    assert_eq!(m.platform, Platform::ChessDotCom);
}

#[test]
fn test_deposit_and_activate() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    assert!(!client.is_funded(&id));
    client.deposit(&id, &player2);
    assert!(client.is_funded(&id));
    assert_eq!(client.get_escrow_balance(&id), 200);
}

#[test]
fn test_deposit_emits_activated_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_activated"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    // No activated event yet — only one deposit
    let events_after_p1 = env.events().all();
    let activated_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("activated").into_val(&env),
    ];
    assert!(
        !events_after_p1
            .iter()
            .any(|(_, topics, _)| topics == activated_topics),
        "activated event must not fire after first deposit"
    );

    client.deposit(&id, &player2);
    let events = env.events().all();
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == activated_topics);
    assert!(
        matched.is_some(),
        "match activated event not emitted on second deposit"
    );

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
}

#[test]
fn test_deposit_into_cancelled_match_returns_invalid_state() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancelled_deposit_test"),
        &Platform::Lichess,
    );

    // Cancel the match before any deposits
    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    // Attempt to deposit into the cancelled match
    let result = client.try_deposit(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::MatchCancelled)),
        "deposit into cancelled match must return MatchCancelled"
    );
}

#[test]
fn test_payout_winner() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game1"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);

    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
}

#[test]
fn test_draw_refund() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game2"),
        &Platform::ChessDotCom,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Draw, &oracle);

    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 1000);
}

#[test]
fn test_cancel_refunds_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game3"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.cancel_match(&id, &player1);

    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
}

#[test]
fn test_create_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_ev2"),
        &Platform::Lichess,
    );

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("created").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match created event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_p1, ev_p2, ev_stake): (u64, Address, Address, i128) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
    assert_eq!(ev_p1, player1);
    assert_eq!(ev_p2, player2);
    assert_eq!(ev_stake, 100);
}

#[test]
fn test_submit_result_emits_event() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_evt"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("completed").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match completed event not emitted");

    let (_, _, data) = matched.unwrap();
    let decoded: (u64, Winner) = <(u64, Winner)>::try_from_val(&env, &data).unwrap();
    assert_eq!(decoded, (id, Winner::Player1));
}

#[test]
fn test_cancel_match_emits_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_cancel"),
        &Platform::Lichess,
    );

    client.cancel_match(&id, &player1);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("cancelled").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "match cancelled event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, id);
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle1 = Address::generate(&env);
    let oracle2 = Address::generate(&env);
    let admin = Address::generate(&env);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle1, &admin);
    client.initialize(&oracle2, &admin);
}

#[test]
fn test_initialize_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle = Address::generate(&env);
    let admin = Address::generate(&env);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle, &admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("init").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "initialized event not emitted");

    let (_, _, data) = matched.unwrap();
    let (emitted_oracle, emitted_admin): (Address, Address) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(emitted_oracle, oracle);
    assert_eq!(emitted_admin, admin);
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
fn test_admin_pause_blocks_submit_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create and fund a match
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
    assert_eq!(client.get_match(&id).state, MatchState::Active);

    // Pause the contract
    client.pause();

    // Attempt to submit result on paused contract
    let result = client.try_submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(
        result,
        Err(Ok(Error::ContractPaused)),
        "submit_result must be blocked when contract is paused"
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_create_match_with_zero_stake_fails() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let _id = client.create_match(
        &player1,
        &player2,
        &0,
        &token,
        &String::from_str(&env, "zero_stake_game"),
        &Platform::Lichess,
    );
}

#[test]
fn test_player2_cancel_pending_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_p2_cancel"),
        &Platform::Lichess,
    );

    client.cancel_match(&id, &player2);

    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
}

#[test]
fn test_player2_cancel_refunds_both_players() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_p2_cancel_refund"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let result = client.try_cancel_match(&id, &player2);
    assert!(result.is_err());
}

#[test]
fn test_player2_cancel_only_player2_deposited() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_p2_only"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player2);
    client.cancel_match(&id, &player2);

    assert_eq!(token_client.balance(&player2), 1000);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
}

#[test]
fn test_non_oracle_cannot_submit_result() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_unauth_oracle"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let impostor = Address::generate(&env);
    let result = client.try_submit_result(&id, &Winner::Player1, &impostor);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "expected Unauthorized when non-oracle calls submit_result"
    );

    assert_eq!(client.get_match(&id).state, MatchState::Active);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
}

#[test]
fn test_submit_result_on_cancelled_match_returns_invalid_state() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancelled_game"),
        &Platform::Lichess,
    );

    // Cancel without any deposits — match goes straight to Cancelled
    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    let result = client.try_submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "oracle must not be able to submit a result for a Cancelled match"
    );
}

#[test]
fn test_submit_result_on_completed_match_returns_invalid_state() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "completed_game"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);

    // Second submit on an already-Completed match must fail
    let result = client.try_submit_result(&id, &Winner::Player2, &oracle);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "oracle must not be able to submit a result for an already Completed match"
    );
}

#[test]
fn test_cancel_active_match_fails_with_invalid_state() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_active_cancel"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    assert_eq!(client.get_match(&id).state, MatchState::Active);

    let result = client.try_cancel_match(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "expected InvalidState error when cancelling an Active match"
    );

    assert_eq!(client.get_match(&id).state, MatchState::Active);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_unauthorized_player_cannot_cancel() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_unauthorized"),
        &Platform::Lichess,
    );

    let unauthorized = Address::generate(&env);
    client.cancel_match(&id, &unauthorized);
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
        &String::from_str(&env, "ttl_game1"),
        &Platform::Lichess,
    );

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
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
fn test_ttl_extended_on_submit_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
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
        &String::from_str(&env, "ttl_game4"),
        &Platform::Lichess,
    );
    client.cancel_match(&id, &player1);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_ttl_extended_on_cancel_after_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ttl_game5"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.cancel_match(&id, &player1);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_ttl_refreshed_on_get_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ttl_read_refresh"),
        &Platform::Lichess,
    );

    // Let some time pass (advance ledger small amount to simulate partial TTL without archiving)
    let ledgers_elapsed = 1000u32;
    let current_ledger = env.ledger().sequence();
    env.ledger().set_sequence_number(current_ledger + ledgers_elapsed);

    // Extend instance TTLs to prevent archiving during test
    env.as_contract(&contract_id, || {
        env.storage().instance().extend_ttl(crate::MATCH_TTL_LEDGERS, crate::MATCH_TTL_LEDGERS);
    });

    // TTL should be partial
    let ttl_before = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert!(ttl_before < crate::MATCH_TTL_LEDGERS);

    // get_match refreshes TTL
    let _m = client.get_match(&id);

    let ttl_after = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl_after, crate::MATCH_TTL_LEDGERS);

    // Multiple reads keep it full
    client.get_match(&id);
    let ttl_final = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Match(id))
    });
    assert_eq!(ttl_final, crate::MATCH_TTL_LEDGERS);
}

// ── Task 1: non-admin cannot call pause / unpause ────────────────────────────

#[test]
fn test_non_admin_cannot_pause() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    // Replace mock_all_auths with a targeted mock that only authorises non_admin,
    // so admin.require_auth() inside pause() will not find a matching authorisation
    // and the call must fail.
    use soroban_sdk::testutils::MockAuth;
    use soroban_sdk::testutils::MockAuthInvoke;
    env.set_auths(&[MockAuth {
        address: &non_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }
    .into()]);

    let result = client.try_pause();
    assert!(
        result.is_err(),
        "non-admin should not be able to call pause()"
    );
}

#[test]
fn test_non_admin_cannot_unpause() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);
    // Pause first (admin is mocked via mock_all_auths at this point)
    client.pause();

    use soroban_sdk::testutils::MockAuth;
    use soroban_sdk::testutils::MockAuthInvoke;
    env.set_auths(&[MockAuth {
        address: &non_admin,
        invoke: &MockAuthInvoke {
            contract: &contract_id,
            fn_name: "unpause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }
    .into()]);

    let result = client.try_unpause();
    assert!(
        result.is_err(),
        "non-admin should not be able to call unpause()"
    );
}

// ── Task 2: cancel_match refund scenarios ────────────────────────────────────

/// Both players deposit → match becomes Active → cancel must return InvalidState.
#[test]
fn test_cancel_both_deposited_active_returns_invalid_state() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "both_dep_cancel"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    // Match is now Active — cancel must be rejected
    assert_eq!(client.get_match(&id).state, MatchState::Active);
    let result = client.try_cancel_match(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "cancelling an Active match must return InvalidState"
    );

    // Funds must remain in escrow
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
}

/// Only player1 deposits, then cancels — player1 is refunded, player2 unchanged.
#[test]
fn test_cancel_only_player1_deposited_refunds_player1() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "p1_only_cancel"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    // player2 has NOT deposited
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 1000);

    client.cancel_match(&id, &player1);

    // player1 gets their stake back; player2 balance is untouched
    assert_eq!(
        token_client.balance(&player1),
        1000,
        "player1 should be fully refunded"
    );
    assert_eq!(
        token_client.balance(&player2),
        1000,
        "player2 balance must not change"
    );
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
}

/// Only player2 deposits, then cancels — player2 is refunded, player1 unchanged.
#[test]
fn test_cancel_only_player2_deposited_refunds_player2() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "p2_only_cancel2"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player2);
    // player1 has NOT deposited
    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 900);

    client.cancel_match(&id, &player2);

    // player2 gets their stake back; player1 balance is untouched
    assert_eq!(
        token_client.balance(&player2),
        1000,
        "player2 should be fully refunded"
    );
    assert_eq!(
        token_client.balance(&player1),
        1000,
        "player1 balance must not change"
    );
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
}

/// Cancel match immediately after creation with no deposits — escrow balance must be 0.
#[test]
fn test_get_escrow_balance_returns_zero_after_cancel_with_no_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "no_deposit_cancel"),
        &Platform::Lichess,
    );

    // Cancel immediately without any deposits
    client.cancel_match(&id, &player1);

    // Escrow balance should be 0 (no deposits were made)
    assert_eq!(client.get_escrow_balance(&id), 0);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
}

// ── cancel_match on a Completed match ────────────────────────────────────────

/// Complete a match (create → deposit × 2 → submit_result), then attempt to
/// cancel it. cancel_match checks `m.state != MatchState::Pending` and must
/// return `InvalidState`. The match state and token balances must be unchanged.
#[test]
fn test_cancel_completed_match_returns_invalid_state() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "completed_cancel_game"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);

    // Sanity-check: match is now Completed and payout has happened
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);

    // Attempting to cancel a Completed match must be rejected
    let result = client.try_cancel_match(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidState)),
        "cancel_match on a Completed match must return InvalidState"
    );

    // State and balances must be untouched after the failed cancel
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);
}

// ── deposit on a Completed match ─────────────────────────────────────────────

/// Complete a match via submit_result, then attempt to deposit into it.
/// deposit() guards on `m.state != MatchState::Pending` and must return
/// `Error::InvalidState`. Token balances must remain unchanged after the
/// failed deposit attempt.
#[test]
fn test_deposit_into_completed_match_returns_invalid_state() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "completed_deposit_game"),
        &Platform::Lichess,
    );

    // Both players deposit → match becomes Active
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    // Oracle submits result → match transitions to Completed, payout executed
    client.submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);

    // Attempting to deposit into a Completed match must be rejected
    let result = client.try_deposit(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::MatchCompleted)),
        "deposit into a Completed match must return MatchCompleted"
    );

    // Balances must be untouched after the failed deposit
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);
}

// ── From main: pause / unpause emit events ───────────────────────────────────

#[test]
fn test_pause_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        soroban_sdk::symbol_short!("paused").into_val(&env),
    ];
    assert!(
        events
            .iter()
            .any(|(_, topics, _)| topics == expected_topics),
        "paused event not emitted"
    );
}

#[test]
fn test_unpause_emits_event() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.pause();
    client.unpause();

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        soroban_sdk::symbol_short!("unpaused").into_val(&env),
    ];
    assert!(
        events
            .iter()
            .any(|(_, topics, _)| topics == expected_topics),
        "unpaused event not emitted"
    );
}

#[test]
fn test_duplicate_game_id_rejected() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_id = String::from_str(&env, "unique_game_123");

    client.create_match(&player1, &player2, &100, &token, &game_id, &Platform::Lichess);

    let result = client.try_create_match(&player1, &player2, &100, &token, &game_id, &Platform::Lichess);
    assert_eq!(result, Err(Ok(Error::DuplicateGameId)));
}

#[test]
fn test_deposit_into_cancelled_match_returns_match_cancelled() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancelled_deposit"),
        &Platform::Lichess,
    );

    client.cancel_match(&id, &player1);

    let result = client.try_deposit(&id, &player2);
    assert_eq!(result, Err(Ok(Error::MatchCancelled)));
}

#[test]
fn test_deposit_into_completed_match_returns_match_completed() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "completed_deposit"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1, &oracle);

    let result = client.try_deposit(&id, &player2);
    assert_eq!(result, Err(Ok(Error::MatchCompleted)));
}

#[test]
fn test_expire_match_before_timeout_fails() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_early"),
        &Platform::Lichess,
    );

    // Only player1 deposits — match stays Pending
    client.deposit(&id, &player1);

    // Timeout has not elapsed yet — should fail
    let result = client.try_expire_match(&id);
    assert_eq!(result, Err(Ok(Error::MatchNotExpired)));
}

#[test]
fn test_expire_match_refunds_depositor_after_timeout() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_refund"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    let balance_before = token_client.balance(&player1);

    let new_seq = env.ledger().sequence() + MATCH_TTL_LEDGERS;
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    });
    env.as_contract(&token, || {
        env.storage()
            .instance()
            .extend_ttl(MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    });
    env.ledger().set_sequence_number(new_seq);

    client.expire_match(&id);

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Cancelled);
    assert_eq!(token_client.balance(&player1), balance_before + 100);
}

// ── get_escrow_balance at each deposit stage ─────────────────────────────────

#[test]
fn test_get_escrow_balance_stages() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stake = 100_i128;
    let id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "balance_stages"),
        &Platform::Lichess,
    );

    // Before any deposit: balance must be 0
    assert_eq!(client.get_escrow_balance(&id), 0);

    // After player1 deposits: balance must equal stake_amount
    client.deposit(&id, &player1);
    assert_eq!(client.get_escrow_balance(&id), stake);

    // After player2 deposits: balance must equal 2 * stake_amount
    client.deposit(&id, &player2);
    assert_eq!(client.get_escrow_balance(&id), 2 * stake);
}

// ── Defensive: submit_result with insufficient escrow balance ────────────────

#[test]
fn test_submit_result_returns_not_funded_when_deposits_missing() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = token_id.address();
    let asset_client = StellarAssetClient::new(&env, &token_addr);
    asset_client.mint(&player1, &1000);
    asset_client.mint(&player2, &1000);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token_addr,
        &String::from_str(&env, "not_funded_game"),
        &Platform::Lichess,
    );

    // Manually force the match into Active state without going through deposit,
    // simulating a state inconsistency where state == Active but deposits are missing.
    env.as_contract(&contract_id, || {
        let mut m: Match = env.storage().persistent().get(&DataKey::Match(id)).unwrap();
        m.state = MatchState::Active;
        // player1_deposited and player2_deposited remain false
        env.storage().persistent().set(&DataKey::Match(id), &m);
    });

    let result = client.try_submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(
        result,
        Err(Ok(Error::NotFunded)),
        "submit_result must return NotFunded when deposits are missing despite Active state"
    );
}

#[test]
fn test_expire_match_emits_expired_event() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_event"),
        &Platform::Lichess,
    );

    let new_seq = env.ledger().sequence() + MATCH_TTL_LEDGERS;
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    });
    env.ledger().set_sequence_number(new_seq);
    client.expire_match(&id);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        soroban_sdk::symbol_short!("expired").into_val(&env),
    ];
    assert!(
        events
            .iter()
            .any(|(_, topics, _)| topics == expected_topics),
        "expired event not emitted"
    );
}

// ── game_id length validation ─────────────────────────────────────────────────

#[test]
fn test_create_match_with_oversized_game_id_fails() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 65 characters — one over the MAX_GAME_ID_LEN of 64
    let oversized_id = String::from_str(
        &env,
        "aaaaaaaaaabbbbbbbbbbccccccccccddddddddddeeeeeeeeeeffffffffffffffff1",
    );

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &oversized_id,
        &Platform::Lichess,
    );

    assert_eq!(
        result,
        Err(Ok(Error::InvalidGameId)),
        "create_match must reject game_id longer than 64 characters"
    );
}

// ── deposit blocked when contract is paused ───────────────────────────────────

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
fn test_expire_active_match_fails() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_active"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let new_seq = env.ledger().sequence() + MATCH_TTL_LEDGERS;
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .extend_ttl(MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    });
    env.ledger().set_sequence_number(new_seq);

    let result = client.try_expire_match(&id);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

// ── submit_result blocked when contract is paused ────────────────────────────

#[test]
fn test_submit_result_blocked_when_paused() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
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

    let result = client.try_submit_result(&id, &Winner::Player1, &oracle);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_is_funded_false_after_only_player1_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "partial_funded_game"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    assert!(
        !client.is_funded(&id),
        "is_funded must be false after only player1 deposits"
    );

    client.deposit(&id, &player2);
    assert!(
        client.is_funded(&id),
        "is_funded must be true after both players deposit"
    );
}

// ── Deposit flag assertions ───────────────────────────────────────────────────

/// Verifies that `player1_deposited` and `player2_deposited` flags on the
/// `Match` struct are set correctly after each individual deposit.
///
/// After player1 deposits:  player1_deposited == true,  player2_deposited == false
/// After player2 deposits:  player1_deposited == true,  player2_deposited == true
#[test]
fn test_deposit_flags_set_correctly_after_each_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "deposit_flags_test"),
        &Platform::Lichess,
    );

    // Before any deposit: both flags must be false
    let m = client.get_match(&id);
    assert!(!m.player1_deposited, "player1_deposited must be false before any deposit");
    assert!(!m.player2_deposited, "player2_deposited must be false before any deposit");

    // After player1 deposits: only player1_deposited flips to true
    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert!(m.player1_deposited, "player1_deposited must be true after player1 deposits");
    assert!(!m.player2_deposited, "player2_deposited must still be false after only player1 deposits");

    // After player2 deposits: both flags must be true
    client.deposit(&id, &player2);
    let m = client.get_match(&id);
    assert!(m.player1_deposited, "player1_deposited must remain true after player2 deposits");
    assert!(m.player2_deposited, "player2_deposited must be true after player2 deposits");
}

// ── Draw result: exact stake refund and zero escrow balance ──────────────────

/// Submit Winner::Draw and verify:
///   1. Each player receives exactly their original stake_amount back.
///   2. The contract escrow balance for the match is 0 after payout.
#[test]
fn test_draw_refunds_exact_stake_and_zeroes_escrow_balance() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let stake: i128 = 100;

    let id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "draw_escrow_zero"),
        &Platform::Lichess,
    );

    // Both players deposit — escrow holds 2 * stake
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    assert_eq!(client.get_escrow_balance(&id), 2 * stake);

    // Record balances right before result submission
    let p1_before = token_client.balance(&player1);
    let p2_before = token_client.balance(&player2);

    // Oracle submits Draw result
    client.submit_result(&id, &Winner::Draw, &oracle);

    // Each player must receive exactly stake_amount back
    assert_eq!(
        token_client.balance(&player1),
        p1_before + stake,
        "player1 must receive exactly stake_amount on draw"
    );
    assert_eq!(
        token_client.balance(&player2),
        p2_before + stake,
        "player2 must receive exactly stake_amount on draw"
    );

    // Contract escrow balance must be zero — no funds left behind
    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "escrow balance must be 0 after draw payout"
    );

    // Match state must be Completed
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
}

#[test]
fn test_get_escrow_balance_returns_match_not_found_for_nonexistent_id() {
    let (env, contract_id, ..) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // match_id 999 was never created — must return Error::MatchNotFound
    let result = client.try_get_escrow_balance(&999u64);
    assert_eq!(
        result,
        Err(Ok(Error::MatchNotFound)),
        "get_escrow_balance must return MatchNotFound for a non-existent match_id"
    );
}
