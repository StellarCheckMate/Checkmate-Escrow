use soroban_sdk::{contracttype, Address, BytesN, String};

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
    /// When true, only tokens issued by a registered stablecoin issuer are
    /// accepted for new matches.  Disabled by default.
    pub stablecoin_only_mode: bool,
    /// Upper bound on `stake_amount` accepted by `create_match` and friends.
    /// `None` means unlimited (default).
    pub maximum_stake: Option<i128>,
    /// Runtime-configurable match expiration timeout, in seconds.
    pub match_timeout_seconds: u64,
    /// Protocol fee charged on winner payouts, in basis points of the pot
    /// (1 bp = 0.01 %). Draw refunds are never charged this fee. Default 0.
    pub protocol_fee_bps: u32,
    /// Recipient of the protocol fee collected on winner payouts.
    pub fee_recipient: Address,
    /// Minimum stake amount enforced in create_match (default 1).
    pub minimum_stake: i128,
}

/// A single fee tier entry: matches with a stake up to `max_stake` are charged
/// `fee_basis_points` (e.g. 50 = 0.5 %).  Tiers must be stored in ascending
/// `max_stake` order; the last tier acts as the catch-all for any stake that
/// exceeds all explicit thresholds.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeTier {
    /// Maximum stake (inclusive) for this tier.  Use `i128::MAX` for the
    /// open-ended final tier.
    pub max_stake: i128,
    /// Fee charged as basis points of the total pot (1 bp = 0.01 %).
    pub fee_basis_points: u32,
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
    /// Ledger sequence when conversion_rate was validated against oracle price.
    pub conversion_rate_ledger: Option<u32>,
    /// Ledger when pause started (if any).
    pub paused_ledger: Option<u32>,
    /// Total pause duration in ledgers.
    pub total_pause_duration: u32,
    /// Optional referrer address for referral fee sharing.
    pub referrer: Option<Address>,
    /// Ledger timestamp (Unix seconds) of the last recorded match activity.
    /// Initialized at match creation and refreshed on each deposit so that
    /// `dispute_and_rollback_match` can enforce the 24h dispute window based
    /// on the most recent in-game activity.
    pub last_heartbeat: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TempOracleRotation {
    pub old_oracle: Address,
    pub temp_oracle: Address,
    pub expiry: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingOracleRotation {
    pub old_oracle: Address,
    pub new_oracle: Address,
}

/// Combined oracle-rotation state, stored under a single `DataKey::OracleRotation`
/// key so the two independent rotation mechanisms (temporary auto-expiring vs.
/// permanent two-step) don't each need their own top-level storage key.
///
/// Fields are flattened rather than `Option<TempOracleRotation>` /
/// `Option<PendingOracleRotation>` because this SDK version's `#[contracttype]`
/// doesn't support `Option<CustomStruct>` fields — only `Option<primitive>`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct OracleRotationState {
    pub temp_old_oracle: Option<Address>,
    pub temp_new_oracle: Option<Address>,
    pub temp_expiry: Option<u64>,
    pub pending_old_oracle: Option<Address>,
    pub pending_new_oracle: Option<Address>,
}

impl OracleRotationState {
    pub fn temp(&self) -> Option<TempOracleRotation> {
        match (
            &self.temp_old_oracle,
            &self.temp_new_oracle,
            self.temp_expiry,
        ) {
            (Some(old), Some(new), Some(expiry)) => Some(TempOracleRotation {
                old_oracle: old.clone(),
                temp_oracle: new.clone(),
                expiry,
            }),
            _ => None,
        }
    }

    pub fn set_temp(&mut self, temp: Option<TempOracleRotation>) {
        match temp {
            Some(t) => {
                self.temp_old_oracle = Some(t.old_oracle);
                self.temp_new_oracle = Some(t.temp_oracle);
                self.temp_expiry = Some(t.expiry);
            }
            None => {
                self.temp_old_oracle = None;
                self.temp_new_oracle = None;
                self.temp_expiry = None;
            }
        }
    }

    pub fn pending(&self) -> Option<PendingOracleRotation> {
        match (&self.pending_old_oracle, &self.pending_new_oracle) {
            (Some(old), Some(new)) => Some(PendingOracleRotation {
                old_oracle: old.clone(),
                new_oracle: new.clone(),
            }),
            _ => None,
        }
    }

