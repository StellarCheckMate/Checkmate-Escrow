#![no_std]

mod errors;
pub mod types;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env, String, Symbol, Vec};
use types::{BatchResultEntry, DataKey, Platform, ResultEntry, Winner};

/// Maximum number of entries accepted in a single batch submission.
/// Designed for v2.0 tournament use; future versions may raise this limit.
const MAX_BATCH_SIZE: u32 = 100;

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
}

#[cfg(test)]
mod tests;

