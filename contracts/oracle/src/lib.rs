#![no_std]

mod errors;
mod types;
pub use types::MatchResult;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol};
use types::{DataKey, ResultEntry};

/// ~30 days at 5s/ledger.
const MATCH_TTL_LEDGERS: u32 = 518_400;

#[contract]
pub struct OracleContract;

#[contractimpl]
impl OracleContract {
    /// Initialize with a trusted admin (the off-chain oracle service).
    ///
    /// Must be called by the deployer immediately after deployment.
    /// The deployer address is passed as `deployer` and must authorize this call,
    /// preventing any third party from front-running initialization.
    pub fn initialize(env: Env, admin: Address, deployer: Address) {
        deployer.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Contract already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish((Symbol::new(&env, "oracle"), symbol_short!("init")), admin);
    }

    /// Admin submits a verified match result on-chain.
    ///
    /// `escrow` is the deployed escrow contract address. `match_id` must
    /// correspond to a real match in that contract — if no such match exists,
    /// `Error::MatchNotFound` is returned and nothing is stored, preventing
    /// orphaned result entries from polluting storage.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        game_id: String,
        result: MatchResult,
        escrow: Address,
    ) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        // Cross-contract call: verify the match exists in the escrow contract.
        // get_match returns Err if the match_id is unknown; we map that to
        // Error::MatchNotFound to prevent orphaned result entries.
        use soroban_sdk::IntoVal;
        let args = soroban_sdk::vec![&env, match_id.into_val(&env)];
        let call_result: Result<
            Result<soroban_sdk::Val, soroban_sdk::ConversionError>,
            Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
        > = env.try_invoke_contract(&escrow, &soroban_sdk::Symbol::new(&env, "get_match"), args);
        if call_result.is_err() {
            return Err(Error::MatchNotFound);
        }

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
    ///
    /// # Access
    /// This function is intentionally **public and unauthenticated**. It is a
    /// read-only probe that returns a boolean — no result data is exposed.
    ///
    /// For most tournament contexts this is acceptable: knowing that *a* result
    /// exists leaks no information about *who* won. If your use-case requires
    /// keeping result existence private until an official announcement, use
    /// [`has_result_admin`] instead, which requires admin authorisation.
    pub fn has_result(env: Env, match_id: u64) -> bool {
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
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        Ok(env.storage().persistent().has(&DataKey::Result(match_id)))
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
    use escrow::{EscrowContract, EscrowContractClient};
    use soroban_sdk::{
        testutils::{storage::Persistent as _, Address as _, Events},
        token::StellarAssetClient,
        Address, Env, IntoVal, String, Symbol,
    };

    /// Returns (env, oracle_contract_id, escrow_contract_id, admin, player1, player2, token)
    /// with a real escrow match (id=0) already created and both players deposited (Active).
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
        let escrow_id = env.register(EscrowContract, ());
        let escrow_client = EscrowContractClient::new(&env, &escrow_id);
        escrow_client.initialize(&oracle_admin, &admin, &admin);
        escrow_client.create_match(
            &player1,
            &player2,
            &100,
            &token_addr,
            &String::from_str(&env, "test_game"),
            &escrow::types::Platform::Lichess,
        );
        escrow_client.deposit(&0u64, &player1);
        escrow_client.deposit(&0u64, &player2);

        // Register oracle contract
        let oracle_id = env.register(OracleContract, ());
        let oracle_client = OracleContractClient::new(&env, &oracle_id);
        oracle_client.initialize(&oracle_admin, &oracle_admin);

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

    // ── has_result (public, unauthenticated) ─────────────────────────────────

    /// Confirms that any caller can invoke has_result without authentication.
    /// Returns false before a result is submitted and true afterwards.
    #[test]
    fn test_has_result_is_public_and_unauthenticated() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // Before submission — any caller can probe, no auth required
        assert!(!client.has_result(&0u64));
        assert!(!client.has_result(&999u64));

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player1Wins,
            &escrow_id,
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
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player1Wins,
            &escrow_id,
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
        let non_admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);

        env.mock_all_auths();
        client.initialize(&admin, &admin);

        // Only authorise non_admin — should fail
        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &non_admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "has_result_admin",
                args: (0u64,).into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.has_result_admin(&0u64);
    }

    #[test]
    fn test_submit_and_get_result() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player1Wins,
            &escrow_id,
        );

        assert!(client.has_result(&0u64));
        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, MatchResult::Player1Wins);
        assert_eq!(entry.game_id, String::from_str(&env, "test_game"));
    }

    #[test]
    fn test_submit_result_emits_event() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player1Wins,
            &escrow_id,
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
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Draw,
            &escrow_id,
        );
        // second submit should panic
        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Draw,
            &escrow_id,
        );
    }

    #[test]
    #[should_panic]
    fn test_double_initialize_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);

        client.initialize(&admin, &admin);
        client.initialize(&admin, &admin);
    }

    #[test]
    fn test_ttl_extended_on_submit_result() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player1Wins,
            &escrow_id,
        );

        let ttl = env.as_contract(&contract_id, || {
            env.storage().persistent().get_ttl(&DataKey::Result(0u64))
        });
        assert_eq!(ttl, crate::MATCH_TTL_LEDGERS);
    }

    #[test]
    fn test_admin_rotation() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);
        let new_admin = Address::generate(&env);

        client.update_admin(&new_admin);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player2Wins,
            &escrow_id,
        );
        assert!(client.has_result(&0u64));
    }

    #[test]
    #[should_panic]
    fn test_old_admin_cannot_act_after_rotation() {
        let env = Env::default();
        let old_admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);
        let escrow_id = env.register(EscrowContract, ());

        env.mock_all_auths();
        client.initialize(&old_admin, &old_admin);
        client.update_admin(&new_admin);

        env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &old_admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "submit_result",
                args: (
                    1u64,
                    String::from_str(&env, "game_old"),
                    MatchResult::Player1Wins,
                    escrow_id.clone(),
                )
                    .into_val(&env),
                sub_invokes: &[],
            },
        }]);
        client.submit_result(
            &1u64,
            &String::from_str(&env, "game_old"),
            &MatchResult::Player1Wins,
            &escrow_id,
        );
    }

    // ── Non-existent match_id rejected ───────────────────────────────────────

    /// Submitting a result for a match_id that does not exist in the escrow
    /// contract must return Error::MatchNotFound and store nothing.
    #[test]
    fn test_submit_result_for_nonexistent_match_returns_match_not_found() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        // match_id 999 was never created in the escrow contract
        let result = client.try_submit_result(
            &999u64,
            &String::from_str(&env, "ghost_game"),
            &MatchResult::Player1Wins,
            &escrow_id,
        );

        assert_eq!(
            result,
            Err(Ok(Error::MatchNotFound)),
            "submit_result must return MatchNotFound for a non-existent match_id"
        );

        // Nothing should have been stored
        assert!(!client.has_result(&999u64));
    }

    #[test]
    fn test_initialize_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);

        client.initialize(&admin, &admin);

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
        let emitted_admin: Address = soroban_sdk::TryFromVal::try_from_val(&env, &data).unwrap();
        assert_eq!(emitted_admin, admin);
    }

    #[test]
    fn test_submit_draw_result_stores_correctly() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Draw,
            &escrow_id,
        );

        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, MatchResult::Draw);
    }

    #[test]
    fn test_submit_player2wins_result_stores_correctly() {
        let (env, contract_id, escrow_id, ..) = setup();
        let client = OracleContractClient::new(&env, &contract_id);

        client.submit_result(
            &0u64,
            &String::from_str(&env, "test_game"),
            &MatchResult::Player2Wins,
            &escrow_id,
        );

        let entry = client.get_result(&0u64);
        assert_eq!(entry.result, MatchResult::Player2Wins);
    }

    // ── #216: oracle initialize front-run guard ───────────────────────────────

    /// A non-deployer must not be able to call initialize on the oracle contract.
    #[test]
    fn test_oracle_initialize_rejects_unauthorized_caller() {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let attacker = Address::generate(&env);
        let admin = Address::generate(&env);

        let contract_id = env.register(OracleContract, ());
        let client = OracleContractClient::new(&env, &contract_id);

        use soroban_sdk::testutils::{MockAuth, MockAuthInvoke};
        env.set_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &contract_id,
                fn_name: "initialize",
                args: (&admin, &deployer).into_val(&env),
                sub_invokes: &[],
            },
        }
        .into()]);

        let result = client.try_initialize(&admin, &deployer);
        assert!(result.is_err(), "oracle initialize must reject a non-deployer caller");
    }
}
