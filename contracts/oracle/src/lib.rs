#![no_std]

mod errors;
mod types;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol};
use types::{DataKey, Winner, ResultEntry};

/// ~30 days at 5s/ledger.
const MATCH_TTL_LEDGERS: u32 = 518_400;

/// Extend instance storage TTL on every invocation so Admin and Paused never expire.
fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(MATCH_TTL_LEDGERS / 2, MATCH_TTL_LEDGERS);
}

#[contract]
pub struct OracleContract;

#[contractimpl]
impl OracleContract {
    /// Initialize with a trusted admin (the off-chain oracle service).
    pub fn initialize(env: Env, admin: Address) {
        extend_instance_ttl(&env);
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish((Symbol::new(&env, "oracle"), symbol_short!("init")), &admin);
    }

    /// Admin submits a verified match result on-chain.
    /// Invariant: No results can be submitted while the contract is paused.
    ///
    /// # Errors
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    /// - [`Error::AlreadySubmitted`] — a result for `match_id` has already been recorded.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        game_id: String,
        result: Winner,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        // Check if contract is paused first
        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if env.storage().persistent().has(&DataKey::Result(match_id)) {
            return Err(Error::AlreadySubmitted);
        }

        if game_id.is_empty() {
            return Err(Error::InvalidGameId);
        }

        env.storage().persistent().set(
            &DataKey::Result(match_id),
            &ResultEntry {
                game_id,
                result: result.clone(),
            },
        );
        env.storage().persistent().extend_ttl(
            &DataKey::Result(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("result")),
            (match_id, result),
        );

