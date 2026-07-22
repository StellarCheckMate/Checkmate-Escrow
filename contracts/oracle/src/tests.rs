extern crate std;

use super::*;
use escrow::types::{MatchState, Platform as EscrowPlatform, Winner as EscrowWinner};
use escrow::{EscrowContract, EscrowContractClient};
use soroban_sdk::{
    testutils::storage::{Instance as _, Persistent as _},
    testutils::{Address as _, Events as _, Ledger as _},
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
        &EscrowPlatform::Lichess,
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
fn test_register_oracle_with_stake_transfers_tokens_and_allows_submission() {
    let (env, contract_id, .., oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let asset_client = StellarAssetClient::new(&env, &token_addr);
    let balance_client = soroban_sdk::token::Client::new(&env, &token_addr);

    asset_client.mint(&oracle_admin, &200);
    client.register_oracle_with_stake(&oracle_admin, &200i128, &token_addr);

    assert_eq!(balance_client.balance(&contract_id), 200);
    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
}

#[test]
fn test_slash_oracle_reduces_stake_and_transfers_tokens() {
    let (env, contract_id, .., oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let asset_client = StellarAssetClient::new(&env, &token_addr);
    let balance_client = soroban_sdk::token::Client::new(&env, &token_addr);

    asset_client.mint(&oracle_admin, &300);
    client.register_oracle_with_stake(&oracle_admin, &300i128, &token_addr);

    let admin_balance_before = balance_client.balance(&oracle_admin);
    client.slash_oracle(&oracle_admin, &75i128);

    assert_eq!(balance_client.balance(&contract_id), 225);
    assert_eq!(balance_client.balance(&oracle_admin), admin_balance_before + 75);
}

#[test]
fn test_submit_result_rejects_registered_oracle_without_sufficient_stake() {
    let (env, contract_id, .., oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let asset_client = StellarAssetClient::new(&env, &token_addr);

    asset_client.mint(&oracle_admin, &100);
    client.register_oracle_with_stake(&oracle_admin, &100i128, &token_addr);
    client.slash_oracle(&oracle_admin, &100i128);

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::InsufficientStake)));
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
fn test_oracle_submit_result_emits_event() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let events = env.events().all();
    // Documented schema: topic = ["oracle", "result"], payload = (match_id: u64, result: Winner)
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
fn test_submit_result_duplicate_game_id_rejected() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let result = client.try_submit_result(
        &0u64,
        &String::from_str(&env, "abc123"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    assert_eq!(result, Err(Ok(Error::AlreadySubmitted)));
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
fn test_oracle_submit_result_while_paused() {
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
fn test_oracle_delete_result_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    // No admin set — delete_result must return Unauthorized.
    let result = client.try_delete_result(&0u64);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
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
fn test_oracle_admin_rotation() {
    let (env, contract_id, _escrow_id, old_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    // Rotate admin
    client.update_admin(&new_admin);

    // get_admin reflects the new admin
    assert_eq!(client.get_admin(), new_admin);

    // Old admin can no longer call an admin-gated function (pause)
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &old_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    assert!(
        client.try_pause().is_err(),
        "old admin must be rejected after rotation"
    );

    // New admin can still call admin-gated functions
    env.mock_auths(&[soroban_sdk::testutils::MockAuth {
        address: &new_admin,
        invoke: &soroban_sdk::testutils::MockAuthInvoke {
            contract: &contract_id,
            fn_name: "pause",
            args: ().into_val(&env),
            sub_invokes: &[],
        },
    }]);
    client.pause();
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
        &EscrowPlatform::Lichess,
    );
    escrow_client.deposit(&match_id, &player1);
    escrow_client.deposit(&match_id, &player2);

    // Oracle submits result
    oracle_client.submit_result(
        &match_id,
        &String::from_str(&env, "integration_game"),
        &Platform::Lichess,
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

// ── submit_batch_results ─────────────────────────────────────────────────

fn make_batch_entry(
    env: &Env,
    match_id: u64,
    game_id: &str,
) -> types::BatchResultEntry {
    types::BatchResultEntry {
        match_id,
        game_id: String::from_str(env, game_id),
        platform: Platform::Lichess,
        result: Winner::Player1,
    }
}

#[test]
fn test_batch_submit_single_entry() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![&env, make_batch_entry(&env, 0, "game_a")];
    client.submit_batch_results(&entries);

    assert!(client.has_result(&0u64));
    let entry = client.get_result(&0u64);
    assert_eq!(entry.result, Winner::Player1);
}

#[test]
fn test_batch_submit_multiple_entries() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![
        &env,
        make_batch_entry(&env, 0, "game_0"),
        types::BatchResultEntry {
            match_id: 1,
            game_id: String::from_str(&env, "game_1"),
            platform: Platform::Lichess,
            result: Winner::Player2,
        },
        types::BatchResultEntry {
            match_id: 2,
            game_id: String::from_str(&env, "game_2"),
            platform: Platform::ChessDotCom,
            result: Winner::Draw,
        },
    ];
    client.submit_batch_results(&entries);

    assert!(client.has_result(&0u64));
    assert!(client.has_result(&1u64));
    assert!(client.has_result(&2u64));
    assert_eq!(client.get_result(&1u64).result, Winner::Player2);
    assert_eq!(client.get_result(&2u64).result, Winner::Draw);
}

#[test]
fn test_batch_submit_max_size_100() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let mut entries: soroban_sdk::Vec<types::BatchResultEntry> = soroban_sdk::vec![&env];
    for i in 0u64..100 {
        entries.push_back(types::BatchResultEntry {
            match_id: i,
            game_id: String::from_str(&env, "g"),
            platform: Platform::Lichess,
            result: Winner::Player1,
        });
    }
    client.submit_batch_results(&entries);

    assert!(client.has_result(&0u64));
    assert!(client.has_result(&99u64));
}

#[test]
fn test_batch_submit_over_limit_returns_batch_too_large() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let mut entries: soroban_sdk::Vec<types::BatchResultEntry> = soroban_sdk::vec![&env];
    for i in 0u64..101 {
        entries.push_back(types::BatchResultEntry {
            match_id: i,
            game_id: String::from_str(&env, "g"),
            platform: Platform::Lichess,
            result: Winner::Player1,
        });
    }
    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

#[test]
fn test_batch_submit_intra_batch_duplicate_returns_error() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![
        &env,
        make_batch_entry(&env, 0, "game_a"),
        make_batch_entry(&env, 0, "game_b"), // duplicate match_id
    ];
    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::BatchDuplicateEntry)));
}

