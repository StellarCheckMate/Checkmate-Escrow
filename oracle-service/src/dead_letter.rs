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

use crate::queue::PendingEntry;
use crate::oracle::errors::OracleServiceError;

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
pub struct DeadLetterStore {
    file_path: PathBuf,
}

impl DeadLetterStore {
    /// Open (or create) the dead-letter file in `dir`.
    pub fn new(dir: &str) -> Self {
        let mut path = PathBuf::from(dir);
        path.push("dead_letter.json");
        Self { file_path: path }
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
        if !entries.iter().any(|e| e.entry.match_id == dl.entry.match_id) {
            entries.push(dl);
            self.save(&entries).await?;
        }
        Ok(())
    }

    /// Remove an entry from the dead-letter store (called after successful replay).
    pub async fn remove(&self, match_id: u64) -> Result<(), OracleServiceError> {
        let mut entries = self.load().await?;
        entries.retain(|e| e.entry.match_id != match_id);
        self.save(&entries).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Platform;
    use tempfile::TempDir;

    fn make_store(dir: &TempDir) -> DeadLetterStore {
        DeadLetterStore::new(dir.path().to_str().unwrap())
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
}
