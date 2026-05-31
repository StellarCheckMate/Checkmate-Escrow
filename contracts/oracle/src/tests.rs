extern crate std;

use super::*;
use escrow::types::{MatchState, Platform, Winner as EscrowWinner};
use escrow::{EscrowContract, EscrowContractClient};
use soroban_sdk::{
    testutils::storage::{Instance as _, Persistent as _},
    testutils::{Address as _, Events as _},
    token::StellarAssetClient,
    Address, Env, IntoVal, String, Symbol,
};

fn setup() -> (Env, Address, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle_admin = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = token_id.address();
    let asset_client = StellarAssetClient::new(&env, &token_addr);
    asset_client.mint(&player1, &1000);
    asset_client.mint(&player2, &1000);

    let escrow_id = env.register_contract(None, EscrowContract);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    escrow_client.initialize(&oracle_admin, &admin);
    escrow_client.create_match(
        &player1,
        &player2,
        &100,
        &token_addr,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
    );
    escrow_client.deposit(&0u64, &player1);
    escrow_client.deposit(&0u64, &player2);

    let oracle_id = env.register_contract(None, OracleContract);
    let oracle_client = OracleContractClient::new(&env, &oracle_id);
    oracle_client.initialize(&oracle_admin);

    (
        env,
        oracle_id,
        escrow_id,
        oracle_admin,
        player1,
        player2,
        token_addr,
    )
}

#[test]
fn test_initialize_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("init").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "oracle initialized event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_admin: Address = soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_admin, admin);
}

#[test]
fn test_duplicate_initialize_returns_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    client.initialize(&admin1);
    let result = client.try_initialize(&admin2);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}
}

// ── has_result (public, unauthenticated) ─────────────────────────────────

#[test]
fn test_has_result_returns_false_for_match_id_0_on_fresh_contract() {
    let (env, contract_id, _escrow_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    assert!(!client.has_result(&0u64));
}

#[test]
fn test_has_result_is_public_and_unauthenticated() {
    let (env, contract_id, _escrow_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    assert!(!client.has_result(&0u64));
    assert!(!client.has_result(&999u64));

    client.submit_result(
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(client.has_result(&0u64));
    assert!(!client.has_result(&999u64));
}

// ── has_result_admin (admin-gated) ────────────────────────────────────────

#[test]
fn test_has_result_admin_returns_false_before_submission() {
    let (env, contract_id, _escrow_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    assert!(!client.has_result_admin(&0u64));
    assert!(!client.has_result_admin(&999u64));
}

#[test]
fn test_has_result_admin_returns_true_after_submission() {
    let (env, contract_id, _escrow_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(client.has_result_admin(&0u64));
}

#[test]
#[should_panic]
fn test_has_result_admin_rejects_non_admin() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.has_result_admin(&0u64);
}

#[test]
fn test_submit_and_get_result() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(client.has_result(&0u64));
    let entry = client.get_result(&0u64);
    assert_eq!(entry.result, Winner::Player1);
    assert_eq!(entry.platform, Platform::Lichess);
}

#[test]
fn test_submit_result_stores_submitted_ledger() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let ledger_before = env.ledger().sequence();
    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let entry = client.get_result(&0u64);
    assert!(
        entry.submitted_ledger >= ledger_before,
        "submitted_ledger must be >= ledger at call time"
    );
}

#[test]
fn test_submit_result_stores_submitter() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let entry = client.get_result(&0u64);
    assert_eq!(entry.submitter, oracle_admin);
}

#[test]
fn test_submit_result_emits_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("result").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "oracle result event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_result): (u64, Winner) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, 0u64);
    assert_eq!(ev_result, Winner::Player1);
}

#[test]
fn test_submit_draw_result_emits_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Draw,
    );

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("result").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "oracle result event not emitted for Draw"
    );

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_result): (u64, Winner) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, 0u64);
    assert_eq!(ev_result, Winner::Draw);
}

#[test]
#[should_panic]
fn test_duplicate_submit_fails() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Draw,
    );
    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Draw,
    );
}

#[test]
fn test_duplicate_submit_returns_already_submitted() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Draw,
    );
    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Draw,
    );
    assert_eq!(result, Err(Ok(Error::AlreadySubmitted)));
}

#[test]
fn test_double_initialize_returns_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    let result = client.try_initialize(&admin);
    assert_eq!(result, Err(Ok(Error::AlreadyInitialized)));
}

#[test]
fn test_submit_result_on_uninitialized_contract_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "game_abc"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_is_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    assert!(!client.is_initialized());
    client.initialize(&admin);
    assert!(client.is_initialized());
}

#[test]
fn test_ttl_extended_on_submit_result() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Result(0u64))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_get_result_not_found() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.get_result(&9999u64);
}

#[test]
fn test_pause_on_uninitialized_contract_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_pause();
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_pause_admin_only() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_unpause_admin_only() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();
    client.unpause();

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));
}

#[test]
fn test_submit_result_blocked_when_paused() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));

    assert!(!client.has_result(&0u64));
}

#[test]
fn test_submit_result_works_after_unpause() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));

    client.unpause();

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));
    let entry = client.get_result(&0u64);
    assert_eq!(entry.result, Winner::Player1);
}

