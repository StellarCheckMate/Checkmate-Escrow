use soroban_sdk::contracterror;

/// Oracle contract error codes and variants.
///
/// Every error is represented as a small integer (`u32`) discriminant. When a contract call fails,
/// the CLI/SDK returns something like `Error(Contract, #1)`.
///
/// **For the complete error reference with causes, recovery actions, and examples,**
/// **see [`docs/error-codes.md`](../../../docs/error-codes.md).**
///
/// This document is kept in lockstep with this enum — if you add or remove a variant,
/// update `docs/error-codes.md` in the same PR.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// Caller is not the authorized oracle submitter.
    Unauthorized = 1,
    /// A result has already been submitted for this match.
    AlreadySubmitted = 2,
    /// No result has been submitted for the requested match.
    ResultNotFound = 3,
    /// The contract has already been initialized.
    AlreadyInitialized = 4,
    /// The contract is paused and not accepting submissions.
    ContractPaused = 5,
    InvalidGameId = 6,
    /// Batch exceeds the maximum allowed size (100 entries).
    BatchTooLarge = 7,
    /// Batch contains duplicate match_ids.
    BatchDuplicateEntry = 8,
    /// Oracle has exceeded its hourly or daily submission rate limit.
    RateLimitExceeded = 9,
    /// Rate limit values supplied to `set_oracle_rate_limits` are invalid.
    InvalidRateLimit = 10,
    /// The oracle does not have enough staked balance to submit a result.
    InsufficientStake = 11,
    /// `submit_oracle_result` was called by an address that has never
    /// registered via `register_oracle_with_stake`.
    NotRegisteredOracle = 12,
    /// The match's m-of-n consensus has deadlocked (no remaining eligible
    /// oracle vote can push any candidate result over the threshold) and is
    /// awaiting admin resolution via `resolve_disputed_match`.
    MatchDisputed = 14,
    /// `set_consensus_threshold` was called with a threshold of 0.
    InvalidThreshold = 15,
    /// `resolve_disputed_match` was called for a match that is not in a
    /// disputed (deadlocked) consensus state.
    MatchNotDisputed = 16,
    /// The oracle has been deactivated due to SLA violations.
    OracleDeactivated = 17,
    /// The oracle cannot be deactivated because its average response time is within SLA (<= 5s).
    OracleNotSlow = 18,
    InvalidAmount = 19,
    Overflow = 20,
    SlippageExceeded = 21,
}