#[test]
fn test_batch_duplicate_does_not_write_partial_state() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![
        &env,
        make_batch_entry(&env, 0, "game_a"),
        make_batch_entry(&env, 0, "game_b"), // triggers duplicate error
    ];
    let _ = client.try_submit_batch_results(&entries);

    // Nothing should have been written (validate-first, all-or-nothing).
    assert!(!client.has_result(&0u64));
}

#[test]
fn test_batch_already_submitted_returns_error() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "game_existing"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let entries = soroban_sdk::vec![&env, make_batch_entry(&env, 0, "game_a")];
    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::AlreadySubmitted)));
}

#[test]
fn test_batch_already_submitted_does_not_overwrite() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "game_existing"),
        &Platform::Lichess,
        &Winner::Draw,
    );

    let entries = soroban_sdk::vec![
        &env,
        make_batch_entry(&env, 0, "game_override"), // match_id 0 already has a result
    ];
    let _ = client.try_submit_batch_results(&entries);

    // Original result must be untouched.
    assert_eq!(client.get_result(&0u64).result, Winner::Draw);
}

#[test]
fn test_batch_invalid_game_id_returns_error() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![
        &env,
        types::BatchResultEntry {
            match_id: 0,
            game_id: String::from_str(&env, ""), // empty
            platform: Platform::Lichess,
            result: Winner::Player1,
        },
    ];
    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

#[test]
fn test_batch_paused_returns_contract_paused() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();

    let entries = soroban_sdk::vec![&env, make_batch_entry(&env, 0, "game_a")];
    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_batch_paused_writes_nothing() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.pause();

    let entries = soroban_sdk::vec![&env, make_batch_entry(&env, 0, "game_a")];
    let _ = client.try_submit_batch_results(&entries);

    assert!(!client.has_result(&0u64));
}

