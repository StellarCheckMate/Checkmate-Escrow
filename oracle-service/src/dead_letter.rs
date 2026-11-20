//! Dead-letter store for pending entries that have exhausted all retries.
//!
//! ## Behavior
//!
//! When the pipeline poller determines that a [`PendingEntry`] has exhausted
//! its configured `max_retries`, it calls [`DeadLetterStore::push`] to:
//!
//! 1. Append the entry to `{queue_dir}/dead_letter.json` (atomically).
//! 2. Emit a `tracing::error!` log at level ERROR so that any log aggregator
//!    or alerting system (e.g. CloudWatch Alarms, Datadog monitors) can fire
//!    an alert on `ERROR` events from the oracle service.
//!
//! ## Manual replay
//!
//! The `oracle-replay` binary (see `src/bin/replay.rs`) reads the dead-letter
//! file and re-enqueues selected entries into the live pending queue.  After
//! re-enqueueing, the entry is removed from the dead-letter store.
//!
//! ```text
//! oracle-replay --match-id 42          # re-enqueue one specific match
//! oracle-replay --all                  # re-enqueue everything
//! oracle-replay --list                 # list dead-letter entries
//! ```

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::error;

use crate::metrics;
use crate::oracle::errors::OracleServiceError;
use crate::queue::{PendingEntry, PendingQueue};

/// A record in the dead-letter store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterEntry {
    /// The original pending entry that exhausted all retries.
    #[serde(flatten)]
    pub entry: PendingEntry,
    /// When this entry was moved to the dead-letter store.
    pub dead_lettered_at: DateTime<Utc>,
    /// Total number of attempts made before giving up.
    pub total_attempts: u32,
}

/// Append-friendly dead-letter file store.
///
/// ## Capacity management
///
/// When `max_entries > 0` and the store is at capacity, [`DeadLetterStore::push`]
/// evicts the oldest entry (by `dead_lettered_at`) before writing the new one,
/// preventing unbounded disk growth in production.
///
/// Set `max_entries = 0` to disable the limit (not recommended for production).
pub struct DeadLetterStore {
    file_path: PathBuf,
    /// Maximum number of entries to retain.  `0` means no limit.
    max_entries: usize,
}

impl DeadLetterStore {
    /// Open (or create) the dead-letter file in `dir`.
    ///
    /// `max_entries` caps how many entries the store retains.  Pass `0` for
    /// no limit.  See [`DeadLetterStore`] for eviction behaviour.
    pub fn new(dir: &str, max_entries: usize) -> Self {
        let mut path = PathBuf::from(dir);
        path.push("dead_letter.json");
        Self {
            file_path: path,
            max_entries,
        }
    }

    /// Load all dead-letter entries from disk.
    pub async fn load(&self) -> Result<Vec<DeadLetterEntry>, OracleServiceError> {
        if !self.file_path.exists() {
            return Ok(vec![]);
        }
        let raw = fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;
        serde_json::from_str(&raw)
            .map_err(|e| OracleServiceError::QueueIo(format!("corrupt dead-letter file: {}", e)))
    }

    /// Persist the full dead-letter list atomically.
    pub async fn save(&self, entries: &[DeadLetterEntry]) -> Result<(), OracleServiceError> {
        let parent = self
            .file_path
            .parent()
            .ok_or_else(|| OracleServiceError::QueueIo("no parent directory".into()))?;

        fs::create_dir_all(parent)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        let tmp_path = self.file_path.with_extension("tmp");
        let json = serde_json::to_string_pretty(entries)
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        fs::write(&tmp_path, &json)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        fs::rename(&tmp_path, &self.file_path)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        Ok(())
    }

    /// Move a failed entry into the dead-letter store and emit an alert log.
    ///
    /// This is the primary external API called by the pipeline poller.
    ///
    /// ## Capacity enforcement
    ///
    /// If `max_entries > 0` and the store is at or above capacity after
    /// appending the new entry, the oldest entry (by `dead_lettered_at`) is
    /// removed first so the store never exceeds `max_entries`.
    pub async fn push(&self, entry: PendingEntry) -> Result<(), OracleServiceError> {
        // ── Alerting ──────────────────────────────────────────────────────
        // Log at ERROR so that any log-based alerting fires.
        error!(
            match_id = entry.match_id,
            game_id = %entry.game_id,
            platform = %entry.platform,
            attempts = entry.attempts,
            last_error = ?entry.last_error,
            "DEAD_LETTER: match result verification exhausted all retries. \
             Manual replay required via `oracle-replay --match-id {}`.",
            entry.match_id,
        );

        let total_attempts = entry.attempts;
        let dl = DeadLetterEntry {
            entry,
            dead_lettered_at: Utc::now(),
            total_attempts,
        };

        let mut entries = self.load().await?;
        // Avoid duplicates (idempotent push)
        if entries
            .iter()
            .any(|e| e.entry.match_id == dl.entry.match_id)
        {
            return Ok(());
        }
        entries.push(dl);
        self.save(&entries).await?;
        metrics::set_dead_letter_count(entries.len());
        Ok(())
    }

