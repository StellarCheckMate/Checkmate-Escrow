#![no_std]

pub mod types;

use types::{Match, MatchState};
use soroban_sdk::{Address, Env};

/// Minimum match timeout: 1 day (86,400 seconds).
pub const MIN_MATCH_TIMEOUT_SECONDS: u64 = 86_400;

/// Maximum match timeout: 90 days (7,776,000 seconds).
pub const MAX_MATCH_TIMEOUT_SECONDS: u64 = 7_776_000;

#[contract]
pub struct EscrowContract;

impl EscrowContract {
    pub fn create_match(env: Env, player1: Address, player2: Address, stake_amount: i128) -> u64 {
        0
    }

    pub fn get_match(env: Env, match_id: u64) -> Match {
        unimplemented!()
    }

    pub fn set_match_timeout(env: Env, seconds: u64) -> Result<(), u32> {
        Ok(())
    }
}
