#![no_std]

pub mod errors;
pub mod types;

use errors::Error;
use soroban_sdk::{
    contract, contractimpl, symbol_short, token, Address, Env, String, Symbol, Vec,
};
use types::{
    BalanceSnapshot, DataKey, Dispute, DisputeState, Match, MatchState, Platform, SnapshotReason,
    Winner,
};

/// ~30 days at 5s/ledger. Used as the default TTL and expiration threshold.
const MATCH_TTL_LEDGERS: u32 = 518_400;

/// Fixed-size ring buffer capacity for balance snapshots per match. A normal
/// match lifecycle (created + 2 deposits + completed/cancelled) produces at
/// most 4 snapshots, so this leaves headroom while bounding storage growth
/// for matches that somehow generate more transitions.
const MAX_SNAPSHOTS_PER_MATCH: u32 = 8;

/// Default match expiration timeout used when no explicit timeout is configured.
pub const DEFAULT_MATCH_TIMEOUT_LEDGERS: u32 = MATCH_TTL_LEDGERS;

/// Minimum match timeout: 1 day (17,280 ledgers at 5s/ledger).
pub const MIN_MATCH_TIMEOUT_LEDGERS: u32 = 17_280;

/// Maximum match timeout: 90 days (1,555,200 ledgers at 5s/ledger).
pub const MAX_MATCH_TIMEOUT_LEDGERS: u32 = 1_555_200;

