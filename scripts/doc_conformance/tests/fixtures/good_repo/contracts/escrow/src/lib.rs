#![no_std]

pub mod types;

use types::{Match, MatchState};
use soroban_sdk::{Address, Env};

/// Minimum match timeout: 1 day (17,280 ledgers at 5s/ledger).
pub const MIN_MATCH_TIMEOUT_LEDGERS: u32 = 17_280;

/// Maximum match timeout: 90 days (1,555,200 ledgers at 5s/ledger).
pub const MAX_MATCH_TIMEOUT_LEDGERS: u32 = 1_555_200;

#[contract]
pub struct EscrowContract;

impl EscrowContract {
    pub fn create_match(env: Env, player1: Address, player2: Address, stake_amount: i128) -> u64 {
        0
    }

    pub fn get_match(env: Env, match_id: u64) -> Match {
        unimplemented!()
    }

    pub fn set_match_timeout(env: Env, timeout: u32) -> Result<(), u32> {
        Ok(())
    }
}
