//! PostgreSQL-backed persistence layer.
//!
//! ## Connection pools
//! Two pools are created:
//! - **write pool** – used by the ingestion path; all INSERTs go here.
//! - **read pool**  – used by API query endpoints; can point at a PG read replica.
//!
//! ## Idempotency
//! `insert_event` uses `INSERT … ON CONFLICT (id) DO NOTHING` so that re-playing
//! ledger ranges (e.g. after a crash or a leader failover) never produces
//! duplicate rows.
//!
//! ## Leader state
//! The `leader_state` table holds a single row that the `leader` module uses as
//! a distributed mutex.  The table is created here so the schema is fully
//! contained in one place.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;

use crate::models::{IndexedEvent, MatchInfo, MatchStatus, QueryFilters, Winner};

// ── Pool helpers ──────────────────────────────────────────────────────────────

/// Build a `deadpool_postgres` pool from a `postgres://…` DSN.
pub fn build_pool(dsn: &str, max_size: usize) -> Result<Pool> {
    let mut cfg = PoolConfig::new();
    cfg.url = Some(dsn.to_string());
    cfg.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    cfg.pool = Some(deadpool_postgres::PoolConfig::new(max_size));
    cfg.create_pool(Some(Runtime::Tokio1), NoTls)
        .map_err(|e| anyhow!("Failed to create connection pool: {}", e))
}

// ── Database ──────────────────────────────────────────────────────────────────

/// Primary database handle.  Cloning is cheap – both fields are `Arc`-backed.
#[derive(Clone)]
pub struct Database {
    /// Write (primary) pool – used for all mutations.
    write_pool: Pool,
    /// Read pool – may point at a replica; used for query endpoints.
    read_pool: Pool,
}

impl Database {
    pub fn new(write_pool: Pool, read_pool: Pool) -> Self {
        Database { write_pool, read_pool }
    }

    /// Build a `Database` from DSN strings and pool sizes.
    pub fn from_dsns(
        write_dsn: &str,
        read_dsn: &str,
        write_pool_size: usize,
        read_pool_size: usize,
    ) -> Result<Self> {
        let write_pool = build_pool(write_dsn, write_pool_size)?;
        let read_pool = build_pool(read_dsn, read_pool_size)?;
        Ok(Database { write_pool, read_pool })
    }

    // ── Schema ────────────────────────────────────────────────────────────

    pub async fn init_schema(&self) -> Result<()> {
        let conn = self.write_pool.get().await
            .map_err(|e| anyhow!("Pool error: {}", e))?;

        conn.batch_execute(
            r#"
            -- Main events table. PRIMARY KEY provides the idempotency guarantee.
            CREATE TABLE IF NOT EXISTS events (
                id               TEXT        PRIMARY KEY,
                ledger_sequence  INTEGER     NOT NULL,
                match_id         BIGINT      NOT NULL,
                event_type       TEXT        NOT NULL,
                player1          TEXT,
                player2          TEXT,
                status           TEXT,
                winner           TEXT,
                stake_amount     TEXT,
                token            TEXT,
                game_id          TEXT,
                platform         TEXT,
                timestamp        TIMESTAMPTZ NOT NULL,
                txn_hash         TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_events_match_id      ON events(match_id);
            CREATE INDEX IF NOT EXISTS idx_events_player1       ON events(player1);
            CREATE INDEX IF NOT EXISTS idx_events_player2       ON events(player2);
            CREATE INDEX IF NOT EXISTS idx_events_event_type    ON events(event_type);
            CREATE INDEX IF NOT EXISTS idx_events_timestamp     ON events(timestamp);
            CREATE INDEX IF NOT EXISTS idx_events_ledger        ON events(ledger_sequence);
            CREATE INDEX IF NOT EXISTS idx_events_status        ON events(status);

            -- Leader-election table.
            -- The single row (instance_id='__leader__') acts as a distributed
            -- mutex; the leader module updates `held_until` as a heartbeat.
            CREATE TABLE IF NOT EXISTS leader_state (
                instance_id TEXT        PRIMARY KEY,
                held_until  TIMESTAMPTZ NOT NULL
            );
            "#,
        )
        .await
        .map_err(|e| anyhow!("Schema init failed: {}", e))?;

        Ok(())
    }

    // ── Write path ────────────────────────────────────────────────────────

    /// Insert an event.  Uses `ON CONFLICT DO NOTHING` so re-ingestion is safe.
    pub async fn insert_event(&self, event: &IndexedEvent) -> Result<()> {
        let conn = self.write_pool.get().await
            .map_err(|e| anyhow!("Pool error: {}", e))?;

        conn.execute(
            "INSERT INTO events (
                id, ledger_sequence, match_id, event_type,
                player1, player2, status, winner,
                stake_amount, token, game_id, platform,
                timestamp, txn_hash
            ) VALUES (
                $1, $2, $3, $4,
                $5, $6, $7, $8,
                $9, $10, $11, $12,
                $13, $14
            ) ON CONFLICT (id) DO NOTHING",
            &[
                &event.id,
                &(event.ledger_sequence as i32),
                &(event.match_id as i64),
                &event.event_type,
                &event.player1,
                &event.player2,
                &event.status,
                &event.winner,
                &event.stake_amount,
                &event.token,
                &event.game_id,
                &event.platform,
                &event.timestamp,
                &event.txn_hash,
            ],
        )
        .await
        .map_err(|e| anyhow!("insert_event failed: {}", e))?;

        Ok(())
    }