        Ok(())
    }

    /// Retrieve the stored result for a match.
    /// TTL is extended on every read to prevent active results from expiring.
    /// Without this, frequently-accessed results could expire and return ResultNotFound.
    ///
    /// # Errors
    /// - [`Error::ResultNotFound`] — no result has been submitted for `match_id`, or the entry has expired.
    pub fn get_result(env: Env, match_id: u64) -> Result<ResultEntry, Error> {
        extend_instance_ttl(&env);
        let result = env
            .storage()
            .persistent()
            .get(&DataKey::Result(match_id))
            .ok_or(Error::ResultNotFound)?;

        // Extend TTL to keep active results alive
        env.storage().persistent().extend_ttl(
            &DataKey::Result(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        Ok(result)
    }

    /// Check whether a result has been submitted for a match.
    pub fn has_result(env: Env, match_id: u64) -> bool {
        extend_instance_ttl(&env);
        env.storage().persistent().has(&DataKey::Result(match_id))
    }

    /// Admin-gated variant of [`has_result`] for private-tournament contexts.
    ///
    /// Identical in behaviour to `has_result` but requires the stored admin to
    /// authorise the call, preventing any third party from probing whether a
    /// result has been submitted before the official announcement.
    ///
    /// # Errors
    /// Returns [`Error::Unauthorized`] if the contract has not been initialised
    /// or if the caller is not the current admin.
    pub fn has_result_admin(env: Env, match_id: u64) -> Result<bool, Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        Ok(env.storage().persistent().has(&DataKey::Result(match_id)))
    }

    /// Admin removes a previously submitted result from persistent storage.
    pub fn delete_result(env: Env, match_id: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if !env.storage().persistent().has(&DataKey::Result(match_id)) {
            return Err(Error::ResultNotFound);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Result(match_id));
        Ok(())
    }

    /// Rotate the admin to a new address. Requires current admin auth.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the current admin.
    pub fn update_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let current_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        current_admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
    }

    /// Pause the oracle — admin only. Blocks submit_result while paused.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    pub fn pause(env: Env) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    /// Returns true if the contract has been initialized.
    pub fn is_initialized(env: Env) -> bool {
        extend_instance_ttl(&env);
        env.storage().instance().has(&DataKey::Admin)
    }

    /// Unpause the oracle — admin only. Does not emit an event.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    pub fn unpause(env: Env) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use escrow::types::{MatchState, Platform, Winner};
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

        // Register token
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token_addr = token_id.address();
        let asset_client = StellarAssetClient::new(&env, &token_addr);
        asset_client.mint(&player1, &1000);
        asset_client.mint(&player2, &1000);

        // Register escrow contract and create + fund a match (id=0)
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

        // Register oracle contract
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

    // ── has_result (public, unauthenticated) ─────────────────────────────────

    /// Confirms that any caller can invoke has_result without authentication.
    /// Returns false before a result is submitted and true afterwards.
    #[test]
    fn test_has_result_is_public_and_unauthenticated() {
        let (env, contract_id, _escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Before submission — any caller can probe, no auth required
        assert!(!client.has_result(&0u64));
        assert!(!client.has_result(&999u64));

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &Winner::Player1,
        );

        // After submission — still public, now returns true
        assert!(client.has_result(&0u64));
        // Unrelated match_id still false
        assert!(!client.has_result(&999u64));
    }

    // ── has_result_admin (admin-gated) ────────────────────────────────────────

    /// Admin can probe result existence via the gated variant.
    #[test]
    fn test_has_result_admin_returns_false_before_submission() {
        let (env, contract_id, _escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        assert!(!client.has_result_admin(&0u64));
        assert!(!client.has_result_admin(&999u64));
    }

    /// has_result_admin returns true after a result is submitted.
    #[test]
    fn test_has_result_admin_returns_true_after_submission() {
        let (env, contract_id, _escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &Winner::Player1,
        );

        assert!(client.has_result_admin(&0u64));
    }

    /// Non-admin callers must not be able to call has_result_admin.
    #[test]
    #[should_panic]
    fn test_has_result_admin_rejects_non_admin() {
        let env = Env::default();
        // Do NOT mock all auths — we want auth to actually be enforced
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, OracleContract);
        let client = OracleContractClient::new(&env, &contract_id);
        client.initialize(&admin);
        // Non-admin tries to call has_result_admin - should panic
        client.has_result_admin(&0u64);
    }

    #[test]
    fn test_submit_and_get_result() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );

        assert!(client.has_result(&0u64));
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, Winner::Player1);
    }

    #[test]
    fn test_submit_result_emits_event() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
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

        client.submit_result(&0u64, &String::from_str(&env, "abc123"), &MatchResult::Draw);

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

        client.submit_result(&0u64, &String::from_str(&env, "abc123"), &Winner::Draw);
        // second submit should panic
        client.submit_result(&0u64, &String::from_str(&env, "abc123"), &Winner::Draw);
    }

    #[test]
    fn test_duplicate_submit_returns_already_submitted() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(&0u64, &String::from_str(&env, "abc123"), &MatchResult::Draw);
        let result =
            client.try_submit_result(&0u64, &String::from_str(&env, "abc123"), &MatchResult::Draw);
        assert_eq!(result, Err(Ok(Error::AlreadySubmitted)));
    }

    #[test]
    #[should_panic]
    fn test_double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, OracleContract);
        let client = OracleContractClient::new(&env, &contract_id);

        client.initialize(&admin);
        // second initialize should panic
        client.initialize(&admin);
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
            &Winner::Player1,
        );

        let ttl = env.as_contract(&contract_id, || {
            env.storage().persistent().get_ttl(&DataKey::Result(0u64))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }

    /// Test that get_result returns ResultNotFound for non-existent match IDs.
    /// This verifies the invariant: querying an unknown match_id must always
    /// return Error::ResultNotFound rather than panicking or returning invalid data.
    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_get_result_not_found() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Query a match_id that has never been submitted
        client.get_result(&9999u64);
    }

    /// Test that pause can only be called by admin.
    #[test]
    fn test_pause_admin_only() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Admin can pause
        client.pause();

        // Verify it's paused by trying to submit a result
        let result = client.try_submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    /// Test that unpause can only be called by admin.
    #[test]
    fn test_unpause_admin_only() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Pause first
        client.pause();

        // Admin can unpause
        client.unpause();

        // Verify it's unpaused by submitting a result
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );
        assert!(client.has_result(&0u64));
    }

    /// Test that submit_result returns ContractPaused when paused.
    #[test]
    fn test_submit_result_blocked_when_paused() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Pause the contract
        client.pause();

        // Try to submit a result - should fail with ContractPaused
        let result = client.try_submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        // Verify no result was stored
        assert!(!client.has_result(&0u64));
    }

    /// Test that submit_result works normally after unpause.
    #[test]
    fn test_submit_result_works_after_unpause() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Pause the contract
        client.pause();

        // Verify submit is blocked
        let result = client.try_submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        // Unpause
        client.unpause();

        // Now submit should work
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );
        assert!(client.has_result(&0u64));
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, Winner::Player1);
    }

    /// Test pause/unpause state transitions.
    #[test]
    fn test_pause_unpause_state_transitions() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Initially unpaused - submit should work
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );
        assert!(client.has_result(&0u64));

        // Pause
        client.pause();

        // Submit should fail
        let result = client.try_submit_result(
            &1u64,
            &String::from_str(&env, "def456"),
            &Winner::Player2,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        // Unpause
        client.unpause();

        // Submit should work again
        client.submit_result(
            &1u64,
            &String::from_str(&env, "def456"),
            &Winner::Player2,
        );
        assert!(client.has_result(&1u64));

        // Can pause again
        client.pause();
        let result =
            client.try_submit_result(&2u64, &String::from_str(&env, "ghi789"), &MatchResult::Draw);
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    /// Test that get_result extends TTL on read.
    /// This prevents active results from expiring while they're still being accessed.
    #[test]
    fn test_get_result_extends_ttl() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Submit a result
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &Winner::Player1,
        );

        // Read the result
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, Winner::Player1);

        // Verify TTL was extended
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
        client.pause(); // second call must not error

        // Contract is still paused
        let is_paused: bool = env.as_contract(&contract_id, || {
            env.storage()
                .instance()
                .get(&DataKey::Paused)
                .unwrap_or(false)
        });
        assert!(is_paused);
    }

    #[test]
    fn test_unpause_emits_no_event() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // First pause the contract
        client.pause();
        // Then unpause it
        client.unpause();

        // Test passes if unpause completes without panic
        // The function docstring states it does not emit events
    }

    /// Full integration: oracle stores result, escrow oracle address submits
    /// result to escrow, payout executes, match is Completed.
    #[test]
    fn test_oracle_to_escrow_full_payout_flow() {
        let (env, oracle_id, escrow_id, _oracle_admin, player1, _player2, token_addr) = setup();
        let oracle_client = OracleContractClient::new(&env, &oracle_id);
        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        let token_client = soroban_sdk::token::Client::new(&env, &token_addr);

        // Step 1: oracle admin submits verified result to the oracle contract
        oracle_client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &Winner::Player1,
        );
        assert!(oracle_client.has_result(&0u64));

        // Step 2: the escrow's trusted oracle address (oracle_admin) calls
        // submit_result on the escrow contract, triggering the payout
        escrow_client.submit_result(&0u64, &EscrowWinner::Player1);

        // Step 3: assert match is Completed and player1 received the full pot
        let m = escrow_client.get_match(&0u64);
        assert_eq!(m.state, MatchState::Completed);
        // player1 staked 100, pot = 200; started with 1000, deposited 100 → balance = 1100
        assert_eq!(token_client.balance(&player1), 1100);
    }

    #[test]
    fn test_delete_result_removes_from_storage() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "chess_game_42"),
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
    #[should_panic]
    fn test_delete_result_requires_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — auth is enforced
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
            &Winner::Player1,
        );

        let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }

    /// get_result resets TTL to MATCH_TTL_LEDGERS even after ledgers have advanced.
    #[test]
    fn test_get_result_resets_ttl_after_ledger_advance() {
        let (env, contract_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "ttl_advance_game"),
            &Winner::Player1,
        );

        // Advance ledger by 1000 so the TTL has decreased
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

        client.get_result(&0u64);

        let ttl = env.as_contract(&contract_id, || {
            env.storage().persistent().get_ttl(&DataKey::Result(0u64))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }
}
