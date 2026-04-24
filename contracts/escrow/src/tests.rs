#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _, Events, Ledger as _, MockAuth, MockAuthInvoke},
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

fn mint_player_balance(asset_client: &StellarAssetClient, player: &Address, amount: i128) {
    asset_client.mint(player, &amount);
}

#[test]
fn test_initialize_emits_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "escrow").into_val(&env),
        symbol_short!("init").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "escrow initialized event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_oracle, ev_admin): (Address, Address) =
        TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_oracle, oracle);
    assert_eq!(ev_admin, admin);
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
fn test_get_match_returns_stake_and_token() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stake_amount = 500i128;
    let id = client.create_match(
        &player1,
        &player2,
        &stake_amount,
        &token,
        &String::from_str(&env, "game_266"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(m.stake_amount, stake_amount);
    assert_eq!(m.token, token);
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

/// Verify the deposit flags on the Match struct after each individual deposit.
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

    let m = client.get_match(&id);
    assert!(!m.player1_deposited, "player1_deposited must be false before any deposit");
    assert!(!m.player2_deposited, "player2_deposited must be false before any deposit");

    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert!(m.player1_deposited, "player1_deposited must be true after player1 deposits");
    assert!(!m.player2_deposited, "player2_deposited must still be false after only player1 deposits");

    client.deposit(&id, &player2);
    let m = client.get_match(&id);
    assert!(m.player1_deposited, "player1_deposited must remain true after player2 deposits");
    assert!(m.player2_deposited, "player2_deposited must be true after player2 deposits");
}

#[test]
fn test_full_match_lifecycle_winner_and_draw_scenarios() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);
    let asset_client = StellarAssetClient::new(&env, &token);
    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);

    mint_player_balance(&asset_client, &player3, 1000);
    mint_player_balance(&asset_client, &player4, 1000);

    let winner_match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "full_lifecycle_winner"),
        &Platform::Lichess,
    );

    let winner_match = client.get_match(&winner_match_id);
    assert_eq!(winner_match.state, MatchState::Pending);
    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 1000);
    assert_eq!(client.get_escrow_balance(&winner_match_id), 0);

    client.deposit(&winner_match_id, &player1);
    let winner_match = client.get_match(&winner_match_id);
    assert_eq!(winner_match.state, MatchState::Pending);
    assert!(winner_match.player1_deposited);
    assert!(!winner_match.player2_deposited);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 1000);
    assert_eq!(client.get_escrow_balance(&winner_match_id), 100);

    client.deposit(&winner_match_id, &player2);
    let winner_match = client.get_match(&winner_match_id);
    assert_eq!(winner_match.state, MatchState::Active);
    assert!(winner_match.player1_deposited);
    assert!(winner_match.player2_deposited);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(client.get_escrow_balance(&winner_match_id), 200);

    client.submit_result(&winner_match_id, &Winner::Player1);
    let winner_match = client.get_match(&winner_match_id);
    assert_eq!(winner_match.state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(client.get_escrow_balance(&winner_match_id), 0);

    let draw_match_id = client.create_match(
        &player3,
        &player4,
        &75,
        &token,
        &String::from_str(&env, "full_lifecycle_draw"),
        &Platform::ChessDotCom,
    );

    let draw_match = client.get_match(&draw_match_id);
    assert_eq!(draw_match.state, MatchState::Pending);
    assert_eq!(token_client.balance(&player3), 1000);
    assert_eq!(token_client.balance(&player4), 1000);
    assert_eq!(client.get_escrow_balance(&draw_match_id), 0);

    client.deposit(&draw_match_id, &player3);
    let draw_match = client.get_match(&draw_match_id);
    assert_eq!(draw_match.state, MatchState::Pending);
    assert_eq!(token_client.balance(&player3), 925);
    assert_eq!(token_client.balance(&player4), 1000);
    assert_eq!(client.get_escrow_balance(&draw_match_id), 75);

    client.deposit(&draw_match_id, &player4);
    let draw_match = client.get_match(&draw_match_id);
    assert_eq!(draw_match.state, MatchState::Active);
    assert_eq!(token_client.balance(&player3), 925);
    assert_eq!(token_client.balance(&player4), 925);
    assert_eq!(client.get_escrow_balance(&draw_match_id), 150);

    client.submit_result(&draw_match_id, &Winner::Draw);
    let draw_match = client.get_match(&draw_match_id);
    assert_eq!(draw_match.state, MatchState::Completed);
    assert_eq!(token_client.balance(&player3), 1000);
    assert_eq!(token_client.balance(&player4), 1000);
    assert_eq!(client.get_escrow_balance(&draw_match_id), 0);
}

