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
    }

    /// Admin submits a verified match result on-chain.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        game_id: String,
        result: MatchResult,
    ) -> Result<(), Error> {
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
    pub fn get_result(env: Env, match_id: u64) -> Result<ResultEntry, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Result(match_id))
            .ok_or(Error::ResultNotFound)
    }

    /// Check whether a result has been submitted for a match.
    pub fn has_result(env: Env, match_id: u64) -> bool {
        env.storage().persistent().has(&DataKey::Result(match_id))
    }

    /// Rotate the admin to a new address. Requires current admin auth.
    pub fn update_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let current_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        current_admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        Ok(())
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
    fn test_has_result_returns_false_before_submission() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // On a fresh oracle contract, has_result should return false for any match_id
        assert!(!client.has_result(&0u64));
        assert!(!client.has_result(&999u64));
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
            env.storage().persistent().get_ttl(&DataKey::Result(0u64))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }

    #[test]
    fn test_admin_rotation() {
        let (env, contract_id) = setup();
        let client = OracleContractClient::new(&env, &contract_id);
        let new_admin = Address::generate(&env);

        client.update_admin(&new_admin);

        // new admin can submit a result without error
        client.submit_result(
            &1u64,
            &String::from_str(&env, "game_new"),
            &MatchResult::Player2Wins,
        );
        assert!(client.has_result(&1u64));
    }

    #[test]
    #[should_panic]
    fn test_old_admin_cannot_act_after_rotation() {
        let env = Env::default();
        let old_admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);

        // initialize with old_admin, rotate to new_admin
        env.mock_all_auths();
        client.initialize(&old_admin);
        client.update_admin(&new_admin);

        // now only allow auth for old_admin — should panic because stored admin is new_admin
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &old_admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "submit_result",
                args: (
                    1u64,
                    String::from_str(&env, "game_old"),
                    MatchResult::Player1Wins,
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.submit_result(
            &1u64,
            &String::from_str(&env, "game_old"),
            &MatchResult::Player1Wins,
        );
    }
}