    pub fn set_pending(&mut self, pending: Option<PendingOracleRotation>) {
        match pending {
            Some(p) => {
                self.pending_old_oracle = Some(p.old_oracle);
                self.pending_new_oracle = Some(p.new_oracle);
            }
            None => {
                self.pending_old_oracle = None;
                self.pending_new_oracle = None;
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.temp_old_oracle.is_none() && self.pending_old_oracle.is_none()
    }
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
    PlayerMatches(Address),
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
    /// Vote weight snapshot for a dispute voter at dispute-creation time.
    DisputeVoteWeight(u64, Address),
    /// Minimum bond amount required to open a dispute (basis points of match stake).
    DisputeBondBasisPoints,
    /// Minimum ledger hold duration required for vote eligibility.
    MinimumHoldDuration,
    /// Quorum threshold as percentage of dispute snapshot weight (basis points).
    QuorumBasisPoints,
    /// Oracle address implicated by a dispute result (used for automatic slashing).
    DisputeOracle(u64),
    /// Active match for a player: indexed O(1) removal. Replaces the single ActiveMatches vector.
    ActiveMatch(Address, u64),
    /// Count of currently-active matches for a player, capped at MAX_ACTIVE_MATCHES_PER_PLAYER.
    PlayerActiveMatchCount(Address),
    /// Cached count of completed matches for a player, updated atomically at completion.
    PlayerCompletedMatchCount(Address),
    /// Whether a given address is a registered stablecoin issuer.
    StablecoinIssuer(Address),
    /// Total number of registered stablecoin issuers.
    StablecoinIssuerCount,
    PendingUpgradeHash,
    UpgradeScheduledAt,
    ContractVersion,
    ReferralShareBasisPoints,
    BlacklistedToken(Address),
    BlacklistedTokens,
    FeeTiers,
    PlayerPreferredToken(Address),
    /// Platform-wide aggregated statistics (total matches, volume, payouts).
    Stats,
    /// Stores the number of oracle confirmations received for a given match.
    OracleConfirmations(u64),
    /// Tracks the result (Winner) submitted by a specific oracle for a match.
    /// Key: (match_id, oracle_address) → Winner
    OracleVote(u64, Address),
    /// Stores the approved oracle list (Vec<Address>) used by consensus.
    ApprovedOracles,
    /// Stores the required number of confirmations for consensus (u32).
    RequiredOracleConfirmations,
    /// Reentrancy guard for `deposit`, keyed by match id.
    DepositInProgress(u64),
    /// Combined temp + pending oracle rotation state (see `OracleRotationState`).
    OracleRotation,
    /// Tracks a rejected/conflicting vote submitted by an oracle for a match.
    /// Key: (match_id, oracle_address) → Winner (the conflicting result)
    RejectedOracleVote(u64, Address),
    /// Flags a match as deadlocked (threshold unreachable given approved oracle set).
    /// Key: match_id → true
    OracleDeadlock(u64),
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
    /// Bonded stake required to open dispute; refunded on overturn, forfeited on upheld.
    pub dispute_bond: i128,
    /// Snapshot ledger for vote weight calculation; prevents flash-loan acquisition attacks.
    pub snapshot_ledger: u32,
    /// Total participating weight at vote snapshot; used for quorum calculation.
    pub snapshot_total_weight: i128,
    /// Minimum participation weight required for resolution.
    pub quorum_threshold: i128,
}

/// A point-in-time record of a match's escrowed balance, taken at key
/// lifecycle transitions for audit purposes.
///
/// Snapshots are stored in a fixed-size ring buffer per match (see
/// `MAX_SNAPSHOTS_PER_MATCH`); `index` identifies the snapshot's position in
/// the full chronological sequence so callers can detect gaps caused by
/// pruning of older entries.
///
/// `commitment` is a SHA-256 hash-commitment to `(stake_amount,
/// escrow_balance, nonce)`, computed once when the snapshot is recorded. It
/// lets a non-admin caller (see `redact_snapshot`) hold a value that is
/// verifiable against a later admin disclosure without ever seeing
/// `stake_amount`/`escrow_balance` themselves. See `docs/privacy-model.md`.
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
    /// Random salt the commitment is bound to. Only ever populated in the
    /// admin's full-access view — redacted to zero elsewhere so it cannot be
    /// used to re-derive or brute-force `commitment` ahead of an intentional
    /// admin disclosure.
    pub nonce: BytesN<32>,
    /// `sha256(stake_amount || escrow_balance || nonce)`. Present in both the
    /// full and redacted views so a non-admin caller still has something
    /// verifiable in place of the zeroed-out amounts.
    pub commitment: BytesN<32>,
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

/// Result of `get_balance_at_timestamp`, distinguishing a genuine zero
/// balance from data the ring buffer has pruned away.
///
/// Returning a bare `i128` made these two cases indistinguishable: a
/// caller had no way to tell "this player had 0 escrowed at that ledger"
/// from "the answer might be nonzero but the snapshot that would prove it
/// has been overwritten". See `docs/privacy-model.md`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BalanceAtTimestamp {
    /// A snapshot at or before the requested ledger was found and retained;
    /// this is the player's aggregate escrow balance at that point.
    Known(i128),
    /// The player has no snapshot at or before the requested ledger, and no
    /// pruning has occurred — this is a genuine absence of history, not a
    /// blind spot.
    NoHistory,
    /// The ring buffer has overwritten every snapshot old enough to answer
    /// this query. The true balance at that point is unknown, not zero.
    Pruned,
}

/// Platform-wide aggregated statistics for analytics.
///
/// Incremented atomically during `create_match` (for total_matches and total_volume)
/// and `submit_result` (for total_payouts). Used by off-chain analytics to avoid
/// the need for full indexing of on-chain events.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformStats {
    /// Total number of matches created across all time.
    pub total_matches: u64,
    /// Total value (in base token units) staked across all matches.
    pub total_volume: i128,
    /// Total number of successful payouts (winner or draw completed matches).
    pub total_payouts: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    /// `temp()` should reassemble a `TempOracleRotation` from the flattened
    /// fields when all three are present (the only case production code at
    /// `EscrowContract::effective_oracle` treats as an active rotation).
    #[test]
    fn temp_returns_some_when_all_three_fields_are_set() {
        let env = Env::default();
        let old = Address::generate(&env);
        let new = Address::generate(&env);
        let mut state = OracleRotationState::default();
        state.set_temp(Some(TempOracleRotation {
            old_oracle: old.clone(),
            temp_oracle: new.clone(),
            expiry: 12345,
        }));

        let temp = state
            .temp()
            .expect("all three fields set, should round-trip");
        assert_eq!(temp.old_oracle, old);
        assert_eq!(temp.temp_oracle, new);
        assert_eq!(temp.expiry, 12345);
    }

