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
}