#[test]
fn test_batch_uninitialized_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![&env, make_batch_entry(&env, 0, "game_a")];
    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_batch_emits_individual_and_summary_events() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![
        &env,
        make_batch_entry(&env, 0, "game_0"),
        make_batch_entry(&env, 1, "game_1"),
    ];
    client.submit_batch_results(&entries);

    let events = env.events().all();

    let result_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("result").into_val(&env),
    ];
    let result_count = events
        .iter()
        .filter(|(_, topics, _)| *topics == result_topics)
        .count();
    assert_eq!(result_count, 2, "expected 2 individual result events");

    let batch_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("batch").into_val(&env),
    ];
    let batch_event = events
        .iter()
        .find(|(_, topics, _)| *topics == batch_topics);
    assert!(batch_event.is_some(), "batch summary event not emitted");

    let (_, _, data) = batch_event.unwrap();
    let count: u32 = soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(count, 2u32);
}

#[test]
fn test_batch_ttl_set_on_each_entry() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let entries = soroban_sdk::vec![
        &env,
        make_batch_entry(&env, 0, "game_0"),
        make_batch_entry(&env, 5, "game_5"),
    ];
    client.submit_batch_results(&entries);

    for match_id in [0u64, 5u64] {
        let ttl = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get_ttl(&DataKey::Result(match_id))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }
}

// ── Rate limiting ─────────────────────────────────────────────────────────

#[test]
fn test_default_rate_limits_are_100_hourly_1000_daily() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let limits = client.get_oracle_rate_limits(&oracle_admin);
    assert_eq!(limits.hourly_limit, 100);
    assert_eq!(limits.daily_limit, 1000);

    let status = client.get_oracle_rate_limit_status(&oracle_admin);
    assert_eq!(status.hourly_used, 0);
    assert_eq!(status.hourly_remaining, 100);
    assert_eq!(status.daily_used, 0);
    assert_eq!(status.daily_remaining, 1000);
}

#[test]
fn test_hourly_rate_limit_blocks_101st_submission_in_same_hour() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    for match_id in 0u64..100 {
        client.submit_result(
            &match_id,
            &String::from_str(&env, "g"),
            &Platform::Lichess,
            &Winner::Player1,
        );
    }

    let result = client.try_submit_result(
        &100u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::RateLimitExceeded)));
    assert!(!client.has_result(&100u64));

    let status = client.get_oracle_rate_limit_status(&oracle_admin);
    assert_eq!(status.hourly_used, 100);
    assert_eq!(status.hourly_remaining, 0);
}

#[test]
fn test_batch_submission_counts_full_batch_against_rate_limit() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let mut entries: soroban_sdk::Vec<types::BatchResultEntry> = soroban_sdk::vec![&env];
    for i in 0u64..100 {
        entries.push_back(make_batch_entry(&env, i, "g"));
    }
    client.submit_batch_results(&entries); // exactly exhausts the hourly limit

    let result = client.try_submit_result(
        &200u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::RateLimitExceeded)));
}

#[test]
fn test_batch_rejected_when_it_would_exceed_hourly_limit_writes_nothing() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let mut entries: soroban_sdk::Vec<types::BatchResultEntry> = soroban_sdk::vec![&env];
    for i in 1u64..101 {
        // Combined with the single submission above, this batch would push
        // the oracle to 101 submissions this hour — one over the default limit.
        entries.push_back(make_batch_entry(&env, i, "g"));
    }

    let result = client.try_submit_batch_results(&entries);
    assert_eq!(result, Err(Ok(Error::RateLimitExceeded)));

    // The rate-limit check runs before any batch entries are written.
    assert!(!client.has_result(&1u64));
    assert!(!client.has_result(&100u64));
}

#[test]
fn test_rejected_submission_does_not_consume_quota() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    client.set_oracle_rate_limits(&oracle_admin, &1, &10);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let blocked = client.try_submit_result(
        &1u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(blocked, Err(Ok(Error::RateLimitExceeded)));

    // The rejected attempt above must not have consumed any quota.
    let status = client.get_oracle_rate_limit_status(&oracle_admin);
    assert_eq!(status.hourly_used, 1);
    assert_eq!(status.daily_used, 1);
}

#[test]
fn test_hourly_window_resets_after_window_elapses() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    client.set_oracle_rate_limits(&oracle_admin, &1, &1000);

    client.submit_result(
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    let blocked = client.try_submit_result(
        &1u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(blocked, Err(Ok(Error::RateLimitExceeded)));

    // Advance two full hourly windows so the sliding window fully clears.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 2 * crate::HOURLY_WINDOW_SECS + 1);

    client.submit_result(
        &1u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&1u64));

    let status = client.get_oracle_rate_limit_status(&oracle_admin);
    assert_eq!(status.hourly_used, 1);
}

