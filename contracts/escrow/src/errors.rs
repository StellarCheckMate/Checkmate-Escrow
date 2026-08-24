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
/// This enum has 50 variants, which is the hard ceiling `#[contracterror]`
/// supports in this soroban-sdk version (confirmed empirically: adding a 51st
/// panics the proc macro at compile time with `LengthExceedsMax`; not documented
/// by the SDK). Adding a new error requires repurposing an existing,
/// semantically-close variant — discriminant values may be sparse (gaps exist at
/// 11, 12, 38, 44, 53) but the variant *count* must not exceed 50.
///
/// **Reuse history**:
/// - `TooManyResults = 45` was previously `TooManyActiveMatches`. Its semantics
///   were broadened to cover any "result set exceeds an on-chain cap" situation,
///   including the scan cap enforced by `get_completed_matches`.
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
    /// A "result set too large" guard fired. Used for two distinct situations
    /// that share the same "too many" semantics:
    ///
    /// 1. **Active-match cap**: The player has exceeded the maximum number of
    ///    concurrent active matches (`MAX_ACTIVE_MATCHES_PER_PLAYER`). Wait for
    ///    some existing matches to complete or be cancelled.
    ///
    /// 2. **Scan cap**: `get_completed_matches` was called on a contract whose
    ///    total match count exceeds `GET_COMPLETED_MATCHES_SCAN_CAP`. Switch to
    ///    `get_completed_matches_paginated` to fetch completed matches in
    ///    bounded pages.
    TooManyResults = 45,
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
}
