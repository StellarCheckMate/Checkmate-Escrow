use soroban_sdk::{contracttype, Address, String};

/// Canonical result enum shared conceptually with the escrow contract.
/// Variants mirror escrow's `Winner` enum for consistency.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Winner {
    Player1,
    Player2,
    Draw,
}

/// Chess platform identifier. Mirrors escrow's `Platform` for cross-contract consistency.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Platform {
    Lichess,
    ChessDotCom,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ResultEntry {
    pub game_id: String,
    pub platform: Platform,
    pub result: Winner,
    /// Ledger sequence number at which this result was submitted.
    pub submitted_ledger: u32,
    /// Address of the admin who submitted this result.
    pub submitter: Address,
}

/// A single entry in a batch result submission.
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchResultEntry {
    pub match_id: u64,
    pub game_id: String,
    pub platform: Platform,
    pub result: Winner,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Result(u64), // keyed by match_id
    Paused,      // emergency pause state
}
