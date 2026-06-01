#![no_std]

pub mod errors;
pub mod types;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, String, Symbol};
use types::{DataKey, Match, MatchState, Platform, Winner};

/// ~30 days at 5s/ledger. Used as the default TTL and expiration threshold.
const MATCH_TTL_LEDGERS: u32 = 518_400;

/// Default match expiration timeout used when no explicit timeout is configured.
const DEFAULT_MATCH_TIMEOUT_LEDGERS: u32 = MATCH_TTL_LEDGERS;

/// Maximum allowed byte length for a game_id string.
///
/// Platform-specific formats:
/// - Lichess:      8 alphanumeric characters (e.g. `"abcd1234"`)
/// - Chess.com:    numeric string, typically 7–12 digits (e.g. `"123456789"`)
///
/// Both formats fit well within this limit.
const MAX_GAME_ID_LEN: u32 = 64;

/// Extend instance storage TTL on every invocation so Admin, Oracle, Paused, and other
/// instance keys never expire.
fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(MATCH_TTL_LEDGERS / 2, MATCH_TTL_LEDGERS);
}

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with a trusted oracle address and an admin.
    pub fn initialize(env: Env, oracle: Address, admin: Address) {
        if env.storage().instance().has(&DataKey::Oracle) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MatchCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    /// Pause the contract — admin only. Blocks create_match, deposit, and submit_result.
    pub fn pause(env: Env) -> Result<(), Error> {
        extend_instance_ttl(&env);
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
        extend_instance_ttl(&env);
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

    /// Add a token to the allowlist — admin only.
    pub fn add_allowed_token(env: Env, token: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .persistent()
            .set(&DataKey::AllowedToken(token.clone()), &true);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::AllowedToken(token.clone()), MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
        env.storage().instance().set(&DataKey::AllowlistEnforced, &true);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("token_add")),
            token,
        );
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
            .persistent()
            .remove(&DataKey::AllowedToken(token.clone()));

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("token_remove")),
            token,
        );
        Ok(())
    }

    /// Check if a token is allowed.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::AllowedToken(token))
            .unwrap_or(false)
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
        extend_instance_ttl(&env);
        player1.require_auth();

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        // Check allowlist enforcement
        let allowlist_enforced: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistEnforced)
            .unwrap_or(false);
        if allowlist_enforced && !Self::is_token_allowed(env.clone(), token.clone()) {
            return Err(Error::TokenNotAllowed);
        }

        if stake_amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if game_id.len() == 0 || game_id.len() > MAX_GAME_ID_LEN {
            return Err(Error::InvalidGameId);
        }

        // Reject if player2 is the contract address
        if player2 == env.current_contract_address() {
            return Err(Error::InvalidPlayers);
        }

        if env.storage().persistent().has(&DataKey::GameId(game_id.clone())) {
            return Err(Error::DuplicateGameId);
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
            completed_ledger: None,
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
        env.storage().persistent().set(&DataKey::GameId(m.game_id.clone()), &true);

        // Add match ID to both players' match lists
        let mut player1_matches: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerMatches(player1.clone()))
            .unwrap_or_else(|| soroban_sdk::vec![&env]);
        player1_matches.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::PlayerMatches(player1.clone()), &player1_matches);
        env.storage().persistent().extend_ttl(
            &DataKey::PlayerMatches(player1),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let mut player2_matches: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerMatches(player2.clone()))
            .unwrap_or_else(|| soroban_sdk::vec![&env]);
        player2_matches.push_back(id);
        env.storage()
            .persistent()
            .set(&DataKey::PlayerMatches(player2.clone()), &player2_matches);
        env.storage().persistent().extend_ttl(
            &DataKey::PlayerMatches(player2),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("created")),
            (id, m.player1, m.player2, stake_amount),
        );

        Ok(id)
    }

    /// Player deposits their stake into escrow.
    pub fn deposit(env: Env, match_id: u64, player: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        player.require_auth();

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

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
    pub fn submit_result(
        env: Env,
        match_id: u64,
        winner: Winner,
        caller: Address,
    ) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;

        if caller != oracle {
            return Err(Error::Unauthorized);
        }
        caller.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        if !m.player1_deposited || !m.player2_deposited {
            return Err(Error::NotFunded);
        }

        let client = token::Client::new(&env, &m.token);
        let pot = m.stake_amount * 2;

        match winner {
            Winner::Player1 => client.transfer(&env.current_contract_address(), &m.player1, &pot),
            Winner::Player2 => client.transfer(&env.current_contract_address(), &m.player2, &pot),
            Winner::Draw => {
                client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
                client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
            }
        }

        m.state = MatchState::Completed;
        m.completed_ledger = Some(env.ledger().sequence());
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

        Ok(())
    }

    /// Submit result with oracle record integration.
    /// This is the canonical path for oracle-initiated payouts.
    /// The oracle contract calls this to atomically store the result and execute payout.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — caller is not the oracle.
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::MatchNotFound`] — no match exists for `match_id`.
    /// - [`Error::NotFunded`] — one or both players have not deposited.
    /// - [`Error::InvalidState`] — match is not in `Active` state.
    pub fn submit_result_with_oracle_record(
        env: Env,
        match_id: u64,
        winner: Winner,
        game_id: String,
    ) -> Result<(), Error> {
        // Validate and execute payout via standard submit_result
        Self::submit_result(env.clone(), match_id, winner)?;

        // Store oracle record in a canonical location for audit trail
        env.storage()
            .persistent()
            .set(&DataKey::OracleRecord(match_id), &game_id);
        env.storage().persistent().extend_ttl(
            &DataKey::OracleRecord(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        Ok(())
    }

    /// Cancel a pending match and refund any deposits.
    /// Either player can cancel a pending match.
    pub fn cancel_match(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Pending {
            return Err(Error::InvalidState);
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
        m.completed_ledger = Some(env.ledger().sequence());
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

        Ok(())
    }

    /// Expire a pending match that has not been fully funded within MATCH_TIMEOUT_LEDGERS.
    /// Anyone can call this; funds are returned to whoever deposited.
    pub fn expire_match(env: Env, match_id: u64) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Pending {
            return Err(Error::InvalidState);
        }

        let elapsed = env.ledger().sequence().saturating_sub(m.created_ledger);
        let timeout = Self::current_match_timeout(&env);

        if elapsed < timeout {
            return Err(Error::MatchNotExpired);
        }

        let client = token::Client::new(&env, &m.token);

        if m.player1_deposited {
            client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
        }
        if m.player2_deposited {
            client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
        }

        m.state = MatchState::Cancelled;
        m.completed_ledger = Some(env.ledger().sequence());
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

    /// Return the oracle address currently configured on the contract.
    pub fn get_oracle(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)
    }

    fn current_match_timeout(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MatchTimeout)
            .unwrap_or(DEFAULT_MATCH_TIMEOUT_LEDGERS)
    }

    pub fn get_match_timeout(env: Env) -> Result<u32, Error> {
        Ok(Self::current_match_timeout(&env))
    }

    pub fn set_match_timeout(env: Env, timeout: u32) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let old_timeout = Self::current_match_timeout(&env);
        env.storage().instance().set(&DataKey::MatchTimeout, &timeout);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("timeout")),
            (old_timeout, timeout),
        );
        Ok(())
    }

    /// Propose a new admin. Current admin only. Stores pending admin without transferring authority.
    pub fn propose_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("propose")),
            new_admin,
        );
        Ok(())
    }

    /// Accept pending admin proposal. Pending admin only. Finalizes the transfer.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(Error::Unauthorized)?;
        pending_admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::Admin, &pending_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("xfer")),
            pending_admin,
        );
        Ok(())
    }

    /// Read a match by ID.
    pub fn get_match(env: Env, match_id: u64) -> Result<Match, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)
    }

    /// Check whether both players have deposited.
    pub fn is_funded(env: Env, match_id: u64) -> Result<bool, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        Ok(m.player1_deposited && m.player2_deposited)
    }

    /// Return the number of players who have deposited for a match (0, 1, or 2).
    pub fn get_depositor_count(env: Env, match_id: u64) -> Result<u32, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        Ok(Self::depositor_count(&m) as u32)
    }

    /// Return the total escrowed balance for a match (0, 1x, or 2x stake).
    pub fn get_escrow_balance(env: Env, match_id: u64) -> Result<i128, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        if m.state == MatchState::Completed || m.state == MatchState::Cancelled {
            return Ok(0);
        }
        // Count depositors explicitly — avoids fragile bool-to-integer casting.
        let depositors: i128 = Self::depositor_count(&m);
        Ok(depositors * m.stake_amount)
    }

    fn collect_matches_by_state(
        env: &Env,
        state: MatchState,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        let mut matches = soroban_sdk::vec![env];
        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0);

        for i in 0..count {
            if let Ok(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(i))
            {
                if m.state == state {
                    matches.push_back(m);
                }
            }
        }

        Ok(matches)
    }

    fn collect_matches_by_state_paginated(
        env: &Env,
        state: MatchState,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        let mut matches = soroban_sdk::vec![env];
        if limit == 0 {
            return Ok(matches);
        }

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0);
        let mut skipped = 0u32;
        let mut added = 0u32;

        for i in 0..count {
            if let Ok(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(i))
            {
                if m.state != state {
                    continue;
                }
                if skipped < offset {
                    skipped = skipped.saturating_add(1);
                    continue;
                }
                matches.push_back(m);
                added = added.saturating_add(1);
                if added >= limit {
                    break;
                }
            }
        }

        Ok(matches)
    }

    /// Return all matches currently in Pending state (created and awaiting deposits).
    pub fn get_pending_matches(env: Env) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state(&env, MatchState::Pending)
    }

    /// Return a paginated page of pending matches ordered by match ID ascending.
    pub fn get_pending_matches_paginated(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state_paginated(&env, MatchState::Pending, offset, limit)
    }

    /// Return all matches that are in Active state (fully funded).
    pub fn get_active_matches(env: Env) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state(&env, MatchState::Active)
    }

    /// Return all matches that are in Active state (fully funded).
    pub fn get_live_matches(env: Env) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::get_active_matches(env)
    }

    /// Return a paginated page of active matches ordered by match ID ascending.
    pub fn get_active_matches_paginated(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state_paginated(&env, MatchState::Active, offset, limit)
    }

    /// Alias for `get_active_matches_paginated` with a live-match naming convention.
    pub fn get_live_matches_paginated(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::get_active_matches_paginated(env, offset, limit)
    }

    /// Return the total number of matches created.
    pub fn get_match_count(env: Env) -> Result<u64, Error> {
        Ok(env
            .storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0))
    }

    /// Return all match IDs for a given player (past and present).
    pub fn get_player_matches(env: Env, player: Address) -> Result<soroban_sdk::Vec<u64>, Error> {
        Ok(env
            .storage()
            .persistent()
            .get(&DataKey::PlayerMatches(player))
            .unwrap_or_else(|| soroban_sdk::vec![&env]))
    }
}

#[cfg(test)]
mod tests;
