use soroban_sdk::contracterror;

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
    /// The computed `amount_out` from a swap is below the caller's
    /// `min_amount_out` slippage floor.  The swap is aborted and no
    /// funds change hands.
    SlippageExceeded = 12,
    /// A numeric overflow occurred during swap price computation.
    Overflow = 13,
    /// `amount_in` supplied to `swap` must be strictly positive.
    InvalidAmount = 14,
}
