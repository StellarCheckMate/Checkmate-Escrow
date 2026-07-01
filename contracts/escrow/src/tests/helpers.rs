/// Shared test helpers for the escrow contract test suite.
///
/// Centralises the common setup boilerplate that was previously duplicated
/// across every test module, and exposes small utility functions that make
/// individual tests more readable.
use super::*;
use soroban_sdk::testutils::Address as _;

// ── Primary setup fixture ────────────────────────────────────────────────────

/// Full environment fixture used by the majority of escrow tests.
///
/// Returns `(env, contract_id, oracle, player1, player2, token_addr, admin)`.
///
/// Both players start with **1 000** token units each.  The contract is
/// initialised but no matches have been created yet.
pub fn setup_env() -> (Env, Address, Address, Address, Address, Address, Address) {
    // Delegates to the existing `setup()` in mod.rs so there is a single
    // source of truth.  All modules that previously called `setup()` directly
    // can call `helpers::setup_env()` instead.
    setup()
}

// ── Convenience builders ─────────────────────────────────────────────────────

/// Create a match with sensible defaults and return its ID.
///
/// Uses `Platform::Lichess` and a stake of **100**.  The `game_id` must be
/// unique across the test — callers are responsible for passing a distinct
/// string.
pub fn create_default_match(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> u64 {
    client.create_match(
        player1,
        player2,
        &100,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    )
}

/// Create a match with a custom stake amount.
pub fn create_match_with_stake(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
    stake: i128,
) -> u64 {
    client.create_match(
        player1,
        player2,
        &stake,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    )
}

/// Deposit for both players, bringing the match to `Active` state.
pub fn fund_match(client: &EscrowContractClient, match_id: u64, player1: &Address, player2: &Address) {
    client.deposit(&match_id, player1);
    client.deposit(&match_id, player2);
}

/// Full happy-path helper: create → fund → submit result.
///
/// Returns the match ID.
pub fn run_full_match(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
    winner: &Winner,
) -> u64 {
    let id = create_default_match(client, env, player1, player2, token, game_id);
    fund_match(client, id, player1, player2);
    client.submit_result(&id, winner);
    match winner {
        Winner::Player1 => {
            client.claim_vested_payout(&id, player1);
        }
        Winner::Player2 => {
            client.claim_vested_payout(&id, player2);
        }
        Winner::Draw => {
            client.claim_vested_payout(&id, player1);
            client.claim_vested_payout(&id, player2);
        }
    }
    id
}

// ── Balance snapshot helpers ─────────────────────────────────────────────────

/// Snapshot of token balances for the two players and the contract itself.
#[derive(Debug)]
pub struct BalanceSnapshot {
    pub player1: i128,
    pub player2: i128,
    pub contract: i128,
}

impl BalanceSnapshot {
    /// Capture current balances.
    pub fn capture(
        env: &Env,
        token: &Address,
        player1: &Address,
        player2: &Address,
        contract_id: &Address,
    ) -> Self {
        let tc = soroban_sdk::token::Client::new(env, token);
        Self {
            player1: tc.balance(player1),
            player2: tc.balance(player2),
            contract: tc.balance(contract_id),
        }
    }

    /// Total tokens visible across all three accounts.
    pub fn total(&self) -> i128 {
        self.player1 + self.player2 + self.contract
    }
}

// ── Invariant assertion helpers ──────────────────────────────────────────────

/// Assert that the sum of tokens held by `player1 + player2 + contract` equals
/// `expected_total`.  This is the core fund-conservation invariant: no tokens
/// should be created or destroyed by any escrow operation.
pub fn assert_total_balance(
    env: &Env,
    token: &Address,
    player1: &Address,
    player2: &Address,
    contract_id: &Address,
    expected_total: i128,
) {
    let snap = BalanceSnapshot::capture(env, token, player1, player2, contract_id);
    assert_eq!(
        snap.total(),
        expected_total,
        "fund conservation violated: expected total={} but got player1={} + player2={} + contract={}",
        expected_total,
        snap.player1,
        snap.player2,
        snap.contract,
    );
}

/// Assert that a match is in a terminal state (`Completed` or `Cancelled`) and
/// that the contract holds **zero** tokens for it (i.e. all funds have been
/// disbursed).
pub fn assert_terminal_state_zero_escrow(
    client: &EscrowContractClient,
    match_id: u64,
) {
    let m = client.get_match(&match_id);
    assert!(
        m.state == MatchState::Completed || m.state == MatchState::Cancelled,
        "expected terminal state (Completed or Cancelled) but got {:?}",
        m.state,
    );
    assert_eq!(
        client.get_escrow_balance(&match_id),
        0,
        "escrow balance must be 0 in terminal state {:?}",
        m.state,
    );
}

/// Assert that a match in a terminal state cannot accept further deposits.
pub fn assert_no_deposit_after_terminal(
    client: &EscrowContractClient,
    match_id: u64,
    player: &Address,
) {
    let result = client.try_deposit(&match_id, player);
    assert!(
        result.is_err(),
        "deposit must be rejected once match is in a terminal state"
    );
}

/// Assert that a match in a terminal state cannot have its result submitted again.
pub fn assert_no_submit_after_terminal(
    client: &EscrowContractClient,
    match_id: u64,
) {
    let result = client.try_submit_result(&match_id, &Winner::Player1);
    assert!(
        result.is_err(),
        "submit_result must be rejected once match is in a terminal state"
    );
}
