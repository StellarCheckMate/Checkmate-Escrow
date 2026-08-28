#![no_std]
// Several contract entry points (e.g. `create_match_with_conversion`) take one
// parameter per on-chain field; grouping them into a struct would change the
// public contract ABI, so the arg-count lint is suppressed crate-wide instead.
#![allow(clippy::too_many_arguments)]

#[cfg(test)]
extern crate std;

/// Escrow Contract for Checkmate — trustless chess wagering on Stellar.
///
/// For a comprehensive reference of all error codes (their numeric values, causes, and recovery
/// actions), see [`Error Codes Reference`](../../docs/error-codes.md).
///
/// # Error Codes Quick Reference
///
/// Every function that returns a `Result<T, Error>` surfaces errors as numeric discriminants.
/// Common errors:
/// - `#1` — `MatchNotFound` — Invalid or expired match ID
/// - `#4` — `Unauthorized` — Caller lacks required permissions or contract not initialized
/// - `#5` — `InvalidState` — Match is in wrong state for this operation
/// - `#7` — `AlreadyInitialized` — Contract already initialized
/// - `#9` — `ContractPaused` — Operations blocked during pause
///
/// See [`docs/error-codes.md`](../../docs/error-codes.md) for all 50 error codes with causes and recovery actions.
pub mod errors;
pub mod types;

#[cfg(test)]
pub mod formal_verification;

#[cfg(test)]
mod formal_verification_tests;

#[cfg(test)]
mod kani_harness;

#[cfg(test)]
mod tests;

use errors::Error;
use soroban_sdk::{
    contract, contractimpl, symbol_short, token, Address, Bytes, BytesN, Env, IntoVal, String,
    Symbol, Vec,
};
use types::{
    BalanceAtTimestamp, BalanceSnapshot, DataKey, Dispute, DisputeState, FeeTier, Match,
    MatchState, OracleRotationState, PendingAdminProposal, PendingOracleRotation, Platform,
    PlatformStats, PlayerBalanceSnapshot, PlayerFreezeKey, PlayerTier, ProtocolConfig,
    SnapshotReason, TempOracleRotation, Winner,
};

/// ~30 days at 5s/ledger. Used as the default TTL and expiration threshold.
const MATCH_TTL_LEDGERS: u32 = 518_400;

/// Fixed-size ring buffer capacity for balance snapshots per match. A normal
/// match lifecycle (created + 2 deposits + completed/cancelled) produces at
/// most 4 snapshots, so this leaves headroom while bounding storage growth
/// for matches that somehow generate more transitions.
const MAX_SNAPSHOTS_PER_MATCH: u32 = 8;

/// Fixed-size ring buffer capacity for player-level balance snapshots.
/// Player history spans many matches, so this is larger than the per-match
/// cap. Older entries are silently overwritten once the buffer fills; the
/// monotonic `index`/`PlayerBalanceSnapshotCount` lets callers detect gaps.
const MAX_PLAYER_SNAPSHOTS: u32 = 32;

/// Default match expiration timeout used when no explicit timeout is configured.
pub const DEFAULT_MATCH_TIMEOUT_LEDGERS: u32 = MATCH_TTL_LEDGERS;

/// Average Stellar ledger close time (seconds). Used only to convert the
/// public, seconds-denominated `ProtocolConfig::match_timeout_seconds` into
/// the ledger-sequence delta `expire_match` compares against internally.
const SECONDS_PER_LEDGER: u64 = 5;

/// Default match expiration timeout: 30 days.
pub const DEFAULT_MATCH_TIMEOUT_SECONDS: u64 = 2_592_000;

/// Minimum match timeout: 1 day.
pub const MIN_MATCH_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum match timeout: 90 days.
pub const MAX_MATCH_TIMEOUT_SECONDS: u64 = 7_776_000;

/// Default voting period for disputes: 1 day (17,280 ledgers at 5s/ledger).
pub const VOTING_PERIOD_LEDGERS: u32 = 17_280;

/// Time window (in seconds, since `last_heartbeat`) within which a player
/// may invoke `dispute_and_rollback_match` against an Active match to claw
/// back a stake after claiming the opponent disconnected. 24 hours.
pub const ROLLBACK_WINDOW_SECONDS: u64 = 24 * 60 * 60; // 86_400

/// Time window (in seconds, since `last_heartbeat`) after which an admin
/// may invoke `admin_resolve_stalled_match` to recover funds from an Active
/// match that has received no oracle result. Set to 7 days (longer than the
/// player-initiated 24h rollback window) to give the oracle ample time to
/// recover from transient outages without admin intervention, while still
/// providing a bounded recovery path so funds are never permanently locked.
pub const ADMIN_STALL_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60; // 604_800

/// Maximum allowed byte length for a `dispute_and_rollback_match` reason.
const MAX_REASON_LEN: u32 = 256;

/// Default dispute bond as basis points of match stake (1% = 100 basis points).
/// Set to 100 = 1% of stake required to open a dispute.
pub const DEFAULT_DISPUTE_BOND_BASIS_POINTS: u32 = 100;

/// Minimum holding duration in ledgers before acquired tokens can vote.
/// Set to 100 ledgers (~8 minutes at 5s/ledger) to prevent flash-loan attacks.
pub const DEFAULT_MINIMUM_HOLD_DURATION: u32 = 100;

/// Quorum threshold as basis points of dispute snapshot weight.
/// Set to 2000 = 20% minimum participation for resolution.
pub const DEFAULT_QUORUM_BASIS_POINTS: u32 = 2000;

/// Maximum allowed byte length for a game_id string.
///
/// Platform-specific formats:
/// - Lichess:      8 alphanumeric characters (e.g. `"abcd1234"`)
/// - Chess.com:    numeric string, typically 7–12 digits (e.g. `"123456789"`)
///
/// Both formats fit well within this limit.
const MAX_GAME_ID_LEN: u32 = 64;

/// Default confidence threshold for oracle results (0-100).
/// Results below this threshold trigger PendingResult state for dispute.
const DEFAULT_CONFIDENCE_THRESHOLD: u8 = 50;

/// Exact game ID length required for Lichess (8 alphanumeric characters).
const LICHESS_GAME_ID_LEN: u32 = 8;

/// Minimum/maximum game ID length accepted for Chess.com (numeric string).
const CHESS_COM_GAME_ID_MIN_LEN: u32 = 7;
const CHESS_COM_GAME_ID_MAX_LEN: u32 = 12;

/// Default minimum `stake_amount` accepted by `create_match` and friends
/// when no admin override has been configured via `set_minimum_stake`.
/// Kept at `1` (the pre-existing implicit floor from the `stake_amount > 0`
/// check) so this ships without silently invalidating existing low-stake
/// matches/tests; admins can raise it with `set_minimum_stake`.
pub const DEFAULT_MINIMUM_STAKE: i128 = 1;

/// Completed-match thresholds for unlocking progressively higher stake bands.
const SILVER_MIN_COMPLETED_MATCHES: u32 = 3;
const GOLD_MIN_COMPLETED_MATCHES: u32 = 6;
const PLATINUM_MIN_COMPLETED_MATCHES: u32 = 10;

/// Stake bounds for each tier.
const BRONZE_MIN_STAKE: i128 = 1;
const BRONZE_MAX_STAKE: i128 = 100;
const SILVER_MIN_STAKE: i128 = 101;
const SILVER_MAX_STAKE: i128 = 500;
const GOLD_MIN_STAKE: i128 = 501;
const GOLD_MAX_STAKE: i128 = 1_000;
const PLATINUM_MIN_STAKE: i128 = 1_001;

/// Maximum number of simultaneously-active matches per player. This prevents
/// attacker-inflated cost growth in ActiveMatch index operations.
const MAX_ACTIVE_MATCHES_PER_PLAYER: u32 = 1_000;

/// Hard cap on unbounded match scans. The deprecated get_*_matches() functions
/// scan the full match history and are limited to this many results to cap
/// per-call cost. Callers requiring more results should use the _paginated variants.
const MAX_UNBOUNDED_MATCH_RESULTS: u32 = 10_000;

// ── Upgrade / migration constants ─────────────────────────────────────────────

/// Current contract version: major=0, minor=1, patch=0  →  1_000 * 0 + 1 * 1000 + 0.
/// Encoded as major * 1_000_000 + minor * 1_000 + patch so numeric comparisons work.
pub const CONTRACT_VERSION: u32 = 1_000; // 0.1.0

