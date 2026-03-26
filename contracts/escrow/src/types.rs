use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchState {
    Pending,   // created, awaiting deposits
    Active,    // both players deposited, game in progress
    Completed, // result submitted, payout executed
    Cancelled, // cancelled before activation
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Platform {
    Lichess,
    ChessDotCom,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Winner {
    Player1,
    Player2,
    Draw,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct Match {
    pub id: u64,
    pub player1: Address,
    pub player2: Address,
    pub stake_amount: i128,
    pub token: Address,
    pub game_id: String,
    pub platform: Platform,
    pub state: MatchState,
    pub player1_deposited: bool,
    pub player2_deposited: bool,
    /// Ledger sequence number at match creation. Used for timeout and ordering logic.
    pub created_ledger: u32,
}

#[contracttype]
pub enum DataKey {
    Match(u64),
    MatchCount,
    Oracle,
    Admin,
    Paused,
    GameId(String),
}
