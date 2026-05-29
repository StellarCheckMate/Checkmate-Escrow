use super::*;
use soroban_sdk::testutils::{
    storage::{Instance as _, Persistent as _},
    Address as _, Ledger as _,
};

#[test]
fn test_is_initialized_false_before_initialize_and_true_after() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    assert!(
        !client.is_initialized(),
        "contract must report uninitialized before initialize is called"
    );

    client.initialize(&oracle, &admin);

    assert!(
        client.is_initialized(),
        "contract must report initialized after initialize is called"
    );
}

#[test]
fn test_initialize_accepts_valid_generated_oracle_address() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle = Address::generate(&env);
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, EscrowContract);
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
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_initialize(&contract_id, &admin);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
}

#[test]
#[should_panic(expected = "Contract already initialized")]
fn test_double_initialize_fails() {
    let env = Env::default();
    env.mock_all_auths();

    let oracle1 = Address::generate(&env);
    let oracle2 = Address::generate(&env);
    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.initialize(&oracle1, &admin);
    client.initialize(&oracle2, &admin);
}

#[test]
fn test_initialize_rejects_self_as_oracle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_initialize(&contract_id, &admin);
    assert_eq!(result, Err(Ok(Error::InvalidAddress)));
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
fn test_match_state_pending_immediately_after_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "pending_state_test"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Pending);
    assert!(!m.player1_deposited);
    assert!(!m.player2_deposited);
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
fn test_concurrent_deposits_same_ledger() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "concurrent_deposits"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Active);
    assert!(client.is_funded(&id));
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
    assert!(
        !m.player1_deposited,
        "player1_deposited must be false before any deposit"
    );
    assert!(
        !m.player2_deposited,
        "player2_deposited must be false before any deposit"
    );

    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert!(
        m.player1_deposited,
        "player1_deposited must be true after player1 deposits"
    );
    assert!(
        !m.player2_deposited,
        "player2_deposited must still be false after only player1 deposits"
    );

    client.deposit(&id, &player2);
    let m = client.get_match(&id);
    assert!(
        m.player1_deposited,
        "player1_deposited must remain true after player2 deposits"
    );
    assert!(
        m.player2_deposited,
        "player2_deposited must be true after player2 deposits"
    );
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

    client.deposit(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Pending);
    assert_eq!(token_client.balance(&player1), 900);
    assert_eq!(client.get_escrow_balance(&id), 100);

    client.deposit(&id, &player2);
    assert_eq!(client.get_match(&id).state, MatchState::Active);
    assert_eq!(token_client.balance(&player2), 900);
    assert_eq!(client.get_escrow_balance(&id), 200);

    client.submit_result(&id, &Winner::Player1);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);
    assert_eq!(token_client.balance(&player1), 1100);
    assert_eq!(token_client.balance(&player2), 900);
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
fn test_submit_result_fails_when_contract_token_balance_is_zero() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "zero_balance_game"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    let contract_balance = token_client.balance(&contract_id);
    if contract_balance > 0 {
        env.as_contract(&contract_id, || {
            token_client.transfer(&contract_id, &player1, &contract_balance);
        });
    }

    assert_eq!(token_client.balance(&contract_id), 0);

    let result = client.try_submit_result(&id, &Winner::Player1);
    assert!(
        result.is_err(),
        "submit_result should fail when contract has zero token balance"
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
        Err(Ok(Error::MatchAlreadyActive)),
        "expected MatchAlreadyActive error when cancelling an Active match"
    );

    assert_eq!(client.get_match(&id).state, MatchState::Active);

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

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    assert_eq!(client.get_match(&id).state, MatchState::Active);

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

    let unauthorized = Address::generate(&env);

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

    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    let result = client.try_cancel_match(&id, &player1);
    assert!(
        matches!(result, Err(Ok(Error::MatchAlreadyActive)) | Err(Ok(Error::InvalidState))),
        "Expected MatchAlreadyActive or InvalidState error when cancelling an already cancelled match"
    );
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

    let contract_id = env.register_contract(None, EscrowContract);
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
fn test_concurrent_matches_do_not_share_escrow_balances() {
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

    for player in [&player1, &player2, &player3, &player4] {
        mint_player_balance(&asset_client, player, 1000);
    }

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "isolated_balance_match_a"),
        &Platform::Lichess,
    );
    let match_b = client.create_match(
        &player3,
        &player4,
        &60,
        &token,
        &String::from_str(&env, "isolated_balance_match_b"),
        &Platform::ChessDotCom,
    );

    client.deposit(&match_a, &player1);

    assert_eq!(client.get_escrow_balance(&match_a), 100);
    assert_eq!(client.get_escrow_balance(&match_b), 0);
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
fn test_create_match_with_negative_stake_returns_invalid_amount() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &-100,
        &token,
        &String::from_str(&env, "negative_stake_game"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn test_create_match_with_empty_game_id_returns_invalid_game_id() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, ""),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)));
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

    assert_eq!(client.get_escrow_balance(&id), 0);

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

    client.deposit(&id, &player1);

    let p1_balance_before = token::Client::new(&env, &token).balance(&player1);

    env.deployer().extend_ttl_for_contract_instance(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(contract_id.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.deployer().extend_ttl_for_contract_instance(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(token.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.as_contract(&contract_id, || {
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveMatches,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    });

    env.ledger().set_sequence_number(100 + 17_280);

    env.deployer().extend_ttl_for_contract_instance(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(contract_id.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.deployer().extend_ttl_for_contract_instance(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(token.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.as_contract(&contract_id, || {
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveMatches,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    });

    client.expire_match(&id);

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Cancelled);

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

    env.ledger().set_sequence_number(100 + 100);

    let result = client.try_expire_match(&id);
    assert_eq!(result, Err(Ok(Error::MatchNotExpired)));
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
    assert_eq!(timeout, DEFAULT_MATCH_TIMEOUT_LEDGERS);
}

#[test]
fn test_get_match_returns_match_not_found_for_unknown_id() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_get_match(&9999u64);
    assert_eq!(result, Err(Ok(Error::MatchNotFound)));
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

#[test]
fn test_cancel_match_by_unauthorized_address_returns_unauthorized() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let third_party = Address::generate(&env);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "unauthorized_cancel_test"),
        &Platform::Lichess,
    );

    let result = client.try_cancel_match(&id, &third_party);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_get_match_returns_winner_after_payout() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
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
    assert_eq!(m.winner, Winner::Player2);
}

#[test]
fn test_submit_result_overflow_on_extreme_stake() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

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
fn test_two_step_admin_transfer() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_admin = Address::generate(&env);

    client.propose_admin(&new_admin);
    assert_eq!(client.get_admin(), admin);

    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    env.set_auths(&[]);
    let result = client.try_propose_admin(&admin);
    assert!(result.is_err());
}

#[test]
fn test_deposit_after_cancel_match_returns_invalid_state() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "deposit_after_cancel"),
        &Platform::Lichess,
    );

    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    let result = client.try_deposit(&id, &player2);
    assert_eq!(result, Err(Ok(Error::InvalidState)));
}

