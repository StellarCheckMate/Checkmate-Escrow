use soroban_sdk::{contracttype, Address, String};

/// Canonical result enum shared conceptually with the escrow contract.
/// Variants mirror escrow's `Winner` enum for consistency.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Winner {
    Player1,
    Player2,
    Draw,
}

/// Chess platform identifier. Mirrors escrow's `Platform` for cross-contract consistency.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Platform {
    Lichess,
    ChessDotCom,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ResultEntry {
    pub game_id: String,
    pub platform: Platform,
    pub result: Winner,
    /// Ledger sequence number at which this result was submitted.
    pub submitted_ledger: u32,
    /// Address of the admin who submitted this result.
    pub submitter: Address,
}

/// A single entry in a batch result submission.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchResultEntry {
    pub match_id: u64,
    pub game_id: String,
    pub platform: Platform,
    pub result: Winner,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Result(u64), // keyed by match_id
    Paused,      // emergency pause state
    /// Per-oracle override of the default hourly/daily submission limits.
    OracleRateLimit(Address),
    /// Sliding window submission counters for the hourly limit, keyed by oracle address.
    OracleHourlyWindow(Address),
    /// Sliding window submission counters for the daily limit, keyed by oracle address.
    OracleDailyWindow(Address),
    Rate(Address, Address),
}

/// Configurable submission limits for a single oracle address.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    pub hourly_limit: u32,
    pub daily_limit: u32,
}

/// Sliding-window counter state for a single rate-limit window.
///
/// Uses the "sliding window counter" approximation: `current_count` tracks
/// submissions since `window_start`, and `previous_count` carries the count
/// from the immediately preceding window so it can be weighted by the
/// fraction of that window which still overlaps the sliding lookback period.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateWindow {
    pub window_start: u64,
    pub current_count: u32,
    pub previous_count: u32,
}

/// Point-in-time rate limit usage for a single oracle, returned to callers
/// in lieu of HTTP rate-limit headers (there is no HTTP layer on-chain).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RateLimitStatus {
    pub hourly_used: u32,
    pub hourly_limit: u32,
    pub hourly_remaining: u32,
    pub daily_used: u32,
    pub daily_limit: u32,
    pub daily_remaining: u32,
}
