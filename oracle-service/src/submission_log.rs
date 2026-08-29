//! Persistent submission log for oracle result exports.
//!
//! ## Purpose
//!
//! When the oracle poller successfully submits a `submit_result` transaction to
//! Soroban, it appends a [`SubmissionRecord`] to `{queue_dir}/submissions.json`.
//! The `oracle-export-csv` binary reads this file and writes a CSV audit trail
//! suitable for off-chain record-keeping and compliance review.
//!
//! ## Design
//!
//! The log is a JSON array file (`submissions.json`) that lives alongside the
//! queue and dead-letter files.  Every successful `submit_result` call atomically
//! appends one record.  The file grows unboundedly; for production deployments
//! consider rotating it periodically.
//!
//! ## CSV columns
//!
//! | Column           | Description                                         |
//! |------------------|-----------------------------------------------------|
//! | `match_id`       | On-chain match identifier                           |
//! | `game_id`        | Platform-specific game identifier                   |
//! | `platform`       | `lichess` or `chess.com`                            |
//! | `winner`         | `player1`, `player2`, or `draw`                     |
//! | `submitted_at`   | RFC 3339 UTC timestamp of the submission            |
//! | `soroban_tx_hash`| Transaction hash returned by the Soroban RPC        |

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::config::Platform;
use crate::oracle::errors::OracleServiceError;
use crate::oracle::Winner;

/// A single successfully-submitted oracle result record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionRecord {
    /// On-chain match identifier.
    pub match_id: u64,
    /// Platform-specific game identifier.
    pub game_id: String,
    /// Which chess platform this result came from (serialized as a string).
    pub platform: String,
    /// The result that was submitted (serialized as a string).
    pub winner: String,
    /// When the Soroban transaction was confirmed.
    pub submitted_at: DateTime<Utc>,
    /// Transaction hash returned by `sendTransaction` / `getTransaction`.
    pub soroban_tx_hash: String,
}

impl SubmissionRecord {
    /// Build a record from the canonical oracle types.
    pub fn new(
        match_id: u64,
        game_id: String,
        platform: Platform,
        winner: &Winner,
        submitted_at: DateTime<Utc>,
        soroban_tx_hash: String,
    ) -> Self {
        let platform_str = match platform {
            Platform::Lichess => "lichess".to_string(),
            Platform::ChessDotCom => "chess.com".to_string(),
        };
        let winner_str = match winner {
            Winner::Player1 => "player1".to_string(),
            Winner::Player2 => "player2".to_string(),
            Winner::Draw => "draw".to_string(),
        };
        Self {
            match_id,
            game_id,
            platform: platform_str,
            winner: winner_str,
            submitted_at,
            soroban_tx_hash,
        }
    }
}

/// Append-only submission log backed by a JSON file.
pub struct SubmissionLog {
    file_path: std::path::PathBuf,
}

impl SubmissionLog {
    /// Open (or create) the submission log at `{dir}/submissions.json`.
    pub fn new(dir: &str) -> Self {
        let mut path = PathBuf::from(dir);
        path.push("submissions.json");
        Self { file_path: path }
    }

    /// Load all submission records from disk.
    ///
    /// Returns an empty `Vec` if the file does not yet exist.
    pub async fn load(&self) -> Result<Vec<SubmissionRecord>, OracleServiceError> {
        if !self.file_path.exists() {
            return Ok(vec![]);
        }
        let raw = fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        if raw.trim().is_empty() {
            return Ok(vec![]);
        }

        serde_json::from_str::<Vec<SubmissionRecord>>(&raw).map_err(|e| {
            OracleServiceError::QueueIo(format!("failed to parse submissions.json: {e}"))
        })
    }

    /// Append a new record to the log.
    ///
    /// The write is atomic: the existing entries are read, the new one is
    /// appended in memory, then the whole list is written to a `.tmp` file and
    /// renamed into place.
    pub async fn append(&self, record: SubmissionRecord) -> Result<(), OracleServiceError> {
        let mut entries = self.load().await.unwrap_or_default();
        entries.push(record);

        let json = serde_json::to_string_pretty(&entries).map_err(|e| {
            OracleServiceError::QueueIo(format!("failed to serialize submission log: {e}"))
        })?;

        // Write to a tmp file first so a crash mid-write never corrupts the log.
        let tmp_path = self.file_path.with_extension("tmp");
        fs::write(&tmp_path, &json)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;
        fs::rename(&tmp_path, &self.file_path)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        Ok(())
    }
}
