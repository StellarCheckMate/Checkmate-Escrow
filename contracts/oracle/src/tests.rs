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


// ============================================================================
// SWAP, SET_RATE, GET_RATE TESTS
// ============================================================================

#[test]
fn test_set_rate_requires_admin_auth() {
    let (env, contract_id, .., oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    let other = Address::generate(&env);
    env.as_contract(&contract_id, || {
        let result = client.try_set_rate(&token_addr, &token2_addr, &10_000_000);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    });
}

#[test]
fn test_set_rate_rejects_zero_or_negative_rate() {
    let (env, contract_id, .., oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    // As admin, try to set rate = 0
    env.as_contract(&contract_id, || {
        let result = client.try_set_rate(&token_addr, &token2_addr, &0);
        assert_eq!(result, Err(Ok(Error::InvalidRateLimit)));
    });

    // As admin, try to set rate = -1
    env.as_contract(&contract_id, || {
        let result = client.try_set_rate(&token_addr, &token2_addr, &-1);
        assert_eq!(result, Err(Ok(Error::InvalidRateLimit)));
    });
}

#[test]
fn test_set_rate_admin_can_set() {
    let (env, contract_id, .., oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    let rate = 10_000_000i128; // 1:1 after scaling
    env.as_contract(&contract_id, || {
        let result = client.try_set_rate(&token_addr, &token2_addr, &rate);
        assert_eq!(result, Ok(Ok(())));
    });

    let retrieved = env.as_contract(&contract_id, || {
        client.get_rate(&token_addr, &token2_addr)
    });
    assert_eq!(retrieved, rate);
}

#[test]
fn test_get_rate_returns_error_when_rate_not_set() {
    let (env, contract_id, .., _oracle_admin, _, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token2_addr = token2.address();

    let result = env.as_contract(&contract_id, || {
        client.try_get_rate(&token_addr, &token2_addr)
    });
    assert_eq!(result, Err(Ok(Error::ResultNotFound)));
}

#[test]
fn test_swap_rejects_unauthenticated_caller() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    // Set a rate: 1 token_addr = 2 token2
    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &20_000_000);
    });

    // Mint some token2 into the contract so it has funds
    let token2_client = StellarAssetClient::new(&env, &token2_addr);
    token2_client.mint(&contract_id, &1000);

    // Unauthenticated swap attempt should fail
    env.mock_all_auths_allowing_non_root_auth();
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,     // caller
            &token_addr,  // token_in
            &token2_addr, // token_out
            &100,         // amount_in
            &0,           // min_amount_out
            &player1,     // recipient
        )
    });
    // Result will be auth failure during the transfer of token_in
    // (Soroban's require_auth will reject it)
    assert!(result.is_err());
}

#[test]
fn test_swap_rejects_zero_amount_in() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &10_000_000);
    });

    let token2_client = StellarAssetClient::new(&env, &token2_addr);
    token2_client.mint(&contract_id, &1000);

    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &0, // amount_in = 0
            &0,
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_swap_rejects_negative_amount_in() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &10_000_000);
    });

    let token2_client = StellarAssetClient::new(&env, &token2_addr);
    token2_client.mint(&contract_id, &1000);

    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &-100, // amount_in = -100
            &0,
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_swap_rejects_missing_rate() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    // No rate set between token_addr and token2_addr

    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &0,
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::ResultNotFound)));
}

#[test]
fn test_swap_rejects_slippage_exceeded_forward_rate() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    // Set rate: 1 token_addr = 2 token2 (rate = 2 * 1e7)
    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &(2 * 10_000_000));
    });

    let token2_client = StellarAssetClient::new(&env, &token2_addr);
    token2_client.mint(&contract_id, &1000);

    // Try to swap 100 token_addr for min 300 token2 (but only get 200)
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &300, // min_amount_out too high
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::SlippageExceeded)));
}

#[test]
fn test_swap_rejects_slippage_exceeded_inverse_rate() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    // Set rate inversely: 2 token2 = 1 token_addr (rate = 0.5 * 1e7 = 5_000_000)
    env.as_contract(&contract_id, || {
        client.set_rate(&token2_addr, &token_addr, &(5_000_000));
    });

    let token2_client = StellarAssetClient::new(&env, &token2_addr);
    token2_client.mint(&contract_id, &1000);

    // Try to swap 100 token_addr for min 100 token2 (but only get 50)
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &100, // min_amount_out too high
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::SlippageExceeded)));
}

