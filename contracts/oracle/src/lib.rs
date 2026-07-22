#![no_std]

mod errors;
pub mod types;

use errors::Error;
use soroban_sdk::{
    contract, contractimpl, symbol_short, token, Address, Env, String, Symbol, Vec,
};
use types::{
    BatchResultEntry, CandidateTally, ConsensusState, DataKey, OracleRegistration,
    OracleVoteRecord, Platform, RateLimitConfig, RateLimitStatus, RateWindow, ResultEntry, Winner,
};

/// Maximum number of entries accepted in a single batch submission.
/// Designed for v2.0 tournament use; future versions may raise this limit.
const MAX_BATCH_SIZE: u32 = 100;

/// ~30 days at 5s/ledger.
const MATCH_TTL_LEDGERS: u32 = 518_400;

/// Default maximum submissions accepted from a single oracle per rolling hour.
const DEFAULT_HOURLY_LIMIT: u32 = 100;
/// Default maximum submissions accepted from a single oracle per rolling day.
const DEFAULT_DAILY_LIMIT: u32 = 1_000;

/// Length of the hourly rate-limit window, in seconds.
const HOURLY_WINDOW_SECS: u64 = 3_600;
/// Length of the daily rate-limit window, in seconds.
const DAILY_WINDOW_SECS: u64 = 86_400;

/// Emit a suspicious-pattern alert once usage reaches this percentage of a limit.
const RATE_LIMIT_ALERT_THRESHOLD_PCT: u64 = 80;

/// TTL for rate-limit window storage: ~2 days at 5s/ledger, comfortably longer
/// than the daily window so counters never expire mid-window.
const RATE_LIMIT_TTL_LEDGERS: u32 = 34_560;

/// Default m-of-n consensus threshold: a single matching submission finalizes
/// a result. This is the degenerate n=1 configuration that reproduces the
/// original single-admin-oracle deployment via `submit_oracle_result`.
const DEFAULT_CONSENSUS_THRESHOLD: u32 = 1;

/// Basis points of an oracle's remaining stake slashed automatically when it
/// is caught equivocating (submitting two conflicting results for the same
/// match_id). Equivocation is unambiguous and provable on-chain, so it is
/// slashed at the maximum: the oracle's entire remaining stake.
const EQUIVOCATION_SLASH_BPS: i128 = 10_000;

