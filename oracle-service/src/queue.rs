//! Durable pending-verification queue.
//!
//! ## Design
//!
//! The queue is a JSON file (`{queue_dir}/pending.json`) that survives process
//! restarts.  Each entry tracks:
//! - the `match_id` and `game_id` to verify,
//! - which chess platform to query,
//! - retry state (attempt count, next eligible time with exponential backoff),
//! - when the entry was originally enqueued.
//!
//! All mutations are atomic: we write to a `.tmp` file and then `rename`
//! it into place so a crash mid-write never corrupts the queue.
//!
//! ## Retry schedule
//!
//! ```text
//! delay = base_delay * 2^(attempt - 1)   (capped at 1 hour)
//! ```
//!
//! For the default `base_delay = 10s`:
//! | attempt | delay   |
//! |---------|---------|
//! | 1       | 10 s    |
//! | 2       | 20 s    |
//! | 3       | 40 s    |
//! | 4       | 80 s    |
//! | 5       | 160 s   |
//!
//! After `max_retries` consecutive failures the entry is moved to the
//! dead-letter store by the pipeline poller.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::config::Platform;
use crate::oracle::errors::OracleServiceError;

/// A single entry in the pending-verification queue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingEntry {
    /// On-chain match identifier.
    pub match_id: u64,
    /// Platform-specific game identifier.
    pub game_id: String,
    /// Which chess platform this game is on.
    pub platform: Platform,
    /// How many verification attempts have been made (0 = not yet tried).
    pub attempts: u32,
    /// Earliest time at which the next attempt is allowed.
    pub next_attempt_at: DateTime<Utc>,
    /// When this entry was first added to the queue.
    pub enqueued_at: DateTime<Utc>,
    /// Human-readable reason for the last failure (for alerting / dead-letter).
    pub last_error: Option<String>,
}

impl PendingEntry {
    /// Create a new entry that is immediately eligible for its first attempt.
    pub fn new(match_id: u64, game_id: String, platform: Platform) -> Self {
        let now = Utc::now();
        Self {
            match_id,
            game_id,
            platform,
            attempts: 0,
            next_attempt_at: now,
            enqueued_at: now,
            last_error: None,
        }
    }

    /// Record a failed attempt and advance the retry schedule.
    ///
    /// Returns `true` if the entry has exhausted all retries and should be
    /// moved to the dead-letter store.
    pub fn record_failure(&mut self, error: String, base_delay_secs: u64, max_retries: u32) -> bool {
        self.attempts += 1;
        self.last_error = Some(error);

        if self.attempts >= max_retries {
            return true; // exhausted
        }

        // Exponential backoff: base * 2^(attempt-1), capped at 1 hour.
        let exp = (self.attempts - 1) as u32;
        let multiplier = 1u64.checked_shl(exp).unwrap_or(u64::MAX);
        let delay_secs = base_delay_secs
            .saturating_mul(multiplier)
            .min(3600); // cap at 1 hour

        self.next_attempt_at = Utc::now() + chrono::Duration::seconds(delay_secs as i64);
        false
    }

    /// Returns true when this entry is eligible for a new attempt.
    pub fn is_due(&self) -> bool {
        Utc::now() >= self.next_attempt_at
    }
}

/// File-backed durable pending-verification queue.
pub struct PendingQueue {
    file_path: PathBuf,
}

impl PendingQueue {
    /// Open (or create) the queue file in `dir`.
    pub fn new(dir: &str) -> Self {
        let mut path = PathBuf::from(dir);
        path.push("pending.json");
        Self { file_path: path }
    }

    /// Load all entries from disk, or return an empty list if the file does
    /// not exist yet.
    pub async fn load(&self) -> Result<Vec<PendingEntry>, OracleServiceError> {
        if !self.file_path.exists() {
            return Ok(vec![]);
        }
        let raw = fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;
        serde_json::from_str(&raw)
            .map_err(|e| OracleServiceError::QueueIo(format!("corrupt queue file: {}", e)))
    }