#[test]
fn test_swap_correct_2_sided_settlement_forward_rate() {
    let (env, contract_id, .., oracle_admin, player1, player2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    // Set rate: 1 token_addr = 2 token2 (rate = 2 * 1e7)
    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &(2 * 10_000_000));
    });

    // Mint player1 has token_addr (from setup), and contract has token2
    token2_client.mint(&contract_id, &10000);

    let player1_token2_before = token2_client.balance(&player1);
    let contract_token_addr_before = token::Client::new(&env, &token_addr).balance(&contract_id);
    let contract_token2_before = token2_client.balance(&contract_id);

    // player1 swaps 100 token_addr for 200 token2
    env.as_contract(&contract_id, || {
        let result = client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &200, // Expect exactly 200
            &player1,
        );
        assert_eq!(result, Ok(Ok(())));
    });

    let player1_token2_after = token2_client.balance(&player1);
    let contract_token_addr_after = token::Client::new(&env, &token_addr).balance(&contract_id);
    let contract_token2_after = token2_client.balance(&contract_id);

    // player1 received 200 token2
    assert_eq!(player1_token2_after - player1_token2_before, 200);
    // contract received 100 token_addr
    assert_eq!(contract_token_addr_after - contract_token_addr_before, 100);
    // contract gave out 200 token2
    assert_eq!(contract_token2_before - contract_token2_after, 200);
}

#[test]
fn test_swap_correct_2_sided_settlement_inverse_rate() {
    let (env, contract_id, .., oracle_admin, player1, _player2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    // Set rate inversely: 1 token_addr = 0.5 token2 (rate = 5_000_000)
    env.as_contract(&contract_id, || {
        client.set_rate(&token2_addr, &token_addr, &(5_000_000));
    });

    token2_client.mint(&contract_id, &10000);

    let player1_token2_before = token2_client.balance(&player1);
    let contract_token_addr_before = token::Client::new(&env, &token_addr).balance(&contract_id);
    let contract_token2_before = token2_client.balance(&contract_id);

    // player1 swaps 100 token_addr for 50 token2
    env.as_contract(&contract_id, || {
        let result = client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &50,
            &player1,
        );
        assert_eq!(result, Ok(Ok(())));
    });

    let player1_token2_after = token2_client.balance(&player1);
    let contract_token_addr_after = token::Client::new(&env, &token_addr).balance(&contract_id);
    let contract_token2_after = token2_client.balance(&contract_id);

    // player1 received 50 token2
    assert_eq!(player1_token2_after - player1_token2_before, 50);
    // contract received 100 token_addr
    assert_eq!(contract_token_addr_after - contract_token_addr_before, 100);
    // contract gave out 50 token2
    assert_eq!(contract_token2_before - contract_token2_after, 50);
}

#[test]
fn test_swap_slippage_bound_at_boundary() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    // Rate: 1 token_addr = 2 token2
    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &(2 * 10_000_000));
    });

    token2_client.mint(&contract_id, &10000);

    // Exact slippage bound: min_amount_out = 200 (exactly what we get)
    env.as_contract(&contract_id, || {
        let result = client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &200, // Exact match
            &player1,
        );
        assert_eq!(result, Ok(Ok(())));
    });

    // One less than the bound should fail
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &201, // One more than we get
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::SlippageExceeded)));
}

#[test]
fn test_swap_cannot_drain_contract_without_funds() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();

    // Set rate
    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &(2 * 10_000_000));
    });

    // Do NOT mint token2 into the contract
    // Attempted swap should fail at the transfer step
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &0,
            &player1,
        )
    });
    // Will fail because the contract doesn't have enough token2 to send out
    assert!(result.is_err());
}

#[test]
fn test_swap_requires_sufficient_token_in_from_caller() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &(2 * 10_000_000));
    });

    token2_client.mint(&contract_id, &10000);

    // player1 only has 1000 token_addr from setup
    // Try to swap 1001, should fail
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &1001,
            &0,
            &player1,
        )
    });
    assert!(result.is_err()); // Will fail at the transfer step
}

#[test]
fn test_swap_to_different_recipient() {
    let (env, contract_id, .., oracle_admin, player1, player2, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &(2 * 10_000_000));
    });

    token2_client.mint(&contract_id, &10000);

    let player2_token2_before = token2_client.balance(&player2);

    // player1 swaps but sends output to player2
    env.as_contract(&contract_id, || {
        let result = client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &200,
            &player2, // recipient is player2
        );
        assert_eq!(result, Ok(Ok(())));
    });

    let player2_token2_after = token2_client.balance(&player2);
    assert_eq!(player2_token2_after - player2_token2_before, 200);
}

#[test]
fn test_swap_overflow_detection_on_multiplication() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    // Set a huge rate to trigger overflow
    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &i128::MAX);
    });

    token2_client.mint(&contract_id, &10000);

    // Try to swap with a large amount_in that will overflow when multiplied
    let result = env.as_contract(&contract_id, || {
        client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &i128::MAX, // Huge amount
            &0,
            &player1,
        )
    });
    assert_eq!(result, Err(Ok(Error::Overflow)));
}


// ============================================================================
// FUZZ TESTS FOR SWAP
// ============================================================================
//
// These fuzz tests exercise swap against a range of adversarial rate, amount,
// and recipient combinations to detect edge cases and invariant violations.