#[test]
fn test_full_match_lifecycle() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    // Step 1: create_match → Pending
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "lifecycle_game"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&id).state, MatchState::Pending);
    assert_eq!(client.get_escrow_balance(&id), 0);

    // Step 2: player1 deposits → still Pending
    client.deposit(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Pending);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(client.get_escrow_balance(&id), 100);

    // Step 3: player2 deposits → Active
    client.deposit(&id, &player2);
    assert_eq!(client.get_match(&id).state, MatchState::Active);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(client.get_escrow_balance(&id), 200);

    // Step 4: submit_result → Completed, winner paid, escrow zeroed
    client.submit_result(&id, &Winner::Player1);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100); // won the pot
    assert_eq!(token_client.balance(&player2), 900);  // lost stake
    assert_eq!(client.get_escrow_balance(&id), 0);
}

#[test]
fn test_payout_winner() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
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
    client.submit_result(&id, &Winner::Player1);

    // player1 started with 1000, deposited 100, won the 200 pot → 1100
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
    assert!(client.get_match(&id).completed_ledger.is_some());
}

#[test]
fn test_draw_refund() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
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
    client.submit_result(&id, &Winner::Draw);

    assert_eq!(token_client.balance(&player1), 1000);
    assert_eq!(token_client.balance(&player2), 1000);
}

#[test]
fn test_player2_balance_decreases_after_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "player2_balance_after_deposit"),
        &Platform::Lichess,
    );

    let balance_before = token_client.balance(&player2);
    client.deposit(&id, &player2);
    let balance_after = token_client.balance(&player2);

    assert_eq!(balance_before, 1000);
    assert_eq!(balance_after, 900);
    assert_eq!(balance_before - balance_after, 100);
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
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
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
    client.submit_result(&id, &Winner::Player1);

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
fn test_submit_result_fails_if_not_fully_funded() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_nofund"),
        &Platform::Lichess,
    );

    // Only player1 deposits — player2 has not
    client.deposit(&id, &player1);

    env.as_contract(&contract_id, || {
        let mut m: Match = env.storage().persistent().get(&DataKey::Match(id)).unwrap();
        m.state = MatchState::Active;
        env.storage().persistent().set(&DataKey::Match(id), &m);
    });

    let result = client.try_submit_result(&id, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::NotFunded)));
}

#[test]
fn test_initialize_accepts_valid_generated_oracle_address() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle, &admin);

    let stored_oracle: Address = env.as_contract(&contract_id, || {
        env.storage().instance().get(&DataKey::Oracle).unwrap()
    });
    assert_eq!(stored_oracle, oracle);
}