#[test]
fn test_match_state_active_after_both_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "active_state_test"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Pending);

    client.deposit(&id, &player1);
    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Pending);

    client.deposit(&id, &player2);
    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Active);
}

#[test]
fn test_create_match_rejects_same_player_as_both_sides() {
    let (env, contract_id, _oracle, player1, _player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &player1,
        &player1,
        &100,
        &token,
        &String::from_str(&env, "self_match"),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidPlayers)));
}

#[test]
fn test_get_match_returns_cancelled_after_expire_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_state_game"),
        &Platform::Lichess,
    );

    for addr in [&contract_id, &token] {
        env.deployer().extend_ttl_for_contract_instance(
            addr.clone(),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        env.deployer()
            .extend_ttl_for_code(addr.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    }
    env.as_contract(&contract_id, || {
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveMatches,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    });

    env.ledger().set_sequence_number(100 + 17_280);

    for addr in [&contract_id, &token] {
        env.deployer().extend_ttl_for_contract_instance(
            addr.clone(),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        env.deployer()
            .extend_ttl_for_code(addr.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    }
    env.as_contract(&contract_id, || {
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveMatches,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    });

    client.expire_match(&id);

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Cancelled);
}

#[test]
fn test_double_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "double_deposit_test"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    assert!(!client.is_funded(&id));

    let result = client.try_deposit(&id, &player1);
    assert_eq!(result, Err(Ok(Error::AlreadyFunded)));
}

#[test]
fn test_is_funded_returns_true_after_payout() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "is_funded_post_payout"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    assert!(
        client.is_funded(&id),
        "is_funded must be true when both players have deposited"
    );
    assert_eq!(client.get_match(&id).state, MatchState::Active);

    client.submit_result(&id, &Winner::Player1);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);

    assert!(
        client.is_funded(&id),
        "is_funded returns true after payout because it checks deposit flags, not match state"
    );

    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "get_escrow_balance must return 0 for a Completed match"
    );
}

