#![no_std]

pub mod errors;
pub mod types;

use errors::Error;
use soroban_sdk::{
    contract, contractimpl, symbol_short, token, vec, Address, Env, IntoVal, String, Symbol,
    TryFromVal, Vec,
};
use types::{
    DataKey, Match, MatchState, MatchWinner, OracleMatchResult, OracleResultEntry, Platform, Winner,
};

/// ~30 days at 5s/ledger. Storage TTL only — controls how long match data is kept on-chain.
const MATCH_TTL_LEDGERS: u32 = 518_400;

/// ~7 days at 5s/ledger. Business timeout — how long a Pending match may wait
/// for both deposits before anyone can call `expire_match`.
const MATCH_TIMEOUT_LEDGERS: u32 = 120_960;

/// Maximum allowed byte length for a game_id string.
///
/// Platform-specific formats:
/// - Lichess:      8 alphanumeric characters (e.g. `"abcd1234"`)
/// - Chess.com:    numeric string, typically 7–12 digits (e.g. `"123456789"`)
///
/// Both formats fit well within this limit.
const MAX_GAME_ID_LEN: u32 = 64;

/// Load a match from persistent storage, distinguishing between a match that
/// was never created and one whose storage entry has been evicted (TTL lapsed).
///
/// # How it works
/// `MatchCount` is stored in *instance* storage (which has a much longer
/// effective lifetime than persistent entries). If `match_id < MatchCount` the
/// ID was once allocated, so a missing persistent entry means the TTL elapsed
/// and the entry was evicted — `Error::MatchStorageExpired` is returned.
/// If `match_id >= MatchCount` the ID was never issued — `Error::MatchNotFound`
/// is returned.
///
/// # Limitation
/// This heuristic cannot distinguish an evicted entry from one that was
/// manually deleted (which is not possible through the public API, but could
/// happen via low-level ledger operations). In practice, for this contract,
/// a missing persistent entry for a known ID always means TTL eviction.
fn load_match(env: &Env, match_id: u64) -> Result<types::Match, Error> {
    if let Some(m) = env.storage().persistent().get(&DataKey::Match(match_id)) {
        return Ok(m);
    }
    let count: u64 = env
        .storage()
        .instance()
        .get(&DataKey::MatchCount)
        .unwrap_or(0);
    if match_id < count {
        Err(Error::MatchStorageExpired)
    } else {
        Err(Error::MatchNotFound)
    }
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with a trusted oracle address and an admin.
    ///
    /// Must be called by the deployer immediately after deployment.
    /// The deployer address is passed as `deployer` and must authorize this call,
    /// preventing any third party from front-running initialization.
    pub fn initialize(env: Env, oracle: Address, admin: Address, deployer: Address) {
        deployer.require_auth();
        if env.storage().instance().has(&DataKey::Oracle) {
            panic!("Contract already initialized");
        }
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MatchCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("init")),
            (oracle, admin),
        );
    }

    /// Add a token to the allowlist — admin only.
    pub fn add_allowed_token(env: Env, token: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if env
            .storage()
            .instance()
            .has(&DataKey::AllowedToken(token.clone()))
        {
            return Err(Error::TokenAlreadyAllowed);
        }

        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(token), &true);
        Ok(())
    }

    /// Remove a token from the allowlist — admin only.
    pub fn remove_allowed_token(env: Env, token: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .remove(&DataKey::AllowedToken(token));
        Ok(())
    }

    /// Check if a token is allowed.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AllowedToken(token))
            .unwrap_or(false)
    }

    /// Pause the contract — admin only. Blocks create_match, deposit, and submit_result.
    pub fn pause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("paused")), ());
        Ok(())
    }

    /// Unpause the contract — admin only.
    pub fn unpause(env: Env) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("unpaused")), ());
        Ok(())
    }

    /// Rotate the oracle address. Requires authorization from the current oracle or the admin.
    pub fn update_oracle(env: Env, new_oracle: Address, caller: Address) -> Result<(), Error> {
        let current_oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;

        if caller != current_oracle && caller != admin {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        env.storage().instance().set(&DataKey::Oracle, &new_oracle);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("oracle_up")),
            (current_oracle, new_oracle),
        );

        Ok(())
    }

    /// Create a new match. Both players must call `deposit` before the game starts.
    ///
    /// # Parameters
    /// - `game_id`: The platform-specific game identifier. Must be ≤ 64 bytes.
    ///   - **Lichess**: 8-character alphanumeric string (e.g. `"abcd1234"`).
    ///     Taken from the game URL: `https://lichess.org/<game_id>`
    ///   - **Chess.com**: numeric string, typically 7–12 digits (e.g. `"123456789"`).
    ///     Taken from the game URL: `https://www.chess.com/game/live/<game_id>`
    ///   Passing an ID from the wrong platform or a malformed ID will not be
    ///   rejected on-chain, but the oracle will fail to verify the result.
    /// - `platform`: Must match the platform the `game_id` was issued by.
    ///   Use `Platform::Lichess` or `Platform::ChessDotCom` accordingly.
    ///
    /// # Errors
    /// Returns `Error::InvalidGameId` if `game_id` exceeds `MAX_GAME_ID_LEN` (64 bytes).
    /// Returns `Error::DuplicateGameId` if the same `game_id` has already been used.
    pub fn create_match(
        env: Env,
        player1: Address,
        player2: Address,
        stake_amount: i128,
        token: Address,
        game_id: String,
        platform: Platform,
    ) -> Result<u64, Error> {
        player1.require_auth();

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }
        if player1 == player2 {
            return Err(Error::InvalidPlayers);
        }
        if stake_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if game_id.is_empty() || game_id.len() > MAX_GAME_ID_LEN {
            return Err(Error::InvalidGameId);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::GameId(game_id.clone()))
        {
            return Err(Error::DuplicateGameId);
        }

        // Validate that `token` implements the token interface by probing `balance`.
        // An invalid address will cause try_invoke_contract to return Err, which we
        // map to Error::InvalidToken rather than letting it panic at deposit time.
        {
            use soroban_sdk::IntoVal;
            let args = vec![&env, env.current_contract_address().into_val(&env)];
            let probe: Result<
                Result<soroban_sdk::Val, soroban_sdk::ConversionError>,
                Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
            > = env.try_invoke_contract(&token, &Symbol::new(&env, "balance"), args);
            if probe.is_err() {
                return Err(Error::InvalidToken);
            }
        }

        // Validate token is in allowlist
        if !env
            .storage()
            .instance()
            .get(&DataKey::AllowedToken(token.clone()))
            .unwrap_or(false)
        {
            return Err(Error::TokenNotAllowed);
        }

        let id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0);

        if env.storage().persistent().has(&DataKey::Match(id)) {
            return Err(Error::AlreadyExists);
        }

        let m = Match {
            id,
            player1,
            player2,
            stake_amount,
            token,
            game_id,
            platform,
            state: MatchState::Pending,
            player1_deposited: false,
            player2_deposited: false,
            created_ledger: env.ledger().sequence(),
            winner: MatchWinner::Pending,
        };

        env.storage().persistent().set(&DataKey::Match(id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        // Guard against u64 overflow in release mode where wrapping would occur silently
        let next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::MatchCount, &next_id);
        env.storage()
            .persistent()
            .set(&DataKey::GameId(m.game_id.clone()), &true);

        // Update player matches index
        for player in [m.player1.clone(), m.player2.clone()] {
            let mut matches: soroban_sdk::Vec<u64> = env
                .storage()
                .persistent()
                .get(&DataKey::PlayerMatches(player.clone()))
                .unwrap_or_else(|| soroban_sdk::Vec::new(&env));
            matches.push_back(id);
            env.storage()
                .persistent()
                .set(&DataKey::PlayerMatches(player.clone()), &matches);
            env.storage().persistent().extend_ttl(
                &DataKey::PlayerMatches(player),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("created")),
            (id, m.player1, m.player2, stake_amount),
        );

        // Append to the on-chain index of active matches
        let mut active: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveMatches)
            .unwrap_or(Vec::new(&env));
        active.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::ActiveMatches, &active);

        Ok(id)
    }

    /// Player deposits their stake into escrow.
    pub fn deposit(env: Env, match_id: u64, player: Address) -> Result<(), Error> {
        player.require_auth();

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        let mut m: Match = load_match(&env, match_id)?;

        if m.state == MatchState::Cancelled {
            return Err(Error::MatchCancelled);
        }
        if m.state == MatchState::Completed {
            return Err(Error::MatchCompleted);
        }
        if m.state != MatchState::Pending {
            return Err(Error::InvalidState);
        }

        let is_p1 = player == m.player1;
        let is_p2 = player == m.player2;

        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }
        if is_p1 && m.player1_deposited {
            return Err(Error::AlreadyFunded);
        }
        if is_p2 && m.player2_deposited {
            return Err(Error::AlreadyFunded);
        }

        let client = token::Client::new(&env, &m.token);
        client.transfer(&player, &env.current_contract_address(), &m.stake_amount);

        if is_p1 {
            m.player1_deposited = true;
        } else {
            m.player2_deposited = true;
        }

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("deposit")),
            (match_id, player.clone()),
        );

        if m.player1_deposited && m.player2_deposited {
            m.state = MatchState::Active;
            env.events().publish(
                (Symbol::new(&env, "match"), symbol_short!("activated")),
                match_id,
            );
        }

        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        Ok(())
    }

    /// Oracle submits the verified match result and triggers payout.
    pub fn submit_result(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;

        if caller != oracle {
            return Err(Error::Unauthorized);
        }
        // require the oracle's signature before any other checks (e.g. paused)
        oracle.require_auth();

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        let mut m: Match = load_match(&env, match_id)?;

        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        if !m.player1_deposited || !m.player2_deposited {
            return Err(Error::NotFunded);
        }

        let winner = Self::fetch_oracle_result(&env, &oracle, match_id, &m.game_id)?;
        let client = token::Client::new(&env, &m.token);
        let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;

        match winner {
            Winner::Player1 => client.transfer(&env.current_contract_address(), &m.player1, &pot),
            Winner::Player2 => client.transfer(&env.current_contract_address(), &m.player2, &pot),
            Winner::Draw => {
                client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
                client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
            }
        }

        m.state = MatchState::Completed;
        m.winner = MatchWinner::Decided(winner.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let topics = (Symbol::new(&env, "match"), symbol_short!("completed"));
        env.events().publish(topics, (match_id, winner));

        // Remove from active matches index
        let active: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveMatches)
            .unwrap_or(Vec::new(&env));
        let mut new_active: Vec<u64> = Vec::new(&env);
        let len = active.len();
        let mut i = 0u32;
        while i < len {
            let v = active.get(i).unwrap();
            if v != match_id {
                new_active.push_back(v);
            }
            i += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::ActiveMatches, &new_active);

        Ok(())
    }

    fn fetch_oracle_result(
        env: &Env,
        oracle: &Address,
        match_id: u64,
        expected_game_id: &String,
    ) -> Result<Winner, Error> {
        let args = vec![&env, match_id.into_val(env)];
        let symbol = Symbol::new(env, "get_result");
        let call_result: Result<
            Result<soroban_sdk::Val, soroban_sdk::ConversionError>,
            Result<soroban_sdk::Error, soroban_sdk::InvokeError>,
        > = env.try_invoke_contract(oracle, &symbol, args);

        let value = match call_result {
            Ok(Ok(val)) => val,
            _ => return Err(Error::ResultNotFound),
        };

        let entry: OracleResultEntry =
            OracleResultEntry::try_from_val(env, &value).map_err(|_| Error::ResultNotFound)?;

        if entry.game_id != *expected_game_id {
            return Err(Error::InvalidGameId);
        }

        let resolved = match entry.result {
            OracleMatchResult::Player1Wins => Winner::Player1,
            OracleMatchResult::Player2Wins => Winner::Player2,
            OracleMatchResult::Draw => Winner::Draw,
        };
        Ok(resolved)
    }

    /// Cancel a pending match and refund any deposits.
    /// Either player can cancel a pending match.
    pub fn cancel_match(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        let mut m: Match = load_match(&env, match_id)?;

        if m.state != MatchState::Pending {
            return Err(Error::InvalidState);
        }

        // Defensive: the contract itself must never be accepted as a valid caller.
        if caller == env.current_contract_address() {
            return Err(Error::Unauthorized);
        }

        // Either player1 or player2 can cancel a pending match
        let is_p1 = caller == m.player1;
        let is_p2 = caller == m.player2;

        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }

        caller.require_auth();

        let client = token::Client::new(&env, &m.token);

        if m.player1_deposited {
            client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
        }
        if m.player2_deposited {
            client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
        }

        m.state = MatchState::Cancelled;
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("cancelled")),
            match_id,
        );

        // Remove from active matches index
        let active: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveMatches)
            .unwrap_or(Vec::new(&env));
        let mut new_active: Vec<u64> = Vec::new(&env);
        let len = active.len();
        let mut i = 0u32;
        while i < len {
            let v = active.get(i).unwrap();
            if v != match_id {
                new_active.push_back(v);
            }
            i += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::ActiveMatches, &new_active);

        Ok(())
    }

    /// Expire a pending match that has not been fully funded within MATCH_TIMEOUT_LEDGERS.
    /// Anyone can call this; funds are returned to whoever deposited.
    pub fn expire_match(env: Env, match_id: u64) -> Result<(), Error> {
        let mut m: Match = load_match(&env, match_id)?;

        if m.state != MatchState::Pending {
            return Err(Error::InvalidState);
        }

        let elapsed = env.ledger().sequence().saturating_sub(m.created_ledger);

        if elapsed < MATCH_TIMEOUT_LEDGERS {
            return Err(Error::MatchNotExpired);
        }

        let client = token::Client::new(&env, &m.token);

        if m.player1_deposited {
            client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
        }
        if m.player2_deposited {
            client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
        }

        m.state = MatchState::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("expired")),
            match_id,
        );

        Ok(())
    }

    /// Return the admin address set at initialization.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)
    }

    /// Read a match by ID.
    ///
    /// # Errors
    /// - `Error::MatchNotFound` — the `match_id` was never allocated.
    /// - `Error::MatchStorageExpired` — the `match_id` was once valid but its
    ///   persistent storage entry has been evicted (TTL elapsed). The match
    ///   existed on-chain but is no longer accessible.
    pub fn get_match(env: Env, match_id: u64) -> Result<Match, Error> {
        let m = load_match(&env, match_id)?;
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        Ok(m)
    }

    /// Check whether both players have deposited.
    pub fn is_funded(env: Env, match_id: u64) -> Result<bool, Error> {
        let m: Match = load_match(&env, match_id)?;
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        Ok(m.player1_deposited && m.player2_deposited)
    }

    /// Return the total escrowed balance for a match (0, 1x, or 2x stake).
    ///
    /// Returns `Err(Error::MatchCompleted)` or `Err(Error::MatchCancelled)` for
    /// terminal states so callers can distinguish them from an unfunded match.
    pub fn get_escrow_balance(env: Env, match_id: u64) -> Result<i128, Error> {
        let m: Match = load_match(&env, match_id)?;
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        if m.state == MatchState::Completed {
            return Err(Error::MatchCompleted);
        }
        if m.state == MatchState::Cancelled {
            return Err(Error::MatchCancelled);
        }
        // Count depositors explicitly — avoids fragile bool-to-integer casting.
        let depositors: i128 =
            if m.player1_deposited { 1 } else { 0 } + if m.player2_deposited { 1 } else { 0 };
        Ok(depositors * m.stake_amount)
    }

    /// Return true if the contract has been initialized.
    pub fn is_initialized(env: Env) -> bool {
        env.storage().instance().has(&DataKey::Oracle)
    }

    /// Return the total number of matches created.
    pub fn get_match_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests;
