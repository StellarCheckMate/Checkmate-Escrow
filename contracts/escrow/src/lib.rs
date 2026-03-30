#![no_std]

mod errors;
mod types;

use errors::Error;
use soroban_sdk::{contract, contractimpl, symbol_short, token, Address, Env, String, Symbol};
use types::{DataKey, Match, MatchState, Platform, Winner};

/// ~30 days at 5s/ledger. Used as both the TTL threshold and the extend-to value.
const MATCH_TTL_LEDGERS: u32 = 518_400;

#[contract]
pub struct EscrowContract;

#[contractimpl]
impl EscrowContract {
    /// Initialize the contract with a trusted oracle address and an admin.
    ///
    /// The `oracle` address must be a valid generated address for the current
    /// environment. Callers should not use placeholder or otherwise invalid
    /// oracle values.
    pub fn initialize(env: Env, oracle: Address, admin: Address) {
        if env.storage().instance().has(&DataKey::Oracle) {
            panic!("Contract already initialized");
        }
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::MatchCount, &0u64);
        env.storage().instance().set(&DataKey::Paused, &false);
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
        Ok(())
    }

    /// Create a new match. Both players must call `deposit` before the game starts.
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
        if stake_amount <= 0 {
            return Err(Error::InvalidAmount);
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

        env.events().publish(
            (Symbol::new(&env, "match"), symbol_short!("created")),
            (id, m.player1, m.player2, stake_amount),
        );

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
    pub fn submit_result(env: Env, match_id: u64, winner: Winner) -> Result<(), Error> {
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

        if env.storage().instance().get(&DataKey::Paused).unwrap_or(false) {
            return Err(Error::ContractPaused);
        }

        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Active {
            return Err(Error::InvalidState);
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

    /// Cancel a pending match and refund any deposits.
    /// Either player can cancel a pending match.
    pub fn cancel_match(env: Env, match_id: u64, caller: Address) -> Result<(), Error> {
        let mut m: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(match_id))
            .ok_or(Error::MatchNotFound)?;

        if m.state != MatchState::Pending {
            return Err(Error::MatchAlreadyActive);
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

    /// Return the oracle address set at initialization.
    pub fn get_oracle(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Oracle)
            .ok_or(Error::Unauthorized)
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
        let deposited = m.player1_deposited as i128 + m.player2_deposited as i128;
        Ok(deposited * m.stake_amount)
    }
}

#[cfg(test)]
mod tests;
