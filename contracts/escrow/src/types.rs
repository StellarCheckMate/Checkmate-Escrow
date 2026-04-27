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

/// Represents a single wagered chess match held in escrow.
///
/// # Stable API
/// The following fields are part of the stable public API and safe to read
/// by external callers and integrations:
/// `id`, `player1`, `player2`, `stake_amount`, `token`, `game_id`,
/// `platform`, `state`, `winner`, `created_ledger`, `completed_ledger`.
///
/// # Internal State
/// `player1_deposited` and `player2_deposited` are internal bookkeeping
/// fields used by the contract to track deposit progress. Callers should
/// prefer `is_funded()` to check whether a match is fully funded.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Match {
    /// Unique auto-incrementing match identifier assigned at creation.
    pub id: u64,
    /// Stellar address of the first player (match creator).
    pub player1: Address,
    /// Stellar address of the second player (invited opponent).
    pub player2: Address,
    /// Amount each player must stake, in the smallest unit of `token`.
    pub stake_amount: i128,
    /// Contract address of the token used for staking (XLM or USDC).
    pub token: Address,
    /// External game identifier from the chess platform (e.g. Lichess game ID).
    pub game_id: String,
    /// Chess platform where the game is being played.
    pub platform: Platform,
    /// Current lifecycle state of the match.
    pub state: MatchState,
    /// Internal: whether player1 has deposited their stake. Use `is_funded()` externally.
    pub player1_deposited: bool,
    /// Internal: whether player2 has deposited their stake. Use `is_funded()` externally.
    pub player2_deposited: bool,
    /// Ledger sequence number at which the match was created.
    pub created_ledger: u32,
    /// Ledger sequence number at which the match was completed or cancelled, if applicable.
    pub completed_ledger: Option<u32>,
    /// Outcome of the match. Defaults to `Winner::Draw` until a result is submitted.
    pub winner: Winner,
}

#[contracttype]
pub enum DataKey {
    Match(u64),
    MatchCount,
    Oracle,
    Admin,
    PendingAdmin,
    Paused,
    GameId(String),
    MatchTimeout,
    PlayerMatches(Address),
    ActiveMatches,
    AllowedToken(Address),
    AllowlistEnabled,
}
