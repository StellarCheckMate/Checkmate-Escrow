//! Integration test: full match lifecycle via the escrow contract's public API.
//!
//! Sequence: create match → both players deposit → oracle submits result →
//! match state is `Completed` and the winner is paid out.
//!
//! Runs entirely against a local Soroban test environment (`Env::default()`),
//! no testnet or network access required.
//!
//! Run with:
//!   cargo test -p escrow --test match_lifecycle_test

use escrow::types::{MatchState, Platform, ProtocolConfig, Winner};
use escrow::{EscrowContract, EscrowContractClient};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, Env, String as SorobanString,
};

const STAKE: i128 = 100;
const MINT_AMOUNT: i128 = 10_000;

/// Returns `(env, contract_id, oracle, player1, player2, token)`.
fn setup() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_id.address();
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &MINT_AMOUNT);
    asset.mint(&player2, &MINT_AMOUNT);

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
    });

    (env, contract_id, oracle, player1, player2, token)
}

#[test]
fn full_match_lifecycle_create_deposit_result_completed() {
    let (env, contract_id, _oracle, player1, player2, token) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 1. Create match — starts Pending, awaiting deposits.
    let match_id = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &SorobanString::from_str(&env, "lifecycle_e2e_game"),
        &Platform::Lichess,
    );
    assert_eq!(client.get_match(&match_id).state, MatchState::Pending);

    // 2. Both players deposit — match becomes Active and fully funded.
    client.deposit(&match_id, &player1);
    assert_eq!(client.get_match(&match_id).state, MatchState::Pending);

    client.deposit(&match_id, &player2);
    assert_eq!(client.get_match(&match_id).state, MatchState::Active);
    assert!(client.is_funded(&match_id));
    assert_eq!(client.get_escrow_balance(&match_id), STAKE * 2);

    let token_client = soroban_sdk::token::Client::new(&env, &token);
    let winner_balance_before = token_client.balance(&player1);

    // 3. Oracle submits the result — player1 wins.
    client.submit_result(&match_id, &Winner::Player1);

    // 4. Match transitions to Completed and the winner is paid the full pot.
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
    assert_eq!(client.get_escrow_balance(&match_id), 0);
    assert_eq!(
        token_client.balance(&player1),
        winner_balance_before + STAKE * 2
    );
}