#[test]
fn fuzz_swap_various_rates_and_amounts() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    // Test matrix of (rate, amount_in, min_amount_out)
    let test_cases = vec![
        (1_000_000, 100, 0),           // 0.1x rate, small amount
        (10_000_000, 50, 0),           // 1x rate, tiny amount
        (100_000_000, 200, 0),         // 10x rate, larger amount
        (5_000_000, 1000, 0),          // 0.5x rate, large amount
        (20_000_000, 10, 0),           // 2x rate, very small
        (50_000_000, 999, 0),          // 5x rate, near-max amount from setup
        (1_000_000, 100, 10),          // 0.1x rate with slippage bound
        (10_000_000, 100, 100),        // Exact slippage bound
        (2_000_000, 500, 50),          // 0.2x rate with slippage
    ];

    for (rate, amount_in, min_out) in test_cases {
        // Reset contract for each test case
        let contract_id = env.register_contract(None, OracleContract);
        let client = OracleContractClient::new(&env, &contract_id);
        client.initialize(&oracle_admin);

        let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
        let token2_addr = token2.address();
        let token2_client = StellarAssetClient::new(&env, &token2_addr);

        env.as_contract(&contract_id, || {
            client.set_rate(&token_addr, &token2_addr, &rate);
        });

        // Fund contract with sufficient token2
        token2_client.mint(&contract_id, &100_000_000);

        // Only attempt swap if amount_in is valid
        if amount_in > 0 {
            env.as_contract(&contract_id, || {
                // Calculate expected output
                let expected_out = (amount_in as i128)
                    .checked_mul(rate)
                    .unwrap_or(i128::MAX)
                    .checked_div(10_000_000)
                    .unwrap_or(i128::MAX);

                let result = client.try_swap(
                    &player1,
                    &token_addr,
                    &token2_addr,
                    &amount_in,
                    &min_out,
                    &player1,
                );

                // If min_out <= expected_out, swap should succeed
                if (min_out as i128) <= expected_out {
                    assert!(
                        result.is_ok(),
                        "Swap failed for rate={}, amount_in={}, min_out={}, expected_out={}",
                        rate, amount_in, min_out, expected_out
                    );
                } else {
                    // Otherwise should fail with slippage
                    assert_eq!(
                        result,
                        Err(Ok(Error::SlippageExceeded)),
                        "Expected slippage error for rate={}, amount_in={}, min_out={}, expected_out={}",
                        rate, amount_in, min_out, expected_out
                    );
                }
            });
        }
    }
}

#[test]
fn fuzz_swap_boundary_amounts() {
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &10_000_000); // 1:1 rate
    });

    token2_client.mint(&contract_id, &100_000_000);

    // Test boundary conditions
    let boundary_amounts = vec![
        1,                      // Minimum valid amount
        i128::MAX / 20_000_000, // Large but safe amount
    ];

    for amount_in in boundary_amounts {
        if amount_in > 0 {
            env.as_contract(&contract_id, || {
                let result = client.try_swap(
                    &player1,
                    &token_addr,
                    &token2_addr,
                    &amount_in,
                    &0,
                    &player1,
                );

                // All should succeed with sufficient contract balance
                assert!(
                    result.is_ok(),
                    "Swap failed for boundary amount_in={}",
                    amount_in
                );
            });
        }
    }
}

#[test]
fn fuzz_swap_with_oracle_stake_present() {
    // Verify that swap cannot drain oracle stakes accidentally
    let (env, contract_id, .., oracle_admin, player1, _, token_addr) = setup();
    let client = OracleContractClient::new(&env, &contract_id);

    // Register an oracle with a stake
    let stake_token = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let stake_token_addr = stake_token.address();
    let stake_token_client = StellarAssetClient::new(&env, &stake_token_addr);

    stake_token_client.mint(&oracle_admin, &50000);
    env.as_contract(&contract_id, || {
        client.register_oracle_with_stake(&oracle_admin, &10000, &stake_token_addr);
    });

    let contract_stake_before = stake_token_client.balance(&contract_id);

    // Now try to swap a different pair
    let token2 = env.register_stellar_asset_contract_v2(oracle_admin.clone());
    let token2_addr = token2.address();
    let token2_client = StellarAssetClient::new(&env, &token2_addr);

    env.as_contract(&contract_id, || {
        client.set_rate(&token_addr, &token2_addr, &10_000_000);
    });

    token2_client.mint(&contract_id, &100_000_000);

    env.as_contract(&contract_id, || {
        let result = client.try_swap(
            &player1,
            &token_addr,
            &token2_addr,
            &100,
            &0,
            &player1,
        );
        assert!(result.is_ok());
    });

    // Verify the stake balance is unchanged (swap didn't touch it)
    let contract_stake_after = stake_token_client.balance(&contract_id);
    assert_eq!(
        contract_stake_before, contract_stake_after,
        "Swap incorrectly drained oracle stake"
    );
}