#[test]
fn test_pause_unpause_state_transitions() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));

    client.pause();

    let result = client.try_submit_result(
        &1u64,
        &String::from_str(&env, "def456"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));

    client.unpause();

    client.submit_result(
        &1u64,
        &String::from_str(&env, "def456"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    assert!(client.has_result(&1u64));

    client.pause();
    let result = client.try_submit_result(
        &2u64,
        &String::from_str(&env, "ghi789"),
        &Platform::Lichess,
        &Winner::Draw,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_get_result_extends_ttl() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let entry = client.get_result(&0u64);
    assert_eq!(entry.result, Winner::Player1);

    let ttl = env.as_contract(&contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::Result(0u64))
    });
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_pause_twice_is_idempotent() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();
    client.pause();

    let is_paused: bool = env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    });
    assert!(is_paused);
}

#[test]
fn test_unpause_emits_unpaused_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();
    client.unpause();

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("unpaused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "unpaused event not emitted");
}

#[test]
fn test_pause_emits_paused_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("paused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "paused event not emitted");
}

#[test]
fn test_oracle_to_escrow_full_payout_flow() {
    let (env, oracle_id, escrow_id, _oracle_admin, player1, _player2, token_addr) = setup();
    let oracle_client = OracleContractClient::new(&env, &oracle_id);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    let token_client = soroban_sdk::token::Client::new(&env, &token_addr);

    oracle_client.submit_result(
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(oracle_client.has_result(&0u64));

    escrow_client.submit_result(&0u64, &EscrowWinner::Player1);

    let m = escrow_client.get_match(&0u64);
    assert_eq!(m.state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
}

#[test]
fn test_delete_result_removes_from_storage() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "chess_game_42"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));

    client.delete_result(&0u64);
    assert!(!client.has_result(&0u64));
}

#[test]
fn test_delete_result_not_found_errors() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_delete_result(&999u64);
    assert_eq!(result, Err(Ok(Error::ResultNotFound)));
}

#[test]
fn test_delete_result_blocked_when_paused() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "chess_game_99"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    assert!(client.has_result(&0u64));

    client.pause();

    let result = client.try_delete_result(&0u64);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));

    assert!(client.has_result(&0u64));
}

#[test]
fn test_delete_result_emits_deletion_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "chess_game_42"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));

    client.delete_result(&0u64);

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("deleted").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "deletion event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, 0u64);
}

#[test]
#[should_panic]
fn test_delete_result_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.delete_result(&0u64);
}

#[test]
fn test_instance_ttl_extended_on_submit_result() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "ttl_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
}

#[test]
fn test_transfer_admin_old_rejected_new_accepted() {
    let (env, contract_id, _escrow_id, old_admin, _player1, _player2, _token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    client.update_admin(&new_admin);

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &old_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (
                0u64,
                String::from_str(&env, "test_game"),
                Platform::Lichess,
                Winner::Player1,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(
        result.is_err(),
        "old admin must be rejected after transfer_admin"
    );

    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &new_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "submit_result",
            args: (
                0u64,
                String::from_str(&env, "test_game"),
                Platform::Lichess,
                Winner::Player1,
            )
                .into_val(&env),
            sub_invokes: &[],
        },
    }]);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(
        client.has_result(&0u64),
        "new admin must be able to submit results after transfer"
    );
    let entry = client.get_result(&0u64);
    assert_eq!(entry.result, Winner::Player1);
}

#[test]
fn test_update_admin_emits_rotation_event() {
    let (env, contract_id, _escrow_id, old_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.update_admin(&new_admin);

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("admin_rot").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "admin_rot event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_old, ev_new): (Address, Address) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_old, old_admin);
    assert_eq!(ev_new, new_admin);
}

#[test]
fn test_oracle_escrow_integration_submit_result_with_oracle_record() {
    let (env, oracle_id, escrow_id, oracle_admin, player1, player2, token_addr) = setup();
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);
    let oracle_client = OracleContractClient::new(&env, &oracle_id);

    // Create and fund a match
    let match_id = escrow_client.create_match(
        &player1,
        &player2,
        &100,
        &token_addr,
        &String::from_str(&env, "integration_game"),
        &Platform::Lichess,
    );
    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Oracle submits result
    oracle_client.submit_result(
        &match_id,
        &String::from_str(&env, "integration_game"),
        &Winner::Player1,
    );

    // Verify oracle stored the result
    assert!(oracle_client.has_result(&match_id));
    let result = oracle_client.get_result(&match_id);
    assert_eq!(result.result, Winner::Player1);

    // Verify escrow match is still active (oracle doesn't trigger payout)
    let m = escrow_client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Active);
}


// ── Test #599: delete_result emits event ────────────────────────────────

#[test]
fn test_delete_result_emits_deletion_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "chess_game_42"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    client.delete_result(&0u64);

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("deleted").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "deletion event not emitted");

    let (_, _, data) = matched.unwrap();
    let ev_id: u64 = soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, 0u64);
}


// ── Test #600: delete_result leaves has_result false ──────────────────────

#[test]
fn test_delete_result_leaves_has_result_false() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "chess_game_42"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));

    client.delete_result(&0u64);
    assert!(!client.has_result(&0u64));
}


// ── Test #598: unpause emits event ───────────────────────────────────────

#[test]
fn test_unpause_emits_unpaused_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();
    client.unpause();

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("unpaused").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "unpaused event not emitted");
}


// ── Test #597: update_admin emits event ──────────────────────────────────

#[test]
fn test_update_admin_emits_rotation_event() {
    let (env, contract_id, _escrow_id, old_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);
    client.update_admin(&new_admin);

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        symbol_short!("admin_rot").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "admin_rot event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_old, ev_new): (Address, Address) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_old, old_admin);
    assert_eq!(ev_new, new_admin);
}
