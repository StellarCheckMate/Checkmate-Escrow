use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// Code 1 — The caller is not the authorised oracle admin.
    Unauthorized = 1,

    /// Code 2 — A result for this match ID has already been submitted;
    /// results are immutable once recorded.
    AlreadySubmitted = 2,

    /// Code 3 — No result has been recorded for the requested match ID.
    ResultNotFound = 3,

    /// Code 4 — `initialize` has already been called on this contract;
    /// it can only be called once.
    AlreadyInitialized = 4,

    /// Code 5 — The match ID referenced does not exist in the escrow contract.
    MatchNotFound = 5,
}