#[test]
fn test_daily_limit_persists_across_hourly_window_reset() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    client.set_oracle_rate_limits(&oracle_admin, &5, &8);

    let mut match_id = 0u64;
    for _ in 0..5 {
        client.submit_result(
            &match_id,
            &String::from_str(&env, "g"),
            &Platform::Lichess,
            &Winner::Player1,
        );
        match_id += 1;
    }
    let blocked_hourly = client.try_submit_result(
        &match_id,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(blocked_hourly, Err(Ok(Error::RateLimitExceeded)));

    // Roll into the next hourly window — hourly quota recovers, daily does not.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 2 * crate::HOURLY_WINDOW_SECS + 1);

    for _ in 0..3 {
        client.submit_result(
            &match_id,
            &String::from_str(&env, "g"),
            &Platform::Lichess,
            &Winner::Player1,
        );
        match_id += 1;
    }

    let blocked_daily = client.try_submit_result(
        &match_id,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(blocked_daily, Err(Ok(Error::RateLimitExceeded)));

    let status = client.get_oracle_rate_limit_status(&oracle_admin);
    assert_eq!(status.hourly_used, 3);
    assert_eq!(status.daily_used, 8);
    assert_eq!(status.daily_remaining, 0);
}

#[test]
fn test_set_oracle_rate_limits_rejects_hourly_greater_than_daily() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_set_oracle_rate_limits(&oracle_admin, &200, &100);
    assert_eq!(result, Err(Ok(Error::InvalidRateLimit)));
}

#[test]
fn test_set_oracle_rate_limits_zero_falls_back_to_defaults() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.set_oracle_rate_limits(&oracle_admin, &0, &0);

    let limits = client.get_oracle_rate_limits(&oracle_admin);
    assert_eq!(limits.hourly_limit, 100);
    assert_eq!(limits.daily_limit, 1000);
}

#[test]
fn test_set_oracle_rate_limits_emits_event() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.set_oracle_rate_limits(&oracle_admin, &50, &500);

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("ratelim").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "ratelim event not emitted");

    let (_, _, data) = matched.unwrap();
    let (oracle, hourly, daily): (Address, u32, u32) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(oracle, oracle_admin);
    assert_eq!(hourly, 50);
    assert_eq!(daily, 500);
}

#[test]
#[should_panic]
fn test_set_oracle_rate_limits_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.set_oracle_rate_limits(&admin, &50, &500);
}

#[test]
fn test_set_oracle_rate_limits_on_uninitialized_contract_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    let oracle = Address::generate(&env);

    let result = client.try_set_oracle_rate_limits(&oracle, &50, &500);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_alert_emitted_at_80_percent_hourly_usage() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    client.set_oracle_rate_limits(&oracle_admin, &10, &1000);

    for match_id in 0u64..8 {
        // 8 / 10 == 80% of the hourly limit.
        client.submit_result(
            &match_id,
            &String::from_str(&env, "g"),
            &Platform::Lichess,
            &Winner::Player1,
        );
    }

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("alert").into_val(&env),
    ];
    let alert_count = events
        .iter()
        .filter(|(_, topics, _)| *topics == expected_topics)
        .count();
    assert!(
        alert_count >= 1,
        "expected a suspicious-pattern alert once usage reached 80% of the hourly limit"
    );
}

#[test]
fn test_no_alert_below_80_percent_usage() {
    let (env, contract_id, _escrow_id, oracle_admin, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    client.set_oracle_rate_limits(&oracle_admin, &10, &1000);

    for match_id in 0u64..5 {
        // 5 / 10 == 50% of the hourly limit — below the alert threshold.
        client.submit_result(
            &match_id,
            &String::from_str(&env, "g"),
            &Platform::Lichess,
            &Winner::Player1,
        );
    }

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("alert").into_val(&env),
    ];
    let alert_count = events
        .iter()
        .filter(|(_, topics, _)| *topics == expected_topics)
        .count();
    assert_eq!(alert_count, 0);
}

