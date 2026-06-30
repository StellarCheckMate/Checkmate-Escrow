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