    // ── Read path (via read_pool) ─────────────────────────────────────────

    pub async fn get_events_by_match(&self, match_id: u64) -> Result<Vec<IndexedEvent>> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error: {}", e))?;

        let rows = conn
            .query(
                "SELECT id, ledger_sequence, match_id, event_type, player1, player2,
                        status, winner, stake_amount, token, game_id, platform,
                        timestamp, txn_hash
                 FROM events
                 WHERE match_id = $1
                 ORDER BY ledger_sequence ASC",
                &[&(match_id as i64)],
            )
            .await
            .map_err(|e| anyhow!("get_events_by_match failed: {}", e))?;

        rows.iter().map(row_to_event).collect()
    }

    pub async fn get_events_by_match_paginated(
        &self,
        match_id: u64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<IndexedEvent>> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error: {}", e))?;

        let rows = conn
            .query(
                "SELECT id, ledger_sequence, match_id, event_type, player1, player2,
                        status, winner, stake_amount, token, game_id, platform,
                        timestamp, txn_hash
                 FROM events
                 WHERE match_id = $1
                 ORDER BY ledger_sequence ASC
                 LIMIT $2 OFFSET $3",
                &[&(match_id as i64), &limit, &offset],
            )
            .await
            .map_err(|e| anyhow!("get_events_by_match_paginated failed: {}", e))?;

        rows.iter().map(row_to_event).collect()
    }

    pub async fn query_events(&self, filters: &QueryFilters) -> Result<Vec<IndexedEvent>> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error: {}", e))?;

        // Build a parameterised query dynamically to avoid SQL injection while
        // still allowing flexible filtering.
        let mut query = String::from(
            "SELECT id, ledger_sequence, match_id, event_type, player1, player2,
                    status, winner, stake_amount, token, game_id, platform,
                    timestamp, txn_hash
             FROM events WHERE TRUE",
        );

        // Collect parameter values; positions are 1-indexed in Postgres.
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
        let mut idx = 1usize;

        if let Some(ref player) = filters.player_address {
            query.push_str(&format!(
                " AND (player1 = ${idx} OR player2 = ${})",
                idx + 1
            ));
            params.push(Box::new(player.clone()));
            params.push(Box::new(player.clone()));
            idx += 2;
        }

        if let Some(ref status) = filters.status {
            let s = match status {
                MatchStatus::Pending => "pending",
                MatchStatus::Active => "active",
                MatchStatus::Completed => "completed",
                MatchStatus::Cancelled => "cancelled",
                MatchStatus::Expired => "expired",
            };
            query.push_str(&format!(" AND status = ${idx}"));
            params.push(Box::new(s.to_string()));
            idx += 1;
        }

        if let Some(ref start) = filters.start_date {
            query.push_str(&format!(" AND timestamp >= ${idx}"));
            params.push(Box::new(*start));
            idx += 1;
        }

        if let Some(ref end) = filters.end_date {
            query.push_str(&format!(" AND timestamp <= ${idx}"));
            params.push(Box::new(*end));
            idx += 1;
        }

        query.push_str(" ORDER BY ledger_sequence DESC");

        if let Some(limit) = filters.limit {
            query.push_str(&format!(" LIMIT ${idx}"));
            params.push(Box::new(limit));
            idx += 1;
        }

        if let Some(offset) = filters.offset {
            query.push_str(&format!(" OFFSET ${idx}"));
            params.push(Box::new(offset));
            // idx += 1; // would be used for further params
        }

        let param_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let rows = conn
            .query(query.as_str(), param_refs.as_slice())
            .await
            .map_err(|e| anyhow!("query_events failed: {}", e))?;

        rows.iter().map(row_to_event).collect()
    }

    pub async fn get_matches_by_status(
        &self,
        status: Option<&MatchStatus>,
    ) -> Result<Vec<MatchInfo>> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error: {}", e))?;

        let match_ids: Vec<i64> = if let Some(s) = status {
            let status_str = match s {
                MatchStatus::Pending => "pending",
                MatchStatus::Active => "active",
                MatchStatus::Completed => "completed",
                MatchStatus::Cancelled => "cancelled",
                MatchStatus::Expired => "expired",
            };
            conn.query(
                "SELECT DISTINCT match_id FROM events WHERE status = $1",
                &[&status_str],
            )
            .await
            .map_err(|e| anyhow!("get_matches_by_status failed: {}", e))?
            .iter()
            .map(|r| r.get::<_, i64>(0))
            .collect()
        } else {
            conn.query("SELECT DISTINCT match_id FROM events", &[])
                .await
                .map_err(|e| anyhow!("get_matches_by_status (all) failed: {}", e))?
                .iter()
                .map(|r| r.get::<_, i64>(0))
                .collect()
        };

        let mut matches = Vec::new();
        for id in match_ids {
            if let Some(info) = self.build_match_info(id as u64).await? {
                matches.push(info);
            }
        }
        Ok(matches)
    }

    // ── Utility ───────────────────────────────────────────────────────────

    pub async fn ping(&self) -> Result<()> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error: {}", e))?;
        conn.query_one("SELECT 1", &[])
            .await
            .map_err(|e| anyhow!("ping failed: {}", e))?;
        Ok(())
    }

    pub async fn total_event_count(&self) -> Result<i64> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error: {}", e))?;
        let row = conn
            .query_one("SELECT COUNT(*)::BIGINT FROM events", &[])
            .await
            .map_err(|e| anyhow!("total_event_count failed: {}", e))?;
        Ok(row.get(0))
    }

    pub async fn get_latest_ledger(&self) -> Result<Option<u32>> {
        let conn = self.read_pool.get().await
            .map_err(|e| anyhow!("Read pool error (get_latest_ledger): {}", e))?;
        let row = conn
            .query_one("SELECT MAX(ledger_sequence) FROM events", &[])
            .await
            .map_err(|e| anyhow!("get_latest_ledger failed: {}", e))?;
        let val: Option<i32> = row.get(0);
        Ok(val.map(|v| v as u32))
    }

    pub async fn build_match_info(&self, match_id: u64) -> Result<Option<MatchInfo>> {
        let events = self.get_events_by_match(match_id).await?;

        if events.is_empty() {
            return Ok(None);
        }

        let created_event = events
            .iter()
            .find(|e| e.event_type == "match:created" || e.event_type.contains("created"))
            .ok_or_else(|| anyhow!("no created event found for match {}", match_id))?;
        let latest_event = events.last().unwrap();

        let status = if let Some(ref s) = latest_event.status {
            match s.as_str() {
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
            completed_ledger: Some(latest_event.ledger_sequence),
            events,
        }))
    }

    // ── Expose pools for leader election ─────────────────────────────────

    /// Return a reference to the write pool (used by the leader module).
    pub fn write_pool(&self) -> &Pool {
        &self.write_pool
    }
}

// ── Row mapping ───────────────────────────────────────────────────────────────

fn row_to_event(row: &tokio_postgres::Row) -> Result<IndexedEvent> {
    let ledger: i32 = row.get(1);
    let match_id: i64 = row.get(2);
    Ok(IndexedEvent {
        id: row.get(0),
        ledger_sequence: ledger as u32,
        match_id: match_id as u64,
        event_type: row.get(3),
        player1: row.get(4),
        player2: row.get(5),
        status: row.get(6),
        winner: row.get(7),
        stake_amount: row.get(8),
        token: row.get(9),
        game_id: row.get(10),
        platform: row.get(11),
        timestamp: row.get::<_, DateTime<Utc>>(12),
        txn_hash: row.get(13),
    })
}
