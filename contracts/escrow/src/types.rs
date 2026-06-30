use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchState {
    Pending,       // created, awaiting deposits
    Active,        // both players deposited, game in progress
    PendingResult, // oracle submitted result, awaiting dispute window or finalization
    Completed,     // payout executed
    Cancelled,     // cancelled before activation
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
#[derive(Clone, Debug, PartialEq)]
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
    /// Ledger sequence number when match reached terminal state (Completed or Cancelled).
    pub completed_ledger: Option<u32>,
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
    ActiveMatches,
    PlayerMatches(Address),
    MatchTimeout,
    AllowedToken(Address),
    AllowedTokenCount,
    AllowlistEnforced,
    AllowedTokens,
    OracleRecord(u64),
    /// Balance snapshot for a match at a given ring-buffer slot.
    /// Slot = (snapshot index) % MAX_SNAPSHOTS_PER_MATCH — see lib.rs.
    Snapshot(u64, u32),
    /// Total number of snapshots ever recorded for a match (monotonic, never reset).
    SnapshotCount(u64),
    /// Dispute period in ledgers. 0 means no dispute period (immediate payout).
    DisputePeriod,
    /// Dispute by ID.
    Dispute(u64),
    /// Mapping from match_id to dispute_id (separate from Dispute to avoid key collisions).
    MatchDispute(u64),
    /// Monotonically increasing dispute counter.
    DisputeCount,
    /// Whether a voter has already voted on a dispute.
    DisputeVote(u64, Address),
    /// Pending winner for a match in PendingResult state.
    PendingWinner(u64),
    /// Ledger sequence by which a dispute must be raised for the match.
    ResultDeadline(u64),
}

/// The lifecycle event that triggered a balance snapshot.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotReason {
    Created,
    Deposit,
    Completed,
    Cancelled,
    ResultSubmitted,
    Finalized,
}

/// A point-in-time record of a match's escrowed balance, taken at key
/// lifecycle transitions for audit purposes.
///
/// Snapshots are stored in a fixed-size ring buffer per match (see
/// `MAX_SNAPSHOTS_PER_MATCH`); `index` identifies the snapshot's position in
/// the full chronological sequence so callers can detect gaps caused by
/// pruning of older entries.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BalanceSnapshot {
    pub match_id: u64,
    /// Monotonically increasing position in the match's snapshot history.
    pub index: u32,
    pub reason: SnapshotReason,
    /// Ledger sequence number at the time of the snapshot.
    pub ledger: u32,
    pub token: Address,
    pub token_symbol: String,
    pub stake_amount: i128,
    /// Total tokens held in escrow for this match at snapshot time.
    pub escrow_balance: i128,
    pub player1_deposited: bool,
    pub player2_deposited: bool,
}

/// State of a dispute against an oracle result.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisputeState {
    Active,
    ResolvedUpheld,
    ResolvedOverturned,
}

/// A community dispute raised against an oracle-submitted match result.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub match_id: u64,
    pub disputer: Address,
    pub evidence_hash: String,
    /// Total "yes" votes (overturn the oracle result).
    pub yes_votes: i128,
    /// Total "no" votes (uphold the oracle result).
    pub no_votes: i128,
    /// Ledger sequence by which voting must conclude.
    pub voting_deadline: u32,
    pub state: DisputeState,
}
