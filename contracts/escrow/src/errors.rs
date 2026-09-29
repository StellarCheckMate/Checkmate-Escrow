use soroban_sdk::contracterror;

/// Escrow contract error codes and variants.
///
/// Every error is represented as a small integer (`u32`) discriminant. When a contract call fails,
/// the CLI/SDK returns something like `Error(Contract, #4)`.
///
/// **For the complete error reference with causes, recovery actions, and examples,**
/// **see [`docs/error-codes.md`](../../../docs/error-codes.md).**
///
/// This document is kept in lockstep with this enum — if you add or remove a variant,
/// update `docs/error-codes.md` in the same PR.
///
/// There is no XDR-enforced 50-variant cap on `#[contracterror]` enums in Soroban.
/// New variants can be added at any time. Previously occupied reserved slots (11, 12, 38,
/// 44, 53) may be reclaimed for dedicated errors rather than reusing unrelated codes.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    MatchNotFound = 1,
    AlreadyFunded = 2,
    NotFunded = 3,
    Unauthorized = 4,
    InvalidState = 5,
    AlreadyExists = 6,
    AlreadyInitialized = 7,
    Overflow = 8,
    ContractPaused = 9,
    InvalidAmount = 10,
    /// The player account has been frozen by the admin and cannot create or join
    /// new matches or deposit into escrow until the admin calls `admin_unfreeze_player`.
    /// A freeze never blocks fund recovery — `cancel_match`, `expire_match`, and
    /// `claim_vested_payout` remain accessible to frozen players.
    PlayerFrozen = 11,
    DuplicateGameId = 13,
    MatchNotExpired = 14,
    InvalidGameId = 15,
    InvalidPlayers = 16,
    TokenNotAllowed = 17,
    InvalidAddress = 18,
    MatchAlreadyActive = 19,
    InvalidTimeout = 20,
    SnapshotNotFound = 21,
    VestingNotExpired = 22,
    AlreadyClaimed = 23,
    DisputeNotFound = 24,
    PendingResultNotFound = 25,
    DisputeAlreadyResolved = 26,
    /// The dispute voting window has elapsed. Also returned by `dispute_oracle_result`
    /// when the dispute period has elapsed (`env.ledger().sequence() >= deadline`).
    VotingPeriodElapsed = 27,
    AlreadyVoted = 28,
    NotStaker = 29,
    VotingPeriodNotElapsed = 30,
    MatchNotInPendingResult = 31,
    DisputePeriodNotElapsed = 32,
    DisputeAlreadyRaised = 33,
    InvalidEvidenceHash = 34,
    TierStakeNotAllowed = 35,
    NotInitialized = 36,
    InvalidPauseState = 37,
    ConversionRateOutOfBounds = 39,
    ConversionRateStalePriceSource = 40,
    InsufficientBond = 41,
    QuorumNotMet = 42,
    InsufficientHoldingDuration = 43,
    TooManyActiveMatches = 45,
    /// Token is not issued by a registered stablecoin issuer and stablecoin-only mode is enabled.
    NotStablecoin = 46,
    UpgradeNotScheduled = 47,
    UpgradeReviewPeriodNotElapsed = 48,
    InvalidVersion = 49,
    UpgradeAlreadyScheduled = 50,
    /// Oracle has already submitted a confirmation for this match.
    OracleAlreadyConfirmed = 51,
    /// Oracle submitted a result that conflicts with a previously recorded majority result.
    ConflictingResult = 52,
    /// The caller is not a registered oracle.
    NotAnOracle = 54,
    /// A deposit for this match is already in progress (reentrancy guard).
    DepositInProgress = 55,
    /// The dispute's voting deadline plus grace period has not yet elapsed, so the
    /// no-quorum fallback resolution cannot be applied yet.
    DisputeFallbackNotElapsed = 56,
}
