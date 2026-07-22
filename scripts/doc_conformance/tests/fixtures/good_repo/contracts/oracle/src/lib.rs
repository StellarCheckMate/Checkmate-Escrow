#![no_std]

use soroban_sdk::{Address, Env};

#[contract]
pub struct OracleContract;

impl OracleContract {
    pub fn submit_result(env: Env, match_id: u64) -> Result<(), u32> {
        Ok(())
    }

    pub fn get_result(env: Env, match_id: u64) -> u32 {
        0
    }
}
