#![no_std]

mod errors;
pub mod types;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol, Vec};
use types::{
    BatchResultEntry, DataKey, Platform, RateLimitConfig, RateLimitStatus, RateWindow, ResultEntry,
    Winner,
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