#[test]
fn test_high_volume_burst_is_throttled_then_recovers_next_hour() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    env.budget().reset_unlimited();

    // Simulate a burst of 150 submissions within a single hour — only the
    // first 100 (the default hourly limit) should be accepted.
    let mut accepted = 0u32;
    let mut rejected = 0u32;
    for match_id in 0u64..150 {
        let result = client.try_submit_result(
            &match_id,
            &String::from_str(&env, "g"),
            &Platform::Lichess,
            &Winner::Player1,
        );
        match result {
            Ok(_) => accepted += 1,
            Err(e) => {
                assert_eq!(e, Ok(Error::RateLimitExceeded));
                rejected += 1;
            }
        }
    }
    assert_eq!(accepted, 100);
    assert_eq!(rejected, 50);

    // Once the next hourly window begins, the oracle can resume submitting.
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 2 * crate::HOURLY_WINDOW_SECS + 1);

    client.submit_result(
        &999u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&999u64));
}

#[test]
fn test_get_admin_returns_admin_after_initialize() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_get_admin_returns_unauthorized_when_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_get_admin();
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

// ── m-of-n oracle consensus ──────────────────────────────────────────────

/// Registers `n` freshly-generated oracle addresses, each staking `stake` of
/// `token_addr`, and returns them in registration order.
fn register_n_oracles(
    env: &Env,
    client: &OracleContractClient,
    token_addr: &Address,
    n: u32,
    stake: i128,
) -> std::vec::Vec<Address> {
    let asset_client = StellarAssetClient::new(env, token_addr);
    let mut oracles = std::vec::Vec::new();
    for _ in 0..n {
        let oracle = Address::generate(env);
        asset_client.mint(&oracle, &stake);
        client.register_oracle_with_stake(&oracle, &stake, token_addr);
        oracles.push(oracle);
    }
    oracles
}

#[test]
fn test_consensus_threshold_defaults_to_one() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    assert_eq!(client.get_consensus_threshold(), 1);
}

#[test]
fn test_set_consensus_threshold_updates_value() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.set_consensus_threshold(&3);
    assert_eq!(client.get_consensus_threshold(), 3);
}

#[test]
fn test_set_consensus_threshold_rejects_zero() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_set_consensus_threshold(&0);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
#[should_panic]
fn test_set_consensus_threshold_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.set_consensus_threshold(&2);
}

#[test]
fn test_get_registered_oracle_count() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    assert_eq!(client.get_registered_oracle_count(), 0);
    register_n_oracles(&env, &client, &token_addr, 5, 100);
    assert_eq!(client.get_registered_oracle_count(), 5);
}

/// Backward compatibility: the original admin-gated `submit_result` path
/// (n=1 via admin auth) and the new consensus `submit_oracle_result` path at
/// threshold=1 (n=1 via a registered oracle's own auth) both work, on
/// different matches, on the same contract instance.
#[test]
fn test_both_legacy_admin_mode_and_new_consensus_mode_work_side_by_side() {
    let (env, contract_id, _escrow_id, oracle_admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    // Legacy path: admin-gated submit_result, no consensus machinery involved.
    client.submit_result(
        &0u64,
        &String::from_str(&env, "legacy_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));

    // New path: a registered oracle submits via submit_oracle_result; default
    // threshold of 1 finalizes on the first vote (degenerate n=1).
    let oracles = register_n_oracles(&env, &client, &token_addr, 1, 500);
    client.submit_oracle_result(
        &oracles[0],
        &1u64,
        &String::from_str(&env, "consensus_game"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    assert!(client.has_result(&1u64));
    assert_eq!(client.get_result(&1u64).result, Winner::Player2);

    // The admin can still submit legacy-path results for other matches too.
    client.submit_result(
        &2u64,
        &String::from_str(&env, "legacy_game_2"),
        &Platform::Lichess,
        &Winner::Draw,
    );
    assert!(client.has_result(&2u64));
    let _ = oracle_admin; // sanity: admin identity unused beyond setup wiring
}

#[test]
fn test_submit_oracle_result_not_registered_rejected() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let stranger = Address::generate(&env);

    let result = client.try_submit_oracle_result(
        &stranger,
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::NotRegisteredOracle)));
    assert!(!client.has_result(&0u64));
}

#[test]
fn test_submit_oracle_result_rejects_zero_stake() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let oracles = register_n_oracles(&env, &client, &token_addr, 1, 100);

    client.slash_oracle(&oracles[0], &100i128);

    let result = client.try_submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::InsufficientStake)));
}

#[test]
fn test_submit_oracle_result_empty_game_id_rejected() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let oracles = register_n_oracles(&env, &client, &token_addr, 1, 100);

    let result = client.try_submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, ""),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
}

