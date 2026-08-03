use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MatchStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "expired")]
    Expired,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Winner {
    #[serde(rename = "player1")]
    Player1,
    #[serde(rename = "player2")]
    Player2,
    #[serde(rename = "draw")]
    Draw,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexedEvent {
    pub id: String,
    pub ledger_sequence: u32,
    pub match_id: u64,
    pub event_type: String,
    pub player1: Option<String>,
    pub player2: Option<String>,
    pub status: Option<String>,
    pub winner: Option<String>,
    pub stake_amount: Option<String>,
    pub token: Option<String>,
    pub game_id: Option<String>,
    pub platform: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub txn_hash: Option<String>,
    pub event_index_in_txn: Option<u16>,
    pub reorg_invalidated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchInfo {
    pub match_id: u64,
    pub player1: String,
    pub player2: String,
    pub status: MatchStatus,
    pub winner: Option<Winner>,
    pub stake_amount: String,
    pub token: String,
    pub game_id: String,
    pub platform: String,
    pub created_ledger: u32,
    pub completed_ledger: Option<u32>,
    pub events: Vec<IndexedEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryFilters {
    pub player_address: Option<String>,
    pub status: Option<MatchStatus>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ── Analytics models ──────────────────────────────────────────────────────────

/// Platform-wide statistics returned by `GET /analytics/overview`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnalyticsOverview {
    /// Total number of distinct matches ever created.
    pub total_matches: i64,
    /// Sum of all stake_amount values (as string to avoid precision loss).
    pub total_volume: String,
    /// Average stake amount across all matches (as string).
    pub average_stake: String,
    /// Number of matches that reached the `completed` state.
    pub completed_matches: i64,
    /// Number of matches that reached the `cancelled` state.
    pub cancelled_matches: i64,
}

/// Per-player statistics returned by `GET /analytics/player/:player_address`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerAnalytics {
    pub player_address: String,
    /// Total matches the player participated in.
    pub total_matches: i64,
    pub wins: i64,
    pub losses: i64,
    pub draws: i64,
    /// Win rate as a percentage (0.0–100.0).
    pub win_rate: f64,
    /// Sum of stake_amount for matches the player won (as string).
    pub total_winnings: String,
    /// Paginated match history.
    pub match_history: Vec<MatchInfo>,
    /// Total match count (before pagination) for the caller to compute pages.
    pub total: i64,
}

/// Per-token statistics returned by `GET /analytics/token/:token_address`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TokenAnalytics {
    pub token_address: String,
    /// Sum of all stake amounts for this token (as string).
    pub total_volume: String,
    /// Number of matches that used this token.
    pub match_count: i64,
    /// Average stake amount for this token (as string).
    pub average_stake: String,
    /// Paginated match history.
    pub match_history: Vec<MatchInfo>,
    /// Total match count (before pagination).
    pub total: i64,
}