    /// Persist the full entry list atomically.
    pub async fn save(&self, entries: &[PendingEntry]) -> Result<(), OracleServiceError> {
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

    /// Add a new entry for `match_id` / `game_id` if it is not already queued.
    /// Returns `true` if the entry was actually added.
    pub async fn enqueue(
        &self,
        match_id: u64,
        game_id: String,
        platform: Platform,
    ) -> Result<bool, OracleServiceError> {
        let mut entries = self.load().await?;
        if entries.iter().any(|e| e.match_id == match_id) {
            return Ok(false);
        }
        entries.push(PendingEntry::new(match_id, game_id, platform));
        self.save(&entries).await?;
        Ok(true)
    }

    /// Return all entries that are due for a retry attempt right now.
    pub async fn due_entries(&self) -> Result<Vec<PendingEntry>, OracleServiceError> {
        let entries = self.load().await?;
        Ok(entries.into_iter().filter(|e| e.is_due()).collect())
    }

    /// Persist an updated entry (identified by `match_id`).
    pub async fn update_entry(&self, updated: PendingEntry) -> Result<(), OracleServiceError> {
        let mut entries = self.load().await?;
        if let Some(pos) = entries.iter().position(|e| e.match_id == updated.match_id) {
            entries[pos] = updated;
            self.save(&entries).await?;
        }
        Ok(())
    }

    /// Remove an entry by `match_id`.
    pub async fn remove(&self, match_id: u64) -> Result<(), OracleServiceError> {
        let mut entries = self.load().await?;
        entries.retain(|e| e.match_id != match_id);
        self.save(&entries).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_queue(dir: &TempDir) -> PendingQueue {
        PendingQueue::new(dir.path().to_str().unwrap())
    }

    #[tokio::test]
    async fn enqueue_and_load() {
        let dir = TempDir::new().unwrap();
        let q = make_queue(&dir);

        let added = q.enqueue(1, "abcd1234".into(), Platform::Lichess).await.unwrap();
        assert!(added);

        let entries = q.load().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].match_id, 1);
        assert_eq!(entries[0].game_id, "abcd1234");
    }

    #[tokio::test]
    async fn dedup_enqueue() {
        let dir = TempDir::new().unwrap();
        let q = make_queue(&dir);

        q.enqueue(1, "abcd1234".into(), Platform::Lichess).await.unwrap();
        let added = q.enqueue(1, "abcd1234".into(), Platform::Lichess).await.unwrap();
        assert!(!added, "should not add duplicate");
        assert_eq!(q.load().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn record_failure_advances_retry() {
        let mut entry = PendingEntry::new(1, "abcd1234".into(), Platform::Lichess);
        let exhausted = entry.record_failure("transient".into(), 10, 5);
        assert!(!exhausted);
        assert_eq!(entry.attempts, 1);
        assert!(entry.next_attempt_at > Utc::now());
    }

    #[tokio::test]
    async fn record_failure_exhausted() {
        let mut entry = PendingEntry::new(1, "abcd1234".into(), Platform::Lichess);
        entry.attempts = 4; // one below max
        let exhausted = entry.record_failure("final error".into(), 10, 5);
        assert!(exhausted, "should be exhausted at max_retries=5");
    }

    #[tokio::test]
    async fn remove_entry() {
        let dir = TempDir::new().unwrap();
        let q = make_queue(&dir);

        q.enqueue(1, "abcd1234".into(), Platform::Lichess).await.unwrap();
        q.enqueue(2, "efgh5678".into(), Platform::ChessDotCom).await.unwrap();
        q.remove(1).await.unwrap();

        let entries = q.load().await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].match_id, 2);
    }

    #[tokio::test]
    async fn save_is_atomic() {
        // If save completes without error, the file must be valid JSON.
        let dir = TempDir::new().unwrap();
        let q = make_queue(&dir);

        let entries = vec![PendingEntry::new(42, "test1234".into(), Platform::Lichess)];
        q.save(&entries).await.unwrap();

        let loaded = q.load().await.unwrap();
        assert_eq!(loaded[0].match_id, 42);
    }
}
