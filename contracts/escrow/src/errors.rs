use soroban_sdk::contracterror;

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
    InvalidGameId = 11,
}
