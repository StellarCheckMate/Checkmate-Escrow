use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MatchState {
    Pending,       // created, awaiting deposits
    Active,        // both players deposited, game in progress
    PendingResult, // oracle submitted result, awaiting dispute window or finalization
    Completed,     // payout executed
    Cancelled,     // cancelled before activation
    Paused,        // match paused by player
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
    /// No winner determined yet (match still in progress).
    None,
    Player1,
    Player2,
    Draw,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlayerTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtocolConfig {
    pub vesting_duration_seconds: u64,
    pub cancellation_fee_basis_points: u32,
    pub treasury: Address,
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
    pub winner: Winner,
    pub vested_at: Option<u64>,
    pub player1_claimed: bool,
    pub player2_claimed: bool,
    /// Optional conversion rate for multi-token matches.
    pub conversion_rate: Option<i128>,
    /// Optional second token for multi-token matches.
    pub token_b: Option<Address>,
    /// Ledger when pause started (if any).
    pub paused_ledger: Option<u32>,
    /// Total pause duration in ledgers.
    pub total_pause_duration: u32,
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
    ProtocolConfig,
    /// Dispute period in ledger blocks (0 = immediate payout).
    DisputePeriod,
    /// Pending result winner for a match awaiting dispute resolution.
    PendingWinner(u64),
    /// Deadline ledger for dispute voting on a match.
    ResultDeadline(u64),
    /// Dispute record for a match.
    Dispute(u64),
    /// Dispute vote by voter on a match.
    DisputeVote(u64, Address),
    /// Global dispute count.
    DisputeCount,
    /// Match dispute ID.
    MatchDispute(u64),
    /// Player balance snapshot: (player, index % MAX_PLAYER_SNAPSHOTS).
    PlayerBalanceSnapshot(Address, u64),
    /// Total count of player balance snapshots (monotonic).
    PlayerBalanceSnapshotCount(Address),
}

/// The lifecycle event that triggered a balance snapshot.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotReason {
    Created,
    Deposit,
    Paused,
    Resumed,
    Completed,
    Cancelled,
    ResultSubmitted,
    Finalized,
}

/// Dispute state for contested match results.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DisputeState {
    Active,
    Upheld,
    Overturned,
    ResolvedUpheld,
    ResolvedOverturned,
}

/// Dispute record for a contested match result.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct Dispute {
    pub id: u64,
    pub match_id: u64,
    pub disputer: Address,
    pub created_ledger: u32,
    pub voting_deadline: u32,
    pub state: DisputeState,
    pub evidence_hash: String,
    pub uphold_votes: u32,
    pub overturn_votes: u32,
    pub yes_votes: u32,
    pub no_votes: u32,
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

/// A point-in-time record of a player's aggregate escrow balance across all
/// of that player's deposit-eligible positions.
///
/// Recorded on every deposit, payout, refund (cancel_match), and timeout
/// (expire_match) so callers can ask "what was this player's escrow balance
/// at ledger X?". The balance field sums `stake_amount` over every non-
/// terminal match in which the player is `player1` (with `player1_deposited`)
/// or `player2` (with `player2_deposited`), i.e. the player's current
/// attributable escrow position.
///
/// Stored in a fixed-size ring buffer per player keyed by
/// `DataKey::PlayerBalanceSnapshot(player, slot)` where `slot = index %
/// MAX_PLAYER_SNAPSHOTS` (see lib.rs). Older entries are silently
/// overwritten once the buffer fills, so `index` (monotonic) lets callers
/// detect gaps caused by pruning.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerBalanceSnapshot {
    pub player: Address,
    /// Monotonically increasing position in the player's snapshot history.
    pub index: u64,
    /// Ledger sequence number at the time of the snapshot. Stored as `u64`
    /// so callers can pass arbitrary ledger sequences to
    /// `get_balance_at_timestamp` (the spec'd type).
    pub ledger: u64,
    /// Aggregate escrow balance captured at this point in time.
    pub balance: i128,
}
