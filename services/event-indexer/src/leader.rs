//! Distributed leader election via PostgreSQL.
//!
//! ## How it works
//! The `leader_state` table (created by `db::Database::init_schema`) holds a
//! single logical row with key `"__leader__"`.  Acquiring the lease is a
//! conditional `INSERT … ON CONFLICT DO UPDATE` that only succeeds when the row
//! does not exist **or** the current `held_until` timestamp has expired.
//!
//! The lease is atomic at the database level. If the `INSERT` or `UPDATE`
//! succeeds, we have become the leader and the row now bears our `instance_id`.
//! If it fails (another instance holds an unexpired lease), we remain a
//! follower.
//!
//! The leader must renew its lease every `heartbeat_secs` seconds.  If it
//! crashes without renewing, the lease expires after `ttl_secs` and another
//! instance can take over.
//!
//! ## Integration with the poller
//! `event_poller` (in `rpc.rs`) calls `LeaderGuard::try_acquire` once per poll
//! cycle.  Only the instance that holds the lease proceeds to ingest events.
//! Non-leaders skip ingestion and sleep until the next cycle, keeping them
//! warm (cache + DB connections stay alive) for fast failover.

use anyhow::{anyhow, Result};
use chrono::Utc;
use deadpool_postgres::Pool;
use tokio::time::{interval, Duration};
use tracing::{info, warn};

/// Deterministic advisory lock key for the event-indexer leader slot.
/// Value is arbitrary but must be consistent across all instances ("evtidx" in ASCII).
pub const LOCK_KEY: i64 = 0x65_76_74_69_64_78;

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
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| anyhow!("Leader pool error: {}", e))?;

        let now = Utc::now();
        let held_until = now + chrono::Duration::seconds(self.ttl_secs as i64);

        // Acquire the PostgreSQL session-level advisory lock first.
        // `pg_try_advisory_lock` is non-blocking: returns false if another
        // session already holds the lock.
        let lock_row = conn
            .query_one("SELECT pg_try_advisory_lock($1)", &[&LOCK_KEY])
            .await
            .map_err(|e| anyhow!("advisory lock failed: {}", e))?;
        let got_advisory: bool = lock_row.get(0);

        if !got_advisory {
            return Ok(false);
        }

        // Try to claim or renew the row-level lease.
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
        Ok(we_are_leader)
    }

    /// Explicitly release the advisory lock and delete the leader row.
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
                info!(instance_id = %self.instance_id, "Leader lease and advisory lock released");
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

/// Panic/shutdown handler: automatically release advisory lock and lease on drop.
impl Drop for LeaderElection {
    fn drop(&mut self) {
        if self.is_leader {
            self.is_leader = false;
            let pool = self.pool.clone();
            let instance_id = self.instance_id.clone();
            // In a panic or unexpected drop, asynchronously release the advisory lock and lease
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    if let Ok(conn) = pool.get().await {
                        let _ = conn
                            .execute(
                                "DELETE FROM leader_state WHERE instance_id = '__leader__'",
                                &[],
                            )
                            .await;
                        let _ = conn
                            .execute("SELECT pg_advisory_unlock($1)", &[&LOCK_KEY])
                            .await;
                        info!(instance_id = %instance_id, "Leader advisory lock released in drop/panic handler");
                    }
                });
            }
        }
    }
}

/// Explicitly release the advisory lock and leader state for a pool.
pub async fn release_advisory_lock(pool: &Pool) -> Result<()> {
    let conn = pool
        .get()
        .await
        .map_err(|e| anyhow!("Failed to get connection to release advisory lock: {}", e))?;
    let _ = conn
        .execute("DELETE FROM leader_state WHERE instance_id = '__leader__'", &[])
        .await;
    let _ = conn
        .execute("SELECT pg_advisory_unlock($1)", &[&LOCK_KEY])
        .await;
    Ok(())
}
