use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// (1) No match exists for the given `match_id`.
    MatchNotFound = 1,

    /// (2) The calling player has already deposited their stake for this match.
    AlreadyFunded = 2,

    /// (3) `submit_result` was called before both players have deposited.
    NotFunded = 3,

    /// (4) The caller is not authorised to perform this action (not a player,
    /// not the admin, or not the oracle).
    Unauthorized = 4,

    /// (5) The match is not in the required state for this operation
    /// (e.g. trying to deposit into an Active match, or expire a Completed one).
    InvalidState = 5,

    /// (6) A match with this ID already exists in storage.
    AlreadyExists = 6,

    /// (7) `initialize` has already been called; the contract cannot be
    /// re-initialized.
    AlreadyInitialized = 7,

    /// (8) An arithmetic operation would overflow. Currently guards the
    /// internal match-ID counter.
    Overflow = 8,

    /// (9) The contract is paused. `create_match`, `deposit`, and
    /// `submit_result` are blocked until an admin calls `unpause`.
    ContractPaused = 9,

    /// (10) `stake_amount` is zero or negative.
    InvalidAmount = 10,

    /// (11) `cancel_match` was called on a match that is no longer Pending
    /// (i.e. it is Active, Completed, or already Cancelled).
    MatchAlreadyActive = 11,

    /// (12) `expire_match` was called before the match timeout has elapsed.
    MatchNotExpired = 12,

    /// (13) The supplied address is invalid for the context — e.g. passing the
    /// escrow contract's own address as the oracle during `initialize`.
    InvalidAddress = 13,
    InvalidPlayers = 14,

    /// (15) The `game_id` is empty or invalid.
    InvalidGameId = 15,
}