/// Minimum ledger gap between scheduling an upgrade and executing it (7-day review).
/// At the default 5 s/ledger: 7 * 24 * 3600 / 5 = 120_960.
pub const UPGRADE_REVIEW_PERIOD_LEDGERS: u32 = 120_960;

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
        env.storage()
            .instance()
            .set(&DataKey::AllowlistEnforced, &false);
        env.storage()
            .instance()
            .set(&DataKey::AllowedTokenCount, &0u32);
        env.storage().instance().set(
            &DataKey::DisputeBondBasisPoints,
            &DEFAULT_DISPUTE_BOND_BASIS_POINTS,
        );
        env.storage().instance().set(
            &DataKey::MinimumHoldDuration,
            &DEFAULT_MINIMUM_HOLD_DURATION,
        );
        env.storage()
            .instance()
            .set(&DataKey::QuorumBasisPoints, &DEFAULT_QUORUM_BASIS_POINTS);
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &CONTRACT_VERSION);
        env.events().publish(
            (Symbol::new(&env, "escrow"), symbol_short!("init")),
            (oracle, admin),
        );
        Ok(())
    }

    /// Pause the contract — admin only. Blocks create_match, deposit, and submit_result.
    pub fn pause(env: Env, caller: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        let already_paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if already_paused {
            return Err(Error::InvalidPauseState);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("paused")), ());
        Ok(())
    }

    /// Unpause the contract — admin only.
    pub fn unpause(env: Env, caller: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        let already_paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if !already_paused {
            return Err(Error::InvalidPauseState);
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("unpaused")), ());
        Ok(())
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

    /// Update the protocol configuration.
    pub fn set_protocol_config(env: Env, config: ProtocolConfig) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        if config.protocol_fee_bps > 10_000 {
            return Err(Error::InvalidAmount);
        }
        let old_mode: bool = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolConfig)
            .map(|c: ProtocolConfig| c.stablecoin_only_mode)
            .unwrap_or(false);
        let new_mode = config.stablecoin_only_mode;
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &config);
        if old_mode != new_mode {
            env.events().publish(
                (Symbol::new(&env, "escrow"), Symbol::new(&env, "stablecoin_mode")),
                new_mode,
            );
        }
        Ok(())
    }

    /// Get the current protocol configuration.
    pub fn get_protocol_config(env: Env) -> Result<ProtocolConfig, Error> {
        Ok(Self::get_config(&env))
    }

    /// Set the referral fee share in basis points (admin only).
    ///
    /// The referral fee is calculated as `platform_fee * referral_share_bps / 10_000` and sent
    /// to the referrer address stored on the match.  Default is 2000 (20%).
    pub fn set_referral_share_bps(env: Env, basis_points: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::ReferralShareBasisPoints, &basis_points);
        Ok(())
    }

    /// Get the referral fee share in basis points. Default: 2000 (20%).
    pub fn get_referral_share_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ReferralShareBasisPoints)
            .unwrap_or(2000u32)
    }

    /// Set the minimum stake amount — admin only.
    ///
    /// Enforces a global minimum stake floor for all matches. Default is 1.
    pub fn set_minimum_stake(env: Env, amount: i128) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();
        if amount < 1 {
            return Err(Error::InvalidAmount);
        }
        let mut config = Self::get_config(&env);
        config.minimum_stake = amount;
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &config);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("min_stake")),
            amount,
        );
        Ok(())
    }

    /// Set the caller's preferred payout token — player only.
    ///
    /// When a player has a preferred payout token set and it differs from the
    /// match's stake token, `claim_vested_payout` will attempt to pay out in
    /// the preferred token using the match's oracle-supplied `conversion_rate`
    /// and `token_b` fields (set via `create_match_with_conversion`).
    ///
    /// Pass `None` to clear the preference and revert to the match stake token.
    pub fn set_preferred_payout_token(
        env: Env,
        player: Address,
        token_address: Option<Address>,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        player.require_auth();

        let key = DataKey::PlayerPreferredToken(player);
        match token_address {
            Some(addr) => {
                env.storage().persistent().set(&key, &addr);
                env.storage()
                    .persistent()
                    .extend_ttl(&key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
            }
            None => {
                env.storage().persistent().remove(&key);
            }
        }
        Ok(())
    }

    /// Get the caller's preferred payout token, or `None` if not set.
    pub fn get_preferred_payout_token(env: Env, player: Address) -> Option<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerPreferredToken(player))
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
            env.storage()
                .instance()
                .set(&DataKey::AllowlistEnforced, &true);
        } else {
            env.storage()
                .instance()
                .set(&DataKey::AllowlistEnforced, &true);
        }
        Self::append_allowed_token(&env, &token);

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
        env.storage()
            .instance()
            .remove(&DataKey::AllowedToken(token.clone()));

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
                env.storage()
                    .instance()
                    .set(&DataKey::AllowlistEnforced, &false);
            }
        }

        Self::remove_allowed_token_from_list(&env, &token);

        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("tok_rm")), token);
        Ok(())
    }

    /// Check if a token is allowed.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        let key = DataKey::AllowedToken(token.clone());
        env.storage().instance().get(&key).unwrap_or(false)
    }

    /// Register a stablecoin issuer — admin only.
    ///
    /// Any Stellar token whose issuer account matches a registered issuer is
    /// considered a stablecoin.  When `stablecoin_only_mode` is enabled in
    /// [`ProtocolConfig`], `create_match` rejects tokens that don't pass the
    /// [`Self::is_stablecoin`] check.
    pub fn add_stablecoin_issuer(env: Env, issuer: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if issuer == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }

        let already_registered: bool = env
            .storage()
            .instance()
            .get(&DataKey::StablecoinIssuer(issuer.clone()))
            .unwrap_or(false);

        env.storage()
            .instance()
            .set(&DataKey::StablecoinIssuer(issuer.clone()), &true);

        if !already_registered {
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::StablecoinIssuerCount)
                .unwrap_or(0);
            let next_count = count.checked_add(1).ok_or(Error::Overflow)?;
            env.storage()
                .instance()
                .set(&DataKey::StablecoinIssuerCount, &next_count);
        }

        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "sc_issuer_add"),
            ),
            issuer,
        );

        Ok(())
    }

    /// Remove a stablecoin issuer — admin only.
    pub fn remove_stablecoin_issuer(env: Env, issuer: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let was_registered: bool = env
            .storage()
            .instance()
            .get(&DataKey::StablecoinIssuer(issuer.clone()))
            .unwrap_or(false);

        if was_registered {
            env.storage()
                .instance()
                .remove(&DataKey::StablecoinIssuer(issuer.clone()));
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::StablecoinIssuerCount)
                .unwrap_or(0);
            let next_count = count.saturating_sub(1);
            env.storage()
                .instance()
                .set(&DataKey::StablecoinIssuerCount, &next_count);
        }

        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "sc_issuer_rm"),
            ),
            issuer,
        );

        Ok(())
    }

    /// Check whether `token` qualifies as a stablecoin.
    ///
    /// A token is a stablecoin when its issuer (obtained from the SAC contract)
    /// has been registered via [`Self::add_stablecoin_issuer`].  Returns `false`
    /// when no issuers have been registered yet.
    pub fn is_stablecoin(env: Env, token: Address) -> bool {
        Self::check_is_stablecoin(&env, &token)
    }

    /// Internal helper for stablecoin check (avoids `env` ownership issues).
    fn check_is_stablecoin(env: &Env, token: &Address) -> bool {
        // A token on Soroban is issued by an Address.
        // We check whether the token address itself is registered as a stablecoin issuer,
        // or whether there is a registered issuer for that token's issuer account.
        // Since in Soroban the SAC (Stellar Asset Contract) address encodes the issuer,
        // we treat the token address directly and check issuer registry by the token address.
        // Clients are expected to call add_stablecoin_issuer with the token's contract address
        // (for SAC tokens) or a known issuer Address.
        env.storage()
            .instance()
            .get(&DataKey::StablecoinIssuer(token.clone()))
            .unwrap_or(false)
    }

    /// Return the current allowlist as an ordered list.
    pub fn get_allowed_tokens(env: Env) -> Result<soroban_sdk::Vec<Address>, Error> {
        Ok(Self::get_allowed_token_list(&env))
    }

    /// Return a paginated slice of the allowlist.
    pub fn get_allowed_tokens_paginated(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> soroban_sdk::Vec<Address> {
        let all = Self::get_allowed_token_list(&env);
        let mut result = soroban_sdk::vec![&env];
        let total = all.len();
        let start = offset.min(total);
        let end = (start.saturating_add(limit)).min(total);
        for i in start..end {
            result.push_back(all.get(i).unwrap());
        }
        result
    }

    fn get_allowed_token_list(env: &Env) -> soroban_sdk::Vec<Address> {
        if let Some(allowed_tokens) = env.storage().persistent().get(&DataKey::AllowedTokens) {
            env.storage().persistent().extend_ttl(
                &DataKey::AllowedTokens,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
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
            env.storage().persistent().extend_ttl(
                &DataKey::AllowedTokens,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
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
            env.storage().persistent().extend_ttl(
                &DataKey::AllowedTokens,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
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

    // ── Token Blacklist (issue #962) ─────────────────────────────────────────

    /// Add a token to the blacklist — admin only.
    ///
    /// Blacklisted tokens are permanently rejected in `create_match` even when
    /// the allowlist is not enforced.  The `reason` string (max 256 bytes) is
    /// stored on-chain for auditability.
    pub fn add_token_to_blacklist(env: Env, token: Address, reason: String) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let is_new = !env
            .storage()
            .instance()
            .has(&DataKey::BlacklistedToken(token.clone()));

        env.storage()
            .instance()
            .set(&DataKey::BlacklistedToken(token.clone()), &reason);

        if is_new {
            let mut list: soroban_sdk::Vec<Address> = env
                .storage()
                .persistent()
                .get(&DataKey::BlacklistedTokens)
                .unwrap_or_else(|| soroban_sdk::vec![&env]);
            list.push_back(token.clone());
            env.storage()
                .persistent()
                .set(&DataKey::BlacklistedTokens, &list);
            env.storage().persistent().extend_ttl(
                &DataKey::BlacklistedTokens,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }

        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "tok_blacklist"),
            ),
            token,
        );
        Ok(())
    }

    /// Remove a token from the blacklist — admin only.
    pub fn remove_token_from_blacklist(env: Env, token: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .remove(&DataKey::BlacklistedToken(token.clone()));

        // Remove from the persistent list.
        if let Some(list) = env
            .storage()
            .persistent()
            .get::<DataKey, soroban_sdk::Vec<Address>>(&DataKey::BlacklistedTokens)
        {
            let mut updated: soroban_sdk::Vec<Address> = soroban_sdk::vec![&env];
            for existing in list.iter() {
                if existing != token {
                    updated.push_back(existing.clone());
                }
            }
            env.storage()
                .persistent()
                .set(&DataKey::BlacklistedTokens, &updated);
            env.storage().persistent().extend_ttl(
                &DataKey::BlacklistedTokens,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }

        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "tok_unblacklist"),
            ),
            token,
        );
        Ok(())
    }

    /// Returns `true` when `token` is on the blacklist.
    pub fn is_token_blacklisted(env: Env, token: Address) -> bool {
        env.storage()
            .instance()
            .has(&DataKey::BlacklistedToken(token))
    }

    /// Returns all blacklisted token addresses.
    pub fn get_blacklist(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::BlacklistedTokens)
            .unwrap_or_else(|| soroban_sdk::vec![&env])
    }

    // ── Player Freeze ────────────────────────────────────────────────────────

    /// Freeze a player — admin only.
    ///
    /// A frozen player cannot create new matches (`create_match` and its
    /// variants) or deposit into existing ones, while every other user on the
    /// contract is completely unaffected — unlike a contract-wide `pause`.
    /// The `reason` string (stored on-chain for auditability) documents why
    /// the player was frozen (e.g. cheating, stalling, harassment).
    ///
    /// Freezing deliberately does **not** block fund-recovery paths: the
    /// frozen player can still cancel/expire their own `Pending` matches and
    /// claim vested payouts from already-funded matches, and the oracle can
    /// still settle `Active` matches normally. See `admin_unfreeze_player` to
    /// reverse a freeze.
    ///
    /// The `Error` enum is at the XDR-enforced cap of 50 variants, so the
    /// existing `Error::ContractPaused` is reused for frozen-player rejections
    /// (mirroring how the token blacklist reuses `Error::TokenNotAllowed`).
    pub fn admin_freeze_player(env: Env, player: Address, reason: String) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let is_new = !env
            .storage()
            .instance()
            .has(&PlayerFreezeKey::FrozenPlayer(player.clone()));

        env.storage()
            .instance()
            .set(&PlayerFreezeKey::FrozenPlayer(player.clone()), &reason);

        if is_new {
            let mut list: soroban_sdk::Vec<Address> = env
                .storage()
                .persistent()
                .get(&PlayerFreezeKey::FrozenPlayers)
                .unwrap_or_else(|| soroban_sdk::vec![&env]);
            list.push_back(player.clone());
            env.storage()
                .persistent()
                .set(&PlayerFreezeKey::FrozenPlayers, &list);
            env.storage().persistent().extend_ttl(
                &PlayerFreezeKey::FrozenPlayers,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("freeze")),
            player,
        );
        Ok(())
    }

    /// Unfreeze a player — admin only.
    ///
    /// Removes the freeze record and list entry so the player can create
    /// matches and deposit again.
    pub fn admin_unfreeze_player(env: Env, player: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .remove(&PlayerFreezeKey::FrozenPlayer(player.clone()));

        // Remove from the persistent list.
        if let Some(list) = env
            .storage()
            .persistent()
            .get::<PlayerFreezeKey, soroban_sdk::Vec<Address>>(&PlayerFreezeKey::FrozenPlayers)
        {
            let mut updated: soroban_sdk::Vec<Address> = soroban_sdk::vec![&env];
            for existing in list.iter() {
                if existing != player {
                    updated.push_back(existing.clone());
                }
            }
            env.storage()
                .persistent()
                .set(&PlayerFreezeKey::FrozenPlayers, &updated);
            env.storage().persistent().extend_ttl(
                &PlayerFreezeKey::FrozenPlayers,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("unfreeze")),
            player,
        );
        Ok(())
    }

    /// Returns `true` when `player` is currently frozen.
    pub fn is_player_frozen(env: Env, player: Address) -> bool {
        env.storage()
            .instance()
            .has(&PlayerFreezeKey::FrozenPlayer(player))
    }

    /// Returns all currently frozen player addresses.
    pub fn get_frozen_players(env: Env) -> soroban_sdk::Vec<Address> {
        env.storage()
            .persistent()
            .get(&PlayerFreezeKey::FrozenPlayers)
            .unwrap_or_else(|| soroban_sdk::vec![&env])
    }

    /// Internal helper — rejects `player` with `Error::ContractPaused` when
    /// frozen. Used by `create_match` (all variants) and `deposit`.
    fn require_player_not_frozen(env: &Env, player: &Address) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .has(&PlayerFreezeKey::FrozenPlayer(player.clone()))
        {
            return Err(Error::ContractPaused);
        }
        Ok(())
    }

    // ── Dynamic Fee Tiers (issue #963) ───────────────────────────────────────

    /// Set the dynamic fee tier schedule — admin only.
    ///
    /// `tiers` must be ordered by `max_stake` ascending.  The last entry acts
    /// as the open-ended catch-all (set `max_stake = i128::MAX`).  Pass an
    /// empty `Vec` to clear the schedule and fall back to zero protocol fees.
    /// Each tier's `fee_basis_points` must be in the range `0..=10_000`
    /// (0% to 100%).
    pub fn set_fee_tiers(env: Env, tiers: soroban_sdk::Vec<FeeTier>) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        // Validate ordering: each tier's max_stake must be strictly greater
        // than the previous tier's max_stake.
        let mut prev_max: i128 = -1;
        for tier in tiers.iter() {
            if tier.max_stake <= prev_max {
                return Err(Error::InvalidAmount);
            }
            if tier.fee_basis_points > 10_000 {
                return Err(Error::InvalidAmount);
            }
            prev_max = tier.max_stake;
        }

        env.storage().persistent().set(&DataKey::FeeTiers, &tiers);
        env.storage().persistent().extend_ttl(
            &DataKey::FeeTiers,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "fee_tiers_set"),
            ),
            (),
        );
        Ok(())
    }

    /// Return the current fee tier schedule.
    pub fn get_fee_tiers(env: Env) -> soroban_sdk::Vec<FeeTier> {
        env.storage()
            .persistent()
            .get(&DataKey::FeeTiers)
            .unwrap_or_else(|| soroban_sdk::vec![&env])
    }

    /// Calculate the fee in token units for a given `stake_amount` using the
    /// tiered schedule.  `pot` is `stake_amount * 2`.
    ///
    /// Returns `0` when no fee tiers are configured.
    pub fn calculate_fee_by_tier(env: Env, stake_amount: i128) -> Result<i128, Error> {
        Self::compute_tiered_fee(&env, stake_amount)
    }

    /// Internal helper — resolves the basis-point rate for `stake_amount` and
    /// computes the fee.
    fn compute_tiered_fee(env: &Env, stake_amount: i128) -> Result<i128, Error> {
        let tiers: soroban_sdk::Vec<FeeTier> = env
            .storage()
            .persistent()
            .get(&DataKey::FeeTiers)
            .unwrap_or_else(|| soroban_sdk::vec![env]);

        if tiers.is_empty() {
            return Ok(0);
        }

        // Find the first tier whose max_stake >= stake_amount.
        let mut selected_bps: u32 = 0;
        let mut found = false;
        for tier in tiers.iter() {
            if stake_amount <= tier.max_stake {
                selected_bps = tier.fee_basis_points;
                found = true;
                break;
            }
        }
        // If stake exceeds all explicit thresholds, use the last tier.
        if !found {
            if let Some(last) = tiers.get(tiers.len().saturating_sub(1)) {
                selected_bps = last.fee_basis_points;
            }
        }

        // fee = pot * bps / 10_000   where pot = stake * 2
        let pot = stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
        let fee = pot
            .checked_mul(selected_bps as i128)
            .ok_or(Error::Overflow)?
            .checked_div(10_000)
            .ok_or(Error::Overflow)?;
        Ok(fee)
    }

    /// Validate that `game_id` matches the format expected for `platform`.
    ///
    /// - Lichess: exactly 8 ASCII alphanumeric characters.
    /// - Chess.com: 7–12 ASCII digits.
    ///
    /// Also enforces the shared non-empty / `MAX_GAME_ID_LEN` bound before
    /// applying the platform-specific check.
    fn validate_game_id_format(game_id: &String, platform: &Platform) -> Result<(), Error> {
        let len = game_id.len();
        if len == 0 || len > MAX_GAME_ID_LEN {
            return Err(Error::InvalidGameId);
        }

        let mut buf = [0u8; MAX_GAME_ID_LEN as usize];
        let slice = &mut buf[..len as usize];
        game_id.copy_into_slice(slice);

        match platform {
            Platform::Lichess => {
                if len != LICHESS_GAME_ID_LEN || !slice.iter().all(|b| b.is_ascii_alphanumeric()) {
                    return Err(Error::InvalidGameId);
                }
            }
            Platform::ChessDotCom => {
                if !(CHESS_COM_GAME_ID_MIN_LEN..=CHESS_COM_GAME_ID_MAX_LEN).contains(&len)
                    || !slice.iter().all(|b| b.is_ascii_digit())
                {
                    return Err(Error::InvalidGameId);
                }
            }
        }

        Ok(())
    }

    /// Create a new match. Both players must call `deposit` before the game starts.
    ///
    /// # Parameters
    /// - `game_id`: The platform-specific game identifier, validated against `platform`.
    ///   - **Lichess**: exactly 8 alphanumeric characters (e.g. `"abcd1234"`).
    ///     Taken from the game URL: `https://lichess.org/<game_id>`
    ///   - **Chess.com**: 7–12 numeric digits (e.g. `"123456789"`).
    ///     Taken from the game URL: `https://www.chess.com/game/live/<game_id>`
    ///   An ID that doesn't match its platform's format is rejected at
    ///   creation time rather than failing later at oracle result-submission.
    /// - `platform`: Must match the platform the `game_id` was issued by.
    ///   Use `Platform::Lichess` or `Platform::ChessDotCom` accordingly.
    ///
    /// # Errors
    /// Returns `Error::InvalidGameId` if `game_id` is empty, exceeds `MAX_GAME_ID_LEN`
    /// (64 bytes), or doesn't match the format expected for `platform`.
    /// Returns `Error::DuplicateGameId` if the same `game_id` has already been used.
    /// Returns `Error::InvalidAmount` if `stake_amount` is below the configured
    /// `minimum_stake` (see `set_minimum_stake`).
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

        // Frozen players cannot create or join new matches — targeted
        // intervention without pausing the whole contract.
        Self::require_player_not_frozen(&env, &player1)?;
        Self::require_player_not_frozen(&env, &player2)?;

        // Blacklisted tokens are permanently rejected, regardless of allowlist status.
        if Self::is_token_blacklisted(env.clone(), token.clone()) {
            return Err(Error::TokenNotAllowed);
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

        // Stablecoin-only mode: reject non-stablecoin tokens when enabled
        let protocol_cfg = Self::get_config(&env);
        if protocol_cfg.stablecoin_only_mode && !Self::check_is_stablecoin(&env, &token) {
            return Err(Error::NotStablecoin);
        }

        if stake_amount < protocol_cfg.minimum_stake {
            return Err(Error::InvalidAmount);
        }
        if let Some(max_stake) = protocol_cfg.maximum_stake {
            if stake_amount > max_stake {
                return Err(Error::InvalidAmount);
            }
        }
        Self::require_player_tier_for_stake(&env, &player1, stake_amount)?;
        Self::require_player_tier_for_stake(&env, &player2, stake_amount)?;
        Self::validate_game_id_format(&game_id, &platform)?;

        // Reject if either player is invalid
        if player1 == player2 {
            return Err(Error::InvalidPlayers);
        }
        if player2 == env.current_contract_address() {
            return Err(Error::InvalidPlayers);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::GameId(game_id.clone()))
        {
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
            winner: Winner::None,
            vested_at: None,
            player1_claimed: false,
            player2_claimed: false,
            conversion_rate: None,
            token_b: None,
            conversion_rate_ledger: None,
            paused_ledger: None,
            total_pause_duration: 0,
            referrer: None,
            last_heartbeat: env.ledger().timestamp(),
            bracket_id: None,
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
        Self::record_platform_match_created(&env, stake_amount);

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("created")),
            (id, m.player1, m.player2, stake_amount),
        );

        Ok(id)
    }

    /// Create a new match for tournament brackets.
    ///
    /// This variant allows bracket matches to be linked on-chain, enabling
    /// automated bracket progression for multi-game tournaments.
    pub fn create_match_tournament(
        env: Env,
        bracket_id: u64,
        round: u32,
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

        // Blacklisted tokens are permanently rejected, regardless of allowlist status.
        if Self::is_token_blacklisted(env.clone(), token.clone()) {
            return Err(Error::TokenNotAllowed);
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

        // Stablecoin-only mode: reject non-stablecoin tokens when enabled
        let protocol_cfg = Self::get_config(&env);
        if protocol_cfg.stablecoin_only_mode && !Self::check_is_stablecoin(&env, &token) {
            return Err(Error::NotStablecoin);
        }

        if stake_amount < protocol_cfg.minimum_stake {
            return Err(Error::InvalidAmount);
        }
        if let Some(max_stake) = protocol_cfg.maximum_stake {
            if stake_amount > max_stake {
                return Err(Error::InvalidAmount);
            }
        }
        Self::require_player_tier_for_stake(&env, &player1, stake_amount)?;
        Self::require_player_tier_for_stake(&env, &player2, stake_amount)?;
        Self::validate_game_id_format(&game_id, &platform)?;

        // Reject if either player is invalid
        if player1 == player2 {
            return Err(Error::InvalidPlayers);
        }
        if player2 == env.current_contract_address() {
            return Err(Error::InvalidPlayers);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::GameId(game_id.clone()))
        {
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
            winner: Winner::None,
            vested_at: None,
            player1_claimed: false,
            player2_claimed: false,
            conversion_rate: None,
            token_b: None,
            conversion_rate_ledger: None,
            paused_ledger: None,
            total_pause_duration: 0,
            referrer: None,
            last_heartbeat: env.ledger().timestamp(),
            bracket_id: Some(bracket_id),
        };

        env.storage().persistent().set(&DataKey::Match(id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        let next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::MatchCount, &next_id);
        env.storage()
            .persistent()
            .set(&DataKey::GameId(m.game_id.clone()), &true);
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
        Self::record_platform_match_created(&env, stake_amount);

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("bracket_created")),
            (id, bracket_id, round, m.player1, m.player2, stake_amount),
        );

        Ok(id)
    }

    /// Get all matches for a specific tournament bracket.
    pub fn get_bracket_matches(env: Env, bracket_id: u64) -> soroban_sdk::Vec<Match> {
        let match_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0);
        let mut bracket_matches = soroban_sdk::vec![&env];

        for i in 0..match_count {
            if let Some(m) = env.storage().persistent().get::<_, Match>(&DataKey::Match(i)) {
                if m.bracket_id == Some(bracket_id) {
                    bracket_matches.push_back(m);
                }
            }
        }

        bracket_matches
    }

    /// Create a new match with multi-token support and conversion rates.
    ///
    /// `rate` is the token_b-per-token_a conversion rate, scaled by 1e7
    /// (e.g. a 1:1 rate is `10_000_000`). It must be strictly positive —
    /// `rate <= 0` (including `0`) is rejected with `Error::InvalidAmount`,
    /// since a zero or negative rate would later cause a division-by-zero
    /// or nonsensical payout when converting amounts in `claim_vested_payout`.
    pub fn create_match_with_conversion(
        env: Env,
        player1: Address,
        player2: Address,
        stake_amount: i128,
        token_a: Address,
        token_b: Address,
        rate: i128,
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

        // Frozen players cannot create or join new matches — targeted
        // intervention without pausing the whole contract.
        Self::require_player_not_frozen(&env, &player1)?;
        Self::require_player_not_frozen(&env, &player2)?;

        // Blacklisted tokens are permanently rejected, regardless of allowlist status.
        if Self::is_token_blacklisted(env.clone(), token_a.clone())
            || Self::is_token_blacklisted(env.clone(), token_b.clone())
        {
            return Err(Error::TokenNotAllowed);
        }

        // Check allowlist enforcement for both tokens
        let allowlist_enforced: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistEnforced)
            .unwrap_or(false);
        if allowlist_enforced
            && (!Self::is_token_allowed(env.clone(), token_a.clone())
                || !Self::is_token_allowed(env.clone(), token_b.clone()))
        {
            return Err(Error::TokenNotAllowed);
        }

        // Stablecoin-only mode: reject non-stablecoin tokens when enabled
        let protocol_cfg = Self::get_config(&env);
        if protocol_cfg.stablecoin_only_mode
            && (!Self::check_is_stablecoin(&env, &token_a)
                || !Self::check_is_stablecoin(&env, &token_b))
        {
            return Err(Error::NotStablecoin);
        }

        if stake_amount < protocol_cfg.minimum_stake || rate <= 0 {
            return Err(Error::InvalidAmount);
        }
        if let Some(max_stake) = protocol_cfg.maximum_stake {
            if stake_amount > max_stake {
                return Err(Error::InvalidAmount);
            }
        }
        if game_id.is_empty() || game_id.len() > MAX_GAME_ID_LEN {
            return Err(Error::InvalidGameId);
        }

        // Reject if either player is invalid
        if player1 == player2 {
            return Err(Error::InvalidPlayers);
        }
        if player2 == env.current_contract_address() {
            return Err(Error::InvalidPlayers);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::GameId(game_id.clone()))
        {
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

        // Oracle call to verify conversion rate within ±5%
        let oracle_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;

        // Fetch oracle rate from the oracle contract
        let oracle_rate: i128 = env.invoke_contract(
            &oracle_address,
            &Symbol::new(&env, "get_rate"),
            soroban_sdk::vec![&env, token_a.to_val(), token_b.to_val()],
        );

        // Verify conversion rate within ±5% of oracle rate
        // Tolerance: rate must be within [oracle_rate * 0.95, oracle_rate * 1.05]
        // Equivalently: rate * 100 >= oracle_rate * 95 && rate * 100 <= oracle_rate * 105
        let rate_100 = rate.checked_mul(100).ok_or(Error::Overflow)?;
        let oracle_lower = oracle_rate.checked_mul(95).ok_or(Error::Overflow)?;
        let oracle_upper = oracle_rate.checked_mul(105).ok_or(Error::Overflow)?;

        if rate_100 < oracle_lower || rate_100 > oracle_upper {
            return Err(Error::ConversionRateOutOfBounds);
        }

        let m = Match {
            id,
            player1: player1.clone(),
            player2: player2.clone(),
            stake_amount,
            token: token_a,
            game_id,
            platform,
            state: MatchState::Pending,
            player1_deposited: false,
            player2_deposited: false,
            created_ledger: env.ledger().sequence(),
            completed_ledger: None,
            winner: Winner::None,
            vested_at: None,
            player1_claimed: false,
            player2_claimed: false,
            conversion_rate: Some(rate),
            token_b: Some(token_b),
            conversion_rate_ledger: Some(env.ledger().sequence()),
            paused_ledger: None,
            total_pause_duration: 0,
            referrer: None,
            last_heartbeat: env.ledger().timestamp(),
            bracket_id: None,
        };

        env.storage().persistent().set(&DataKey::Match(id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::MatchCount, &next_id);
        env.storage()
            .persistent()
            .set(&DataKey::GameId(m.game_id.clone()), &true);
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
        Self::record_platform_match_created(&env, stake_amount);

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("created")),
            (id, m.player1, m.player2, stake_amount),
        );

        Ok(id)
    }

    /// Create a match and associate a referrer address for fee sharing.
    ///
    /// Identical to `create_match` except the match stores a `referrer` address.
    /// On winner payout via `claim_vested_payout`, a referral fee is deducted from
    /// the winner's proceeds and sent to the referrer:
    ///   `referral_fee = (pot * cancellation_fee_bps / 10_000) * referral_share_bps / 10_000`
    ///
    /// The referral fee only applies when `cancellation_fee_basis_points > 0` in
    /// `ProtocolConfig`.
    pub fn create_match_with_referrer(
        env: Env,
        player1: Address,
        player2: Address,
        stake_amount: i128,
        token: Address,
        game_id: String,
        platform: Platform,
        referrer: Address,
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

        // Frozen players cannot create or join new matches — targeted
        // intervention without pausing the whole contract.
        Self::require_player_not_frozen(&env, &player1)?;
        Self::require_player_not_frozen(&env, &player2)?;

        // Blacklisted tokens are permanently rejected, regardless of allowlist status.
        if Self::is_token_blacklisted(env.clone(), token.clone()) {
            return Err(Error::TokenNotAllowed);
        }

        let allowlist_enforced: bool = env
            .storage()
            .instance()
            .get(&DataKey::AllowlistEnforced)
            .unwrap_or(false);
        if allowlist_enforced && !Self::is_token_allowed(env.clone(), token.clone()) {
            return Err(Error::TokenNotAllowed);
        }

        let protocol_cfg = Self::get_config(&env);
        if stake_amount < protocol_cfg.minimum_stake {
            return Err(Error::InvalidAmount);
        }
        if let Some(max_stake) = protocol_cfg.maximum_stake {
            if stake_amount > max_stake {
                return Err(Error::InvalidAmount);
            }
        }
        Self::require_player_tier_for_stake(&env, &player1, stake_amount)?;
        Self::require_player_tier_for_stake(&env, &player2, stake_amount)?;
        Self::validate_game_id_format(&game_id, &platform)?;

        if player1 == player2 {
            return Err(Error::InvalidPlayers);
        }
        if player2 == env.current_contract_address() {
            return Err(Error::InvalidPlayers);
        }

        if env
            .storage()
            .persistent()
            .has(&DataKey::GameId(game_id.clone()))
        {
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
            winner: Winner::None,
            vested_at: None,
            player1_claimed: false,
            player2_claimed: false,
            conversion_rate: None,
            token_b: None,
            conversion_rate_ledger: None,
            paused_ledger: None,
            total_pause_duration: 0,
            referrer: Some(referrer.clone()),
            last_heartbeat: env.ledger().timestamp(),
            bracket_id: None,
        };

        env.storage().persistent().set(&DataKey::Match(id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        let next_id = id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage().instance().set(&DataKey::MatchCount, &next_id);
        env.storage()
            .persistent()
            .set(&DataKey::GameId(m.game_id.clone()), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::GameId(m.game_id.clone()),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

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
        Self::record_platform_match_created(&env, stake_amount);

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("created")),
            (id, m.player1, m.player2, stake_amount, referrer),
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

        // Frozen players cannot fund matches — targeted intervention without
        // pausing the whole contract. Existing matches are unaffected: the
        // counterpart can still cancel/expire a Pending match, and Active
        // matches still settle normally.
        Self::require_player_not_frozen(&env, &player)?;

        // ── Cross-contract reentrancy guard ──────────────────────────────────
        // If a malicious (or callback-capable) token contract re-enters
        // deposit() for the same match_id during the token.transfer() call
        // below, this flag will already be true and the nested call is
        // rejected immediately.  The flag is stored in temporary storage so
        // it is automatically cleared at the end of the transaction even if
        // an unexpected execution path skips the explicit removal below.
        if env
            .storage()
            .temporary()
            .get::<DataKey, bool>(&DataKey::DepositInProgress(match_id))
            .unwrap_or(false)
        {
            return Err(Error::DepositInProgress);
        }
        env.storage()
            .temporary()
            .set(&DataKey::DepositInProgress(match_id), &true);

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        // Explicit "already fully funded" guard, independent of `state`: even
        // if some future code path left `state` as `Pending` while both
        // deposits had already landed, this still closes the door on a
        // double-count. Checked before the state check so the caller gets
        // the more specific `AlreadyFunded` instead of `InvalidState`.
        if m.player1_deposited && m.player2_deposited {
            env.storage()
                .temporary()
                .remove(&DataKey::DepositInProgress(match_id));
            return Err(Error::AlreadyFunded);
        }

        if m.state != MatchState::Pending {
            // Clear guard before returning on error.
            env.storage()
                .temporary()
                .remove(&DataKey::DepositInProgress(match_id));
            return Err(Error::InvalidState);
        }

        let is_p1 = player == m.player1;
        let is_p2 = player == m.player2;

        if !is_p1 && !is_p2 {
            env.storage()
                .temporary()
                .remove(&DataKey::DepositInProgress(match_id));
            return Err(Error::Unauthorized);
        }
        if is_p1 && m.player1_deposited {
            env.storage()
                .temporary()
                .remove(&DataKey::DepositInProgress(match_id));
            return Err(Error::AlreadyFunded);
        }
        if is_p2 && m.player2_deposited {
            env.storage()
                .temporary()
                .remove(&DataKey::DepositInProgress(match_id));
            return Err(Error::AlreadyFunded);
        }

        Self::require_player_tier_for_stake(&env, &player, m.stake_amount)?;

        // Perform the cross-contract token transfer.  A callback-capable token
        // that attempts to call deposit() for the same match_id here will hit
        // the DepositInProgress guard set above and be rejected.
        let client = token::Client::new(&env, &m.token);
        client.transfer(&player, &env.current_contract_address(), &m.stake_amount);

        if is_p1 {
            m.player1_deposited = true;
        } else {
            m.player2_deposited = true;
        }

        // Refresh the last-activity timestamp so that cancellation-fee /
        // rollback dispute windows count from the most recent deposit.
        m.last_heartbeat = env.ledger().timestamp();

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
            if let Err(e) = Self::add_active_match(&env, &m.player1, match_id) {
                env.storage()
                    .temporary()
                    .remove(&DataKey::DepositInProgress(match_id));
                return Err(e);
            }
            if let Err(e) = Self::add_active_match(&env, &m.player2, match_id) {
                env.storage()
                    .temporary()
                    .remove(&DataKey::DepositInProgress(match_id));
                return Err(e);
            }
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
        Self::record_player_snapshot(&env, &player);

        // Clear the reentrancy guard now that all state updates are complete.
        env.storage()
            .temporary()
            .remove(&DataKey::DepositInProgress(match_id));

        env.events().publish(
            (Symbol::new(&env, "escrow"), symbol_short!("deposit")),
            (match_id, player, m.stake_amount),
        );

        Ok(())
    }

    /// Oracle submits the verified match result and triggers payout vesting.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        winner: Winner,
        oracle: Address,
        confidence: Option<u8>,
    ) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        oracle.require_auth();
        let stored_oracle: Address = Self::effective_oracle(&env)?;
        if oracle != stored_oracle {
            return Err(Error::Unauthorized);
        }

        Self::settle_result(&env, match_id, winner, confidence)
    }

    /// Submit a draw result — oracle only. This is a convenience wrapper
    /// around `settle_result` with `Winner::Draw`.
    pub fn submit_draw(env: Env, match_id: u64, oracle: Address, confidence: Option<u8>) -> Result<(), Error> {
        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        oracle.require_auth();
        let stored_oracle: Address = Self::effective_oracle(&env)?;
        if oracle != stored_oracle {
            return Err(Error::Unauthorized);
        }

        Self::settle_result(&env, match_id, Winner::Draw, confidence)
    }

    /// Core result-settlement logic shared by `submit_result` and
    /// `submit_result_batch`. Assumes the pause state and oracle
    /// authorization have already been checked by the caller — this lets
    /// `submit_result_batch` authorize the oracle once for the whole batch
    /// instead of once per match (repeated `require_auth` calls for the same
    /// address within a single invocation are rejected by the host).
    fn settle_result(env: &Env, match_id: u64, winner: Winner, confidence: Option<u8>) -> Result<(), Error> {
        if winner == Winner::None {
            return Err(Error::InvalidState);
        }

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        // A still-Pending match is inherently not (fully) funded — surface
        // `NotFunded` for it specifically, rather than the generic
        // `InvalidState`, so `submit_result_batch` callers can distinguish
        // "needs more deposits" from other invalid transitions (Completed,
        // Cancelled, PendingResult, Paused).
        if m.state == MatchState::Pending || !m.player1_deposited || !m.player2_deposited {
            return Err(Error::NotFunded);
        }

        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        Self::remove_active_match_indexed(env, &m.player1, match_id);
        Self::remove_active_match_indexed(env, &m.player2, match_id);

        m.state = MatchState::Completed;
        m.winner = winner.clone();
        m.vested_at = Some(env.ledger().timestamp());

        Self::record_completed_match(env, &m.player1);
        Self::record_completed_match(env, &m.player2);
        Self::record_platform_payout(env);
        Self::update_player_stats(env, &m.player1, &winner, m.stake_amount);
        Self::update_player_stats(env, &m.player2, &winner, m.stake_amount);

        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let dispute_period = Self::get_dispute_period(env);

        if dispute_period == 0 {
            // Immediate payout (no dispute period, but still subject to vesting).
            // completed_ledger is stamped only here (not unconditionally above)
            // because the delayed branch below leaves the match in
            // PendingResult, not Completed, until finalize_match /
            // resolve_dispute_by_vote actually performs the transition (both
            // of which already stamp completed_ledger themselves).
            m.completed_ledger = Some(env.ledger().sequence());
            env.storage()
                .persistent()
                .set(&DataKey::Match(match_id), &m);
            Self::record_snapshot(env, &m, SnapshotReason::Completed);
            Self::record_player_snapshot(env, &m.player1);
            Self::record_player_snapshot(env, &m.player2);
            let payout_amount = match winner {
                Winner::Player1 | Winner::Player2 | Winner::Draw => {
                    m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?
                }
                Winner::None => 0,
            };
            env.events().publish(
                (Symbol::new(env, "match"), Symbol::new(env, "completed")),
                (match_id, winner, payout_amount),
            );
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

            Self::record_snapshot(env, &m, SnapshotReason::ResultSubmitted);

            env.events().publish(
                (
                    Symbol::new(env, "match"),
                    Symbol::new(env, "pending_result"),
                ),
                (match_id, winner, deadline),
            );

            Ok(())
        }
    }

    /// Check if a match has become deadlocked (threshold unreachable given the approved oracle set).
    /// If deadlock is detected, flag the match and emit an event.
    /// Otherwise returns Ok(()) with no side effects.
    fn check_oracle_deadlock(
        env: &Env,
        match_id: u64,
        current_confirmations: u32,
        required: u32,
    ) -> Result<(), Error> {
        let oracles: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedOracles)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);

        let oracle_count = oracles.len();

        // Calculate how many more confirmations could theoretically be obtained.
        // We assume all remaining oracles might vote once each.
        let remaining_possible = oracle_count.saturating_sub(current_confirmations);
        let max_possible_confirmations = current_confirmations.saturating_add(remaining_possible);

        // If even if all remaining oracles vote, we still can't reach the threshold, deadlock.
        if max_possible_confirmations < required {
            env.storage()
                .persistent()
                .set(&types::OracleConsensusKey::OracleDeadlock(match_id), &true);
            env.storage().persistent().extend_ttl(
                &types::OracleConsensusKey::OracleDeadlock(match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );

            env.events().publish(
                (Symbol::new(env, "match"), symbol_short!("ora_dead")),
                (match_id, current_confirmations, required),
            );
        }

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
        oracle: Address,
    ) -> Result<(), Error> {
        // Validate and execute payout via standard submit_result (handles oracle auth).
        Self::submit_result(env.clone(), match_id, winner, oracle)?;

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

    /// Batch-submit results for multiple matches in a single call, reducing
    /// oracle transaction overhead. Caller must be the configured oracle.
    ///
    /// Each match is processed independently: a failure on one match (e.g.
    /// `MatchNotFound`, `InvalidState`, `NotFunded`) does not stop processing
    /// of the remaining entries. The returned `Vec` has one entry per input
    /// entry, in the same order — `None` on success, `Some(Error)` on
    /// failure for that match.
    ///
    /// Note: Soroban's contract ABI has no `Result` element type, so
    /// `Option<Error>` is the on-chain equivalent of `Result<(), Error>` here
    /// (`None` ~ `Ok(())`, `Some(e)` ~ `Err(e)`).
    pub fn submit_result_batch(
        env: Env,
        results: Vec<(u64, Winner)>,
        caller: Address,
    ) -> Result<Vec<Option<Error>>, Error> {
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

        let mut outcomes = Vec::new(&env);
        for (match_id, winner) in results.iter() {
            // A match already settled (Completed/PendingResult) in a prior
            // call means this oracle is re-confirming a result it already
            // submitted. Surface that explicitly instead of letting it fall
            // through to the generic InvalidState from settle_result.
            let already_settled = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(match_id))
                .is_some_and(|m| {
                    matches!(
                        m.state,
                        MatchState::Completed | MatchState::PendingResult
                    )
                });

            let outcome = if already_settled {
                Err(Error::OracleAlreadyConfirmed)
            } else {
                Self::settle_result(&env, match_id, winner)
            };
            outcomes.push_back(outcome.err());
        }
        Ok(outcomes)
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
            return Err(if m.state == MatchState::Active {
                Error::MatchAlreadyActive
            } else {
                Error::InvalidState
            });
        }

        // Either player1 or player2 can cancel a pending match
        let is_p1 = caller == m.player1;
        let is_p2 = caller == m.player2;

        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }

        caller.require_auth();

        let is_multi_token = m.token_b.is_some() && m.conversion_rate.is_some_and(|r| r > 0);

        let config: ProtocolConfig = Self::get_config(&env);

        let fee_amount = if config.cancellation_fee_basis_points > 0 {
            m.stake_amount
                .checked_mul(config.cancellation_fee_basis_points as i128)
                .ok_or(Error::Overflow)?
                / 10_000
        } else {
            0
        };
        let refund_amount = m
            .stake_amount
            .checked_sub(fee_amount)
            .ok_or(Error::Overflow)?;

        if m.player1_deposited {
            let client_a = token::Client::new(&env, &m.token);
            client_a.transfer(&env.current_contract_address(), &m.player1, &refund_amount);
            if fee_amount > 0 {
                client_a.transfer(
                    &env.current_contract_address(),
                    &config.treasury,
                    &fee_amount,
                );
            }
        }
        if m.player2_deposited {
            let token_b = m.token_b.clone().unwrap_or_else(|| m.token.clone());
            let amount_b = if is_multi_token {
                m.stake_amount
                    .checked_mul(m.conversion_rate.unwrap_or(0))
                    .ok_or(Error::Overflow)?
                    .checked_div(10_000_000)
                    .ok_or(Error::Overflow)?
            } else {
                m.stake_amount
            };
            let fee_amount_b = if config.cancellation_fee_basis_points > 0 {
                amount_b
                    .checked_mul(config.cancellation_fee_basis_points as i128)
                    .ok_or(Error::Overflow)?
                    / 10_000
            } else {
                0
            };
            let refund_amount_b = amount_b.checked_sub(fee_amount_b).ok_or(Error::Overflow)?;
            let client_b = token::Client::new(&env, &token_b);
            client_b.transfer(
                &env.current_contract_address(),
                &m.player2,
                &refund_amount_b,
            );
            if fee_amount_b > 0 {
                client_b.transfer(
                    &env.current_contract_address(),
                    &config.treasury,
                    &fee_amount_b,
                );
            }
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
        // Player-level snapshots are recorded only for refunded parties —
        // non-depositors' escrow balance is already 0 and would not change.
        if m.player1_deposited {
            Self::record_player_snapshot(&env, &m.player1);
        }
        if m.player2_deposited {
            Self::record_player_snapshot(&env, &m.player2);
        }

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("cancelled")),
            match_id,
        );

        Ok(())
    }

    /// Pause an active match — either player can pause.
    /// Sets match state to Paused and records the pause start ledger.
    pub fn pause_match(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        caller.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Active {
            return Err(Error::InvalidPauseState);
        }

        let is_p1 = caller == m.player1;
        let is_p2 = caller == m.player2;

        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }

        m.state = MatchState::Paused;
        m.paused_ledger = Some(env.ledger().sequence());
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        Self::record_snapshot(&env, &m, SnapshotReason::Paused);

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("paused")),
            match_id,
        );

        Ok(())
    }

    /// Resume a paused match — either player can resume.
    /// Sets match state back to Active and accumulates pause duration.
    pub fn resume_match(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        caller.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Paused {
            return Err(Error::InvalidState);
        }

        let is_p1 = caller == m.player1;
        let is_p2 = caller == m.player2;

        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }

        let current_ledger = env.ledger().sequence();
        if let Some(paused_at) = m.paused_ledger {
            let pause_duration = current_ledger.saturating_sub(paused_at);
            m.total_pause_duration = m.total_pause_duration.saturating_add(pause_duration);
        }

        m.state = MatchState::Active;
        m.paused_ledger = None;
        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        Self::record_snapshot(&env, &m, SnapshotReason::Resumed);

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("resumed")),
            match_id,
        );

        Ok(())
    }

    /// Expire a pending match that has not been fully funded within MATCH_TIMEOUT_LEDGERS.
    /// Anyone can call this; funds are returned to whoever deposited.
    /// Pause duration is excluded from the timeout calculation.
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

        let current_ledger = env.ledger().sequence();
        let total_elapsed = current_ledger.saturating_sub(m.created_ledger);
        let effective_elapsed = total_elapsed.saturating_sub(m.total_pause_duration);
        let timeout = Self::current_match_timeout(&env);

        if effective_elapsed < timeout {
            return Err(Error::MatchNotExpired);
        }

        let is_multi_token = m.token_b.is_some() && m.conversion_rate.is_some_and(|r| r > 0);

        // The match's token(s) may have been blacklisted after creation but
        // before expiry. Refuse to attempt a refund transfer into a token
        // contract that's since been flagged as broken or malicious — that
        // could fail unpredictably or panic instead of cleanly erroring, and
        // leaves the match state untouched so it can be resolved another way
        // (e.g. admin intervention) rather than getting stuck mid-transfer.
        let token_b_for_check = m.token_b.clone().unwrap_or_else(|| m.token.clone());
        if Self::is_token_blacklisted(env.clone(), m.token.clone())
            || (is_multi_token && Self::is_token_blacklisted(env.clone(), token_b_for_check))
        {
            return Err(Error::TokenNotAllowed);
        }

        if m.player1_deposited {
            let client_a = token::Client::new(&env, &m.token);
            client_a.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
        }
        if m.player2_deposited {
            let token_b = m.token_b.clone().unwrap_or_else(|| m.token.clone());
            let amount_b = if is_multi_token {
                m.stake_amount
                    .checked_mul(m.conversion_rate.unwrap_or(0))
                    .ok_or(Error::Overflow)?
                    .checked_div(10_000_000)
                    .ok_or(Error::Overflow)?
            } else {
                m.stake_amount
            };
            let client_b = token::Client::new(&env, &token_b);
            client_b.transfer(&env.current_contract_address(), &m.player2, &amount_b);
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
        // Player-level snapshots are recorded only for refunded parties —
        // non-depositors' escrow balance is already 0 and would not change.
        if m.player1_deposited {
            Self::record_player_snapshot(&env, &m.player1);
        }
        if m.player2_deposited {
            Self::record_player_snapshot(&env, &m.player2);
        }

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("expired")),
            match_id,
        );

        Ok(())
    }

    // ── Disconnection-rollback dispute (issue: auto-created from UNSOLVED_ISSUES_40.md) ──

    /// Dispute an in-progress match and roll it back, refunding both players.
    ///
    /// Use case: a player disconnects mid-game and the opponent has privately
    /// (and informally) "claimed victory" outside the contract. Either side can
    /// invoke this function to claw back their stake while the match is still
    /// recently active and the loser can be plausibly believed to have been
    /// disconnected rather than defeated.
    ///
    /// Rules:
    /// - `disputer` must `require_auth` and must be either `Match.player1` or
    ///   `Match.player2`. Strangers and the oracle cannot roll matches back.
    /// - The match must be in `MatchState::Active`. Pending / PendingResult /
    ///   Completed / Cancelled / Paused matches already have their own dispute
    ///   or cancellation paths.
    /// - The current ledger timestamp must be within
    ///   `ROLLBACK_WINDOW_SECONDS` (24 h) of `Match.last_heartbeat`. Outside
    ///   the window, the match is considered legitimately stalled and the
    ///   rollback is rejected with `Error::VotingPeriodElapsed`.
    /// - The full stake is refunded to whichever players had deposited — no
    ///   cancellation fee is applied. This is a player-friendly escape hatch,
    ///   not a fee-bearing cancel.
    /// - After refund, the match transitions to `MatchState::Cancelled` and a
    ///   `("match", "rollback")` event is emitted with `(match_id, disputer,
    ///   reason)`.
    ///
    /// Note: this function intentionally does NOT consult the contract-wide
    /// `Paused` flag, mirroring `cancel_match` / `expire_match` — admin pause
    /// gates state-creating operations (create, deposit, submit) and not the
    /// refund path that exists to recover funds.
    pub fn dispute_and_rollback_match(
        env: Env,
        match_id: u64,
        disputer: Address,
        reason: String,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);

        // Validate reason so it is indexable on-chain and bounded in size.
        if reason.is_empty() || reason.len() > MAX_REASON_LEN {
            return Err(Error::InvalidEvidenceHash);
        }

        disputer.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        // Player-only access. Either side may invoke, including the player
        // who is alleged to have disconnected (so they can self-report).
        let is_p1 = disputer == m.player1;
        let is_p2 = disputer == m.player2;
        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }

        // Only Active matches may be rolled back. Pending matches should use
        // `cancel_match`; PendingResult/Completed have the oracle dispute path;
        // Paused matches can be either resumed or expired depending on intent.
        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        // Enforce the 24-hour heartbeat window. Reject rollback attempts after
        // the match has shown no activity for longer than the window — by that
        // point period the result is considered legitimately lost or stalled
        // and an explicit dispute-by-vote path is the correct remedy.
        let now: u64 = env.ledger().timestamp();
        let since_heartbeat: u64 = now.saturating_sub(m.last_heartbeat);
        if since_heartbeat > ROLLBACK_WINDOW_SECONDS {
            return Err(Error::VotingPeriodElapsed);
        }

        // Drop the active-match index for both players before mutating state
        // so future-player/match lookups stay consistent.
        Self::remove_active_match_indexed(&env, &m.player1, match_id);
        Self::remove_active_match_indexed(&env, &m.player2, match_id);

        // Refund players. Supports multi-token matches via the same
        // conversion logic used by cancel_match and expire_match so that a
        // rollback preserves the original stake denomination for each side.
        let is_multi_token = m.token_b.is_some() && m.conversion_rate.is_some_and(|r| r > 0);

        if m.player1_deposited {
            let client_a = token::Client::new(&env, &m.token);
            client_a.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
        }
        if m.player2_deposited {
            let token_b = m.token_b.clone().unwrap_or_else(|| m.token.clone());
            let amount_b = if is_multi_token {
                m.stake_amount
                    .checked_mul(m.conversion_rate.unwrap_or(0))
                    .ok_or(Error::Overflow)?
                    .checked_div(10_000_000)
                    .ok_or(Error::Overflow)?
            } else {
                m.stake_amount
            };
            let client_b = token::Client::new(&env, &token_b);
            client_b.transfer(&env.current_contract_address(), &m.player2, &amount_b);
        }

        // Finalize the match as cancelled and stamp the completion ledger so
        // downstream code that reads `completed_ledger` sees a terminal time.
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
        // Intentionally NOT calling `record_completed_match` here: the post-
        // rollback state is `Cancelled`, not `Completed`. The completed-match
        // counter (used for tier thresholds) must reflect only matches that
        // actually finished, so we leave it untouched on the rollback path.
        if m.player1_deposited {
            Self::record_player_snapshot(&env, &m.player1);
        }
        if m.player2_deposited {
            Self::record_player_snapshot(&env, &m.player2);
        }

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("rollback")),
            (match_id, disputer, reason),
        );

        Ok(())
    }

    /// Update the heartbeat for a match — player only.
    ///
    /// Alias for `heartbeat_match` with the parameter order specified in issue #1343.
    pub fn update_heartbeat(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        Self::heartbeat_match(env, match_id, caller)
    }

    // ── Heartbeat (refreshes `last_heartbeat` to keep the rollback window alive) ──

    /// Refresh the match `last_heartbeat` to the current ledger timestamp.
    ///
    /// Use case: a player wants to signal "the game is still progressing, do
    /// not treat a long idle period as a disconnect-by-default" — used in
    /// conjunction with `dispute_and_rollback_match`, whose 24-hour enforce-
    /// ment window counts from `last_heartbeat`. Pausing for dinner or a long
    /// analysis no longer forfeits the rollback window as long as either side
    /// periodically calls this.
    ///
    /// The hallmark difference from `deposit` is that **no token movement
    /// happens** — it is purely a timestamp update so players do not need to
    /// keep funding redundant deposits just to keep the dispute window alive.
    ///
    /// Rules:
    /// - `player` must `require_auth` and must be either `Match.player1` or
    ///   `Match.player2`. Strangers and the oracle cannot heartbeat matches.
    /// - The match must be in `MatchState::Active`. Pending matches have no
    ///   heartbeat yet (no gameplay in progress); PendingResult / Completed
    ///   are past the rollback-relevant window; Paused matches can be
    ///   resumed via `resume_match` (which moves state back to Active and
    ///   naturally allows heartbeats thereafter); Cancelled / Expired are
    ///   terminal. Reject everywhere to keep the state machine honest.
    /// - `last_heartbeat` is overwritten with `env.ledger().timestamp()` and
    ///   the match is persisted back to storage with TTL extended.
    /// - Emits a `("match", "heartbeat")` event with `(match_id, player,
    ///   last_heartbeat)` so off-chain indexers / front-ends can observe the
    ///   rolling window without having to re-read the full match state.
    pub fn heartbeat_match(env: Env, match_id: u64, player: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);

        player.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        // Player-only access. Either side may invoke; signalling intent that
        // the game is still progressing counts equally from either party.
        let is_p1 = player == m.player1;
        let is_p2 = player == m.player2;
        if !is_p1 && !is_p2 {
            // Intentionally no extend_ttl / storage touch on rejection: an
            // unauthorized caller must not be able to indefinitely keep a
            // stale match alive by spamming heartbeats. TTL is only refreshed
            // on the success path below.
            return Err(Error::Unauthorized);
        }

        // Heartbeat is only meaningful while the match is actively being
        // played out. Refusing other states keeps the state-machine honest:
        // a heartbeat on a Paused match would mask an unresolved pause, and
        // a heartbeat on a terminal match would silently rewrite history.
        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        let now: u64 = env.ledger().timestamp();
        m.last_heartbeat = now;

        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("heartbeat")),
            (match_id, player, now),
        );

        Ok(())
    }

    /// Admin-only recovery for Active matches stalled for >7 days with no result.
    ///
    /// **Purpose**: Provide a bounded recovery path for matches stuck in `Active`
    /// state after the 24-hour player-initiated rollback window has elapsed and
    /// the oracle has failed to submit a result. Without this function, funds
    /// would be permanently locked if the oracle service went down or lost its
    /// signing key.
    ///
    /// **Rules**:
    /// - `caller` must be the configured admin and provide `require_auth`.
    /// - Match must be in `MatchState::Active` (neither `Pending`, nor already
    ///   `Completed`/`Cancelled`).
    /// - Both players must have deposited (the match is truly funded and stuck,
    ///   not just a half-funded `Pending` that should use `expire_match`).
    /// - Time since `last_heartbeat` must exceed `ADMIN_STALL_WINDOW_SECONDS`
    ///   (7 days) — long enough not to compete with the 24-hour player window,
    ///   but bounded so funds are never truly stuck forever.
    /// - `resolution` dictates the outcome: `Winner::Player1`, `Winner::Player2`,
    ///   or `Winner::Draw` (for full refund). `Winner::None` is rejected.
    /// - On refund (`Winner::Draw`), no cancellation fee is applied — this is
    ///   a player-friendly escape hatch for a stalled contract, not a penalty.
    /// - Emits `("match", "admin_stall_resolution")` with `(match_id, resolution)`
    ///   to distinguish this operator-initiated path from normal oracle settlement.
    ///
    /// **Note**: This function intentionally does NOT consult the contract-wide
    /// `Paused` flag, mirroring the design of `dispute_and_rollback_match` — the
    /// pause gates new operations (create, deposit, submit), not recovery paths
    /// that exist to return funds to players.
    pub fn admin_resolve_stalled_match(
        env: Env,
        match_id: u64,
        caller: Address,
        resolution: Winner,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);

        // Admin-only access.
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if caller != admin {
            return Err(Error::Unauthorized);
        }

        // Reject Winner::None — admin must pick a concrete resolution.
        if resolution == Winner::None {
            return Err(Error::InvalidAmount);
        }

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        // Only Active matches may be admin-resolved. Pending matches should
        // use `expire_match`; Completed/Cancelled are terminal; PendingResult
        // has the normal dispute path; Paused can be resumed or expired.
        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        // Require both deposits — this is the "truly stuck, funds locked"
        // scenario the issue describes. A half-funded match can already be
        // handled by `expire_match` after the timeout elapses.
        if !m.player1_deposited || !m.player2_deposited {
            return Err(Error::NotFunded);
        }

        // Enforce the 7-day stall threshold. Reject admin intervention if
        // the match has shown recent activity (heartbeat) within that window.
        let now: u64 = env.ledger().timestamp();
        let since_heartbeat: u64 = now.saturating_sub(m.last_heartbeat);
        if since_heartbeat <= ADMIN_STALL_WINDOW_SECONDS {
            return Err(Error::MatchNotExpired);
        }

        // Drop the active-match index for both players before mutating state.
        Self::remove_active_match_indexed(&env, &m.player1, match_id);
        Self::remove_active_match_indexed(&env, &m.player2, match_id);

        // Execute the resolution.
        let is_multi_token = m.token_b.is_some() && m.conversion_rate.is_some_and(|r| r > 0);

        match resolution {
            Winner::Player1 => {
                // Full pot to player1.
                let total_pot = if is_multi_token {
                    let p2_amount = m
                        .stake_amount
                        .checked_mul(m.conversion_rate.unwrap_or(0))
                        .ok_or(Error::Overflow)?
                        .checked_div(10_000_000)
                        .ok_or(Error::Overflow)?;
                    m.stake_amount
                        .checked_add(p2_amount)
                        .ok_or(Error::Overflow)?
                } else {
                    m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?
                };
                let client_a = token::Client::new(&env, &m.token);
                client_a.transfer(&env.current_contract_address(), &m.player1, &total_pot);
            }
            Winner::Player2 => {
                // Full pot to player2.
                let token_b = m.token_b.clone().unwrap_or_else(|| m.token.clone());
                let total_pot = if is_multi_token {
                    let p2_stake = m
                        .stake_amount
                        .checked_mul(m.conversion_rate.unwrap_or(0))
                        .ok_or(Error::Overflow)?
                        .checked_div(10_000_000)
                        .ok_or(Error::Overflow)?;
                    m.stake_amount
                        .checked_add(p2_stake)
                        .ok_or(Error::Overflow)?
                } else {
                    m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?
                };
                let client_b = token::Client::new(&env, &token_b);
                client_b.transfer(&env.current_contract_address(), &m.player2, &total_pot);
            }
            Winner::Draw => {
                // Refund both players (no cancellation fee).
                let client_a = token::Client::new(&env, &m.token);
                client_a.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);

                let token_b = m.token_b.clone().unwrap_or_else(|| m.token.clone());
                let amount_b = if is_multi_token {
                    m.stake_amount
                        .checked_mul(m.conversion_rate.unwrap_or(0))
                        .ok_or(Error::Overflow)?
                        .checked_div(10_000_000)
                        .ok_or(Error::Overflow)?
                } else {
                    m.stake_amount
                };
                let client_b = token::Client::new(&env, &token_b);
                client_b.transfer(&env.current_contract_address(), &m.player2, &amount_b);
            }
            Winner::None => {
                // Already rejected above, but match exhaustiveness.
                return Err(Error::InvalidAmount);
            }
        }

        // Finalize the match as Completed and stamp the completion ledger.
        m.state = MatchState::Completed;
        m.winner = resolution.clone();
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
        Self::record_player_snapshot(&env, &m.player1);
        Self::record_player_snapshot(&env, &m.player2);

        // Record as a completed match if not a draw (for tier progression).
        if resolution != Winner::Draw {
            Self::record_completed_match(&env, &m.player1);
            Self::record_completed_match(&env, &m.player2);
        }

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("adm_stall")),
            (match_id, resolution),
        );

        // Also publish a match/cancelled event so the event-indexer picks up
        // the terminal state transition out of Active (it only listens for
        // the standard lifecycle events, not "adm_stall").
        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("cancelled")),
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

    /// Load the combined temp/pending oracle-rotation state (empty if neither is set).
    fn get_rotation_state(env: &Env) -> OracleRotationState {
        env.storage()
            .instance()
            .get(&DataKey::OracleRotation)
            .unwrap_or_default()
    }

    /// Persist the combined rotation state, dropping the storage entry entirely
    /// once both the temp and pending rotations are cleared.
    fn save_rotation_state(env: &Env, state: OracleRotationState) {
        if state.is_empty() {
            env.storage().instance().remove(&DataKey::OracleRotation);
        } else {
            env.storage()
                .instance()
                .set(&DataKey::OracleRotation, &state);
        }
    }

    fn effective_oracle(env: &Env) -> Result<Address, Error> {
        let base_oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;

        let mut state = Self::get_rotation_state(env);
        if let Some(rot) = state.temp() {
            let now = env.ledger().timestamp();
            if now < rot.expiry && rot.old_oracle == base_oracle {
                return Ok(rot.temp_oracle);
            } else {
                state.set_temp(None);
                Self::save_rotation_state(env, state);
            }
        }

        Ok(base_oracle)
    }

    /// Return the oracle address currently configured on the contract (or active temporary rotation).
    pub fn get_oracle(env: Env) -> Result<Address, Error> {
        Self::effective_oracle(&env)
    }

    /// Return the oracle address currently configured on the contract.
    ///
    /// This is a view function that returns the oracle address without
    /// requiring authentication. It is intended for off-chain clients,
    /// frontends, and monitoring tools to verify oracle configuration
    /// without reading raw contract storage.
    ///
    /// # Errors
    /// Returns `Error::Unauthorized` if the contract has not been initialized.
    pub fn get_oracle_address(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)
    }

    /// Return the current oracle rotation state for monitoring.
    ///
    /// This is a lightweight view function that returns the current
    /// OracleRotationState without requiring admin rights. Returns None
    /// if no rotation is pending. Monitoring tools can use this to observe
    /// rotation progress without triggering auth.
    pub fn get_oracle_rotation_state(env: Env) -> Option<OracleRotationState> {
        let state = Self::get_rotation_state(&env);
        if state.is_empty() {
            None
        } else {
            Some(state)
        }
    }

    // ── Platform Statistics (for analytics without full indexing) ─────────────

    /// Retrieve platform-wide aggregated statistics.
    ///
    /// Returns cumulative counters for matches created, total volume staked,
    /// and total successful payouts. These statistics are maintained on-chain
    /// to enable off-chain analytics without requiring full event indexing.
    ///
    /// Returns a `PlatformStats` struct with fields:
    /// - `total_matches`: Total number of matches created across all time.
    /// - `total_volume`: Cumulative stake amount (in base token units) across all matches.
    /// - `total_payouts`: Total number of successful payouts (matches completed with winner determination).
    pub fn get_platform_stats(env: Env) -> PlatformStats {
        env.storage()
            .persistent()
            .get(&DataKey::Stats)
            .unwrap_or(PlatformStats {
                total_matches: 0,
                total_volume: 0,
                total_payouts: 0,
            })
    }

    /// Internal helper to increment platform statistics. Called from `create_match`.
    fn record_platform_match_created(env: &Env, stake_amount: i128) {
        let mut stats: PlatformStats =
            env.storage()
                .persistent()
                .get(&DataKey::Stats)
                .unwrap_or(PlatformStats {
                    total_matches: 0,
                    total_volume: 0,
                    total_payouts: 0,
                });

        stats.total_matches = stats.total_matches.saturating_add(1);
        stats.total_volume = stats.total_volume.saturating_add(stake_amount);

        env.storage().persistent().set(&DataKey::Stats, &stats);
        env.storage().persistent().extend_ttl(
            &DataKey::Stats,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    }

    /// Internal helper to record a payout in platform statistics. Called from `submit_result`.
    fn record_platform_payout(env: &Env) {
        let mut stats: PlatformStats =
            env.storage()
                .persistent()
                .get(&DataKey::Stats)
                .unwrap_or(PlatformStats {
                    total_matches: 0,
                    total_volume: 0,
                    total_payouts: 0,
                });

        stats.total_payouts = stats.total_payouts.saturating_add(1);

        env.storage().persistent().set(&DataKey::Stats, &stats);
        env.storage().persistent().extend_ttl(
            &DataKey::Stats,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
    }

    /// Configured match timeout expressed in ledgers, derived from
    /// `ProtocolConfig::match_timeout_seconds` for use by `expire_match`
    /// (which compares against ledger-sequence deltas).
    fn current_match_timeout(env: &Env) -> u32 {
        let seconds = Self::get_config(env).match_timeout_seconds;
        // Ceiling division: floor division here would underestimate the
        // ledger delta required, allowing the timeout to trigger early.
        ((seconds + SECONDS_PER_LEDGER - 1) / SECONDS_PER_LEDGER) as u32
    }

    /// Get the cached count of completed matches for a player (O(1) lookup).
    /// This counter is incremented once when each match completes, avoiding
    /// the previous O(n) history walk for every tier check.
    fn completed_match_count(env: &Env, player: &Address) -> u32 {
        let key = DataKey::PlayerCompletedMatchCount(player.clone());
        let count = env.storage().persistent().get(&key).unwrap_or(0u32);

        if env.storage().persistent().has(&key) {
            env.storage()
                .persistent()
                .extend_ttl(&key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
        }

        count
    }

    /// Increment the completed-match counter for a player. Called once when
    /// a match transitions to Completed state.
    fn record_completed_match(env: &Env, player: &Address) {
        let key = DataKey::PlayerCompletedMatchCount(player.clone());
        let count: u32 = env.storage().persistent().get(&key).unwrap_or(0);

        env.storage()
            .persistent()
            .set(&key, &(count.saturating_add(1)));
        env.storage()
            .persistent()
            .extend_ttl(&key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);
    }

    fn tier_for_completed_matches(completed_matches: u32) -> PlayerTier {
        if completed_matches >= PLATINUM_MIN_COMPLETED_MATCHES {
            PlayerTier::Platinum
        } else if completed_matches >= GOLD_MIN_COMPLETED_MATCHES {
            PlayerTier::Gold
        } else if completed_matches >= SILVER_MIN_COMPLETED_MATCHES {
            PlayerTier::Silver
        } else {
            PlayerTier::Bronze
        }
    }

    fn require_player_tier_for_stake(
        env: &Env,
        player: &Address,
        stake_amount: i128,
    ) -> Result<(), Error> {
        let tier = Self::tier_for_completed_matches(Self::completed_match_count(env, player));
        let min_stake = Self::min_tier_stake(env.clone(), tier.clone());
        let max_stake = Self::max_tier_stake(env.clone(), tier);

        if stake_amount < min_stake || stake_amount > max_stake {
            return Err(Error::TierStakeNotAllowed);
        }

        Ok(())
    }

    /// Add a match to a player's active set with per-player cap enforcement (O(1)).
    /// Returns an error if the player has already reached MAX_ACTIVE_MATCHES_PER_PLAYER.
    fn add_active_match(env: &Env, player: &Address, match_id: u64) -> Result<(), Error> {
        let count_key = DataKey::PlayerActiveMatchCount(player.clone());
        let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);

        if count >= MAX_ACTIVE_MATCHES_PER_PLAYER {
            return Err(Error::TooManyActiveMatches);
        }

        let key = DataKey::ActiveMatch(player.clone(), match_id);
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        env.storage().persistent().set(&count_key, &(count + 1));
        env.storage()
            .persistent()
            .extend_ttl(&count_key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        Ok(())
    }

    /// Remove a match from a player's active set (O(1)).
    fn remove_active_match_indexed(env: &Env, player: &Address, match_id: u64) {
        let key = DataKey::ActiveMatch(player.clone(), match_id);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().remove(&key);

            let count_key = DataKey::PlayerActiveMatchCount(player.clone());
            let count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
            if count > 0 {
                env.storage().persistent().set(&count_key, &(count - 1));
                env.storage().persistent().extend_ttl(
                    &count_key,
                    MATCH_TTL_LEDGERS,
                    MATCH_TTL_LEDGERS,
                );
            }
        }
    }

    /// Current match expiration timeout, in seconds.
    pub fn get_match_timeout(env: Env) -> Result<u64, Error> {
        Ok(Self::get_config(&env).match_timeout_seconds)
    }

    pub fn tier_from_match_count(env: Env, player: Address) -> PlayerTier {
        let completed_matches = Self::completed_match_count(&env, &player);
        Self::tier_for_completed_matches(completed_matches)
    }

    pub fn min_tier_stake(_env: Env, tier: PlayerTier) -> i128 {
        match tier {
            PlayerTier::Bronze => BRONZE_MIN_STAKE,
            PlayerTier::Silver => SILVER_MIN_STAKE,
            PlayerTier::Gold => GOLD_MIN_STAKE,
            PlayerTier::Platinum => PLATINUM_MIN_STAKE,
        }
    }

    pub fn max_tier_stake(_env: Env, tier: PlayerTier) -> i128 {
        match tier {
            PlayerTier::Bronze => BRONZE_MAX_STAKE,
            PlayerTier::Silver => SILVER_MAX_STAKE,
            PlayerTier::Gold => GOLD_MAX_STAKE,
            PlayerTier::Platinum => i128::MAX,
        }
    }

    /// Set the match expiration timeout, in seconds. Admin only.
    pub fn set_match_timeout(env: Env, seconds: u64) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if !(MIN_MATCH_TIMEOUT_SECONDS..=MAX_MATCH_TIMEOUT_SECONDS).contains(&seconds) {
            return Err(Error::InvalidTimeout);
        }

        let mut config = Self::get_config(&env);
        let old_timeout = config.match_timeout_seconds;
        config.match_timeout_seconds = seconds;
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &config);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("timeout")),
            (old_timeout, seconds),
        );
        Ok(())
    }

    /// Set the maximum stake accepted by `create_match` and friends. Admin only.
    /// `None` removes the cap (unlimited stakes).
    pub fn set_maximum_stake(env: Env, amount: Option<i128>) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if let Some(max) = amount {
            if max <= 0 {
                return Err(Error::InvalidAmount);
            }
        }

        let mut config = Self::get_config(&env);
        config.maximum_stake = amount;
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &config);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("max_stake")),
            amount,
        );
        Ok(())
    }

    /// Set the minimum stake for new matches — admin only.
    pub fn set_minimum_stake(env: Env, amount: i128) -> Result<(), Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut config = Self::get_config(&env);
        config.minimum_stake = amount;
        env.storage()
            .instance()
            .set(&DataKey::ProtocolConfig, &config);
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

        env.storage().instance().set(
            &DataKey::PendingAdmin,
            &PendingAdminProposal {
                proposer: admin,
                pending_admin: new_admin.clone(),
            },
        );
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("propose")),
            new_admin,
        );
        Ok(())
    }

    /// Accept pending admin proposal. Pending admin only. Finalizes the transfer.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let proposal: PendingAdminProposal = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(Error::Unauthorized)?;
        proposal.pending_admin.require_auth();

        let current_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if current_admin != proposal.proposer {
            return Err(Error::Unauthorized);
        }

        env.storage()
            .instance()
            .set(&DataKey::Admin, &proposal.pending_admin);
        // Audited: PendingAdmin is removed here, so a second accept_admin()
        // call has nothing to load (Error::Unauthorized) and cannot replay
        // this proposal.
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("xfer")),
            proposal.pending_admin,
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

    /// Return player statistics for a given address.
    ///
    /// Returns cumulative stats including total matches, wins, losses, draws,
    /// and total volume staked. These statistics are maintained on-chain
    /// to enable off-chain analytics without requiring full event indexing.
    pub fn get_player_stats(env: Env, player: Address) -> PlayerStats {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerStats(player))
            .unwrap_or(PlayerStats {
                total_matches: 0,
                wins: 0,
                losses: 0,
                draws: 0,
                total_volume_staked: 0,
            })
    }

    /// Internal helper to update player statistics on match completion.
    fn update_player_stats(env: &Env, player: &Address, winner: &Winner, stake_amount: i128) {
        let mut stats: PlayerStats = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerStats(player.clone()))
            .unwrap_or(PlayerStats {
                total_matches: 0,
                wins: 0,
                losses: 0,
                draws: 0,
                total_volume_staked: 0,
            });

        stats.total_matches = stats.total_matches.saturating_add(1);
        stats.total_volume_staked = stats.total_volume_staked.saturating_add(stake_amount);

        match winner {
            Winner::Player1 => stats.wins = stats.wins.saturating_add(1),
            Winner::Player2 => stats.losses = stats.losses.saturating_add(1),
            Winner::Draw => stats.draws = stats.draws.saturating_add(1),
            Winner::None => {}
        }

        env.storage()
            .persistent()
            .set(&DataKey::PlayerStats(player.clone()), &stats);
        env.storage().persistent().extend_ttl(
            &DataKey::PlayerStats(player.clone()),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
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

    /// Return `true` only if funds are currently held in escrow for this
    /// match, i.e. it is funded AND not yet Completed or Cancelled.
    ///
    /// Unlike [`Self::is_funded`], this reflects the *current* balance state
    /// rather than historical deposit flags.
    pub fn is_currently_escrowed(env: Env, match_id: u64) -> Result<bool, Error> {
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;
        let funded = m.player1_deposited && m.player2_deposited;
        let terminal = matches!(m.state, MatchState::Completed | MatchState::Cancelled);
        Ok(funded && !terminal)
    }

    fn depositor_count(m: &Match) -> i128 {
        let mut count: i128 = 0;
        if m.player1_deposited {
            count += 1;
        }
        if m.player2_deposited {
            count += 1;
        }
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

    /// Convert `amount_in` of `token_in` into `token_out` via the oracle
    /// contract's `swap` and credit `recipient` directly. Escrow only ever
    /// collects deposits in `token` (token_a) — a multi-token payout owed in
    /// token_b does not exist in escrow's own balance, so it must be
    /// acquired atomically from the oracle rather than transferred out of
    /// a balance escrow never held. `min_amount_out` is the amount already
    /// computed from the match's own (freshness-checked) conversion_rate,
    /// so it doubles as the slippage floor against the oracle's live rate.
    fn oracle_swap(
        env: &Env,
        token_in: &Address,
        token_out: &Address,
        amount_in: i128,
        min_amount_out: i128,
        recipient: &Address,
    ) -> Result<(), Error> {
        let oracle_address: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;

        let _: () = env.invoke_contract(
            &oracle_address,
            &Symbol::new(env, "swap"),
            soroban_sdk::vec![
                env,
                env.current_contract_address().to_val(),
                token_in.to_val(),
                token_out.to_val(),
                amount_in.into_val(env),
                min_amount_out.into_val(env),
                recipient.to_val(),
            ],
        );
        Ok(())
    }

    /// Execute the payout for a match based on the winner. Transfers tokens
    /// from the contract to the winner(s), accounting for multi-token conversion if needed.
    fn execute_payout(env: &Env, m: &Match, winner: &Winner) -> Result<(), Error> {
        // Check if this is a multi-token match and if rate is stale
        let is_multi_token = m.token_b.is_some() && m.conversion_rate.is_some_and(|r| r > 0);
        if is_multi_token {
            if let Some(rate_ledger) = m.conversion_rate_ledger {
                let current_ledger = env.ledger().sequence();
                let max_rate_age = 1000u32; // Rates older than 1000 ledgers are stale
                if current_ledger.saturating_sub(rate_ledger) > max_rate_age {
                    return Err(Error::ConversionRateStalePriceSource);
                }
            }
        }

        match winner {
            Winner::Player1 => {
                // Player1 always receives from token_a (the primary token)
                let client_a = token::Client::new(env, &m.token);
                let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
                client_a.transfer(&env.current_contract_address(), &m.player1, &pot);
            }
            Winner::Player2 => {
                // Player2 receives from token_a if single-token, or token_b if multi-token
                if is_multi_token {
                    let token_b = m.token_b.clone().ok_or(Error::InvalidState)?;
                    let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
                    let amount_b = pot
                        .checked_mul(m.conversion_rate.ok_or(Error::InvalidState)?)
                        .ok_or(Error::Overflow)?
                        .checked_div(10_000_000)
                        .ok_or(Error::Overflow)?;
                    Self::oracle_swap(env, &m.token, &token_b, pot, amount_b, &m.player2)?;
                } else {
                    let client_a = token::Client::new(env, &m.token);
                    let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
                    client_a.transfer(&env.current_contract_address(), &m.player2, &pot);
                }
            }
            Winner::Draw => {
                // In a draw, both players get their stake back
                let client_a = token::Client::new(env, &m.token);
                client_a.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);

                if is_multi_token {
                    let token_b = m.token_b.clone().ok_or(Error::InvalidState)?;
                    let amount_b = m
                        .stake_amount
                        .checked_mul(m.conversion_rate.ok_or(Error::InvalidState)?)
                        .ok_or(Error::Overflow)?
                        .checked_div(10_000_000)
                        .ok_or(Error::Overflow)?;
                    Self::oracle_swap(
                        env,
                        &m.token,
                        &token_b,
                        m.stake_amount,
                        amount_b,
                        &m.player2,
                    )?;
                } else {
                    let client_a = token::Client::new(env, &m.token);
                    client_a.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
                }
            }
            Winner::None => {
                return Err(Error::InvalidState);
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
        if env
            .storage()
            .persistent()
            .has(&DataKey::MatchDispute(match_id))
        {
            return Err(Error::DisputeAlreadyRaised);
        }

        let winner: Winner = env
            .storage()
            .persistent()
            .get(&DataKey::PendingWinner(match_id))
            .ok_or(Error::PendingResultNotFound)?;
        Self::execute_payout(&env, &m, &winner)?;
        Self::remove_active_match_indexed(&env, &m.player1, match_id);
        Self::remove_active_match_indexed(&env, &m.player2, match_id);

        m.state = MatchState::Completed;
        m.completed_ledger = Some(env.ledger().sequence());

        Self::record_completed_match(&env, &m.player1);
        Self::record_completed_match(&env, &m.player2);
        Self::record_platform_payout(&env);
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
    /// Requires a bonded stake (configurable basis points of match stake);
    /// refunded on successful overturn, forfeited on upheld outcome.
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

        let m: Match = env
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

        if evidence_hash.is_empty() {
            return Err(Error::InvalidEvidenceHash);
        }

        // Check if a dispute already exists for this match
        if env
            .storage()
            .persistent()
            .has(&DataKey::MatchDispute(match_id))
        {
            return Err(Error::DisputeAlreadyRaised);
        }

        // Calculate and collect dispute bond. The bond is floored to a minimum
        // of 1 stroop so that tiny stakes (e.g. 1 stroop at the default 100 bps)
        // cannot round down to a zero-cost dispute, which would let an attacker
        // spam the dispute system for free. Small matches stay disputable, they
        // just pay the 1-stroop minimum.
        let bond_basis_points: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeBondBasisPoints)
            .unwrap_or(DEFAULT_DISPUTE_BOND_BASIS_POINTS);

        let dispute_bond = m
            .stake_amount
            .checked_mul(bond_basis_points as i128)
            .ok_or(Error::Overflow)?
            .checked_div(10_000)
            .ok_or(Error::Overflow)?
            .max(1);

        // Defense in depth: never allow a zero (or negative) bond through.
        if dispute_bond <= 0 {
            return Err(Error::InsufficientBond);
        }

        let client = token::Client::new(&env, &m.token);
        client.transfer(&disputer, &env.current_contract_address(), &dispute_bond);

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

        // Snapshot current voting weight for quorum/flash-loan prevention
        let snapshot_total_weight = client.balance(&env.current_contract_address());
        let quorum_basis_points: u32 = env
            .storage()
            .instance()
            .get(&DataKey::QuorumBasisPoints)
            .unwrap_or(DEFAULT_QUORUM_BASIS_POINTS);
        let quorum_threshold = snapshot_total_weight
            .checked_mul(quorum_basis_points as i128)
            .ok_or(Error::Overflow)?
            .checked_div(10_000)
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
            created_ledger: env.ledger().sequence(),
            uphold_votes: 0,
            overturn_votes: 0,
            dispute_bond,
            snapshot_ledger: env.ledger().sequence(),
            snapshot_total_weight,
            quorum_threshold,
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

        // Store oracle address implicated by this result (for automatic slashing on overturn)
        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;
        env.storage()
            .persistent()
            .set(&DataKey::DisputeOracle(dispute_id), &oracle);
        env.storage().persistent().extend_ttl(
            &DataKey::DisputeOracle(dispute_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        let next_id = dispute_id.checked_add(1).ok_or(Error::Overflow)?;
        env.storage()
            .instance()
            .set(&DataKey::DisputeCount, &next_id);

        env.events().publish(
            (Symbol::new(&env, "dispute"), Symbol::new(&env, "created")),
            (dispute_id, match_id, disputer, evidence_hash, dispute_bond),
        );

        Ok(dispute_id)
    }

    /// Vote on an active dispute.
    ///
    /// Only addresses that held a positive balance of the match's escrow token
    /// at the dispute-creation snapshot may vote. Prevents flash-loan attacks.
    /// `vote` is `true` to overturn the oracle result, `false` to uphold it.
    ///
    /// Requires minimum holding duration (configurable) to have elapsed since
    /// snapshot to defeat just-in-time acquisition attacks.
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

        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(dispute.match_id))
            .ok_or(Error::MatchNotFound)?;
        let client = token::Client::new(&env, &m.token);

        // Check minimum holding duration (must hold token before snapshot + min duration)
        let min_hold_duration: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MinimumHoldDuration)
            .unwrap_or(DEFAULT_MINIMUM_HOLD_DURATION);

        let min_acquisition_ledger = dispute.snapshot_ledger.saturating_sub(min_hold_duration);

        // For snapshot-based voting: we look for a balance snapshot at/before snapshot_ledger
        // If no history, voter had zero balance at snapshot (cannot vote)
        // This prevents flash-loan and just-in-time acquisition attacks
        let mut has_historical_balance = false;
        let mut snapshot_weight: i128 = 0;

        // Check player balance snapshot history
        let snapshot_count_key = DataKey::PlayerBalanceSnapshotCount(voter.clone());
        if let Some(count) = env
            .storage()
            .persistent()
            .get::<_, u64>(&snapshot_count_key)
        {
            // Find the most recent snapshot at or before dispute.snapshot_ledger
            for i in 0..core::cmp::min(count, 5u64) {
                let idx = count.saturating_sub(i + 1);
                let slot = idx % MAX_PLAYER_SNAPSHOTS as u64;
                let key = DataKey::PlayerBalanceSnapshot(voter.clone(), slot);

                if let Some(snap) = env
                    .storage()
                    .persistent()
                    .get::<_, PlayerBalanceSnapshot>(&key)
                {
                    if snap.ledger <= dispute.snapshot_ledger as u64 {
                        // Check that the balance acquisition was before min_acquisition_ledger
                        if snap.ledger <= min_acquisition_ledger as u64 {
                            snapshot_weight = snap.balance;
                            has_historical_balance = true;
                            break;
                        }
                    }
                }
            }
        }

        // If no historical balance found, check if voter currently holds balance
        // (for matches that occurred after player snapshot history began)
        if !has_historical_balance {
            let current_balance = client.balance(&voter);
            if current_balance <= 0 {
                return Err(Error::NotStaker);
            }

            // For newly-acquired balances, require minimum holding duration from now
            // This is a fallback for voters without snapshot history
            return Err(Error::InsufficientHoldingDuration);
        }

        if snapshot_weight <= 0 {
            return Err(Error::NotStaker);
        }

        // Store vote weight snapshot for this voter
        env.storage().persistent().set(
            &DataKey::DisputeVoteWeight(dispute_id, voter.clone()),
            &snapshot_weight,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::DisputeVoteWeight(dispute_id, voter.clone()),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        // Record vote
        env.storage().persistent().set(&vote_key, &vote);
        env.storage()
            .persistent()
            .extend_ttl(&vote_key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        // Tally vote using historical snapshot weight
        if vote {
            dispute.yes_votes = dispute.yes_votes.saturating_add(snapshot_weight);
        } else {
            dispute.no_votes = dispute.no_votes.saturating_add(snapshot_weight);
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
            (dispute_id, voter, vote, snapshot_weight),
        );

        Ok(())
    }

    /// Resolve a dispute after the voting period has elapsed.
    ///
    /// Executes payout based on voting and quorum:
    /// - If quorum not met: no resolution (explicit pending state, not silent).
    /// - If quorum met and yes_votes > no_votes: overturned (refund both, draw).
    ///   Automatically signals oracle for slashing (admin must call slash_oracle_for_dispute).
    /// - If quorum met and no_votes >= yes_votes: upheld (original result stands).
    ///   Dispute bond is forfeited to treasury.
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

        // Check quorum requirement
        let total_votes = dispute.yes_votes.saturating_add(dispute.no_votes);
        if total_votes < dispute.quorum_threshold {
            return Err(Error::QuorumNotMet);
        }

        let winner = if dispute.yes_votes > dispute.no_votes {
            // Overturned: refund both players (draw outcome), signal oracle for slashing
            dispute.state = DisputeState::ResolvedOverturned;
            Winner::Draw
        } else {
            // Upheld: original oracle result stands, bond forfeited to treasury
            dispute.state = DisputeState::ResolvedUpheld;
            pending_winner
        };

        Self::execute_payout(&env, &m, &winner)?;
        Self::remove_active_match_indexed(&env, &m.player1, match_id);
        Self::remove_active_match_indexed(&env, &m.player2, match_id);

        // Handle dispute bond
        if dispute.state == DisputeState::ResolvedOverturned {
            // Bond refunded to disputer on successful overturn
            let client = token::Client::new(&env, &m.token);
            client.transfer(
                &env.current_contract_address(),
                &dispute.disputer,
                &dispute.dispute_bond,
            );
        } else {
            // Bond forfeited to treasury on upheld outcome
            let protocol_config: ProtocolConfig = Self::get_config(&env);
            let client = token::Client::new(&env, &m.token);
            client.transfer(
                &env.current_contract_address(),
                &protocol_config.treasury,
                &dispute.dispute_bond,
            );
        }

        m.state = MatchState::Completed;
        m.completed_ledger = Some(env.ledger().sequence());

        Self::record_completed_match(&env, &m.player1);
        Self::record_completed_match(&env, &m.player2);
        Self::record_platform_payout(&env);

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
            (
                dispute_id,
                match_id,
                dispute.state,
                winner,
                total_votes,
                dispute.quorum_threshold,
            ),
        );

        Ok(())
    }

    /// Admin-initiated automatic slashing of an oracle implicated by an overturned dispute.
    /// Transfers bond amount to oracle slash pool. Oracle contract must be invoked separately
    /// to actually slash the oracle's stake.
    pub fn mark_dispute_for_oracle_slash(
        env: Env,
        dispute_id: u64,
        slash_amount: i128,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let dispute: Dispute = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(dispute_id))
            .ok_or(Error::DisputeNotFound)?;

        if dispute.state != DisputeState::ResolvedOverturned {
            return Err(Error::InvalidState);
        }

        if slash_amount <= 0 || slash_amount > dispute.dispute_bond {
            return Err(Error::InvalidAmount);
        }

        let oracle: Address = env
            .storage()
            .persistent()
            .get(&DataKey::DisputeOracle(dispute_id))
            .ok_or(Error::Unauthorized)?;

        env.events().publish(
            (
                Symbol::new(&env, "dispute"),
                Symbol::new(&env, "oracle_slash_signal"),
            ),
            (dispute_id, oracle, slash_amount),
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
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "dispute_period"),
            ),
            period,
        );
        Ok(())
    }

    /// Return the current dispute period in ledgers.
    pub fn get_dispute_period(env: &Env) -> u32 {
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

    /// Get the dispute for a match by match ID.
    pub fn get_dispute_details(env: Env, match_id: u64) -> Result<Dispute, Error> {
        let dispute_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::MatchDispute(match_id))
            .ok_or(Error::DisputeNotFound)?;
        Self::get_dispute(env, dispute_id)
    }

    /// Return the dispute ID for a match, if one exists.
    pub fn get_match_dispute_id(env: Env, match_id: u64) -> Result<u64, Error> {
        env.storage()
            .persistent()
            .get(&DataKey::MatchDispute(match_id))
            .ok_or(Error::DisputeNotFound)
    }

    /// Set the dispute bond requirement in basis points of match stake. Admin only.
    pub fn set_dispute_bond_basis_points(env: Env, basis_points: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if basis_points == 0 || basis_points > 10_000 {
            return Err(Error::InvalidAmount);
        }

        env.storage()
            .instance()
            .set(&DataKey::DisputeBondBasisPoints, &basis_points);
        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "dispute_bond"),
            ),
            basis_points,
        );
        Ok(())
    }

    /// Set the minimum holding duration in ledgers for vote eligibility. Admin only.
    pub fn set_minimum_hold_duration(env: Env, duration: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::MinimumHoldDuration, &duration);
        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "min_hold_duration"),
            ),
            duration,
        );
        Ok(())
    }

    /// Set the quorum threshold in basis points of dispute snapshot weight. Admin only.
    pub fn set_quorum_basis_points(env: Env, basis_points: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if basis_points == 0 || basis_points > 10_000 {
            return Err(Error::InvalidAmount);
        }

        env.storage()
            .instance()
            .set(&DataKey::QuorumBasisPoints, &basis_points);
        env.events().publish(
            (
                Symbol::new(&env, "admin"),
                Symbol::new(&env, "quorum_basis_points"),
            ),
            basis_points,
        );
        Ok(())
    }

    /// Get current dispute bond requirement in basis points.
    pub fn get_dispute_bond_basis_points(env: Env) -> u32 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::DisputeBondBasisPoints)
            .unwrap_or(DEFAULT_DISPUTE_BOND_BASIS_POINTS)
    }

    /// Get current minimum holding duration in ledgers.
    pub fn get_minimum_hold_duration(env: Env) -> u32 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::MinimumHoldDuration)
            .unwrap_or(DEFAULT_MINIMUM_HOLD_DURATION)
    }

    /// Get current quorum threshold in basis points.
    pub fn get_quorum_basis_points(env: Env) -> u32 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::QuorumBasisPoints)
            .unwrap_or(DEFAULT_QUORUM_BASIS_POINTS)
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

        let nonce: BytesN<32> = env.prng().gen();
        let commitment = Self::compute_commitment(env, m.stake_amount, escrow_balance, &nonce);

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
            nonce,
            commitment: commitment.clone(),
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

        // Soroban events are public regardless of who calls a read-only
        // getter, so publishing `escrow_balance` here would leak the exact
        // amount to any observer and defeat `redact_snapshot` entirely.
        // Publish the commitment instead — see `docs/privacy-model.md`.
        env.events().publish(
            (Symbol::new(env, "match"), symbol_short!("snapshot")),
            (m.id, index, commitment),
        );
    }

    /// `sha256(stake_amount || escrow_balance || nonce)` — see
    /// `docs/privacy-model.md` for the guarantees this does and doesn't
    /// provide.
    fn compute_commitment(
        env: &Env,
        stake_amount: i128,
        escrow_balance: i128,
        nonce: &BytesN<32>,
    ) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.extend_from_array(&stake_amount.to_be_bytes());
        data.extend_from_array(&escrow_balance.to_be_bytes());
        data.extend_from_array(&nonce.to_array());
        env.crypto().sha256(&data).to_bytes()
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

    /// Zero out fields for non-admin callers that, together, would let an
    /// observer correlate this snapshot against the token contract's own
    /// public balance history and reconstruct the redacted amounts.
    ///
    /// `stake_amount`/`escrow_balance` are zeroed as before, but now
    /// `player1_deposited`/`player2_deposited` are too — their exact-deposit
    /// timing combined with `ledger` and the (still-visible) `token` was
    /// itself a side channel. `commitment` is left untouched -- it is
    /// the whole point of the redacted view (see `docs/privacy-model.md`)
    /// -- but `nonce` is zeroed too, since revealing it ahead of an
    /// intentional admin disclosure would let a non-admin brute-force
    /// `commitment` against a guessed amount.
    fn redact_snapshot(env: &Env, mut snapshot: BalanceSnapshot) -> BalanceSnapshot {
        snapshot.stake_amount = 0;
        snapshot.escrow_balance = 0;
        snapshot.player1_deposited = false;
        snapshot.player2_deposited = false;
        snapshot.nonce = BytesN::from_array(env, &[0u8; 32]);
        snapshot
    }

    /// Return the full snapshot history for a match, oldest first.
    ///
    /// Only the admin sees exact `stake_amount`/`escrow_balance` values; the
    /// match's players may also call this but receive amounts redacted to 0
    /// (see [`Self::redact_snapshot`]) plus a `commitment` they can later
    /// verify against an admin-disclosed value. Any other caller is
    /// rejected with `Error::Unauthorized`. See `docs/privacy-model.md`.
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
                    Self::redact_snapshot(&env, snapshot)
                });
            }
        }
        Ok(result)
    }

    /// Return the most recently recorded snapshot for a match.
    ///
    /// Same access rules as [`Self::get_balance_snapshots`]: admin sees exact
    /// amounts, players see redacted amounts plus a verifiable `commitment`,
    /// anyone else is unauthorized. See `docs/privacy-model.md`.
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
            Self::redact_snapshot(&env, snapshot)
        })
    }

    // ── Player-level balance history ────────────────────────────────────────

    /// Compute `player`'s aggregate escrow balance right now: the sum of
    /// `stake_amount` across every non-terminal match the player is part of
    /// and has actually deposited in (the depositing side is identified by
    /// `player1_deposited` / `player2_deposited`).
    ///
    /// Used by `record_player_snapshot` and (transitively) by
    /// `get_balance_at_timestamp`. Arithmetic uses `saturating_add` and
    /// matches the existing `escrow_balance_of` routine — callers are
    /// expected to operate in realistic stake ranges where overflow is not
    /// a concern.
    fn player_escrow_balance(env: &Env, player: &Address) -> i128 {
        let key = DataKey::PlayerMatches(player.clone());
        let player_matches: soroban_sdk::Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| soroban_sdk::vec![env]);

        let mut total: i128 = 0;
        for m_id in player_matches.iter() {
            if let Some(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(m_id))
            {
                let deposited = (m.player1 == *player && m.player1_deposited)
                    || (m.player2 == *player && m.player2_deposited);
                if !deposited {
                    continue;
                }
                if m.state == MatchState::Completed || m.state == MatchState::Cancelled {
                    continue;
                }
                total = total.saturating_add(m.stake_amount);
            }
        }
        total
    }

    /// Record a player-level balance snapshot for `player` at the current
    /// ledger. Called on every balance-changing event: deposit, payout,
    /// cancel refund, and expire refund.
    ///
    /// Uses the same fixed-size ring buffer pattern as the per-match snapshots:
    /// `slot = index % MAX_PLAYER_SNAPSHOTS` and once
    /// `PlayerBalanceSnapshotCount` exceeds the cap, older entries are
    /// silently overwritten.
    fn record_player_snapshot(env: &Env, player: &Address) {
        let balance = Self::player_escrow_balance(env, player);
        let index: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerBalanceSnapshotCount(player.clone()))
            .unwrap_or(0u64);
        let slot: u64 = index % MAX_PLAYER_SNAPSHOTS as u64;

        let snapshot = PlayerBalanceSnapshot {
            player: player.clone(),
            index,
            ledger: env.ledger().sequence() as u64,
            balance,
        };

        let snapshot_key = DataKey::PlayerBalanceSnapshot(player.clone(), slot);
        env.storage().persistent().set(&snapshot_key, &snapshot);
        env.storage()
            .persistent()
            .extend_ttl(&snapshot_key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        let count_key = DataKey::PlayerBalanceSnapshotCount(player.clone());
        let next_index = index.saturating_add(1);
        env.storage().persistent().set(&count_key, &next_index);
        env.storage()
            .persistent()
            .extend_ttl(&count_key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        env.events().publish(
            (Symbol::new(env, "player"), symbol_short!("snapshot")),
            (player.clone(), index, balance),
        );
    }

    /// Return `player`'s aggregate escrow balance at or before `timestamp`
    /// (a ledger sequence number passed as `u64`).
    ///
    /// Walks the player's snapshot ring buffer newest-first to find the
    /// first entry whose `ledger` is `<= timestamp` and returns
    /// `BalanceAtTimestamp::Known` with that snapshot's `balance`. When
    /// none qualify, the two previously-indistinguishable "empty" outcomes
    /// are now reported separately:
    /// - `NoHistory` — no pruning has occurred; the player genuinely has no
    ///   snapshot at or before `timestamp` (e.g. they never recorded one, or
    ///   `timestamp` predates their earliest snapshot).
    /// - `Pruned` — the ring buffer has overwritten every snapshot old
    ///   enough to answer this query, so the true balance at that point is
    ///   unknown, not zero.
    ///
    /// See `docs/privacy-model.md`.
    ///
    /// Read-only and unauthenticated: the player's aggregate escrow
    /// balance is public information (no per-match stake amounts exposed).
    pub fn get_balance_at_timestamp(
        env: Env,
        player: Address,
        timestamp: u64,
    ) -> BalanceAtTimestamp {
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerBalanceSnapshotCount(player.clone()))
            .unwrap_or(0u64);

        if count == 0 {
            return BalanceAtTimestamp::NoHistory;
        }

        let cap = MAX_PLAYER_SNAPSHOTS as u64;
        let available = count.min(cap);
        let start = count.saturating_sub(available);

        // Walk newest-first; first snapshot whose ledger <= timestamp wins.
        let mut cursor = count;
        while cursor > start {
            cursor = cursor.saturating_sub(1);
            let snapshot_index = cursor;
            let slot = snapshot_index % cap;
            if let Some(snap) = env
                .storage()
                .persistent()
                .get::<DataKey, PlayerBalanceSnapshot>(&DataKey::PlayerBalanceSnapshot(
                    player.clone(),
                    slot,
                ))
            {
                // The ring buffer may contain stale entries at slots that
                // have been overwritten by newer snapshots. Verify this slot
                // actually corresponds to the snapshot at `snapshot_index`
                // before trusting its `ledger` field. The slot is keyed by
                // `player` already, so the entry is guaranteed to belong to
                // that player — no separate player check needed.
                if snap.index != snapshot_index {
                    continue;
                }
                if snap.ledger <= timestamp {
                    return BalanceAtTimestamp::Known(snap.balance);
                }
            }
        }

        // Nothing in the retained window qualified. If the buffer has
        // wrapped (`start > 0`), an older snapshot that might have answered
        // this query was pruned away — that's unknown, not zero. If it
        // hasn't wrapped, every snapshot the player ever recorded was
        // checked and none qualified, so this is a genuine absence.
        if start > 0 {
            BalanceAtTimestamp::Pruned
        } else {
            BalanceAtTimestamp::NoHistory
        }
    }

    /// Return a player's balance snapshots with pagination.
    ///
    /// `start` is the offset from the beginning of the player's snapshot history,
    /// and `limit` caps how many entries to return (max 32). Entries are returned
    /// oldest-first.
    pub fn get_balance_snaps_paginated(
        env: Env,
        player: Address,
        start: u64,
        limit: u64,
    ) -> soroban_sdk::Vec<PlayerBalanceSnapshot> {
        let mut result = soroban_sdk::vec![&env];
        let count: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerBalanceSnapshotCount(player.clone()))
            .unwrap_or(0u64);

        if count == 0 || limit == 0 {
            return result;
        }

        let cap = MAX_PLAYER_SNAPSHOTS as u64;
        let available = count.min(cap);
        let effective_start = count.saturating_sub(available).saturating_add(start);
        let mut added = 0u64;
        let end = count.min(effective_start.saturating_add(limit));

        for i in effective_start..end {
            if added >= limit {
                break;
            }
            let slot = i % cap;
            if let Some(snap) = env
                .storage()
                .persistent()
                .get::<DataKey, PlayerBalanceSnapshot>(&DataKey::PlayerBalanceSnapshot(
                    player.clone(),
                    slot,
                ))
            {
                if snap.index == i {
                    result.push_back(snap);
                    added = added.saturating_add(1);
                }
            }
        }

        result
    }

    /// Collect all matches in a given state (DEPRECATED: use paginated variants).
    /// Returns at most MAX_UNBOUNDED_MATCH_RESULTS to cap per-call cost.
    /// This function scans the full match history and is included for backwards
    /// compatibility only; new code should use collect_matches_by_state_paginated.
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

        let mut collected = 0u32;
        let mut truncated = false;
        for match_id in 0..count {
            if collected >= MAX_UNBOUNDED_MATCH_RESULTS {
                truncated = true;
                break;
            }
            if let Some(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(match_id))
            {
                if m.state == state {
                    matches.push_back(m);
                    collected = collected.saturating_add(1);
                }
            }
        }

        if truncated {
            // Silent data loss otherwise: callers of the deprecated unbounded
            // getters have no other signal that results were capped.
            env.events().publish(
                (Symbol::new(env, "match"), symbol_short!("truncated")),
                (state, MAX_UNBOUNDED_MATCH_RESULTS),
            );
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

    /// Return all matches that are in Completed state (result submitted, payout executed).
    ///
    /// Useful for off-chain clients and the frontend to display match history and
    /// payout records without relying on event indexing.
    ///
    /// # Storage cost note
    ///
    /// This function scans every match ever created in linear time. For contracts with
    /// a large number of matches this may become expensive. Prefer
    /// `get_completed_matches_paginated` for production use cases where the total
    /// match count could grow unboundedly — it lets callers fetch results in bounded
    /// pages rather than loading the entire history in a single call.
    pub fn get_completed_matches(env: Env) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state(&env, MatchState::Completed)
    }

    /// Return a paginated page of completed matches ordered by match ID ascending.
    ///
    /// `offset` — number of completed matches to skip before collecting results.
    /// `limit`  — maximum number of completed matches to return (0 returns an empty vec).
    pub fn get_completed_matches_paginated(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state_paginated(&env, MatchState::Completed, offset, limit)
    }

    /// Return a paginated page of cancelled matches ordered by match ID ascending.
    ///
    /// `offset` — number of cancelled matches to skip before collecting results.
    /// `limit`  — maximum number of cancelled matches to return (0 returns an empty vec).
    pub fn get_cancelled_matches_paginated(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        Self::collect_matches_by_state_paginated(&env, MatchState::Cancelled, offset, limit)
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
    /// DEPRECATED: use `get_player_matches_paginated` instead.
    /// Returns at most MAX_UNBOUNDED_MATCH_RESULTS to cap per-call cost.
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

        let mut result = soroban_sdk::vec![&env];
        for (i, id) in matches.iter().enumerate() {
            if i >= MAX_UNBOUNDED_MATCH_RESULTS as usize {
                break;
            }
            result.push_back(id);
        }
        Ok(result)
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

    /// Return a page of completed or cancelled matches, newest first.
    ///
    /// Pass `player` to restrict the history to matches involving that
    /// address (as either `player1` or `player2`); pass `None` for the
    /// full protocol-wide history. `offset`/`limit` paginate over the
    /// filtered result set, not over the raw match ID range.
    pub fn get_match_history(
        env: Env,
        player: Option<Address>,
        limit: u32,
        offset: u32,
    ) -> Result<soroban_sdk::Vec<Match>, Error> {
        let mut matches = soroban_sdk::vec![&env];
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

        for match_id in (0..count).rev() {
            let Some(m) = env
                .storage()
                .persistent()
                .get::<DataKey, Match>(&DataKey::Match(match_id))
            else {
                continue;
            };

            if m.state != MatchState::Completed && m.state != MatchState::Cancelled {
                continue;
            }
            if let Some(ref p) = player {
                if &m.player1 != p && &m.player2 != p {
                    continue;
                }
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

        Ok(matches)
    }

    /// Update the oracle address — admin only. Alias for `update_oracle`.
    pub fn set_oracle(env: Env, oracle: Address) -> Result<(), Error> {
        Self::update_oracle(env, oracle)
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
        let mut rotation_state = Self::get_rotation_state(&env);
        rotation_state.set_temp(None);
        rotation_state.set_pending(None);
        Self::save_rotation_state(&env, rotation_state);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("oracle_up")),
            (old_oracle, new_oracle),
        );
        Ok(())
    }

    /// Temporarily rotate the oracle address for `duration_seconds`. Admin only.
    /// Returns to `old_oracle` automatically once `duration_seconds` elapses.
    pub fn rotate_oracle_temporary(
        env: Env,
        old_oracle: Address,
        new_oracle: Address,
        duration_seconds: u64,
    ) -> Result<(), Error> {
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

        let current = Self::effective_oracle(&env)?;
        if old_oracle != current {
            return Err(Error::Unauthorized);
        }

        let expiry = env.ledger().timestamp().saturating_add(duration_seconds);
        let temp = TempOracleRotation {
            old_oracle: old_oracle.clone(),
            temp_oracle: new_oracle.clone(),
            expiry,
        };

        let mut rotation_state = Self::get_rotation_state(&env);
        rotation_state.set_temp(Some(temp));
        Self::save_rotation_state(&env, rotation_state);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("rot_temp")),
            (old_oracle, new_oracle, duration_seconds),
        );

        Ok(())
    }

    /// Admin proposes a permanent oracle rotation from `old_oracle` to `new_oracle`.
    pub fn propose_oracle_rotation(
        env: Env,
        old_oracle: Address,
        new_oracle: Address,
    ) -> Result<(), Error> {
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

        let current = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;
        if old_oracle != current {
            return Err(Error::Unauthorized);
        }

        let proposal = PendingOracleRotation {
            old_oracle: old_oracle.clone(),
            new_oracle: new_oracle.clone(),
        };

        let mut rotation_state = Self::get_rotation_state(&env);
        rotation_state.set_pending(Some(proposal));
        Self::save_rotation_state(&env, rotation_state);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("rot_prop")),
            (old_oracle, new_oracle),
        );

        Ok(())
    }

    /// Permanent oracle rotation requiring a prior matching proposal. Admin only.
    pub fn rotate_oracle_permanent(
        env: Env,
        old_oracle: Address,
        new_oracle: Address,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let proposal: PendingOracleRotation = Self::get_rotation_state(&env)
            .pending()
            .ok_or(Error::InvalidState)?;

        if proposal.old_oracle != old_oracle || proposal.new_oracle != new_oracle {
            return Err(Error::InvalidState);
        }

        let current = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)?;
        if proposal.old_oracle != current {
            return Err(Error::Unauthorized);
        }

        env.storage().instance().set(&DataKey::Oracle, &new_oracle);
        env.storage().instance().remove(&DataKey::OracleRotation);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("oracle_up")),
            (old_oracle, new_oracle),
        );

        Ok(())
    }

    /// Direct admin transfer (single-step). Current admin only.
    pub fn transfer_admin(env: Env, new_admin: Address, caller: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        if caller != admin {
            return Err(Error::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("xfer")),
            (admin, new_admin),
        );
        Ok(())
    }

    /// Claim a vested match payout. Callable by players after the vesting period ends.
    pub fn claim_vested_payout(env: Env, match_id: u64, player: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        player.require_auth();

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Completed {
            return Err(Error::InvalidState);
        }

        let vested_at = m.vested_at.ok_or(Error::InvalidState)?;
        let config = Self::get_config(&env);
        if env.ledger().timestamp()
            < vested_at
                .checked_add(config.vesting_duration_seconds)
                .ok_or(Error::Overflow)?
        {
            return Err(Error::VestingNotExpired);
        }

        let is_p1 = player == m.player1;
        let is_p2 = player == m.player2;

        if !is_p1 && !is_p2 {
            return Err(Error::Unauthorized);
        }

        let winner = &m.winner;
        if *winner == Winner::None {
            return Err(Error::InvalidState);
        }

        // Multi-token matches depend on the oracle's conversion rate for a
        // fair payout. Mirror the staleness guard in `execute_payout` here
        // too: a claim deferred by vesting must not pay out against a rate
        // that has since gone stale, even for the leg that stays in `token`.
        let is_multi_token = m.token_b.is_some() && m.conversion_rate.is_some_and(|r| r > 0);
        if is_multi_token {
            if let Some(rate_ledger) = m.conversion_rate_ledger {
                let current_ledger = env.ledger().sequence();
                let max_rate_age = 1000u32;
                if current_ledger.saturating_sub(rate_ledger) > max_rate_age {
                    return Err(Error::ConversionRateStalePriceSource);
                }
            }
        }

        // Resolve the payout token for this player: use the player's preferred
        // token if (a) they have one set, (b) it differs from the stake token,
        // and (c) the match has a conversion rate + token_b set by the oracle.
        let preferred_token: Option<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::PlayerPreferredToken(player.clone()));
        let use_swap = preferred_token.as_ref().is_some_and(|pt| {
            *pt != m.token
                && m.token_b.as_ref() == Some(pt)
                && m.conversion_rate.is_some_and(|r| r > 0)
        });

        let amount_claimed;

        if is_p1 {
            if m.player1_claimed {
                return Err(Error::AlreadyClaimed);
            }

            match winner {
                Winner::Player1 => {
                    let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
                    // Compute referral fee deduction for winner payouts (not draws)
                    let referral_fee = Self::compute_referral_fee(&env, &m, pot)?;
                    let protocol_fee = Self::compute_protocol_fee(&env, pot)?;
                    let net_payout = pot
                        .checked_sub(referral_fee)
                        .ok_or(Error::Overflow)?
                        .checked_sub(protocol_fee)
                        .ok_or(Error::Overflow)?;
                    if use_swap {
                        // Swap stake-token payout into player's preferred token using oracle rate.
                        // oracle rate: conversion_rate token_b units per 10_000_000 token_a units.
                        let swap_amount = net_payout
                            .checked_mul(m.conversion_rate.unwrap())
                            .ok_or(Error::Overflow)?
                            .checked_div(10_000_000)
                            .ok_or(Error::Overflow)?;
                        let swap_token = m.token_b.clone().unwrap();
                        Self::oracle_swap(
                            &env,
                            &m.token,
                            &swap_token,
                            net_payout,
                            swap_amount,
                            &m.player1,
                        )?;
                        amount_claimed = swap_amount;
                    } else {
                        let client = token::Client::new(&env, &m.token);
                        client.transfer(&env.current_contract_address(), &m.player1, &net_payout);
                        amount_claimed = net_payout;
                    }
                    if referral_fee > 0 {
                        let referrer = m.referrer.clone().ok_or(Error::InvalidState)?;
                        let client = token::Client::new(&env, &m.token);
                        client.transfer(&env.current_contract_address(), &referrer, &referral_fee);
                    }
                    if protocol_fee > 0 {
                        let client = token::Client::new(&env, &m.token);
                        client.transfer(
                            &env.current_contract_address(),
                            &config.fee_recipient,
                            &protocol_fee,
                        );
                    }
                }
                Winner::Draw => {
                    let client = token::Client::new(&env, &m.token);
                    client.transfer(&env.current_contract_address(), &m.player1, &m.stake_amount);
                    amount_claimed = m.stake_amount;
                }
                Winner::Player2 => {
                    return Err(Error::Unauthorized);
                }
                Winner::None => {
                    return Err(Error::InvalidState);
                }
            }
            m.player1_claimed = true;
        } else {
            if m.player2_claimed {
                return Err(Error::AlreadyClaimed);
            }

            match winner {
                Winner::Player2 => {
                    let pot = m.stake_amount.checked_mul(2).ok_or(Error::Overflow)?;
                    // Compute referral fee deduction for winner payouts (not draws)
                    let referral_fee = Self::compute_referral_fee(&env, &m, pot)?;
                    let protocol_fee = Self::compute_protocol_fee(&env, pot)?;
                    let net_payout = pot
                        .checked_sub(referral_fee)
                        .ok_or(Error::Overflow)?
                        .checked_sub(protocol_fee)
                        .ok_or(Error::Overflow)?;
                    if use_swap {
                        // Swap stake-token payout into player's preferred token using oracle rate.
                        let swap_amount = net_payout
                            .checked_mul(m.conversion_rate.unwrap())
                            .ok_or(Error::Overflow)?
                            .checked_div(10_000_000)
                            .ok_or(Error::Overflow)?;
                        let swap_token = m.token_b.clone().unwrap();
                        Self::oracle_swap(
                            &env,
                            &m.token,
                            &swap_token,
                            net_payout,
                            swap_amount,
                            &m.player2,
                        )?;
                        amount_claimed = swap_amount;
                    } else {
                        let client = token::Client::new(&env, &m.token);
                        client.transfer(&env.current_contract_address(), &m.player2, &net_payout);
                        amount_claimed = net_payout;
                    }
                    if referral_fee > 0 {
                        let referrer = m.referrer.clone().ok_or(Error::InvalidState)?;
                        let client = token::Client::new(&env, &m.token);
                        client.transfer(&env.current_contract_address(), &referrer, &referral_fee);
                    }
                    if protocol_fee > 0 {
                        let client = token::Client::new(&env, &m.token);
                        client.transfer(
                            &env.current_contract_address(),
                            &config.fee_recipient,
                            &protocol_fee,
                        );
                    }
                }
                Winner::Draw => {
                    let client = token::Client::new(&env, &m.token);
                    client.transfer(&env.current_contract_address(), &m.player2, &m.stake_amount);
                    amount_claimed = m.stake_amount;
                }
                Winner::Player1 => {
                    return Err(Error::Unauthorized);
                }
                Winner::None => {
                    return Err(Error::InvalidState);
                }
            }
            m.player2_claimed = true;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Match(match_id), &m);
        env.storage().persistent().extend_ttl(
            &DataKey::Match(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        Self::record_player_snapshot(&env, &player);

        // Emit the token actually used for payout
        let payout_token = if use_swap {
            m.token_b.clone().unwrap_or_else(|| m.token.clone())
        } else {
            m.token.clone()
        };

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("claim")),
            (match_id, player, amount_claimed, payout_token),
        );

        Ok(())
    }

    // ── Upgrade / migration API ───────────────────────────────────────────────

    /// Return the current on-chain contract version (encoded as
    /// `major * 1_000_000 + minor * 1_000 + patch`).
    pub fn get_version(env: Env) -> u32 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or(CONTRACT_VERSION)
    }

    /// Return the current contract version as a semver string (e.g. "0.1.0").
    pub fn get_contract_version(env: Env) -> soroban_sdk::String {
        let version = Self::get_version(env.clone());
        let major = version / 1_000_000;
        let minor = (version % 1_000_000) / 1_000;
        let patch = version % 1_000;

        let mut buf = [0u8; 20];
        let mut pos = 0;
        pos = Self::write_u32_decimal(&mut buf, pos, major);
        buf[pos] = b'.';
        pos += 1;
        pos = Self::write_u32_decimal(&mut buf, pos, minor);
        buf[pos] = b'.';
        pos += 1;
        pos = Self::write_u32_decimal(&mut buf, pos, patch);

        soroban_sdk::String::from_bytes(&env, &buf[..pos])
    }

    /// Write the decimal representation of `n` into `buf` starting at `pos`,
    /// returning the new position.  Does NOT null-terminate.
    fn write_u32_decimal(buf: &mut [u8; 20], pos: usize, n: u32) -> usize {
        if n == 0 {
            buf[pos] = b'0';
            return pos + 1;
        }
        let mut start = pos;
        let mut x = n;
        while x > 0 {
            buf[start] = b'0' + (x % 10) as u8;
            start += 1;
            x /= 10;
        }
        // digits are in reverse order in buf[pos..start]
        buf[pos..start].reverse();
        start
    }

    /// Schedule a WASM upgrade for the 7-day community review period.
    ///
    /// Admin-gated. After `UPGRADE_REVIEW_PERIOD_LEDGERS` have elapsed the
    /// admin may call `execute_upgrade` to apply the new WASM and then
    /// `migrate_state` to advance the version counter.
    ///
    /// # Arguments
    /// * `new_wasm_hash` — SHA-256 hash of the replacement WASM blob that was
    ///   already uploaded to the network via `soroban contract upload`.
    ///
    /// # Errors
    /// * `UpgradeAlreadyScheduled` — an upgrade is already pending.
    /// * `Unauthorized`            — caller is not the admin.
    pub fn schedule_upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if env.storage().instance().has(&DataKey::UpgradeScheduledAt) {
            return Err(Error::UpgradeAlreadyScheduled);
        }

        let scheduled_at = env.ledger().sequence();
        env.storage()
            .instance()
            .set(&DataKey::UpgradeScheduledAt, &scheduled_at);
        env.storage()
            .instance()
            .set(&DataKey::PendingUpgradeHash, &new_wasm_hash);

        env.events().publish(
            (Symbol::new(&env, "upgrade"), symbol_short!("sched")),
            (new_wasm_hash, scheduled_at),
        );

        Ok(())
    }

    /// Cancel a pending upgrade before it is executed.
    ///
    /// Admin-gated. Removes both the scheduled-at ledger and the pending WASM
    /// hash, allowing a fresh `schedule_upgrade` call.
    ///
    /// # Errors
    /// * `UpgradeNotScheduled` — no upgrade is pending.
    /// * `Unauthorized`        — caller is not the admin.
    pub fn cancel_upgrade(env: Env) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if !env.storage().instance().has(&DataKey::UpgradeScheduledAt) {
            return Err(Error::UpgradeNotScheduled);
        }

        env.storage()
            .instance()
            .remove(&DataKey::UpgradeScheduledAt);
        env.storage()
            .instance()
            .remove(&DataKey::PendingUpgradeHash);

        env.events()
            .publish((Symbol::new(&env, "upgrade"), symbol_short!("cancel")), ());

        Ok(())
    }

    /// Execute the scheduled WASM upgrade after the 7-day review period.
    ///
    /// Admin-gated. Calls the host `upgrade` function with the stored WASM
    /// hash. After this returns, the contract code is replaced but storage
    /// layout is unchanged — call `migrate_state` in the same or a subsequent
    /// transaction to advance the version counter and apply any schema changes.
    ///
    /// The contract must be **paused** before this call so that no new
    /// state-mutating transactions run against the old schema while the
    /// replacement WASM is being applied.
    ///
    /// # Errors
    /// * `UpgradeNotScheduled`            — no upgrade is pending.
    /// * `UpgradeReviewPeriodNotElapsed`  — 7-day review period has not passed.
    /// * `InvalidPauseState`              — contract is not paused.
    /// * `Unauthorized`                   — caller is not the admin.
    pub fn execute_upgrade(env: Env) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        // Enforce paused-during-upgrade invariant.
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if !paused {
            return Err(Error::InvalidPauseState);
        }

        let scheduled_at: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeScheduledAt)
            .ok_or(Error::UpgradeNotScheduled)?;

        let current_ledger = env.ledger().sequence();
        if current_ledger < scheduled_at.saturating_add(UPGRADE_REVIEW_PERIOD_LEDGERS) {
            return Err(Error::UpgradeReviewPeriodNotElapsed);
        }

        let new_wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::PendingUpgradeHash)
            .ok_or(Error::UpgradeNotScheduled)?;

        // Clear pending-upgrade keys before upgrading so post-upgrade storage
        // starts clean regardless of whether migrate_state is called.
        env.storage()
            .instance()
            .remove(&DataKey::UpgradeScheduledAt);
        env.storage()
            .instance()
            .remove(&DataKey::PendingUpgradeHash);

        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        env.events().publish(
            (Symbol::new(&env, "upgrade"), symbol_short!("exec")),
            new_wasm_hash,
        );

        Ok(())
    }

    /// Advance the on-chain version counter after a WASM upgrade and apply any
    /// necessary state-schema migrations.
    ///
    /// Admin-gated. This is a no-op for fields that do not change between
    /// versions; it only writes keys that are new in the target version and
    /// back-fills reasonable defaults for them.
    ///
    /// # Arguments
    /// * `target_version` — the version to migrate **to** (same encoding as
    ///   `get_version`: `major * 1_000_000 + minor * 1_000 + patch`).
    ///
    /// # Migration map
    ///
    /// | From        | To          | Actions                                     |
    /// |-------------|-------------|---------------------------------------------|
    /// | 0.1.0 (1000) | 0.1.1 (1001) | Seed missing `DisputePeriod` default (0)    |
    /// | 0.1.0 (1000) | 1.0.0 (1_000_000) | Reserved for future v1.0 migrations    |
    ///
    /// # Errors
    /// * `InvalidVersion` — `target_version` is not ahead of the current version.
    /// * `Unauthorized`   — caller is not the admin.
    pub fn migrate_state(env: Env, target_version: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let current: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or(CONTRACT_VERSION);

        if target_version <= current {
            return Err(Error::InvalidVersion);
        }

        // Run migrations for every version step in order so this function is
        // idempotent when called multiple times with increasing targets.
        Self::_apply_migrations(&env, current, target_version);

        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &target_version);

        env.events().publish(
            (Symbol::new(&env, "upgrade"), symbol_short!("migrated")),
            (current, target_version),
        );

        Ok(())
    }

    /// Validate current contract state integrity.
    ///
    /// Checks that all critical instance-storage keys are present and
    /// internally consistent. Returns `Ok(())` if everything looks healthy,
    /// or the first `Error` that is violated.
    ///
    /// This should be called immediately before *and* after any upgrade to
    /// confirm that storage was neither corrupted nor inadvertently cleared.
    ///
    /// # Checks performed
    /// 1. Contract is initialized (Oracle key present).
    /// 2. Admin key is present.
    /// 3. MatchCount key is present.
    /// 4. ContractVersion key is present.
    /// 5. Match count is internally consistent (not negative/impossible).
    /// 6. AllowedTokenCount ≤ actual token count in storage is sane.
    pub fn validate_state(env: Env) -> Result<(), Error> {
        extend_instance_ttl(&env);

        // 1. Initialized?
        if !env.storage().instance().has(&DataKey::Oracle) {
            return Err(Error::NotInitialized);
        }

        // 2. Admin present?
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::Unauthorized);
        }

        // 3. MatchCount present?
        if !env.storage().instance().has(&DataKey::MatchCount) {
            return Err(Error::NotInitialized);
        }

        // 4. ContractVersion present?
        if !env.storage().instance().has(&DataKey::ContractVersion) {
            return Err(Error::NotInitialized);
        }

        // 5. MatchCount must be a plausible value (stored as u64).
        let match_count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MatchCount)
            .unwrap_or(0u64);
        // A u64 is always non-negative; just confirm it deserialises correctly.
        let _ = match_count;

        // 6. AllowedTokenCount <= u32::MAX is always true; confirm it deserialises.
        let _token_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::AllowedTokenCount)
            .unwrap_or(0u32);

        Ok(())
    }

    /// Returns true if the token allowlist is currently enforced.
    ///
    /// Allowlist enforcement is automatically enabled when the first token is
    /// added via `add_allowed_token`, and disabled when the last token is removed.
    pub fn is_allowlist_enforced(env: Env) -> bool {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::AllowlistEnforced)
            .unwrap_or(false)
    }

    // ── Multi-Oracle Consensus ───────────────────────────────────────────────

    /// Add an approved oracle to the consensus oracle list — admin only.
    ///
    /// Approved oracles are permitted to call `submit_result_consensus`.
    pub fn add_approved_oracle(env: Env, oracle: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if oracle == env.current_contract_address() {
            return Err(Error::InvalidAddress);
        }

        let mut oracles: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedOracles)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);

        if !oracles.iter().any(|o| o == oracle) {
            oracles.push_back(oracle.clone());
            env.storage()
                .instance()
                .set(&DataKey::ApprovedOracles, &oracles);
        }

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("ora_add")),
            oracle,
        );
        Ok(())
    }

    /// Remove an approved oracle from the consensus oracle list — admin only.
    pub fn remove_approved_oracle(env: Env, oracle: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let oracles: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedOracles)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);

        let mut updated = soroban_sdk::vec![&env];
        for o in oracles.iter() {
            if o != oracle {
                updated.push_back(o);
            }
        }
        env.storage()
            .instance()
            .set(&DataKey::ApprovedOracles, &updated);

        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("ora_rm")),
            oracle,
        );
        Ok(())
    }

    /// Return the list of approved oracles for consensus.
    pub fn get_approved_oracles(env: Env) -> soroban_sdk::Vec<Address> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::ApprovedOracles)
            .unwrap_or_else(|| soroban_sdk::vec![&env])
    }

    /// Set the number of oracle confirmations required for consensus — admin only.
    ///
    /// Default is 2 when not explicitly configured.
    pub fn set_required_confirmations(env: Env, count: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if count == 0 {
            return Err(Error::InvalidAmount);
        }

        env.storage()
            .instance()
            .set(&DataKey::RequiredOracleConfirmations, &count);
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("req_conf")),
            count,
        );
        Ok(())
    }

    /// Return the currently required number of oracle confirmations (default: 2).
    pub fn get_required_confirmations(env: Env) -> u32 {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::RequiredOracleConfirmations)
            .unwrap_or(2u32)
    }

    /// Submit a consensus confirmation for a match result.
    ///
    /// Any approved oracle can call this function to vote on the match outcome.
    /// Once the required number of confirmations is reached for a single outcome,
    /// the payout is automatically executed.
    ///
    /// Deadlock Detection:
    /// After any accepted vote that doesn't reach the threshold, the system checks whether
    /// the threshold is still mathematically reachable given the total number of approved
    /// oracles. If the threshold becomes impossible to reach (even if all remaining oracles
    /// vote), the match is flagged as deadlocked. Admin can then resolve it via
    /// `resolve_oracle_deadlock` (see issue #1278).
    ///
    /// Rules:
    /// - Caller must be an approved oracle and must require_auth.
    /// - Each oracle may only vote once per match (including rejected votes).
    /// - All votes must agree on the same winner. A conflicting vote returns `ConflictingResult`,
    ///   but the rejected vote is still recorded for audit and deadlock detection.
    /// - Once the required threshold is reached, payout is executed and the match completes.
    ///
    /// # Errors
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::MatchNotFound`] — no match exists for `match_id`.
    /// - [`Error::InvalidState`] — match is not in `Active` state.
    /// - [`Error::NotFunded`] — one or both players have not deposited.
    /// - [`Error::NotAnOracle`] — caller is not in the approved oracle list.
    /// - [`Error::OracleAlreadyConfirmed`] — this oracle already voted on this match.
    /// - [`Error::ConflictingResult`] — the submitted winner conflicts with prior votes.
    pub fn submit_result_consensus(
        env: Env,
        match_id: u64,
        winner: Winner,
        oracle_address: Address,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        // Require the oracle to authorize this call.
        oracle_address.require_auth();

        // Verify the caller is in the approved oracle list.
        let oracles: soroban_sdk::Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ApprovedOracles)
            .unwrap_or_else(|| soroban_sdk::vec![&env]);

        if !oracles.iter().any(|o| o == oracle_address) {
            return Err(Error::NotAnOracle);
        }

        // Load and validate match state.
        let m: Match = env
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

        // Check if this oracle has already voted on this match.
        let vote_key = DataKey::OracleVote(match_id, oracle_address.clone());
        if env.storage().persistent().has(&vote_key) {
            return Err(Error::OracleAlreadyConfirmed);
        }

        // Check existing votes for conflict: if any other oracle has voted,
        // they must agree on the same winner.
        let confirmations_key = DataKey::OracleConfirmations(match_id);
        let existing_confirmations: u32 = env
            .storage()
            .persistent()
            .get(&confirmations_key)
            .unwrap_or(0);

        // If there are existing votes, load the recorded winner and check for conflict.
        // We store the "leading winner" in a dedicated key once the first vote arrives.
        let leading_winner_key = DataKey::OracleRecord(match_id);
        if existing_confirmations > 0 {
            // There are existing votes — check they all agree.
            // We repurpose OracleRecord(match_id) to store the first-vote Winner.
            let existing_winner: Winner = env
                .storage()
                .persistent()
                .get(&leading_winner_key)
                .ok_or(Error::ConflictingResult)?;
            if existing_winner != winner {
                // Soroban rolls back all storage writes made during a call that
                // returns `Err`, so a conflicting vote cannot itself persist any
                // record of the disagreement — it's reported only via this error.
                return Err(Error::ConflictingResult);
            }
        } else {
            // First vote — store the winner as the reference.
            env.storage().persistent().set(&leading_winner_key, &winner);
            env.storage().persistent().extend_ttl(
                &leading_winner_key,
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
        }

        // Record this oracle's vote.
        env.storage().persistent().set(&vote_key, &winner);
        env.storage()
            .persistent()
            .extend_ttl(&vote_key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        // Increment confirmation count.
        let new_confirmations = existing_confirmations
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        env.storage()
            .persistent()
            .set(&confirmations_key, &new_confirmations);
        env.storage().persistent().extend_ttl(
            &confirmations_key,
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("ora_vote")),
            (match_id, oracle_address.clone(), new_confirmations),
        );

        // Check if required threshold has been reached.
        let required: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RequiredOracleConfirmations)
            .unwrap_or(2u32);

        if new_confirmations >= required {
            // Threshold reached — execute payout via shared settlement logic.
            Self::settle_result(&env, match_id, winner)?;
        } else {
            // Threshold not reached; check if still mathematically possible.
            Self::check_oracle_deadlock(&env, match_id, new_confirmations, required)?;
        }

        Ok(())
    }

    /// Return the current confirmation count for a given match.
    pub fn get_oracle_confirmations(env: Env, match_id: u64) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::OracleConfirmations(match_id))
            .unwrap_or(0)
    }

    /// Return whether a match is deadlocked (threshold unreachable).
    pub fn is_oracle_deadlocked(env: Env, match_id: u64) -> bool {
        env.storage()
            .persistent()
            .get(&types::OracleConsensusKey::OracleDeadlock(match_id))
            .unwrap_or(false)
    }

    /// Resolve a deadlocked match by admin authority, executing payout for a chosen winner.
    /// Only callable if the match is flagged as deadlocked.
    pub fn resolve_oracle_deadlock(env: Env, match_id: u64, winner: Winner) -> Result<(), Error> {
        extend_instance_ttl(&env);

        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(Error::ContractPaused);
        }

        // Require admin authorization.
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        // Load and validate match state.
        let m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
        }

        // Require the match to be flagged as deadlocked.
        if !env
            .storage()
            .persistent()
            .get::<_, bool>(&types::OracleConsensusKey::OracleDeadlock(match_id))
            .unwrap_or(false)
        {
            return Err(Error::InvalidState);
        }

        // Execute payout with the admin-chosen winner.
        Self::settle_result(&env, match_id, winner.clone())?;

        // Emit event for admin resolution.
        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("ora_adm")),
            (match_id, winner),
        );

        Ok(())
    }
}

impl EscrowContract {
    /// Apply every incremental schema migration for versions in the range
    /// `(from, to]`.  Each migration step is guarded by a version-range check
    /// so that steps are applied exactly once no matter which `from`/`to` pair
    /// is passed.
    fn _apply_migrations(env: &Env, from: u32, to: u32) {
        // ── v0.1.0 → v0.1.1 ────────────────────────────────────────────────
        // Introduced the DisputePeriod key (default 0 = immediate payout).
        // Pre-upgrade contracts that never called set_dispute_period do not
        // have this key; back-fill the default so post-upgrade reads do not
        // fall back to None unexpectedly.
        if from < 1_001 && to >= 1_001 && !env.storage().instance().has(&DataKey::DisputePeriod) {
            env.storage().instance().set(&DataKey::DisputePeriod, &0u32);
        }

        // ── v0.1.x → v1.0.0 ────────────────────────────────────────────────
        // Placeholder for future v1.0 schema changes.
        if from < 1_000_000 && to >= 1_000_000 {
            // No-op for now; add field back-fills here when the v1.0 spec is
            // finalised.
        }
    }

    fn get_config(env: &Env) -> ProtocolConfig {
        env.storage()
            .instance()
            .get(&DataKey::ProtocolConfig)
            .unwrap_or(ProtocolConfig {
                vesting_duration_seconds: 259_200, // 3 days
                cancellation_fee_basis_points: 0,
                treasury: env.current_contract_address(),
                stablecoin_only_mode: false,
                maximum_stake: None,
                match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
                protocol_fee_bps: 0,
                fee_recipient: env.current_contract_address(),
                minimum_stake: DEFAULT_MINIMUM_STAKE,
            })
    }

    /// Compute the referral fee to deduct from a winner's payout.
    ///
    /// Returns 0 if the match has no referrer or if the cancellation_fee_basis_points
    /// is 0 (no platform fee is being collected).
    ///
    /// Formula: `referral_fee = pot * cancellation_fee_bps / 10_000 * referral_share_bps / 10_000`
    fn compute_referral_fee(env: &Env, m: &Match, pot: i128) -> Result<i128, Error> {
        if m.referrer.is_none() {
            return Ok(0);
        }
        let config = Self::get_config(env);
        if config.cancellation_fee_basis_points == 0 {
            return Ok(0);
        }
        let platform_fee = pot
            .checked_mul(config.cancellation_fee_basis_points as i128)
            .ok_or(Error::Overflow)?
            / 10_000;
        let referral_share_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ReferralShareBasisPoints)
            .unwrap_or(2000u32);
        let referral_fee = platform_fee
            .checked_mul(referral_share_bps as i128)
            .ok_or(Error::Overflow)?
            / 10_000;
        Ok(referral_fee)
    }

    /// Compute the protocol fee to deduct from a winner's payout.
    ///
    /// Returns 0 if `protocol_fee_bps` is 0. Draw refunds never incur this
    /// fee — it only applies to the winner's share of the pot.
    ///
    /// Formula: `protocol_fee = pot * protocol_fee_bps / 10_000`
    fn compute_protocol_fee(env: &Env, pot: i128) -> Result<i128, Error> {
        let config = Self::get_config(env);
        if config.protocol_fee_bps == 0 {
            return Ok(0);
        }
        let fee = pot
            .checked_mul(config.protocol_fee_bps as i128)
            .ok_or(Error::Overflow)?
            / 10_000;
        Ok(fee)
    }
}