/// Basis points of an oracle's remaining stake automatically slashed when its
/// submission ends up on the losing side of a finalized consensus vote (a
/// minority result contradicted by a threshold-strong majority), or on the
/// losing side of an admin's resolution of a deadlocked (disputed) match.
/// Lower than the equivocation penalty because being outvoted can reflect an
/// honest disagreement (e.g. a stale platform API read) rather than malice.
const MINORITY_SLASH_BPS: i128 = 1_000;

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
    ///
    /// # Errors
    /// - [`Error::AlreadyInitialized`] — contract has already been initialized.
    pub fn initialize(env: Env, admin: Address) -> Result<(), Error> {
        extend_instance_ttl(&env);
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.events()
            .publish((Symbol::new(&env, "oracle"), symbol_short!("init")), &admin);
        Ok(())
    }

    /// Register an oracle with a token stake that can be slashed if needed.
    pub fn register_oracle_with_stake(
        env: Env,
        oracle_address: Address,
        stake_amount: i128,
        token: Address,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        oracle_address.require_auth();

        if stake_amount <= 0 {
            return Err(Error::InsufficientStake);
        }

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&oracle_address, &env.current_contract_address(), &stake_amount);

        env.storage().instance().set(
            &DataKey::OracleRegistration(oracle_address.clone()),
            &OracleRegistration {
                oracle_address: oracle_address.clone(),
                oracle_stake: stake_amount,
                token: token.clone(),
            },
        );

        let mut oracle_set: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::OracleSet)
            .unwrap_or(Vec::new(&env));
        if !oracle_set.contains(&oracle_address) {
            oracle_set.push_back(oracle_address.clone());
            env.storage()
                .instance()
                .set(&DataKey::OracleSet, &oracle_set);
        }

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("stake")),
            (oracle_address, stake_amount, token),
        );

        Ok(())
    }

    /// Slash a registered oracle's stake. Admin-only.
    pub fn slash_oracle(env: Env, oracle_address: Address, slash_amount: i128) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let mut registration: OracleRegistration = env
            .storage()
            .instance()
            .get(&DataKey::OracleRegistration(oracle_address.clone()))
            .ok_or(Error::InsufficientStake)?;

        if slash_amount <= 0 || slash_amount > registration.oracle_stake {
            return Err(Error::InsufficientStake);
        }

        registration.oracle_stake -= slash_amount;
        env.storage().instance().set(
            &DataKey::OracleRegistration(oracle_address.clone()),
            &registration,
        );

        let token_client = token::Client::new(&env, &registration.token);
        token_client.transfer(&env.current_contract_address(), &admin, &slash_amount);

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("slash")),
            (oracle_address, slash_amount),
        );

        Ok(())
    }

    /// Admin submits a verified match result on-chain.
    /// Invariant: No results can be submitted while the contract is paused.
    ///
    /// # Errors
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    /// - [`Error::RateLimitExceeded`] — the oracle has exceeded its hourly or daily submission limit.
    /// - [`Error::AlreadySubmitted`] — a result for `match_id` has already been recorded.
    /// - [`Error::InvalidGameId`] — `game_id` is empty.
    pub fn submit_result(
        env: Env,
        match_id: u64,
        game_id: String,
        platform: Platform,
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

        let registration: Option<OracleRegistration> = env
            .storage()
            .instance()
            .get(&DataKey::OracleRegistration(admin.clone()));
        if let Some(registration) = registration {
            if registration.oracle_stake <= 0 {
                return Err(Error::InsufficientStake);
            }
        }

        Self::check_oracle_rate_limit(&env, &admin, 1)?;

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
                platform,
                result: result.clone(),
                submitted_ledger: env.ledger().sequence(),
                submitter: admin.clone(),
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

    /// Submit results for multiple matches atomically.
    ///
    /// All entries are validated before any storage writes occur (all-or-nothing).
    /// Maximum batch size is 100 entries (see [`MAX_BATCH_SIZE`]).
    ///
    /// # Errors
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::Unauthorized`] — not initialized or caller is not the admin.
    /// - [`Error::RateLimitExceeded`] — the oracle has exceeded its hourly or daily submission limit.
    /// - [`Error::BatchTooLarge`] — `entries` exceeds 100 items.
    /// - [`Error::InvalidGameId`] — any entry has an empty `game_id`.
    /// - [`Error::BatchDuplicateEntry`] — two entries share the same `match_id`.
    /// - [`Error::AlreadySubmitted`] — a result for any `match_id` already exists.
    pub fn submit_batch_results(
        env: Env,
        entries: Vec<BatchResultEntry>,
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

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        let registration: Option<OracleRegistration> = env
            .storage()
            .instance()
            .get(&DataKey::OracleRegistration(admin.clone()));
        if let Some(registration) = registration {
            if registration.oracle_stake <= 0 {
                return Err(Error::InsufficientStake);
            }
        }

        let len = entries.len();
        if len > MAX_BATCH_SIZE {
            return Err(Error::BatchTooLarge);
        }

        // Each entry in the batch counts as one submission toward the oracle's
        // rate limit, checked atomically against the whole batch size.
        Self::check_oracle_rate_limit(&env, &admin, len)?;

        // Validate all entries before writing anything (atomic guarantee).
        for i in 0..len {
            let entry = entries.get(i).unwrap();

            if entry.game_id.is_empty() {
                return Err(Error::InvalidGameId);
            }

            // Intra-batch duplicate detection (O(n²) acceptable for n ≤ 100).
            for j in (i + 1)..len {
                if entries.get(j).unwrap().match_id == entry.match_id {
                    return Err(Error::BatchDuplicateEntry);
                }
            }

            if env
                .storage()
                .persistent()
                .has(&DataKey::Result(entry.match_id))
            {
                return Err(Error::AlreadySubmitted);
            }
        }

        // All checks passed — commit atomically.
        let current_ledger = env.ledger().sequence();
        for i in 0..len {
            let entry = entries.get(i).unwrap();
            env.storage().persistent().set(
                &DataKey::Result(entry.match_id),
                &ResultEntry {
                    game_id: entry.game_id,
                    platform: entry.platform,
                    result: entry.result.clone(),
                    submitted_ledger: current_ledger,
                    submitter: admin.clone(),
                },
            );
            env.storage().persistent().extend_ttl(
                &DataKey::Result(entry.match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );
            env.events().publish(
                (Symbol::new(&env, "oracle"), symbol_short!("result")),
                (entry.match_id, entry.result),
            );
        }

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("batch")),
            len,
        );

        Ok(())
    }

    /// Submit a match result as one vote in an m-of-n oracle consensus.
    ///
    /// Unlike [`submit_result`], which is gated by admin auth alone, this is
    /// the genuine multi-oracle path: any address independently registered
    /// via [`register_oracle_with_stake`] with a positive stake may call this
    /// directly (it authenticates itself, not the admin). A match result is
    /// finalized into [`get_result`]-visible storage only once a candidate
    /// (game_id, platform, result) has been submitted by at least
    /// [`get_consensus_threshold`] distinct registered oracles.
    ///
    /// With the default threshold of 1, a single registered oracle's
    /// submission finalizes immediately — the degenerate n=1 configuration
    /// that mirrors the original single-admin-oracle deployment.
    ///
    /// # Disagreement handling
    /// - If a submission's (game_id, platform, result) doesn't yet have
    ///   enough matching votes, it is recorded and the match stays pending.
    /// - If a candidate reaches the threshold, it is finalized and every
    ///   oracle that had already voted for a *different* candidate for this
    ///   match is automatically slashed [`MINORITY_SLASH_BPS`] of its
    ///   remaining stake — majority wins, minority is slashed.
    /// - If votes split enough that no remaining eligible oracle could still
    ///   push any candidate over the threshold (a deadlock), the match is
    ///   flagged disputed and awaits admin resolution via
    ///   [`resolve_disputed_match`].
    /// - If the same oracle submits two different candidates for the same
    ///   match (equivocation), the vote is discarded and the oracle's entire
    ///   remaining stake is slashed immediately. This case returns `Ok(())`
    ///   rather than an error — a contract call that returns `Err` reverts
    ///   every storage write made during it, which would undo the slash. The
    ///   caller detects it via the `oracle/equivoc` event.
    ///
    /// # Errors
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::Unauthorized`] — contract has not been initialized.
    /// - [`Error::InvalidGameId`] — `game_id` is empty.
    /// - [`Error::NotRegisteredOracle`] — `oracle` has never registered stake.
    /// - [`Error::InsufficientStake`] — `oracle`'s stake has been slashed to zero.
    /// - [`Error::AlreadySubmitted`] — the match is already finalized, or this
    ///   oracle already cast this exact vote.
    /// - [`Error::RateLimitExceeded`] — `oracle` has exceeded its submission quota.
    /// - [`Error::MatchDisputed`] — the match has already deadlocked and is
    ///   awaiting admin resolution.
    pub fn submit_oracle_result(
        env: Env,
        oracle: Address,
        match_id: u64,
        game_id: String,
        platform: Platform,
        result: Winner,
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

        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::Unauthorized);
        }

        oracle.require_auth();

        if game_id.is_empty() {
            return Err(Error::InvalidGameId);
        }

        let registration: OracleRegistration = env
            .storage()
            .instance()
            .get(&DataKey::OracleRegistration(oracle.clone()))
            .ok_or(Error::NotRegisteredOracle)?;
        if registration.oracle_stake <= 0 {
            return Err(Error::InsufficientStake);
        }

        if env.storage().persistent().has(&DataKey::Result(match_id)) {
            return Err(Error::AlreadySubmitted);
        }

        Self::check_oracle_rate_limit(&env, &oracle, 1)?;

        let vote_key = DataKey::OracleVote(match_id, oracle.clone());
        let vote = OracleVoteRecord {
            game_id: game_id.clone(),
            platform: platform.clone(),
            result: result.clone(),
        };
        if let Some(prev) = env
            .storage()
            .persistent()
            .get::<_, OracleVoteRecord>(&vote_key)
        {
            if prev == vote {
                return Err(Error::AlreadySubmitted);
            }
            // A contract call that returns `Err` reverts *all* storage writes
            // made during the call, including the slash below — so proven
            // equivocation must return `Ok` for the penalty to actually
            // commit. Callers detect it via the `oracle/equivoc` event (and
            // the resulting drop in the oracle's stake) rather than an error.
            Self::slash_bps(&env, &oracle, EQUIVOCATION_SLASH_BPS);
            env.events().publish(
                (Symbol::new(&env, "oracle"), symbol_short!("equivoc")),
                (match_id, oracle),
            );
            return Ok(());
        }

        let mut state: ConsensusState = env
            .storage()
            .persistent()
            .get(&DataKey::MatchVotes(match_id))
            .unwrap_or(ConsensusState {
                candidates: Vec::new(&env),
                disputed: false,
            });

        if state.disputed {
            return Err(Error::MatchDisputed);
        }

        env.storage().persistent().set(&vote_key, &vote);
        env.storage()
            .persistent()
            .extend_ttl(&vote_key, MATCH_TTL_LEDGERS, MATCH_TTL_LEDGERS);

        let threshold = Self::consensus_threshold(&env);
        let mut winning_idx: Option<u32> = None;
        let mut found_existing = false;

        for i in 0..state.candidates.len() {
            let mut candidate = state.candidates.get(i).unwrap();
            if candidate.result == result
                && candidate.platform == platform
                && candidate.game_id == game_id
            {
                candidate.submitters.push_back(oracle.clone());
                if candidate.submitters.len() >= threshold {
                    winning_idx = Some(i);
                }
                state.candidates.set(i, candidate);
                found_existing = true;
                break;
            }
        }

        if !found_existing {
            let mut submitters = Vec::new(&env);
            submitters.push_back(oracle.clone());
            let reached = submitters.len() >= threshold;
            let idx = state.candidates.len();
            state.candidates.push_back(CandidateTally {
                game_id: game_id.clone(),
                platform: platform.clone(),
                result: result.clone(),
                submitters,
            });
            if reached {
                winning_idx = Some(idx);
            }
        }

        if let Some(idx) = winning_idx {
            let winning = state.candidates.get(idx).unwrap();

            env.storage().persistent().set(
                &DataKey::Result(match_id),
                &ResultEntry {
                    game_id: winning.game_id.clone(),
                    platform: winning.platform.clone(),
                    result: winning.result.clone(),
                    submitted_ledger: env.ledger().sequence(),
                    submitter: oracle,
                },
            );
            env.storage().persistent().extend_ttl(
                &DataKey::Result(match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );

            // Majority wins, minority is slashed: every oracle that voted for
            // a losing candidate is automatically penalized.
            for i in 0..state.candidates.len() {
                if i == idx {
                    continue;
                }
                let losing = state.candidates.get(i).unwrap();
                for j in 0..losing.submitters.len() {
                    let minority_oracle = losing.submitters.get(j).unwrap();
                    Self::slash_bps(&env, &minority_oracle, MINORITY_SLASH_BPS);
                    env.events().publish(
                        (Symbol::new(&env, "oracle"), symbol_short!("minority")),
                        (match_id, minority_oracle),
                    );
                }
            }

            env.storage()
                .persistent()
                .remove(&DataKey::MatchVotes(match_id));

            env.events().publish(
                (Symbol::new(&env, "oracle"), symbol_short!("result")),
                (match_id, winning.result),
            );
            env.events().publish(
                (Symbol::new(&env, "oracle"), symbol_short!("finalzd")),
                (match_id, winning.submitters.len(), threshold),
            );
        } else {
            let remaining = Self::remaining_eligible_oracles(&env, match_id);
            let mut still_possible = false;
            for i in 0..state.candidates.len() {
                let candidate = state.candidates.get(i).unwrap();
                if candidate.submitters.len().saturating_add(remaining) >= threshold {
                    still_possible = true;
                    break;
                }
            }

            if !still_possible {
                state.disputed = true;
                env.events().publish(
                    (Symbol::new(&env, "oracle"), symbol_short!("disputed")),
                    match_id,
                );
            }

            env.storage()
                .persistent()
                .set(&DataKey::MatchVotes(match_id), &state);
            env.storage().persistent().extend_ttl(
                &DataKey::MatchVotes(match_id),
                MATCH_TTL_LEDGERS,
                MATCH_TTL_LEDGERS,
            );

            env.events().publish(
                (Symbol::new(&env, "oracle"), symbol_short!("vote")),
                (match_id, oracle, result),
            );
        }

        Ok(())
    }

    /// Admin resolves a match whose m-of-n consensus deadlocked (see
    /// [`submit_oracle_result`]): no remaining eligible oracle vote could
    /// still push any candidate result over the configured threshold.
    ///
    /// Finalizes the match with the admin's chosen result and slashes every
    /// oracle whose recorded vote disagreed with it — the admin acts as the
    /// tie-breaker of last resort, consistent with the admin's existing
    /// ultimate authority elsewhere in this contract (`slash_oracle`,
    /// `update_admin`, `pause`). See docs/oracle.md for the full consensus
    /// protocol and its migration path from single-oracle deployments.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    /// - [`Error::AlreadySubmitted`] — the match already has a finalized result.
    /// - [`Error::MatchNotDisputed`] — the match has no consensus votes recorded,
    ///   or its consensus has not deadlocked.
    pub fn resolve_disputed_match(
        env: Env,
        match_id: u64,
        game_id: String,
        platform: Platform,
        result: Winner,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if env.storage().persistent().has(&DataKey::Result(match_id)) {
            return Err(Error::AlreadySubmitted);
        }

        let state: ConsensusState = env
            .storage()
            .persistent()
            .get(&DataKey::MatchVotes(match_id))
            .ok_or(Error::MatchNotDisputed)?;
        if !state.disputed {
            return Err(Error::MatchNotDisputed);
        }

        for i in 0..state.candidates.len() {
            let candidate = state.candidates.get(i).unwrap();
            let agrees = candidate.result == result
                && candidate.platform == platform
                && candidate.game_id == game_id;
            if !agrees {
                for j in 0..candidate.submitters.len() {
                    let wrong_oracle = candidate.submitters.get(j).unwrap();
                    Self::slash_bps(&env, &wrong_oracle, MINORITY_SLASH_BPS);
                    env.events().publish(
                        (Symbol::new(&env, "oracle"), symbol_short!("minority")),
                        (match_id, wrong_oracle),
                    );
                }
            }
        }

        env.storage().persistent().set(
            &DataKey::Result(match_id),
            &ResultEntry {
                game_id,
                platform,
                result: result.clone(),
                submitted_ledger: env.ledger().sequence(),
                submitter: admin,
            },
        );
        env.storage().persistent().extend_ttl(
            &DataKey::Result(match_id),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        env.storage()
            .persistent()
            .remove(&DataKey::MatchVotes(match_id));

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("resolved")),
            (match_id, result),
        );

        Ok(())
    }

    /// Configure the m-of-n consensus threshold — admin only. The number of
    /// distinct, independently-registered oracles that must submit a matching
    /// result before `submit_oracle_result` finalizes a match. Pass `1` to
    /// restore the degenerate single-oracle configuration.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    /// - [`Error::InvalidThreshold`] — `threshold` is 0.
    pub fn set_consensus_threshold(env: Env, threshold: u32) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if threshold == 0 {
            return Err(Error::InvalidThreshold);
        }

        env.storage()
            .instance()
            .set(&DataKey::ConsensusThreshold, &threshold);
        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("thresh")),
            threshold,
        );
        Ok(())
    }

    /// Return the currently configured m-of-n consensus threshold. Defaults
    /// to 1 (degenerate single-oracle configuration) if never explicitly set.
    pub fn get_consensus_threshold(env: Env) -> u32 {
        extend_instance_ttl(&env);
        Self::consensus_threshold(&env)
    }

    /// Return the number of distinct addresses ever registered via
    /// `register_oracle_with_stake`. Does not account for stake subsequently
    /// slashed to zero — see `get_oracle_rate_limit_status`-style per-oracle
    /// queries, or read the `OracleRegistration` directly, to check whether a
    /// specific oracle is currently eligible to vote.
    pub fn get_registered_oracle_count(env: Env) -> u32 {
        extend_instance_ttl(&env);
        let set: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::OracleSet)
            .unwrap_or(Vec::new(&env));
        set.len()
    }

    /// Return the in-progress consensus tally for a match: every distinct
    /// candidate result submitted so far and whether the match has deadlocked
    /// into a disputed state. Returns `None` once the match is finalized
    /// (its tally is cleared) or if no oracle has voted on it yet.
    pub fn get_match_votes(env: Env, match_id: u64) -> Option<ConsensusState> {
        extend_instance_ttl(&env);
        env.storage()
            .persistent()
            .get(&DataKey::MatchVotes(match_id))
    }

    /// Read the configured consensus threshold, defaulting to 1.
    fn consensus_threshold(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ConsensusThreshold)
            .unwrap_or(DEFAULT_CONSENSUS_THRESHOLD)
    }

    /// Count registered oracles that (a) still hold a positive stake and
    /// (b) have not yet voted on `match_id` — the maximum number of
    /// additional votes any candidate could still receive. Used to detect an
    /// irreconcilable deadlock: if no candidate's current tally plus this
    /// count can reach the threshold, consensus is no longer achievable.
    fn remaining_eligible_oracles(env: &Env, match_id: u64) -> u32 {
        let set: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::OracleSet)
            .unwrap_or(Vec::new(env));

        let mut remaining = 0u32;
        for i in 0..set.len() {
            let addr = set.get(i).unwrap();
            if env
                .storage()
                .persistent()
                .has(&DataKey::OracleVote(match_id, addr.clone()))
            {
                continue;
            }
            if let Some(registration) = env
                .storage()
                .instance()
                .get::<_, OracleRegistration>(&DataKey::OracleRegistration(addr))
            {
                if registration.oracle_stake > 0 {
                    remaining += 1;
                }
            }
        }
        remaining
    }

    /// Slash `bps` basis points of `oracle`'s remaining stake, transferring
    /// the slashed amount to the admin (treasury). No-op if the oracle is not
    /// registered or has no remaining stake. Returns the amount slashed.
    fn slash_bps(env: &Env, oracle: &Address, bps: i128) -> i128 {
        let key = DataKey::OracleRegistration(oracle.clone());
        let mut registration: OracleRegistration = match env.storage().instance().get(&key) {
            Some(r) => r,
            None => return 0,
        };
        if registration.oracle_stake <= 0 {
            return 0;
        }

        let amount = (registration.oracle_stake * bps) / 10_000;
        let amount = amount.clamp(0, registration.oracle_stake);
        if amount == 0 {
            return 0;
        }

        registration.oracle_stake -= amount;
        let token = registration.token.clone();
        env.storage().instance().set(&key, &registration);

        if let Some(admin) = env.storage().instance().get::<_, Address>(&DataKey::Admin) {
            let token_client = token::Client::new(env, &token);
            token_client.transfer(&env.current_contract_address(), &admin, &amount);
        }

        amount
    }

    /// Retrieve the stored result for a match.    /// TTL is extended on every read to prevent active results from expiring.
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

    /// Return the admin address stored in the contract.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        extend_instance_ttl(&env);
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)
    }

    /// Admin removes a previously submitted result from persistent storage.
    /// Emits a `oracle / deleted` event with the `match_id`.
    ///
    /// # Errors
    /// - [`Error::ContractPaused`] — contract is paused.
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    /// - [`Error::ResultNotFound`] — no result exists for `match_id`.
    pub fn delete_result(env: Env, match_id: u64) -> Result<(), Error> {
        extend_instance_ttl(&env);
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

        if !env.storage().persistent().has(&DataKey::Result(match_id)) {
            return Err(Error::ResultNotFound);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Result(match_id));

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("deleted")),
            match_id,
        );

        Ok(())
    }

    /// Rotate the admin to a new address. Requires current admin auth.
    /// Emits an `admin / admin_rot` event with `(old_admin, new_admin)`.
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
        env.events().publish(
            (Symbol::new(&env, "admin"), symbol_short!("admin_rot")),
            (current_admin, new_admin),
        );
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
        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("paused")), ());
        Ok(())
    }

    /// Returns true if the contract has been initialized.
    pub fn is_initialized(env: Env) -> bool {
        extend_instance_ttl(&env);
        env.storage().instance().has(&DataKey::Admin)
    }

    /// Unpause the oracle — admin only. Emits an `admin / unpaused` event.
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
        env.events()
            .publish((Symbol::new(&env, "admin"), symbol_short!("unpaused")), ());
        Ok(())
    }

    /// Configure the hourly and daily submission limits for a specific oracle
    /// address — admin only. Pass `0` for either field to fall back to the
    /// contract defaults ([`DEFAULT_HOURLY_LIMIT`] / [`DEFAULT_DAILY_LIMIT`]).
    ///
    /// Emits an `oracle / ratelim` event with `(oracle, hourly_limit, daily_limit)`.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`] — contract has not been initialized or caller is not the admin.
    /// - [`Error::InvalidRateLimit`] — `hourly_limit` exceeds `daily_limit` when both are non-zero.
    pub fn set_oracle_rate_limits(
        env: Env,
        oracle: Address,
        hourly_limit: u32,
        daily_limit: u32,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if hourly_limit != 0 && daily_limit != 0 && hourly_limit > daily_limit {
            return Err(Error::InvalidRateLimit);
        }

        let config = RateLimitConfig {
            hourly_limit: if hourly_limit == 0 {
                DEFAULT_HOURLY_LIMIT
            } else {
                hourly_limit
            },
            daily_limit: if daily_limit == 0 {
                DEFAULT_DAILY_LIMIT
            } else {
                daily_limit
            },
        };

        env.storage()
            .instance()
            .set(&DataKey::OracleRateLimit(oracle.clone()), &config);

        env.events().publish(
            (Symbol::new(&env, "oracle"), symbol_short!("ratelim")),
            (oracle, config.hourly_limit, config.daily_limit),
        );

        Ok(())
    }

    /// Return the hourly/daily submission limits currently configured for `oracle`.
    /// Falls back to the contract defaults if the admin has not set an override.
    pub fn get_oracle_rate_limits(env: Env, oracle: Address) -> RateLimitConfig {
        extend_instance_ttl(&env);
        Self::rate_limit_config(&env, &oracle)
    }

    /// Return `oracle`'s current rate-limit usage and remaining quota.
    ///
    /// This is the on-chain analogue of HTTP rate-limit headers: since the
    /// contract has no HTTP surface, callers query this view instead of
    /// reading response headers.
    pub fn get_oracle_rate_limit_status(env: Env, oracle: Address) -> RateLimitStatus {
        let config = Self::rate_limit_config(&env, &oracle);
        let now = env.ledger().timestamp();

        let hourly_window = Self::load_rate_window(
            &env,
            &DataKey::OracleHourlyWindow(oracle.clone()),
            now,
            HOURLY_WINDOW_SECS,
        );
        let hourly_used = Self::estimated_window_count(now, &hourly_window, HOURLY_WINDOW_SECS);

        let daily_window = Self::load_rate_window(
            &env,
            &DataKey::OracleDailyWindow(oracle),
            now,
            DAILY_WINDOW_SECS,
        );
        let daily_used = Self::estimated_window_count(now, &daily_window, DAILY_WINDOW_SECS);

        RateLimitStatus {
            hourly_used,
            hourly_limit: config.hourly_limit,
            hourly_remaining: config.hourly_limit.saturating_sub(hourly_used),
            daily_used,
            daily_limit: config.daily_limit,
            daily_remaining: config.daily_limit.saturating_sub(daily_used),
        }
    }

    /// Read the rate-limit configuration for `oracle`, falling back to the
    /// contract-wide defaults when no override has been set.
    fn rate_limit_config(env: &Env, oracle: &Address) -> RateLimitConfig {
        env.storage()
            .instance()
            .get(&DataKey::OracleRateLimit(oracle.clone()))
            .unwrap_or(RateLimitConfig {
                hourly_limit: DEFAULT_HOURLY_LIMIT,
                daily_limit: DEFAULT_DAILY_LIMIT,
            })
    }

    /// Load a sliding-window counter, rolling it forward if the window (or
    /// both windows) have fully elapsed since it was last written.
    fn load_rate_window(env: &Env, key: &DataKey, now: u64, window_secs: u64) -> RateWindow {
        let window: RateWindow = env.storage().persistent().get(key).unwrap_or(RateWindow {
            window_start: now,
            current_count: 0,
            previous_count: 0,
        });

        let elapsed = now.saturating_sub(window.window_start);
        if elapsed >= window_secs * 2 {
            RateWindow {
                window_start: now,
                current_count: 0,
                previous_count: 0,
            }
        } else if elapsed >= window_secs {
            RateWindow {
                window_start: window.window_start + window_secs,
                current_count: 0,
                previous_count: window.current_count,
            }
        } else {
            window
        }
    }

    /// Estimate the submission count within the sliding lookback window using
    /// the "sliding window counter" approximation: the current window's count
    /// plus the previous window's count weighted by the fraction of the
    /// previous window that still falls inside the lookback period.
    fn estimated_window_count(now: u64, window: &RateWindow, window_secs: u64) -> u32 {
        let elapsed_in_current = now.saturating_sub(window.window_start).min(window_secs);
        let remaining = window_secs - elapsed_in_current;
        let weighted_previous = (window.previous_count as u64 * remaining) / window_secs;
        window.current_count + weighted_previous as u32
    }

    /// Check `oracle`'s hourly and daily sliding-window limits can absorb
    /// `count` more submissions, and if so, record them. Emits a suspicious-
    /// pattern alert once usage crosses [`RATE_LIMIT_ALERT_THRESHOLD_PCT`] of
    /// either limit.
    ///
    /// # Errors
    /// - [`Error::RateLimitExceeded`] — `count` more submissions would exceed
    ///   the hourly or daily limit configured for `oracle`.
    fn check_oracle_rate_limit(env: &Env, oracle: &Address, count: u32) -> Result<(), Error> {
        let config = Self::rate_limit_config(env, oracle);
        let now = env.ledger().timestamp();

        let hourly_key = DataKey::OracleHourlyWindow(oracle.clone());
        let mut hourly_window = Self::load_rate_window(env, &hourly_key, now, HOURLY_WINDOW_SECS);
        let hourly_used = Self::estimated_window_count(now, &hourly_window, HOURLY_WINDOW_SECS);
        if hourly_used.saturating_add(count) > config.hourly_limit {
            return Err(Error::RateLimitExceeded);
        }

        let daily_key = DataKey::OracleDailyWindow(oracle.clone());
        let mut daily_window = Self::load_rate_window(env, &daily_key, now, DAILY_WINDOW_SECS);
        let daily_used = Self::estimated_window_count(now, &daily_window, DAILY_WINDOW_SECS);
        if daily_used.saturating_add(count) > config.daily_limit {
            return Err(Error::RateLimitExceeded);
        }

        hourly_window.current_count += count;
        daily_window.current_count += count;

        env.storage().persistent().set(&hourly_key, &hourly_window);
        env.storage().persistent().extend_ttl(
            &hourly_key,
            RATE_LIMIT_TTL_LEDGERS,
            RATE_LIMIT_TTL_LEDGERS,
        );
        env.storage().persistent().set(&daily_key, &daily_window);
        env.storage().persistent().extend_ttl(
            &daily_key,
            RATE_LIMIT_TTL_LEDGERS,
            RATE_LIMIT_TTL_LEDGERS,
        );

        Self::maybe_alert(
            env,
            oracle,
            symbol_short!("hourly"),
            hourly_used + count,
            config.hourly_limit,
        );
        Self::maybe_alert(
            env,
            oracle,
            symbol_short!("daily"),
            daily_used + count,
            config.daily_limit,
        );

        Ok(())
    }

    /// Emit an `oracle / alert` event when `used` reaches
    /// [`RATE_LIMIT_ALERT_THRESHOLD_PCT`] of `limit`, flagging the submission
    /// pattern for admin review.
    fn maybe_alert(env: &Env, oracle: &Address, window_label: Symbol, used: u32, limit: u32) {
        if limit == 0 {
            return;
        }
        if (used as u64) * 100 >= (limit as u64) * RATE_LIMIT_ALERT_THRESHOLD_PCT {
            env.events().publish(
                (Symbol::new(env, "oracle"), symbol_short!("alert")),
                (oracle.clone(), window_label, used, limit),
            );
        }
    }

    pub fn set_rate(
        env: Env,
        token_a: Address,
        token_b: Address,
        rate: i128,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::Unauthorized)?;
        admin.require_auth();

        if rate <= 0 {
            return Err(Error::InvalidRateLimit);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Rate(token_a.clone(), token_b.clone()), &rate);
        env.storage().persistent().extend_ttl(
            &DataKey::Rate(token_a, token_b),
            MATCH_TTL_LEDGERS,
            MATCH_TTL_LEDGERS,
        );
        Ok(())
    }

    pub fn get_rate(
        env: Env,
        token_a: Address,
        token_b: Address,
    ) -> Result<i128, Error> {
        extend_instance_ttl(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Rate(token_a, token_b))
            .ok_or(Error::ResultNotFound)
    }

    pub fn swap(
        env: Env,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        recipient: Address,
    ) -> Result<(), Error> {
        extend_instance_ttl(&env);

        let mut amount_out = 0i128;
        if let Some(rate) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::Rate(token_out.clone(), token_in.clone()))
        {
            amount_out = amount_in
                .checked_mul(10_000_000)
                .ok_or(Error::Unauthorized)?
                .checked_div(rate)
                .ok_or(Error::Unauthorized)?;
        } else if let Some(rate) = env
            .storage()
            .persistent()
            .get::<_, i128>(&DataKey::Rate(token_in.clone(), token_out.clone()))
        {
            amount_out = amount_in
                .checked_mul(rate)
                .ok_or(Error::Unauthorized)?
                .checked_div(10_000_000)
                .ok_or(Error::Unauthorized)?;
        } else {
            return Err(Error::ResultNotFound);
        }

        let client_out = soroban_sdk::token::Client::new(&env, &token_out);
        client_out.transfer(&env.current_contract_address(), &recipient, &amount_out);

        Ok(())
    }
}

#[cfg(test)]
mod tests;

