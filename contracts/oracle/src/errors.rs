use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// (1) The contract has not been initialized, or the caller is not the
    /// stored admin.
    Unauthorized = 1,

    /// (2) A result for this `match_id` has already been submitted.
    /// Each match may only have one result recorded.
    AlreadySubmitted = 2,

    /// (3) No result has been stored for the given `match_id`.
    /// Returned by `get_result` when the match ID is unknown or the entry
    /// has expired.
    ResultNotFound = 3,

    /// (4) `initialize` has already been called; the contract cannot be
    /// re-initialized.
    AlreadyInitialized = 4,

    /// (5) The contract is paused. `submit_result` is blocked until an admin
    /// calls `unpause`.
    ContractPaused = 5,
}