    /// `set_temp(None)` must clear all three flattened fields so a
    /// subsequent `temp()` observes no active rotation.
    #[test]
    fn set_temp_none_clears_all_fields() {
        let env = Env::default();
        let old = Address::generate(&env);
        let new = Address::generate(&env);
        let mut state = OracleRotationState::default();
        state.set_temp(Some(TempOracleRotation {
            old_oracle: old,
            temp_oracle: new,
            expiry: 999,
        }));
        assert!(state.temp().is_some());

        state.set_temp(None);

        assert!(state.temp().is_none());
        assert!(state.temp_old_oracle.is_none());
        assert!(state.temp_new_oracle.is_none());
        assert!(state.temp_expiry.is_none());
    }

    /// `pending()` must report `None` both when nothing has ever been set and
    /// after a previously-set pending rotation is cleared -- not just when
    /// the fields happen to be partially populated.
    #[test]
    fn pending_returns_none_when_unset_and_after_clearing() {
        let state = OracleRotationState::default();
        assert!(state.pending().is_none());

        let env = Env::default();
        let old = Address::generate(&env);
        let new = Address::generate(&env);
        let mut state = OracleRotationState::default();
        state.set_pending(Some(PendingOracleRotation {
            old_oracle: old,
            new_oracle: new,
        }));
        assert!(state.pending().is_some());

        state.set_pending(None);
        assert!(state.pending().is_none());
    }

    /// `set_pending(Some(..))` followed by `set_pending(None)` must fully
    /// clear both flattened fields (not just make `pending()` return `None`
    /// by coincidence of one field being unset).
    #[test]
    fn set_pending_none_clears_both_fields() {
        let env = Env::default();
        let old = Address::generate(&env);
        let new = Address::generate(&env);
        let mut state = OracleRotationState::default();
        state.set_pending(Some(PendingOracleRotation {
            old_oracle: old,
            new_oracle: new,
        }));

        state.set_pending(None);

        assert!(state.pending_old_oracle.is_none());
        assert!(state.pending_new_oracle.is_none());
    }

    /// `is_empty()` and round-tripping both temp and pending rotations
    /// simultaneously (they're independent, flattened onto the same struct).
    #[test]
    fn temp_and_pending_are_independent() {
        let env = Env::default();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);
        let d = Address::generate(&env);

        let mut state = OracleRotationState::default();
        assert!(state.is_empty());

        state.set_temp(Some(TempOracleRotation {
            old_oracle: a.clone(),
            temp_oracle: b.clone(),
            expiry: 1,
        }));
        assert!(!state.is_empty());
        assert!(state.pending().is_none());

        state.set_pending(Some(PendingOracleRotation {
            old_oracle: c.clone(),
            new_oracle: d.clone(),
        }));
        assert!(state.temp().is_some());
        assert!(state.pending().is_some());

        state.set_temp(None);
        assert!(!state.is_empty(), "pending rotation is still set");
        assert!(state.temp().is_none());

        state.set_pending(None);
        assert!(state.is_empty());
    }
}
