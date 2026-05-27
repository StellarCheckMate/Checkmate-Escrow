use soroban_sdk::{contracttype, String};

/// Canonical result enum shared conceptually with the escrow contract.
/// Variants mirror escrow's `Winner` enum for consistency.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Winner {
    Player1,
    Player2,
    Draw,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct ResultEntry {
    pub game_id: String,
    pub result: Winner,
    /// Ledger sequence number at which this result was submitted.
    pub submitted_ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Result(u64), // keyed by match_id
    Paused,      // emergency pause state
}