#[test]
fn test_initialize_rejects_contract_address_as_oracle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    // Passing the contract's own address as oracle must be rejected
    let result = client.try_initialize(&contract_id, &admin);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
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
fn test_concurrent_matches_remain_isolated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);
    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_id.address();
    let asset_client = StellarAssetClient::new(&env, &token);
    let token_client = TokenClient::new(&env, &token);

    for player in [&player1, &player2, &player3, &player4] {
        mint_player_balance(&asset_client, player, 1000);
    }

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let match_one = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "concurrent_match_one"),
        &Platform::Lichess,
    );
    let match_two = client.create_match(
        &player3,
        &player4,
        &60,
        &token,
        &String::from_str(&env, "concurrent_match_two"),
        &Platform::ChessDotCom,
    );

    client.deposit(&match_one, &player1);
    client.deposit(&match_two, &player3);
    assert_eq!(client.get_match(&match_one).state, MatchState::Pending);
    assert_eq!(client.get_match(&match_two).state, MatchState::Pending);
    assert_eq!(client.get_escrow_balance(&match_one), 100);
    assert_eq!(client.get_escrow_balance(&match_two), 60);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 1000);
    assert_eq!(token_client.balance(&player3), 940);
    assert_eq!(token_client.balance(&player4), 1000);

    client.deposit(&match_one, &player2);
    client.deposit(&match_two, &player4);
    assert_eq!(client.get_match(&match_one).state, MatchState::Active);
    assert_eq!(client.get_match(&match_two).state, MatchState::Active);
    assert_eq!(client.get_escrow_balance(&match_one), 200);
    assert_eq!(client.get_escrow_balance(&match_two), 120);

    client.submit_result(&match_one, &Winner::Player1);
    client.submit_result(&match_two, &Winner::Draw);

    assert_eq!(client.get_match(&match_one).state, MatchState::Completed);
    assert_eq!(client.get_match(&match_two).state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(token_client.balance(&player3), 1000);
    assert_eq!(token_client.balance(&player4), 1000);
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
    assert!(result.is_err());
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

/// Test that deposit is rejected when the contract is paused.
/// This verifies the invariant: no deposits can be made while the contract is paused,
/// preventing players from locking funds in a paused state.
#[test]
fn test_paused_contract_rejects_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create a match before pausing
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game123"),
        &Platform::Lichess,
    );

    // Admin pauses the contract
    client.pause();

    // Attempt to deposit - should fail with ContractPaused
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

    // A random third-party address that is not player1 or player2
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
#[should_panic(expected = "Error(Contract, #10)")]
fn test_create_match_with_zero_stake_fails() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // This should fail because stake_amount is 0
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

    // Player2 cancels the pending match
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

    // Both players deposit - this changes state to Active
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    // Now the match is Active, not Pending - cancel should fail with InvalidState
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

    // Only player2 deposits (player1 abandoned)
    client.deposit(&id, &player2);

    // Player2 cancels and gets refund
    client.cancel_match(&id, &player2);

    assert_eq!(token_client.balance(&player2), 1000);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
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

    // Both players deposit — transitions match to Active
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    // Verify match is Active before attempting cancel
    assert_eq!(client.get_match(&id).state, MatchState::Active);

    // Attempt to cancel an Active match — must return MatchAlreadyActive (error code #11)
    let result = client.try_cancel_match(&id, &player1);
    assert_eq!(
        result,
        Err(Ok(Error::MatchAlreadyActive)),
        "expected MatchAlreadyActive error when cancelling an Active match"
    );

    // Match must still be Active — no state change
    assert_eq!(client.get_match(&id).state, MatchState::Active);

    // Funds must remain in escrow — balances unchanged from post-deposit state
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(token_client.balance(&player2), 900);
}

#[test]
fn test_cancel_active_match_returns_match_already_active() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "game_already_active"),
        &Platform::Lichess,
    );

    // Fund both players — match transitions to Active
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    assert_eq!(client.get_match(&id).state, MatchState::Active);

    // cancel_match must return MatchAlreadyActive, not InvalidState
    let result = client.try_cancel_match(&id, &player1);
    assert_eq!(result, Err(Ok(Error::MatchAlreadyActive)));
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

    // Create a third party who is not part of the match
    let unauthorized = Address::generate(&env);

    // This should panic with Unauthorized error
    client.cancel_match(&id, &unauthorized);
}

#[test]
fn test_cancel_match_on_cancelled_match_returns_error() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancel_cancelled_match"),
        &Platform::Lichess,
    );

    // Cancel the match first
    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    // Try to cancel the already cancelled match
    let result = client.try_cancel_match(&id, &player1);
    assert!(
        matches!(result, Err(Ok(Error::MatchAlreadyActive)) | Err(Ok(Error::InvalidState))),
        "Expected MatchAlreadyActive or InvalidState error when cancelling an already cancelled match"
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

    // A random address that is not the oracle attempts to submit a result
    // while the contract is paused — must get Unauthorized, not ContractPaused.
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

    // Advance ledger so TTL decreases, making the subsequent extend_ttl in
    // cancel_match meaningful — without it the assertion would pass trivially
    // because create_match already set TTL to MATCH_TTL_LEDGERS.
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

    // Advance ledgers so TTL would have decreased without extend
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

// #287 — created_ledger is populated on create_match
#[test]
fn test_created_ledger_is_set() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Advance the ledger so sequence is non-zero
    env.ledger().set_sequence_number(42);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "ledger_game"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(m.created_ledger, 42, "created_ledger should match ledger sequence at creation");
}

