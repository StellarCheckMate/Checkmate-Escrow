use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::sync::Mutex;
use crate::models::{IndexedEvent, MatchStatus, MatchInfo, Winner, QueryFilters};

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Database {
            conn: Mutex::new(conn),
        })
    }

    pub fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                ledger_sequence INTEGER NOT NULL,
                match_id INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                player1 TEXT,
                player2 TEXT,
                status TEXT,
                winner TEXT,
                stake_amount TEXT,
                token TEXT,
                game_id TEXT,
                platform TEXT,
                timestamp TEXT NOT NULL,
                txn_hash TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_match_id ON events(match_id);
            CREATE INDEX IF NOT EXISTS idx_player1 ON events(player1);
            CREATE INDEX IF NOT EXISTS idx_player2 ON events(player2);
            CREATE INDEX IF NOT EXISTS idx_event_type ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_timestamp ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_ledger ON events(ledger_sequence);
            "#
        )?;

        Ok(())
    }

    pub fn insert_event(&self, event: &IndexedEvent) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT OR REPLACE INTO events (id, ledger_sequence, match_id, event_type,
             player1, player2, status, winner, stake_amount, token, game_id, platform,
             timestamp, txn_hash) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                event.id,
                event.ledger_sequence,
                event.match_id,
                event.event_type,
                event.player1,
                event.player2,
                event.status,
                event.winner,
                event.stake_amount,
                event.token,
                event.game_id,
                event.platform,
                event.timestamp.to_rfc3339(),
                event.txn_hash,
            ],
        )?;

        Ok(())
    }

    pub fn get_events_by_match(&self, match_id: u64) -> Result<Vec<IndexedEvent>> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn.prepare(
            "SELECT id, ledger_sequence, match_id, event_type, player1, player2, status,
                    winner, stake_amount, token, game_id, platform, timestamp, txn_hash
             FROM events WHERE match_id = ? ORDER BY ledger_sequence ASC"
        )?;

        let events = stmt.query_map(params![match_id], |row| {
            Ok(IndexedEvent {
                id: row.get(0)?,
                ledger_sequence: row.get(1)?,
                match_id: row.get(2)?,
                event_type: row.get(3)?,
                player1: row.get(4)?,
                player2: row.get(5)?,
                status: row.get(6)?,
                winner: row.get(7)?,
                stake_amount: row.get(8)?,
                token: row.get(9)?,
                game_id: row.get(10)?,
                platform: row.get(11)?,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                txn_hash: row.get(13)?,
            })
        })?;

        let mut result = Vec::new();
        for event in events {
            result.push(event?);
        }
        Ok(result)
    }

    pub fn query_events(&self, filters: &QueryFilters) -> Result<Vec<IndexedEvent>> {
        let conn = self.conn.lock().unwrap();

        let mut query = String::from(
            "SELECT id, ledger_sequence, match_id, event_type, player1, player2, status,
                    winner, stake_amount, token, game_id, platform, timestamp, txn_hash
             FROM events WHERE 1=1"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ref player) = filters.player_address {
            query.push_str(" AND (player1 = ? OR player2 = ?)");
            params.push(Box::new(player.clone()));
            params.push(Box::new(player.clone()));
        }

        if let Some(ref status) = filters.status {
            let status_str = match status {
                MatchStatus::Pending => "pending",
                MatchStatus::Active => "active",
                MatchStatus::Completed => "completed",
                MatchStatus::Cancelled => "cancelled",
                MatchStatus::Expired => "expired",
            };
            query.push_str(" AND status = ?");
            params.push(Box::new(status_str.to_string()));
        }

        if let Some(ref start) = filters.start_date {
            query.push_str(" AND timestamp >= ?");
            params.push(Box::new(start.to_rfc3339()));
        }

        if let Some(ref end) = filters.end_date {
            query.push_str(" AND timestamp <= ?");
            params.push(Box::new(end.to_rfc3339()));
        }

        query.push_str(" ORDER BY ledger_sequence DESC");

        if let Some(limit) = filters.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = filters.offset {
            query.push_str(&format!(" OFFSET {}", offset));
        }

        let mut stmt = conn.prepare(&query)?;

        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let events = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(IndexedEvent {
                id: row.get(0)?,
                ledger_sequence: row.get(1)?,
                match_id: row.get(2)?,
                event_type: row.get(3)?,
                player1: row.get(4)?,
                player2: row.get(5)?,
                status: row.get(6)?,
                winner: row.get(7)?,
                stake_amount: row.get(8)?,
                token: row.get(9)?,
                game_id: row.get(10)?,
                platform: row.get(11)?,
                timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                txn_hash: row.get(13)?,
            })
        })?;

        let mut result = Vec::new();
        for event in events {
            result.push(event?);
        }
        Ok(result)
    }

    pub fn total_event_count(&self) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        let count = conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_latest_ledger(&self) -> Result<Option<u32>> {
        let conn = self.conn.lock().unwrap();

        let result = conn.query_row(
            "SELECT MAX(ledger_sequence) FROM events",
            [],
            |row| row.get::<_, Option<u32>>(0),
        ).ok().flatten();

        Ok(result)
    }

    pub fn build_match_info(&self, match_id: u64) -> Result<Option<MatchInfo>> {
        let events = self.get_events_by_match(match_id)?;

        if events.is_empty() {
            return Ok(None);
        }

        let created_event = events.iter().find(|e| e.event_type == "created")
            .ok_or_else(|| anyhow::anyhow!("no created event found for match {}", match_id))?;
        let latest_event = events.last().unwrap();

        let status = if let Some(ref status_str) = latest_event.status {
            match status_str.as_str() {
                "pending" => MatchStatus::Pending,
                "active" => MatchStatus::Active,
                "completed" => MatchStatus::Completed,
                "cancelled" => MatchStatus::Cancelled,
                "expired" => MatchStatus::Expired,
                _ => MatchStatus::Pending,
            }
        } else {
            MatchStatus::Pending
        };

        let winner = latest_event.winner.as_ref().and_then(|w| match w.as_str() {
            "player1" => Some(Winner::Player1),
            "player2" => Some(Winner::Player2),
            "draw" => Some(Winner::Draw),
            _ => None,
        });

        Ok(Some(MatchInfo {
            match_id,
            player1: created_event.player1.clone().unwrap_or_default(),
            player2: created_event.player2.clone().unwrap_or_default(),
            status,
            winner,
            stake_amount: created_event.stake_amount.clone().unwrap_or_default(),
            token: created_event.token.clone().unwrap_or_default(),
            game_id: created_event.game_id.clone().unwrap_or_default(),
            platform: created_event.platform.clone().unwrap_or_default(),
            created_ledger: created_event.ledger_sequence,
            completed_ledger: latest_event.ledger_sequence.into(),
            events,
        }))
    }
}