#[test]
fn test_submit_oracle_result_blocked_when_paused() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let oracles = register_n_oracles(&env, &client, &token_addr, 1, 100);

    client.pause();

    let result = client.try_submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

/// threshold reached -> payout proceeds: once enough independent oracles
/// agree, the match finalizes (has_result flips true) and the same result is
/// accepted by the escrow contract's own (separately-configured) oracle to
/// actually execute payout, demonstrating the two systems compose.
#[test]
fn test_mofn_threshold_reached_finalizes_result_and_escrow_payout_proceeds() {
    let (env, contract_id, escrow_id, oracle_admin, _player1, _player2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let escrow_client = EscrowContractClient::new(&env, &escrow_id);

    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 3, 500);

    // First vote: not yet finalized.
    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(!client.has_result(&0u64));

    // Second matching vote crosses the threshold.
    client.submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "test_game"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));
    assert_eq!(client.get_result(&0u64).result, Winner::Player1);

    // The oracle contract's finalized result is purely an audit record; the
    // escrow contract trusts its own configured oracle address separately.
    // (Exact settlement amounts are covered by the escrow crate's own test
    // suite; here we only check that the finalized m-of-n result is accepted
    // downstream and drives the match to completion.)
    escrow_client.submit_result(&0u64, &EscrowWinner::Player1);
    let m = escrow_client.get_match(&0u64);
    assert_eq!(m.state, MatchState::Completed);
    let _ = oracle_admin;
}

/// threshold not reached -> match stays pending.
#[test]
fn test_mofn_threshold_not_reached_match_stays_pending() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.set_consensus_threshold(&3);
    let oracles = register_n_oracles(&env, &client, &token_addr, 5, 500);

    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    client.submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(!client.has_result(&0u64));
    let votes = client.get_match_votes(&0u64).unwrap();
    assert!(!votes.disputed);
    assert_eq!(votes.candidates.len(), 1);
    assert_eq!(votes.candidates.get(0).unwrap().submitters.len(), 2);
}

/// Conflicting submissions that still resolve to consensus: the losing
/// (minority) oracle is automatically slashed once the majority finalizes.
#[test]
fn test_mofn_conflicting_submissions_minority_auto_slashed_on_finalize() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let balance_client = soroban_sdk::token::Client::new(&env, &token_addr);

    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 3, 500);

    // Oracle 0 disagrees with the eventual majority.
    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    // Oracles 1 and 2 agree on Player1, reaching the threshold.
    client.submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    client.submit_oracle_result(
        &oracles[2],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(client.has_result(&0u64));
    assert_eq!(client.get_result(&0u64).result, Winner::Player1);

    // Minority oracle 0 (10% of its 500 stake) was auto-slashed.
    assert_eq!(balance_client.balance(&contract_id), 500 + 500 + 450);

    let result = client.try_submit_oracle_result(
        &oracles[0],
        &1u64,
        &String::from_str(&env, "g2"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(
        result.is_ok(),
        "minority oracle keeps its remaining stake and can still vote"
    );
}

/// A single malicious minority oracle cannot force an incorrect result: its
/// lone vote never crosses the threshold, and once the honest majority
/// agrees, the correct result finalizes instead.
#[test]
fn test_mofn_single_malicious_minority_cannot_force_incorrect_result() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 3, 500);

    // Malicious oracle submits a false result alone.
    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    assert!(
        !client.has_result(&0u64),
        "a single oracle's vote must never unilaterally finalize a match above threshold 1"
    );

    // Two honest oracles agree on the true result.
    client.submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    client.submit_oracle_result(
        &oracles[2],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(client.has_result(&0u64));
    assert_eq!(
        client.get_result(&0u64).result,
        Winner::Player1,
        "the honest majority's result must win, not the malicious minority's"
    );
}

#[test]
fn test_mofn_equivocation_slashes_full_stake_and_rejects_submission() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let balance_client = soroban_sdk::token::Client::new(&env, &token_addr);

    // threshold=2 so the first vote doesn't finalize, leaving room to equivocate.
    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 2, 500);

    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    // The call itself succeeds (Ok) — a contract call returning `Err` would
    // revert the slash performed inside it, so equivocation is signaled via
    // the `oracle/equivoc` event and the stake drop, not an error return.
    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player2, // conflicts with its own earlier vote
    );

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("equivoc").into_val(&env),
    ];
    assert!(
        events
            .iter()
            .any(|(_, topics, _)| topics == expected_topics),
        "equivoc event not emitted"
    );

    // The equivocating oracle's entire remaining stake (100% of 500) was slashed.
    assert_eq!(balance_client.balance(&contract_id), 500);

    // Full remaining stake was slashed.
    let further = client.try_submit_oracle_result(
        &oracles[0],
        &1u64,
        &String::from_str(&env, "g2"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(further, Err(Ok(Error::InsufficientStake)));

    // Match 0 still isn't finalized — the equivocating vote was rejected.
    assert!(!client.has_result(&0u64));
}

#[test]
fn test_mofn_duplicate_identical_vote_returns_already_submitted_no_slash() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let balance_client = soroban_sdk::token::Client::new(&env, &token_addr);

    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 2, 500);

    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    let result = client.try_submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1, // identical repeat, not equivocation
    );
    assert_eq!(result, Err(Ok(Error::AlreadySubmitted)));

    // No slashing occurred for a duplicate identical vote.
    assert_eq!(balance_client.balance(&contract_id), 1000);
}

