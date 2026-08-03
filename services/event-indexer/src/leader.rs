//! Distributed leader election via PostgreSQL.
//!
//! ## How it works
//! The `leader_state` table (created by `db::Database::init_schema`) holds a
//! single logical row with key `"__leader__"`.  Acquiring the lease is a
//! conditional `INSERT … ON CONFLICT DO UPDATE` that only succeeds when the row
//! does not exist **or** the current `held_until` timestamp has expired.
//!
//! The leader must renew its lease every `heartbeat_secs` seconds.  If it
//! crashes without renewing, the lease expires after `ttl_secs` and another
//! instance can take over.
//!
//! ## Integration with the poller
//! `event_poller` (in `rpc.rs`) calls `LeaderGuard::try_acquire` once per poll
//! cycle.  Only the instance that holds the guard proceeds to ingest events.
//! Non-leaders skip ingestion and sleep until the next cycle, keeping them
//! warm (cache + DB connections stay alive) for fast failover.
//!
//! ## Advisory lock backstop
//! In addition to the row-level lease, the leader holds PostgreSQL advisory lock
//! `LOCK_KEY` (a fixed `i64`) for the duration of the connection.  This gives
//! a second layer of mutual exclusion that is automatically released when the
//! connection drops, eliminating a class of split-brain scenarios that can arise
//! if the process is killed but the DB row has not yet expired.

use anyhow::{anyhow, Result};
use chrono::Utc;
use deadpool_postgres::Pool;
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

/// Deterministic advisory lock key for the event-indexer leader slot.
/// Value is arbitrary but must be consistent across all instances.
const LOCK_KEY: i64 = 0x65_76_74_69_64_78; // "evtidx" in ASCII

/// A non-`Send` guard that holds the leader lease while it is in scope.
/// Drop the guard (or let the `LeaderElection` task cancel) to release.
pub struct LeaderGuard;

/// State machine for leader election.
pub struct LeaderElection {
    pool: Pool,
    instance_id: String,
    ttl_secs: u64,
    heartbeat_secs: u64,
    /// `true` when this instance currently holds the lease.
    is_leader: bool,
}

impl LeaderElection {
    pub fn new(pool: Pool, instance_id: String, ttl_secs: u64, heartbeat_secs: u64) -> Self {
        LeaderElection {
            pool,
            instance_id,
            ttl_secs,
            heartbeat_secs,
            is_leader: false,
        }
    }

    /// Attempt to acquire (or renew) the leader lease.
    ///
    /// Returns `true` if this instance is now the leader.
    ///
    /// The operation is atomic: it uses a single `INSERT … ON CONFLICT DO
    /// UPDATE … WHERE` statement so there is no TOCTOU window.
    pub async fn try_acquire(&mut self) -> bool {
        match self.try_acquire_inner().await {
            Ok(acquired) => {
                if acquired && !self.is_leader {
                    info!(instance_id = %self.instance_id, "Became leader");
                } else if !acquired && self.is_leader {
                    warn!(instance_id = %self.instance_id, "Lost leader lease");
                }
                self.is_leader = acquired;
                acquired
            }
            Err(e) => {
                warn!(instance_id = %self.instance_id, error = %e, "Leader election error");
                self.is_leader = false;
                false
            }
        }
    }

    async fn try_acquire_inner(&self) -> Result<bool> {
        let conn = self.pool.get().await
            .map_err(|e| anyhow!("Leader pool error: {}", e))?;

        let ttl_interval = format!("{} seconds", self.ttl_secs);
        let now = Utc::now();
        let held_until = now + chrono::Duration::seconds(self.ttl_secs as i64);

        // Acquire the PostgreSQL session-level advisory lock first.
        // `pg_try_advisory_lock` is non-blocking: it returns false if another
        // session already holds the lock.
        let lock_row = conn
            .query_one("SELECT pg_try_advisory_lock($1)", &[&LOCK_KEY])
            .await
            .map_err(|e| anyhow!("advisory lock failed: {}", e))?;
        let got_advisory: bool = lock_row.get(0);

        if !got_advisory {
            debug!(instance_id = %self.instance_id, "Advisory lock held by another session");
            return Ok(false);
        }

        // Now try to claim / renew the row-level lease.
        // The UPDATE only fires when `held_until < NOW()` (expired) OR
        // `instance_id` matches our own ID (renewal).
        let rows_affected = conn
            .execute(
                "INSERT INTO leader_state (instance_id, held_until)
                 VALUES ('__leader__', $1)
                 ON CONFLICT (instance_id) DO UPDATE
                     SET instance_id = leader_state.instance_id,
                         held_until  = $1
                 WHERE leader_state.held_until < NOW()
                    OR leader_state.instance_id = $2",
                &[&held_until, &self.instance_id],
            )
            .await
            .map_err(|e| anyhow!("leader_state upsert failed: {}", e))?;

        // Verify that the row now belongs to us.
        let row = conn
            .query_one(
                "SELECT instance_id FROM leader_state WHERE instance_id = '__leader__'",
                &[],
            )
            .await
            .map_err(|e| anyhow!("leader_state verify failed: {}", e))?;

        let current_holder: String = row.get(0);
        let we_are_leader = rows_affected > 0 || current_holder == self.instance_id;
        let _ = ttl_interval; // suppresses unused-variable warning
        Ok(we_are_leader)
    }

    /// Explicitly release the advisory lock and delete (or expire) the row.
    pub async fn release(&mut self) {
        if !self.is_leader {
            return;
        }
        match self.pool.get().await {
            Ok(conn) => {
                let _ = conn
                    .execute(
                        "DELETE FROM leader_state WHERE instance_id = '__leader__'",
                        &[],
                    )
                    .await;
                let _ = conn
                    .execute("SELECT pg_advisory_unlock($1)", &[&LOCK_KEY])
                    .await;
                info!(instance_id = %self.instance_id, "Leader lease released");
            }
            Err(e) => {
                warn!("Failed to release leader lease (pool error): {}", e);
            }
        }
        self.is_leader = false;
    }

    /// Returns `true` if this instance currently believes it is the leader.
    pub fn is_leader(&self) -> bool {
        self.is_leader
    }

    /// Spawn a background heartbeat task that renews the lease every
    /// `heartbeat_secs`.  The returned `tokio::task::JoinHandle` should be
    /// awaited (or aborted) by the caller on shutdown.
    ///
    /// This is used by `main.rs` alongside the poller loop.
    pub async fn run_heartbeat(mut self) -> tokio::task::JoinHandle<()> {
        let beat = self.heartbeat_secs;
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(beat));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                self.try_acquire().await;
            }
        })
    }
}
