extern crate std;

pub use super::*;
pub use soroban_sdk::{
    testutils::{Address as _, Events as _, MockAuth, MockAuthInvoke},
    token::{Client as TokenClient, StellarAssetClient},
    vec, Address, Env, IntoVal, String, Symbol, TryFromVal,
};
pub use std::{format, vec::Vec};

pub mod helpers;

mod admin;
mod events;
mod index;
mod invariants;
mod lifecycle;
mod pagination;
mod ttl;
mod token_allowlist;

// ── Base fixture ─────────────────────────────────────────────────────────────

/// Minimal initialized contract with two funded players and a token.
/// Returns `(env, contract_id, oracle, player1, player2, token, admin)`.
pub fn setup() -> (Env, Address, Address, Address, Address, Address, Address) {
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

    let contract_id = env.register_contract(None, EscrowContract);
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

// ── Extended fixtures ─────────────────────────────────────────────────────────

/// Returns a `TokenClient` for the given token address — avoids repeating the
/// `TokenClient::new` boilerplate in every test that checks balances.
pub fn token_client<'a>(env: &'a Env, token: &Address) -> TokenClient<'a> {
    TokenClient::new(env, token)
}

/// Like `setup`, but also creates a match and has both players deposit so the
/// match is in `Active` state.  Returns the base tuple plus the `match_id`.
///
/// Signature: `(env, contract_id, oracle, player1, player2, token, admin, match_id)`
pub fn setup_with_funded_match() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    u64,
) {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "funded_fixture_game"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    (env, contract_id, oracle, player1, player2, token, admin, match_id)
}

/// Like `setup`, but mints tokens for two additional players (`player3`,
/// `player4`) so tests that need four participants don't repeat the boilerplate.
///
/// Signature: `(env, contract_id, oracle, player1, player2, player3, player4, token, admin)`
pub fn setup_with_four_players() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let asset_client = StellarAssetClient::new(&env, &token);
    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);
    asset_client.mint(&player3, &1000);
    asset_client.mint(&player4, &1000);

    (env, contract_id, oracle, player1, player2, player3, player4, token, admin)
}

// ── Shared helpers ────────────────────────────────────────────────────────────

pub fn mint_player_balance(asset_client: &StellarAssetClient, player: &Address, amount: i128) {
    asset_client.mint(player, &amount);
}