    /// Remove an entry from the dead-letter store (called after successful replay).
    pub async fn remove(&self, match_id: u64) -> Result<(), OracleServiceError> {
        let mut entries = self.load().await?;
        entries.retain(|e| e.entry.match_id != match_id);
        self.save(&entries).await?;
        // Update gauge after removal.
        metrics::set_dead_letter_count(entries.len());
        Ok(())
    }

    /// Automatically re-enqueue dead-letter entries that have been quiet for at
    /// least `quiet_period`, giving them another chance after a transient
    /// failure may have cleared (#1976).
    ///
    /// At most `max_batch` entries are replayed per call, so a large
    /// dead-letter store cannot trigger a thundering herd of simultaneous
    /// retries.  Each replayed entry is re-enqueued into `queue` as a **fresh**
    /// pending entry (retry schedule reset to zero) and removed from the
    /// dead-letter store; if the underlying failure persists it will work
    /// through the normal retry schedule and be dead-lettered again.
    ///
    /// Entries are replayed oldest-first, so the longest-stuck matches get the
    /// first retry slot.  Returns the number of entries re-enqueued.
    pub async fn sweep_for_retry(
        &self,
        queue: &PendingQueue,
        quiet_period: chrono::Duration,
        max_batch: usize,
    ) -> Result<usize, OracleServiceError> {
        if max_batch == 0 {
            return Ok(0);
        }
        let cutoff = Utc::now() - quiet_period;

        let mut entries = self.load().await?;
        // Oldest-first so the longest-stuck entries are retried first.
        entries.sort_by_key(|e| e.dead_lettered_at);

        let mut replayed = 0usize;
        let mut remaining: Vec<DeadLetterEntry> = Vec::with_capacity(entries.len());
        for dl in entries {
            let past_quiet = dl.dead_lettered_at <= cutoff;
            if past_quiet && replayed < max_batch {
                // Re-enqueue as a fresh entry so the retry schedule starts over.
                match queue
                    .enqueue(dl.entry.match_id, dl.entry.game_id.clone(), dl.entry.platform)
                    .await
                {
                    Ok(_) => {
                        replayed += 1;
                        // `enqueue` returns Ok(false) when the match is already
                        // queued — that still means it is being handled, so it
                        // is safe to drop from the dead-letter store.
                        continue;
                    }
                    Err(e) => {
                        // On I/O failure, keep the entry in the dead-letter
                        // store rather than losing it; surface the error.
                        error!(
                            match_id = dl.entry.match_id,
                            "dead-letter sweep: failed to re-enqueue: {}", e
                        );
                        remaining.push(dl);
                        continue;
                    }
                }
            }
            // Not past the quiet period, or the batch is full: keep it.
            remaining.push(dl);
        }

        if replayed > 0 {
            self.save(&remaining).await?;
            metrics::set_dead_letter_count(remaining.len());
        }
        Ok(replayed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Platform;
    use tempfile::TempDir;

    fn make_store(dir: &TempDir) -> DeadLetterStore {
        DeadLetterStore::new(dir.path().to_str().unwrap(), 0) // 0 = no limit
    }

    fn make_store_with_limit(dir: &TempDir, max: usize) -> DeadLetterStore {
        DeadLetterStore::new(dir.path().to_str().unwrap(), max)
    }

    fn make_entry(match_id: u64) -> PendingEntry {
        let mut e = PendingEntry::new(match_id, "abcd1234".into(), Platform::Lichess);
        e.attempts = 5;
        e.last_error = Some("test error".into());
        e
    }

    #[tokio::test]
    async fn push_and_load() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        store.push(make_entry(1)).await.unwrap();

        let entries = store.load().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry.match_id, 1);
        assert_eq!(entries[0].total_attempts, 5);
    }

    #[tokio::test]
    async fn push_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        store.push(make_entry(1)).await.unwrap();
        store.push(make_entry(1)).await.unwrap(); // duplicate

