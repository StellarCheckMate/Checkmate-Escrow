//! Match state transition logic for the escrow contract.
//!
//! This module centralizes the validation and application of state transitions
//! between [`MatchState`] values. Keeping this logic in one place makes the
//! transitions easier to audit, test, and extend.

use soroban_sdk::{contracterror, panic_with_error, Env};

use crate::types::{Match, MatchState};

/// Errors that can occur when attempting a match state transition.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum StateTransitionError {
    /// The requested transition is not valid from the current state.
    InvalidTransition = 1,
}

/// Returns `true` if a transition from `from` to `to` is permitted.
///
/// The valid lifecycle is:
/// `Pending -> Active -> PendingResult -> Completed | Cancelled`
///
/// Additionally, a `Pending` match may be cancelled directly, and a
/// `PendingResult` match may be cancelled (e.g. via dispute resolution).
pub(crate) fn is_valid_transition(from: &MatchState, to: &MatchState) -> bool {
    match (from, to) {
        // A pending match becomes active once both parties are committed.
        (MatchState::Pending, MatchState::Active) => true,
        // A pending match can be cancelled before it becomes active.
        (MatchState::Pending, MatchState::Cancelled) => true,
        // An active match moves to pending result once a result is submitted.
        (MatchState::Active, MatchState::PendingResult) => true,
        // An active match can be cancelled (e.g. timeout / dispute).
        (MatchState::Active, MatchState::Cancelled) => true,
        // A pending result is finalized as completed.
        (MatchState::PendingResult, MatchState::Completed) => true,
        // A pending result can be cancelled (e.g. dispute resolution).
        (MatchState::PendingResult, MatchState::Cancelled) => true,
        // Any other combination is invalid.
        _ => false,
    }
}

/// Validates that a transition from `from` to `to` is permitted, panicking
/// with [`StateTransitionError::InvalidTransition`] otherwise.
pub(crate) fn validate_transition(env: &Env, from: &MatchState, to: &MatchState) {
    if !is_valid_transition(from, to) {
        panic_with_error!(env, StateTransitionError::InvalidTransition);
    }
}

/// Applies a transition on the given match, validating it first.
///
/// This is the primary entry point used by `lib.rs` when a function needs to
/// move a match to a new state.
pub(crate) fn transition(env: &Env, match_record: &mut Match, to: MatchState) {
    validate_transition(env, &match_record.state, &to);
    match_record.state = to;
}

/// Transitions a match to [`MatchState::Active`].
pub(crate) fn activate(env: &Env, match_record: &mut Match) {
    transition(env, match_record, MatchState::Active);
}

/// Transitions a match to [`MatchState::PendingResult`].
pub(crate) fn submit_result(env: &Env, match_record: &mut Match) {
    transition(env, match_record, MatchState::PendingResult);
}

/// Transitions a match to [`MatchState::Completed`].
pub(crate) fn complete(env: &Env, match_record: &mut Match) {
    transition(env, match_record, MatchState::Completed);
}

/// Transitions a match to [`MatchState::Cancelled`].
pub(crate) fn cancel(env: &Env, match_record: &mut Match) {
    transition(env, match_record, MatchState::Cancelled);
}

/// Returns `true` if the match is in a terminal state.
pub(crate) fn is_terminal(state: &MatchState) -> bool {
    matches!(state, MatchState::Completed | MatchState::Cancelled)
}