use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Unauthorized = 1,
    AlreadySubmitted = 2,
    ResultNotFound = 3,
    AlreadyInitialized = 4,
}
