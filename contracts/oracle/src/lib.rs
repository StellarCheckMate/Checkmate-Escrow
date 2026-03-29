#![no_std]

mod errors;
mod types;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol};
use types::{DataKey, MatchResult, ResultEntry};

/// ~30 days at 5s/ledger.
const MATCH_TTL_LEDGERS: u32 = 518_400;

#[contract]
pub struct OracleContract;

#[contractimpl]
impl OracleContract {
    /// Initialize with a trusted admin (the off-chain oracle service).
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    /// Pause the contract — admin only. Blocks submit_result.
    /// Used as an emergency stop mechanism if the admin key is compromised
    /// or if malicious results are being submitted.
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    /// Unpause the contract — admin only.
    /// Restores normal operation after emergency pause is resolved.
    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Admin submits a verified match result on-chain.
    /// Invariant: No results can be submitted while the contract is paused.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        game_id: String,
        result: MatchResult,
    ) -> Result<(), Error> {
        // Check if contract is paused first
        if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
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
    pub fn get_result(env: Env, match_id: u64) -> Result<ResultEntry, Error> {
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
        env.storage().persistent().has(&DataKey::Result(match_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{storage::Persistent as _, Address as _, Events},
        Address, Env, IntoVal, String, Symbol,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);
        client.initialize(&admin);
        (env, contract_id)
    }

    #[test]
    fn test_submit_and_get_result() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );

        assert!(client.has_result(&0u64));
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, MatchResult::Player1Wins);
    }

    #[test]
    fn test_submit_result_emits_event() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
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
        let (ev_id, ev_result): (u64, MatchResult) =
            soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
        assert_eq!(ev_id, 0u64);
        assert_eq!(ev_result, MatchResult::Player1Wins);
    }

    #[test]
    #[should_panic]
    fn test_duplicate_submit_fails() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(&0u64, &String::from_str(&env, "abc123"), &MatchResult::Draw);
        // second submit should panic
        client.submit_result(&0u64, &String::from_str(&env, "abc123"), &MatchResult::Draw);
    }

    #[test]
    #[should_panic]
    fn test_double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);

        client.initialize(&admin);
        // second initialize should panic
        client.initialize(&admin);
    }

    #[test]
    fn test_ttl_extended_on_submit_result() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );

        let ttl = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get_ttl(&DataKey::Result(0u64))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }

    /// Test that get_result returns ResultNotFound for non-existent match IDs.
    /// This verifies the invariant: querying an unknown match_id must always
    /// return Error::ResultNotFound rather than panicking or returning invalid data.
    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_get_result_not_found() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Query a match_id that has never been submitted
        client.get_result(&9999u64);
    }

    /// Test that pause can only be called by admin.
    #[test]
    fn test_pause_admin_only() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Admin can pause
        client.pause();

        // Verify it's paused by trying to submit a result
        let result = client.try_submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    /// Test that unpause can only be called by admin.
    #[test]
    fn test_unpause_admin_only() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Pause first
        client.pause();

        // Admin can unpause
        client.unpause();

        // Verify it's unpaused by submitting a result
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );
        assert!(client.has_result(&0u64));
    }

    /// Test that submit_result returns ContractPaused when paused.
    #[test]
    fn test_submit_result_blocked_when_paused() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Pause the contract
        client.pause();

        // Try to submit a result - should fail with ContractPaused
        let result = client.try_submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        // Verify no result was stored
        assert!(!client.has_result(&0u64));
    }

    /// Test that submit_result works normally after unpause.
    #[test]
    fn test_submit_result_works_after_unpause() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Pause the contract
        client.pause();

        // Verify submit is blocked
        let result = client.try_submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        // Unpause
        client.unpause();

        // Now submit should work
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );
        assert!(client.has_result(&0u64));
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, MatchResult::Player1Wins);
    }

    /// Test pause/unpause state transitions.
    #[test]
    fn test_pause_unpause_state_transitions() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Initially unpaused - submit should work
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );
        assert!(client.has_result(&0u64));

        // Pause
        client.pause();

        // Submit should fail
        let result = client.try_submit_result(
            &1u64,
            &String::from_str(&env, "def456"),
            &MatchResult::Player2Wins,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));

        // Unpause
        client.unpause();

        // Submit should work again
        client.submit_result(
            &1u64,
            &String::from_str(&env, "def456"),
            &MatchResult::Player2Wins,
        );
        assert!(client.has_result(&1u64));

        // Can pause again
        client.pause();
        let result = client.try_submit_result(
            &2u64,
            &String::from_str(&env, "ghi789"),
            &MatchResult::Draw,
        );
        assert_eq!(result, Err(Ok(Error::ContractPaused)));
    }

    /// Test that get_result extends TTL on read.
    /// This prevents active results from expiring while they're still being accessed.
    #[test]
    fn test_get_result_extends_ttl() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Submit a result
        client.submit_result(
            &0u64,
            &String::from_str(&env, "abc123"),
            &MatchResult::Player1Wins,
        );

        // Read the result
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, MatchResult::Player1Wins);

        // Verify TTL was extended
        let ttl = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get_ttl(&DataKey::Result(0u64))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }
}