/// conflicting submissions -> correct dispute/slash path: an irreconcilable
/// 3-way split (no candidate can reach threshold even with every remaining
/// oracle voting) marks the match disputed rather than hanging forever, and
/// the admin's resolution finalizes it while slashing every oracle that
/// disagreed with the resolution.
#[test]
fn test_mofn_deadlock_marks_disputed_and_admin_resolves() {
    let (env, contract_id, _escrow_id, admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);
    let balance_client = soroban_sdk::token::Client::new(&env, &token_addr);

    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 3, 500);

    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    client.submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player2,
    );
    // Third and final oracle breaks for a third distinct result: no
    // candidate can now possibly reach the threshold of 2.
    client.submit_oracle_result(
        &oracles[2],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Draw,
    );

    assert!(!client.has_result(&0u64));
    let votes = client.get_match_votes(&0u64).unwrap();
    assert!(
        votes.disputed,
        "an irreconcilable 3-way split must be flagged disputed"
    );

    // Admin resolves in favor of oracle 0's submission (Player1).
    client.resolve_disputed_match(
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    assert!(client.has_result(&0u64));
    assert_eq!(client.get_result(&0u64).result, Winner::Player1);

    // Oracles 1 and 2 disagreed with the resolution and were slashed 10%;
    // oracle 0 agreed and keeps its full stake.
    assert_eq!(balance_client.balance(&contract_id), 500 + 450 + 450);
    let _ = admin;
}

#[test]
fn test_mofn_resolve_disputed_match_requires_disputed_state() {
    let (env, contract_id, ..) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let result = client.try_resolve_disputed_match(
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::MatchNotDisputed)));
}

#[test]
#[should_panic]
fn test_mofn_resolve_disputed_match_requires_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, OracleContract);
    let client = OracleContractClient::new(&env, &contract_id);
    client.initialize(&admin);
    client.resolve_disputed_match(
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
}

#[test]
fn test_mofn_already_finalized_match_rejects_further_oracle_submissions() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let oracles = register_n_oracles(&env, &client, &token_addr, 2, 500);

    // Default threshold=1: first oracle's vote finalizes immediately.
    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert!(client.has_result(&0u64));

    let result = client.try_submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    assert_eq!(result, Err(Ok(Error::AlreadySubmitted)));
}

#[test]
fn test_mofn_finalized_event_reports_submitter_count_and_threshold() {
    let (env, contract_id, _escrow_id, _admin, _p1, _p2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    client.set_consensus_threshold(&2);
    let oracles = register_n_oracles(&env, &client, &token_addr, 2, 500);

    client.submit_oracle_result(
        &oracles[0],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );
    client.submit_oracle_result(
        &oracles[1],
        &0u64,
        &String::from_str(&env, "g"),
        &Platform::Lichess,
        &Winner::Player1,
    );

    let events = env.events().all();
    let expected_topics = soroban_sdk::vec![
        &env,
        Symbol::new(&env, "oracle").into_val(&env),
        symbol_short!("finalzd").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "finalzd event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_id, ev_count, ev_threshold): (u64, u32, u32) =
        soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_id, 0u64);
    assert_eq!(ev_count, 2);
    assert_eq!(ev_threshold, 2);
}