/// Default voting period for disputes: 1 day (17,280 ledgers at 5s/ledger).
pub const VOTING_PERIOD_LEDGERS: u32 = 17_280;

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
    pub fn initialize(env: Env, oracle: Address, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Oracle) {
            return Err(Error::AlreadyInitialized);
        }
        if oracle == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MatchCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::AllowlistEnforced, &false);
        env.storage().instance().set(&DataKey::AllowedTokenCount, &0u32);
        env.storage().instance().set(&DataKey::DisputePeriod, &0u32);
        env.events().publish(
            (Symbol::new(&env, "escrow"), symbol_short!("init")),
            (oracle, admin),
        );
        Ok(())
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

        let already_allowed: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowedToken(token.clone()))
            .unwrap_or(false);

        env.storage()
            .instance()
            .set(&DataKey::AllowedToken(token.clone()), &true);

        if !already_allowed {
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::AllowedTokenCount)
                .unwrap_or(0);
            let next_count = count.checked_add(1).ok_or(Error::Overflow)?;
            env.storage()
                .instance()
                .set(&DataKey::AllowedTokenCount, &next_count);
            if count == 0 {
                env.storage().instance().set(&DataKey::AllowlistEnforced, &true);
            }
            Self::append_allowed_token(&env, &token);
        } else {
            env.storage().instance().set(&DataKey::AllowlistEnforced, &true);
            Self::append_allowed_token(&env, &token);
        }

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

        let was_allowed = env
            .storage()
            .instance()
            .has(&DataKey::AllowedToken(token.clone()));
        env.storage().instance().remove(&DataKey::AllowedToken(token.clone()));

        if was_allowed {
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::AllowedTokenCount)
                .unwrap_or(0);
            let next_count = count.saturating_sub(1);
            env.storage()
                .instance()
                .set(&DataKey::AllowedTokenCount, &next_count);
            if next_count == 0 {
                env.storage().instance().set(&DataKey::AllowlistEnforced, &false);
            }
        }

        Self::remove_allowed_token_from_list(&env, &token);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("tok_rm")),
            token,
        );
        Ok(())
    }

    /// Check if a token is allowed.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        let key = DataKey::AllowedToken(token.clone());
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or(false)
    }

    /// Return the current allowlist as an ordered list.
    pub fn get_allowed_tokens(env: Env) -> Result<soroban_sdk::Vec<Address>, Error> {
        Ok(Self::get_allowed_token_list(&env))
    }

    fn get_allowed_token_list(env: &Env) -> soroban_sdk::Vec<Address> {
        if let Some(allowed_tokens) = env
            .storage()
            .persistent()
            .get(&DataKey::AllowedTokens)
        {
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::AllowedTokens, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
            allowed_tokens
        } else {
            soroban_sdk::vec![env]
        }
    }

    fn set_allowed_token_list(env: &Env, allowed_tokens: &soroban_sdk::Vec<Address>) {
        if allowed_tokens.is_empty() {
            env.storage().persistent().remove(&DataKey::AllowedTokens);
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::AllowedTokens, allowed_tokens);
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::AllowedTokens, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
        }
    }

    fn append_allowed_token(env: &Env, token: &Address) {
        let mut allowed_tokens: soroban_sdk::Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::AllowedTokens)
            .unwrap_or_else(|| soroban_sdk::vec![env]);
        if !allowed_tokens.iter().any(|existing| existing == *token) {
            allowed_tokens.push_back(token.clone());
            Self::set_allowed_token_list(env, &allowed_tokens);
        } else if env.storage().persistent().has(&DataKey::AllowedTokens) {
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::AllowedTokens, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
        }
    }

    fn remove_allowed_token_from_list(env: &Env, token: &Address) {
        let allowed_tokens = Self::get_allowed_token_list(env);
        if allowed_tokens.is_empty() {
            return;
        }

        let mut updated = soroban_sdk::vec![env];
        for existing in allowed_tokens.iter() {
            if existing != *token {
                updated.push_back(existing.clone());
            }
        }
        Self::set_allowed_token_list(env, &updated);
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

        // Reject if either player is invalid
        if player1 == player2 {
            return Err(Error::InvalidPlayers);
        }
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
            player1: player1.clone(),
            player2: player2.clone(),
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
        env.storage().persistent().extend_ttl(
            &DataKey::GameId(m.game_id.clone()),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

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

        Self::record_snapshot(&env, &m, SnapshotReason::Created);

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
                (Symbol::new(&env, "match"), symbol_short!("deposit")),
                (match_id, player.clone(), Some(m.state.clone())),
            );
            env.events().publish(
                (Symbol::new(&env, "match"), symbol_short!("activated")),
                match_id,
            );
            Self::append_active_match(&env, match_id);
        } else {
            env.events().publish(
                (Symbol::new(&env, "match"), symbol_short!("deposit")),
                (match_id, player.clone(), Option::<MatchState>::None),
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

        Self::record_snapshot(&env, &m, SnapshotReason::Deposit);

        Ok(())
    }

    /// Oracle submits the verified match result and triggers payout.
    ///
    /// If the dispute period is configured (non-zero), the payout is not
    /// executed immediately. Instead the match transitions to `PendingResult`
    /// and waits for the dispute window to elapse. Anyone may then call
    /// [`finalize_match`] to complete the payout, or a player may call
    /// [`dispute_oracle_result`] to challenge the result.
    ///
    /// If the dispute period is zero (default), the payout executes
    /// immediately — preserving the original immediate-payout behaviour.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        winner: Winner,
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
        oracle.require_auth();

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

        let dispute_period: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DisputePeriod)
            .unwrap_or(0);

        if dispute_period == 0 {
            // Immediate payout (original behaviour)
            Self::execute_payout(&env, &m, &winner)?;
            Self::remove_active_match(&env, match_id);

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

            Self::record_snapshot(&env, &m, SnapshotReason::Completed);

            let topics = (Symbol::new(&env, "match"), symbol_short!("completed"));
            env.events().publish(topics, (match_id, winner));

            Ok(())
        } else {
            // Delayed payout: store the pending result and set dispute deadline
            let deadline = env
                .ledger()
                .sequence()
                .checked_add(dispute_period)
                .ok_or(Error::Overflow)?;

            m.state = MatchState::PendingResult;

            env.storage()
                .persistent()
                .set(&DataKey::Match(match_id), &m);
            env.storage().persistent().extend_ttl(
                &DataKey::Match(match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );

            env.storage()
                .persistent()
                .set(&DataKey::PendingWinner(match_id), &winner);
            env.storage().persistent().extend_ttl(
                &DataKey::PendingWinner(match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );

            env.storage()
                .persistent()
                .set(&DataKey::ResultDeadline(match_id), &deadline);
            env.storage().persistent().extend_ttl(
                &DataKey::ResultDeadline(match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );

            Self::record_snapshot(&env, &m, SnapshotReason::ResultSubmitted);

            env.events().publish(
                (Symbol::new(&env, "match"), Symbol::new(&env, "pending_result")),
                (match_id, winner, deadline),
            );

            Ok(())
        }
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
        // Validate and execute payout via standard submit_result (handles oracle auth).
        Self::submit_result(env.clone(), match_id, winner)?;

        // Store oracle record in a canonical location for audit trail.
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

        if m.state == MatchState::Active {
            return Err(Error::MatchAlreadyActive);
        }
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

        Self::record_snapshot(&env, &m, SnapshotReason::Cancelled);

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

        Self::record_snapshot(&env, &m, SnapshotReason::Cancelled);

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

    fn get_active_match_ids(env: &Env) -> soroban_sdk::Vec<u64> {
        if let Some(active_matches) = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveMatches)
        {
            env.storage()
                .persistent()
                .extend_ttl(&DataKey::ActiveMatches, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
            active_matches
        } else {
            soroban_sdk::vec![env]
        }
    }

    fn set_active_match_ids(env: &Env, active_matches: &soroban_sdk::Vec<u64>) {
        env.storage()
            .persistent()
            .set(&DataKey::ActiveMatches, active_matches);
        env.storage()
            .persistent()
            .extend_ttl(&DataKey::ActiveMatches, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    }

    fn append_active_match(env: &Env, match_id: u64) {
        let mut active_matches = Self::get_active_match_ids(env);
        active_matches.push_back(match_id);
        Self::set_active_match_ids(env, &active_matches);
    }

    fn remove_active_match(env: &Env, match_id: u64) {
        let active_matches = Self::get_active_match_ids(env);
        if active_matches.is_empty() {
            return;
        }

        let mut updated = soroban_sdk::vec![env];
        for id in active_matches.iter() {
            if id != match_id {
                updated.push_back(id);
            }
        }

        Self::set_active_match_ids(env, &updated);
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

        if timeout < MIN_MATCH_TIMEOUT_LEDGERS || timeout > MAX_MATCH_TIMEOUT_LEDGERS {
            return Err(Error::InvalidTimeout);
        }

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
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        Ok(m)
    }

    /// Check whether both players have deposited their stakes.
    /// 
    /// This returns `true` as long as both `player1_deposited` and `player2_deposited` flags
    /// are set, regardless of match state. Specifically, it remains `true` after payout
    /// (when state transitions to `Completed`) because the deposit flags are never cleared.
    /// 
    /// This indicates historical deposit status, not current escrowed funds.
    /// To check if funds are currently held in escrow, use [`is_currently_escrowed`].
    pub fn is_funded(env: Env, match_id: u64) -> Result<bool, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
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
        Ok(Self::escrow_balance_of(&m))
    }

    fn depositor_count(m: &Match) -> i128 {
        let mut count: i128 = 0;
        if m.player1_deposited { count += 1; }
        if m.player2_deposited { count += 1; }
        count
    }

    /// Tokens currently held in escrow for a match. Zero once the match has
    /// reached a terminal state, since funds have been disbursed by then.
    fn escrow_balance_of(m: &Match) -> i128 {
        if m.state == MatchState::Completed || m.state == MatchState::Cancelled {
            0
        } else {
            Self::depositor_count(m) * m.stake_amount
        }
    }

    // ── Payout helper ────────────────────────────────────────────────────────

    /// Execute the payout for a match based on the winner. Transfers tokens
    /// from the contract to the winner(s).
    fn execute_payout(env: &Env, m: &Match, winner: &Winner) -> Result<(), Error> {
        let client = token::Client::new(env, &m.token);
        let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
        match winner {
            Winner::Player1 => {
                client.transfer(&env.current_contract_address(), &m.player1, &pot);
            }
            Winner::Player2 => {
                client.transfer(&env.current_contract_address(), &m.player2, &pot);
            }
            Winner::Draw => {
                client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
                client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
            }
        }
        Ok(())
    }

    /// Finalize an undisputed match after the dispute period has elapsed.
    /// Anyone may call this once `result_deadline` has passed and no dispute
    /// was raised.
    pub fn finalize_match(env: Env, match_id: u64) -> Result<(), Error> {
        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::PendingResult {
            return Err(Error::MatchNotInPendingResult);
        }

        let deadline: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ResultDeadline(match_id))
            .ok_or(Error::PendingResultNotFound)?;

        if env.ledger().sequence() < deadline {
            return Err(Error::DisputePeriodNotElapsed);
        }

        // Ensure no active dispute exists for this match
        // (dispute creates a separate resolution path)
        if env.storage().persistent().has(&DataKey::MatchDispute(match_id)) {
            return Err(Error::DisputeAlreadyRaised);
        }

        let winner: Winner = env
            .storage()
            .persistent()
            .get(&DataKey::PendingWinner(match_id))
            .ok_or(Error::PendingResultNotFound)?;
        Self::execute_payout(&env, &m, &winner)?;
        Self::remove_active_match(&env, match_id);

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

        Self::record_snapshot(&env, &m, SnapshotReason::Finalized);

        env.events().publish(
            (Symbol::new(&env, "match"), Symbol::new(&env, "finalized")),
            (match_id, winner),
        );

        Ok(())
    }

    /// Raise a dispute against an oracle-submitted result.
    ///
    /// Any player (either player1 or player2 of the match) may call this
    /// before the dispute deadline elapses. An `evidence_hash` must be
    /// provided as a reference to off-chain evidence.
    ///
    /// Once a dispute is raised, the match must be resolved via voting
    /// instead of the normal `finalize_match` path.
    pub fn dispute_oracle_result(
        env: Env,
        match_id: u64,
        disputer: Address,
        evidence_hash: String,
    ) -> Result<u64, Error> {
        disputer.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::PendingResult {
            return Err(Error::MatchNotInPendingResult);
        }

        // Only match participants may dispute
        if disputer != m.player1 && disputer != m.player2 {
            return Err(Error::Unauthorized);
        }

        let deadline: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ResultDeadline(match_id))
            .ok_or(Error::PendingResultNotFound)?;
        if env.ledger().sequence() >= deadline {
            return Err(Error::DisputePeriodNotElapsed);
        }

        if evidence_hash.len() == 0 {
            return Err(Error::InvalidEvidenceHash);
        }

        // Check if a dispute already exists for this match
        if env.storage().persistent().has(&DataKey::MatchDispute(match_id)) {
            return Err(Error::DisputeAlreadyRaised);
        }

        let dispute_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeCount)
            .unwrap_or(0);

        let voting_deadline = env
            .ledger()
            .sequence()
            .checked_add(VOTING_PERIOD_LEDGERS)
            .ok_or(Error::Overflow)?;

        let dispute = Dispute {
            id: dispute_id,
            match_id,
            disputer: disputer.clone(),
            evidence_hash: evidence_hash.clone(),
            yes_votes: 0,
            no_votes: 0,
            voting_deadline,
            state: DisputeState::Active,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        env.storage().persistent().extend_ttl(
            &DataKey::Dispute(dispute_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        // Store a mapping from match_id -> dispute_id for quick lookup
        env.storage()
            .persistent()
            .set(&DataKey::MatchDispute(match_id), &dispute_id);
        env.storage().persistent().extend_ttl(
            &DataKey::MatchDispute(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let next_id = dispute_id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::DisputeCount, &next_id);

        env.events().publish(
            (Symbol::new(&env, "dispute"), Symbol::new(&env, "created")),
            (dispute_id, match_id, disputer, evidence_hash),
        );

        Ok(dispute_id)
    }

    /// Vote on an active dispute.
    ///
    /// Only addresses that hold a positive balance of the match's escrow
    /// token may vote (`stakers`). `vote` is `true` to overturn the oracle
    /// result, `false` to uphold it.
    ///
    /// Each address may only vote once per dispute.
    pub fn vote_on_dispute(
        env: Env,
        dispute_id: u64,
        voter: Address,
        vote: bool,
    ) -> Result<(), Error> {
        voter.require_auth();

        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(Error::DisputeNotFound)?;

        if dispute.state != DisputeState::Active {
            return Err(Error::DisputeAlreadyResolved);
        }

        if env.ledger().sequence() >= dispute.voting_deadline {
            return Err(Error::VotingPeriodElapsed);
        }

        // Check voter hasn't already voted
        let vote_key = DataKey::DisputeVote(dispute_id, voter.clone());
        if env.storage().persistent().has(&vote_key) {
            return Err(Error::AlreadyVoted);
        }

        // Verify voter holds a positive balance of the match's escrow token
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(dispute.match_id))
            .ok_or(Error::MatchNotFound)?;
        let client = token::Client::new(&env, &m.token);
        let balance = client.balance(&voter);
        if balance <= 0 {
            return Err(Error::NotStaker);
        }

        // Record vote
        env.storage()
            .persistent()
            .set(&vote_key, &vote);
        env.storage().persistent().extend_ttl(
            &vote_key,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        // Tally vote
        if vote {
            dispute.yes_votes = dispute.yes_votes.saturating_add(balance);
        } else {
            dispute.no_votes = dispute.no_votes.saturating_add(balance);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        env.storage().persistent().extend_ttl(
            &DataKey::Dispute(dispute_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "dispute"), Symbol::new(&env, "voted")),
            (dispute_id, voter, vote),
        );

        Ok(())
    }

    /// Resolve a dispute after the voting period has elapsed.
    ///
    /// Executes payout based on the majority vote:
    /// - If the majority votes to overturn (`yes_votes > no_votes`), stakes
    ///   are refunded to both players (draw outcome).
    /// - If the majority upholds (`no_votes >= yes_votes`), the original
    ///   oracle result stands and the winner receives the full pot.
    pub fn resolve_dispute_by_vote(env: Env, dispute_id: u64) -> Result<(), Error> {
        let mut dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(Error::DisputeNotFound)?;

        if dispute.state != DisputeState::Active {
            return Err(Error::DisputeAlreadyResolved);
        }

        if env.ledger().sequence() < dispute.voting_deadline {
            return Err(Error::VotingPeriodNotElapsed);
        }

        let match_id = dispute.match_id;
        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::PendingResult {
            return Err(Error::MatchNotInPendingResult);
        }

        let pending_winner: Winner = env
            .storage()
            .persistent()
            .get(&DataKey::PendingWinner(match_id))
            .ok_or(Error::PendingResultNotFound)?;

        let winner = if dispute.yes_votes > dispute.no_votes {
            // Overturned: refund both players (draw outcome)
            dispute.state = DisputeState::ResolvedOverturned;
            Winner::Draw
        } else {
            // Upheld: original oracle result stands
            dispute.state = DisputeState::ResolvedUpheld;
            pending_winner
        };

        Self::execute_payout(&env, &m, &winner)?;
        Self::remove_active_match(&env, match_id);

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

        env.storage()
            .persistent()
            .set(&DataKey::Dispute(dispute_id), &dispute);
        env.storage().persistent().extend_ttl(
            &DataKey::Dispute(dispute_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        Self::record_snapshot(&env, &m, SnapshotReason::Finalized);

        env.events().publish(
            (Symbol::new(&env, "dispute"), Symbol::new(&env, "resolved")),
            (dispute_id, match_id, dispute.state, winner),
        );

        Ok(())
    }

    /// Set the dispute period in ledgers. Admin only.
    /// Set to 0 to disable the dispute period (immediate payout).
    pub fn set_dispute_period(env: Env, period: u32) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::DisputePeriod, &period);
        env.events().publish(
            (Symbol::new(&env, "admin"), Symbol::new(&env, "dispute_period")),
            period,
        );
        Ok(())
    }

    /// Return the current dispute period in ledgers.
    pub fn get_dispute_period(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::DisputePeriod)
            .unwrap_or(0)
    }

    /// Get a dispute by ID.
    pub fn get_dispute(env: Env, dispute_id: u64) -> Result<Dispute, Error> {
        let dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(Error::DisputeNotFound)?;
        env.storage().persistent().extend_ttl(
            &DataKey::Dispute(dispute_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        Ok(dispute)
    }

    /// Return the dispute ID for a match, if one exists.
    pub fn get_match_dispute_id(env: Env, match_id: u64) -> Result<u64, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::MatchDispute(match_id))
            .ok_or(Error::DisputeNotFound)
    }

    // ── Balance snapshots ───────────────────────────────────────────────────

    /// Best-effort token symbol lookup for snapshots.
    ///
    /// `create_match` deliberately accepts any address as `token` without
    /// verifying it's a deployed token contract — validity is only enforced
    /// later, when `deposit` actually transfers funds. Snapshots must not
    /// break that contract, so this uses `try_invoke_contract` and falls
    /// back to an empty string if the address isn't a callable token (or
    /// isn't a contract at all) rather than panicking.
    fn fetch_token_symbol(env: &Env, token: &Address) -> String {
        match env.try_invoke_contract::<String, Error>(
            token,
            &Symbol::new(env, "symbol"),
            soroban_sdk::vec![env],
        ) {
            Ok(Ok(symbol)) => symbol,
            _ => String::from_str(env, ""),
        }
    }

    /// Record a balance snapshot for `m` at a lifecycle transition.
    ///
    /// Snapshots are stored in a fixed-size ring buffer keyed by
    /// `DataKey::Snapshot(match_id, slot)` where `slot = index %
    /// MAX_SNAPSHOTS_PER_MATCH`. Once a match's snapshot count exceeds the
    /// buffer capacity, the oldest entry is silently overwritten — this is
    /// the storage-pruning mechanism. `DataKey::SnapshotCount` tracks the
    /// total ever recorded so callers can detect that pruning occurred.
    fn record_snapshot(env: &Env, m: &Match, reason: SnapshotReason) {
        let token_symbol = Self::fetch_token_symbol(env, &m.token);
        let escrow_balance = Self::escrow_balance_of(m);

        let index: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SnapshotCount(m.id))
            .unwrap_or(0);
        let slot = index % MAX_SNAPSHOTS_PER_MATCH;

        let snapshot = BalanceSnapshot {
            match_id: m.id,
            index,
            reason,
            ledger: env.ledger().sequence(),
            token: m.token.clone(),
            token_symbol,
            stake_amount: m.stake_amount,
            escrow_balance,
            player1_deposited: m.player1_deposited,
            player2_deposited: m.player2_deposited,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Snapshot(m.id, slot), &snapshot);
        env.storage().persistent().extend_ttl(
            &DataKey::Snapshot(m.id, slot),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let next_index = index.saturating_add(1);
        env.storage()
            .persistent()
            .set(&DataKey::SnapshotCount(m.id), &next_index);
        env.storage().persistent().extend_ttl(
            &DataKey::SnapshotCount(m.id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(env, "match"), symbol_short!("snapshot")),
            (m.id, index, escrow_balance),
        );
    }

    /// Authorize a snapshot query. Returns `Ok(true)` for the admin (full
    /// access to exact amounts), `Ok(false)` for either player in the match
    /// (partial access — amounts redacted), or `Err(Unauthorized)` otherwise.
    fn authorize_snapshot_query(env: &Env, caller: &Address, m: &Match) -> Result<bool, Error> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if *caller == admin {
            Ok(true)
        } else if *caller == m.player1 || *caller == m.player2 {
            Ok(false)
        } else {
            Err(Error::Unauthorized)
        }
    }

    /// Zero out sensitive amount fields for non-admin callers.
    fn redact_snapshot(mut snapshot: BalanceSnapshot) -> BalanceSnapshot {
        snapshot.stake_amount = 0;
        snapshot.escrow_balance = 0;
        snapshot
    }

    /// Return the full snapshot history for a match, oldest first.
    ///
    /// Only the admin sees exact `stake_amount`/`escrow_balance` values; the
    /// match's players may also call this but receive amounts redacted to 0.
    /// Any other caller is rejected with `Error::Unauthorized`.
    pub fn get_balance_snapshots(
        env: Env,
        caller: Address,
        match_id: u64,
    ) -> Result<Vec<BalanceSnapshot>, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        let full_access = Self::authorize_snapshot_query(&env, &caller, &m)?;

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SnapshotCount(match_id))
            .unwrap_or(0);
        let available = count.min(MAX_SNAPSHOTS_PER_MATCH);
        let start = count.saturating_sub(available);

        let mut result = soroban_sdk::vec![&env];
        for i in start..count {
            let slot = i % MAX_SNAPSHOTS_PER_MATCH;
            if let Some(snapshot) = env
                .storage()
                .persistent()
                .get::<DataKey, BalanceSnapshot>(&DataKey::Snapshot(match_id, slot))
            {
                result.push_back(if full_access {
                    snapshot
                } else {
                    Self::redact_snapshot(snapshot)
                });
            }
        }
        Ok(result)
    }

    /// Return the most recently recorded snapshot for a match.
    ///
    /// Same access rules as [`Self::get_balance_snapshots`]: admin sees exact
    /// amounts, players see redacted amounts, anyone else is unauthorized.
    pub fn get_latest_snapshot(
        env: Env,
        caller: Address,
        match_id: u64,
    ) -> Result<BalanceSnapshot, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        let full_access = Self::authorize_snapshot_query(&env, &caller, &m)?;

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SnapshotCount(match_id))
            .unwrap_or(0);
        if count == 0 {
            return Err(Error::SnapshotNotFound);
        }
        let slot = (count - 1) % MAX_SNAPSHOTS_PER_MATCH;
        let snapshot: BalanceSnapshot = env
            .storage()
            .persistent()
            .get(&DataKey::Snapshot(match_id, slot))
            .ok_or(Error::SnapshotNotFound)?;
        Ok(if full_access {
            snapshot
        } else {
            Self::redact_snapshot(snapshot)
        })
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

        for match_id in 0..count {
            if let Some(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(match_id))
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

        for match_id in 0..count {
            if let Some(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(match_id))
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
        // Extend ActiveMatches TTL if the key exists (keeps the index alive on reads)
        if env.storage().persistent().has(&DataKey::ActiveMatches) {
            env.storage().persistent().extend_ttl(
                &DataKey::ActiveMatches,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }
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
    ///
    /// Deprecated: use `get_player_matches_paginated` to avoid unbounded return sizes.
    pub fn get_player_matches(env: Env, player: Address) -> Result<soroban_sdk::Vec<u64>, Error> {
        let key = DataKey::PlayerMatches(player.clone());
        let matches: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);
        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
        }
        Ok(matches)
    }

    /// Return a page of match IDs for a given player.
    pub fn get_player_matches_paginated(
        env: Env,
        player: Address,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<u64>, Error> {
        let player_matches: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerMatches(player))
            .unwrap_or_else(|| soroban_sdk::vec![&env]);

        if limit == 0 {
            return Ok(soroban_sdk::vec![&env]);
        }

        let mut page = soroban_sdk::vec![&env];
        let mut skipped = 0u32;
        let total = player_matches.len();

        for i in 0..total {
            if skipped < offset {
                skipped = skipped.saturating_add(1);
                continue;
            }
            page.push_back(player_matches.get(i).unwrap());
            if page.len() >= limit {
                break;
            }
        }

        Ok(page)
    }

    /// Update the oracle address — admin only.
    pub fn update_oracle(env: Env, new_oracle: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        if new_oracle == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }
        let old_oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;
        env.storage().instance().set(&DataKey::Oracle, &new_oracle);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("oracle_up")),
            (old_oracle, new_oracle),
        );
        Ok(())
    }

    /// Direct admin transfer (single-step). Current admin only.
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("xfer")),
            (admin, new_admin),
        );
        Ok(())
    }

    /// Returns true if the allowlist is enforced (at least one token has been added).
    pub fn is_allowlist_enforced(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AllowlistEnforced)
            .unwrap_or(false)
    }

    /// Returns true if the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Returns true if the contract has been initialized.
    pub fn is_initialized(env: Env) -> bool {
        extend_instance_ttl(&env);
        env.storage().instance().has(&DataKey::Oracle)
    }

}

#[cfg(test)]
mod tests;