// #292 — MatchCount increments correctly across multiple matches
#[test]
fn test_match_count_increments_sequentially() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let game_ids = ["seq0", "seq1", "seq2", "seq3", "seq4"];
    for (expected_id, game_id_str) in game_ids.iter().enumerate() {
        let id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, game_id_str),
            &Platform::Lichess,
        );
        assert_eq!(id, expected_id as u64);
    }

    let last = client.get_match(&4);
    assert_eq!(last.id, 4);
    assert_eq!(last.state, MatchState::Pending);
}

// #296 — get_escrow_balance returns 0 after draw payout
#[test]
fn test_escrow_balance_zero_after_draw() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "draw_balance_game"),
        &Platform::ChessDotCom,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    assert_eq!(client.get_escrow_balance(&id), 200);

    client.submit_result(&id, &Winner::Draw);

    assert_eq!(client.get_escrow_balance(&id), 0);
}

#[test]
fn test_get_escrow_balance_returns_stake_amount_after_player1_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "escrow_balance_player1"),
        &Platform::Lichess,
    );

    // Before any deposits, escrow balance should be 0
    assert_eq!(client.get_escrow_balance(&id), 0);

    // After player1 deposits, escrow balance should be 100 (1 * stake_amount)
    client.deposit(&id, &player1);
    assert_eq!(client.get_escrow_balance(&id), 100);
}

#[test]
fn test_expire_match_refunds_depositor_after_timeout() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_game"),
        &Platform::Lichess,
    );

    // Only player1 deposits
    client.deposit(&id, &player1);

    let p1_balance_before = token::Client::new(&env, &token).balance(&player1);

    env.deployer().extend_ttl_for_contract_instance(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_code(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_contract_instance(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_code(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );

    // Advance ledger past the default timeout (17_280 ledgers)
    env.ledger().set_sequence_number(100 + 17_280);

    env.deployer().extend_ttl_for_contract_instance(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_code(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_contract_instance(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer().extend_ttl_for_code(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );

    client.expire_match(&id);

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Cancelled);

    // player1 should have their stake back
    let p1_balance_after = token::Client::new(&env, &token).balance(&player1);
    assert_eq!(p1_balance_after - p1_balance_before, 100);
}

#[test]
fn test_expire_match_fails_before_timeout() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "early_expire"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);

    // Not enough ledgers have passed
    env.ledger().set_sequence_number(100 + 100);

    let result = client.try_expire_match(&id);
    assert_eq!(result, Err(Ok(Error::MatchNotExpired)));
}

#[test]
fn test_get_oracle_returns_initialized_address() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    assert_eq!(client.get_oracle(), oracle);
}

#[test]
fn test_get_match_returns_correct_players() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "players_test"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(m.player1, player1);
    assert_eq!(m.player2, player2);
}

#[test]
fn test_get_match_timeout_returns_default() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let timeout = client.try_get_match_timeout().unwrap().unwrap();
    assert_eq!(timeout, MATCH_TTL_LEDGERS);
}

#[test]
fn test_get_match_returns_match_not_found_for_unknown_id() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_get_match(&9999u64);
    assert_eq!(result, Err(Ok(Error::MatchNotFound)));
}


#[test]
fn test_update_oracle_emits_oracle_up_event_with_addresses() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_oracle = Address::generate(&env);
    let old_oracle: Address = client.get_oracle();

    client.update_oracle(&new_oracle);

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "admin").into_val(&env),
        soroban_sdk::symbol_short!("oracle_up").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(matched.is_some(), "oracle_up event not emitted");

    let (_, _, data) = matched.unwrap();
    let (ev_old, ev_new): (Address, Address) = TryFromVal::try_from_val(&env, &data).unwrap();
    assert_eq!(ev_old, old_oracle);
    assert_eq!(ev_new, new_oracle);
}


#[test]
fn test_is_funded_returns_false_when_only_player1_deposited() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "funded_test"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    assert!(!client.is_funded(&id));

    client.deposit(&id, &player2);
    assert!(client.is_funded(&id));
}


#[test]
fn test_submit_result_on_nonexistent_match_id_returns_match_not_found() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_result(&9999u64, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::MatchNotFound)));
}


#[test]
fn test_cancel_match_by_player2_refunds_player1_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancel_test"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    let player1_balance_after_deposit = token_client.balance(&player1);
    assert_eq!(player1_balance_after_deposit, 900);

    client.cancel_match(&id, &player2);

    let player1_balance_after_cancel = token_client.balance(&player1);
    assert_eq!(player1_balance_after_cancel, 1000);
    assert_eq!(token_client.balance(&player2), 1000);
}