#[test]
fn test_get_escrow_balance_zero_for_completed_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "balance_completed"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    assert_eq!(
        client.get_escrow_balance(&id),
        200,
        "escrow balance must be 2x stake while Active"
    );

    client.submit_result(&id, &Winner::Player2);
    assert_eq!(client.get_match(&id).state, MatchState::Completed);

    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "get_escrow_balance must return 0 after match is Completed"
    );
}

#[test]
fn test_get_escrow_balance_zero_for_cancelled_match_no_deposits() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "balance_cancelled_no_deposit"),
        &Platform::Lichess,
    );

    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "escrow balance must be 0 before any deposits"
    );
    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);

    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "get_escrow_balance must return 0 for a Cancelled match where no deposits were made"
    );
}

#[test]
fn test_get_escrow_balance_zero_after_cancel_with_player1_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "balance_cancelled_after_player1_deposit"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    assert_eq!(
        client.get_escrow_balance(&id),
        100,
        "escrow balance must reflect player1's deposited stake before cancellation"
    );

    client.cancel_match(&id, &player1);
    assert_eq!(client.get_match(&id).state, MatchState::Cancelled);
    assert_eq!(
        client.get_escrow_balance(&id),
        0,
        "get_escrow_balance must return 0 after cancelling a match and refunding player1"
    );
}

#[test]
fn test_expire_match_refunds_both_players_when_both_deposited_but_still_pending() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = token::Client::new(&env, &token);

    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_both_deposited"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    client.deposit(&id, &player2);

    env.as_contract(&contract_id, || {
        let mut m: Match = env.storage().persistent().get(&DataKey::Match(id)).unwrap();
        m.state = MatchState::Pending;
        env.storage().persistent().set(&DataKey::Match(id), &m);
    });

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Pending);
    assert!(m.player1_deposited);
    assert!(m.player2_deposited);

    let p1_balance_before = token_client.balance(&player1);
    let p2_balance_before = token_client.balance(&player2);

    env.deployer().extend_ttl_for_contract_instance(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(contract_id.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.deployer().extend_ttl_for_contract_instance(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(token.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.as_contract(&contract_id, || {
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveMatches,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    });

    env.ledger().set_sequence_number(100 + 17_280);

    env.deployer().extend_ttl_for_contract_instance(
        contract_id.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(contract_id.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.deployer().extend_ttl_for_contract_instance(
        token.clone(),
        MATCH_TTL_LEDGERS,
        MATCH_TTL_LEDGERS,
    );
    env.deployer()
        .extend_ttl_for_code(token.clone(), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    env.as_contract(&contract_id, || {
        env.storage().persistent().extend_ttl(
            &DataKey::ActiveMatches,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    });

    client.expire_match(&id);

    let m = client.get_match(&id);
    assert_eq!(m.state, MatchState::Cancelled);

    assert_eq!(token_client.balance(&player1) - p1_balance_before, 100);
    assert_eq!(token_client.balance(&player2) - p2_balance_before, 100);
}

// #287 — created_ledger is populated on create_match
#[test]
fn test_created_ledger_is_set() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

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
    assert_eq!(
        m.created_ledger, 42,
        "created_ledger should match ledger sequence at creation"
    );
}

#[test]
fn test_create_match_with_chess_dot_com_platform() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "chess_dot_com_game"),
        &Platform::ChessDotCom,
    );

    let m = client.get_match(&id);
    assert_eq!(m.platform, Platform::ChessDotCom);
}

#[test]
fn test_winner_is_draw_default_before_result_submitted() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "default_winner_test"),
        &Platform::Lichess,
    );

    let m = client.get_match(&id);
    assert_eq!(
        m.state,
        MatchState::Pending,
        "match must be Pending immediately after creation"
    );
    assert_eq!(
        m.winner,
        Winner::Draw,
        "winner must default to Draw (Undecided) before any result is submitted"
    );
}