        assert_eq!(store.load().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn remove_entry() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        store.push(make_entry(1)).await.unwrap();
        store.push(make_entry(2)).await.unwrap();
        store.remove(1).await.unwrap();

        let entries = store.load().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].entry.match_id, 2);
    }

    /// When the store is at capacity, pushing a new entry evicts the oldest.
    #[tokio::test]
    async fn evict_oldest_at_capacity() {
        let dir = TempDir::new().unwrap();
        let store = make_store_with_limit(&dir, 3);

        // Push 3 entries — fills to capacity.
        store.push(make_entry(1)).await.unwrap();
        store.push(make_entry(2)).await.unwrap();
        store.push(make_entry(3)).await.unwrap();

        let before = store.load().await.unwrap();
        assert_eq!(before.len(), 3, "should hold exactly 3 entries at capacity");

        // Push a 4th entry — entry 1 (oldest) should be evicted.
        store.push(make_entry(4)).await.unwrap();

        let after = store.load().await.unwrap();
        assert_eq!(
            after.len(),
            3,
            "store should still hold at most max_entries=3 after eviction"
        );

        let ids: Vec<u64> = after.iter().map(|e| e.entry.match_id).collect();
        assert!(
            !ids.contains(&1),
            "oldest entry (match_id=1) should have been evicted; remaining: {:?}",
            ids
        );
        assert!(
            ids.contains(&4),
            "new entry (match_id=4) should be present; remaining: {:?}",
            ids
        );
    }

    /// A limit of 1 means only the most recent entry is kept.
    #[tokio::test]
    async fn evict_all_but_newest_when_limit_is_one() {
        let dir = TempDir::new().unwrap();
        let store = make_store_with_limit(&dir, 1);

        store.push(make_entry(10)).await.unwrap();
        store.push(make_entry(20)).await.unwrap();

        let entries = store.load().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].entry.match_id, 20,
            "only the newest entry should remain"
        );
    }

    /// With max_entries=0 (unlimited) the store grows without bound.
    #[tokio::test]
    async fn no_eviction_when_limit_is_zero() {
        let dir = TempDir::new().unwrap();
        let store = make_store_with_limit(&dir, 0);

        for id in 1..=10 {
            store.push(make_entry(id)).await.unwrap();
        }

        assert_eq!(
            store.load().await.unwrap().len(),
            10,
            "all 10 entries should be retained when limit is 0"
        );
    }

    // ── Automatic retry sweep (issue #1976) ──────────────────────────────────

    /// Back-date every dead-letter entry by `age` so each appears past the
    /// quiet period.
    async fn backdate_all(store: &DeadLetterStore, age: chrono::Duration) {
        let mut entries = store.load().await.unwrap();
        let past = Utc::now() - age;
        for e in entries.iter_mut() {
            e.dead_lettered_at = past;
        }
        store.save(&entries).await.unwrap();
    }

    /// A dead-letter entry past the quiet period is automatically retried:
    /// re-enqueued into the pending queue as a fresh entry and removed from
    /// the dead-letter store (#1976).
    #[tokio::test]
    async fn sweep_retries_entry_past_quiet_period() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);
        let queue = PendingQueue::new(dir.path().to_str().unwrap());

        store.push(make_entry(1)).await.unwrap();
        backdate_all(&store, chrono::Duration::seconds(3600)).await;

        let replayed = store
            .sweep_for_retry(&queue, chrono::Duration::seconds(1800), 10)
            .await
            .unwrap();
        assert_eq!(replayed, 1);

        // Removed from the dead-letter store...
        assert!(store.load().await.unwrap().is_empty());
        // ...and re-enqueued as a fresh entry (retry schedule reset to zero).
        let pending = queue.load().await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].match_id, 1);
        assert_eq!(
            pending[0].attempts, 0,
            "replayed entry must start with a fresh retry schedule"
        );
    }

    /// An entry within the quiet period is left alone — it is too soon to
    /// retry (#1976).
    #[tokio::test]
    async fn sweep_leaves_entry_within_quiet_period() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);
        let queue = PendingQueue::new(dir.path().to_str().unwrap());

        // dead_lettered_at = now (just dead-lettered).
        store.push(make_entry(2)).await.unwrap();

        let replayed = store
            .sweep_for_retry(&queue, chrono::Duration::seconds(1800), 10)
            .await
            .unwrap();
        assert_eq!(replayed, 0, "entry within quiet period must not be retried");
        assert_eq!(
            store.load().await.unwrap().len(),
            1,
            "entry should remain in the dead-letter store"
        );
        assert!(queue.load().await.unwrap().is_empty());
    }

    /// The sweep is bounded: at most `max_batch` entries are replayed per call,
    /// so a large dead-letter store cannot trigger a thundering herd (#1976).
    #[tokio::test]
    async fn sweep_is_bounded_by_max_batch() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);
        let queue = PendingQueue::new(dir.path().to_str().unwrap());

        for id in 1..=3 {
            store.push(make_entry(id)).await.unwrap();
        }
        backdate_all(&store, chrono::Duration::seconds(3600)).await;

        let replayed = store
            .sweep_for_retry(&queue, chrono::Duration::seconds(1800), 2)
            .await
            .unwrap();
        assert_eq!(replayed, 2);
        assert_eq!(
            store.load().await.unwrap().len(),
            1,
            "one entry should remain because the batch is bounded"
        );
        assert_eq!(queue.load().await.unwrap().len(), 2);
    }

    /// `max_batch == 0` disables the sweep entirely (#1976).
    #[tokio::test]
    async fn sweep_disabled_when_max_batch_is_zero() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);
        let queue = PendingQueue::new(dir.path().to_str().unwrap());

        store.push(make_entry(3)).await.unwrap();
        backdate_all(&store, chrono::Duration::seconds(3600)).await;

        let replayed = store
            .sweep_for_retry(&queue, chrono::Duration::seconds(1800), 0)
            .await
            .unwrap();
        assert_eq!(replayed, 0);
        assert_eq!(store.load().await.unwrap().len(), 1);
        assert!(queue.load().await.unwrap().is_empty());
    }
}