// #373 — update_oracle routes subsequent submit_result to the new oracle
#[test]
fn test_update_oracle_routes_submit_result() {
    let (env, contract_id, oracle_old, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let oracle_new = Address::generate(&env);
    client.update_oracle(&oracle_new);
    assert_eq!(client.get_oracle(), oracle_new);

    // Match for oracle_new success assertion
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

    // oracle_new must succeed
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

    // Match for oracle_old rejection assertion
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

    // oracle_old must be rejected
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
fn test_get_match_returns_winner_after_payout() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "winner_test"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player2);

    let m = client.get_match(&id);
    assert_eq!(m.winner, Some(Winner::Player2));
}

#[test]
fn test_submit_result_overflow_on_extreme_stake() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create a match with a normal stake, then directly overwrite the stake_amount
    // in storage to i128::MAX so that stake_amount * 2 overflows — bypassing the
    // token layer which would also overflow on deposit.
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "overflow_game"),
        &Platform::Lichess,
    );

    env.as_contract(&contract_id, || {
        let mut m: Match = env.storage().persistent().get(&DataKey::Match(id)).unwrap();
        m.stake_amount = i128::MAX;
        m.state = MatchState::Active;
        m.player1_deposited = true;
        m.player2_deposited = true;
        env.storage().persistent().set(&DataKey::Match(id), &m);
    });

    let result = client.try_submit_result(&id, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::Overflow)));
}

#[test]
fn test_submit_result_uninitialized_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_result(&0, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_submit_result_overflow_on_extreme_stake() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create a match with a normal stake, then directly overwrite the stake_amount
    // in storage to i128::MAX so that stake_amount * 2 overflows — bypassing the
    // token layer which would also overflow on deposit.
    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "overflow_game"),
        &Platform::Lichess,
    );

    env.as_contract(&contract_id, || {
        let mut m: Match = env.storage().persistent().get(&DataKey::Match(id)).unwrap();
        m.stake_amount = i128::MAX;
        m.state = MatchState::Active;
        m.player1_deposited = true;
        m.player2_deposited = true;
        env.storage().persistent().set(&DataKey::Match(id), &m);
    });

    let result = client.try_submit_result(&id, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::Overflow)));
}

#[test]
fn test_submit_result_uninitialized_returns_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_result(&0, &Winner::Player1);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

// ── Budget / resource benchmarks ────────────────────────────────────────────
//
// These tests measure CPU instruction counts and memory bytes consumed by the
// three core operations. They do NOT assert exact values (which vary across
// SDK versions) but print the numbers so developers can track regressions and
// tune fee estimates. Run with `cargo test bench -- --nocapture`.

#[test]
fn bench_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mut budget = env.cost_estimate().budget();
    budget.reset_tracker();

    client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "bench_game"),
        &Platform::Lichess,
    );

    std::println!(
        "[bench_create_match] cpu={} mem={}",
        budget.cpu_instruction_cost(),
        budget.memory_bytes_cost()
    );
}

#[test]
fn bench_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "bench_game"),
        &Platform::Lichess,
    );

    let mut budget = env.cost_estimate().budget();
    budget.reset_tracker();
    client.deposit(&id, &player1);
    std::println!(
        "[bench_deposit p1]   cpu={} mem={}",
        budget.cpu_instruction_cost(),
        budget.memory_bytes_cost()
    );

    budget.reset_tracker();
    client.deposit(&id, &player2);
    std::println!(
        "[bench_deposit p2]   cpu={} mem={}",
        budget.cpu_instruction_cost(),
        budget.memory_bytes_cost()
    );
}

#[test]
fn bench_submit_result() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "bench_game"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let mut budget = env.cost_estimate().budget();
    budget.reset_tracker();
    client.submit_result(&id, &Winner::Player1);
    std::println!(
        "[bench_submit_result] cpu={} mem={}",
        budget.cpu_instruction_cost(),
        budget.memory_bytes_cost()
    );
}

#[test]
fn test_two_step_admin_transfer() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    // Step 1: propose — old admin is still active
    client.propose_admin(&new_admin);
    assert_eq!(client.get_admin(), admin);

    // Step 2: accept — new admin takes over
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    // Old admin is now rejected — clear mocks so auth is enforced
    env.set_auths(&[]);
    let result = client.try_propose_admin(&admin);
    assert!(result.is_err());
}
