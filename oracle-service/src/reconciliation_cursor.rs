//! Persistent reconciliation cursor.
//!
//! The reconciliation loop pages through the escrow contract's `Active`
//! matches using a `u32` offset.  If the oracle service restarts
//! mid-reconciliation the offset is lost, causing the loop to restart from
//! page 0.  Matches that became `Active` between the crash and the rescan can
//! be delayed by up to one full reconciliation cycle.
//!
//! This module solves that problem by writing the current offset to disk after
//! each page and loading it on startup, allowing the loop to resume from where
//! it left off.
//!
//! ## File format
//!
//! A single JSON file `{queue_dir}/reconciliation_cursor.json` containing a
//! flat object:
//!
//! ```json
//! { "offset": 150 }
//! ```
//!
//! The file is written atomically (write to `.tmp`, then rename) so a crash
//! during the write never corrupts the cursor.
//!
//! ## Lifecycle
//!
//! | Event                          | Action                                 |
//! |--------------------------------|----------------------------------------|
//! | Service starts                 | `load()` — resume from saved offset     |
//! | No cursor file exists          | `load()` returns `0`                   |
//! | Page N processed successfully  | `save(offset + page_size)` — advance    |
//! | Reconciliation cycle complete  | `clear()` — reset to 0 for next cycle  |

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, warn};

use crate::oracle::errors::OracleServiceError;

/// On-disk representation of the cursor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CursorFile {
    offset: u32,
}

/// File-backed reconciliation cursor.
pub struct ReconciliationCursor {
    file_path: PathBuf,
}

impl ReconciliationCursor {
    /// Create a cursor backed by `{dir}/reconciliation_cursor.json`.
    pub fn new(dir: &str) -> Self {
        let mut path = PathBuf::from(dir);
        path.push("reconciliation_cursor.json");
        Self { file_path: path }
    }

    /// Load the persisted offset, or return `0` if no cursor file exists.
    pub async fn load(&self) -> u32 {
        if !self.file_path.exists() {
            return 0;
        }
        match fs::read_to_string(&self.file_path).await {
            Ok(raw) => match serde_json::from_str::<CursorFile>(&raw) {
                Ok(c) => {
                    debug!(
                        offset = c.offset,
                        "reconciliation cursor: resuming from persisted offset"
                    );
                    c.offset
                }
                Err(e) => {
                    warn!(
                        "reconciliation cursor: corrupt cursor file, resetting to 0: {}",
                        e
                    );
                    0
                }
            },
            Err(e) => {
                warn!(
                    "reconciliation cursor: could not read cursor file, resetting to 0: {}",
                    e
                );
                0
            }
        }
    }

    /// Persist `offset` so that a restart resumes from this position.
    pub async fn save(&self, offset: u32) -> Result<(), OracleServiceError> {
        let parent = self
            .file_path
            .parent()
            .ok_or_else(|| OracleServiceError::QueueIo("no parent directory".into()))?;

        fs::create_dir_all(parent)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        let tmp_path = self.file_path.with_extension("tmp");
        let json = serde_json::to_string(&CursorFile { offset })
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        fs::write(&tmp_path, &json)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        fs::rename(&tmp_path, &self.file_path)
            .await
            .map_err(|e| OracleServiceError::QueueIo(e.to_string()))?;

        debug!(offset, "reconciliation cursor: saved");
        Ok(())
    }

    /// Remove the cursor file so the next cycle starts from offset 0.
    ///
    /// Called after a reconciliation cycle completes successfully.
    pub async fn clear(&self) {
        if self.file_path.exists() {
            if let Err(e) = fs::remove_file(&self.file_path).await {
                warn!(
                    "reconciliation cursor: could not remove cursor file: {}",
                    e
                );
            } else {
                debug!("reconciliation cursor: cleared");
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_cursor(dir: &TempDir) -> ReconciliationCursor {
        ReconciliationCursor::new(dir.path().to_str().unwrap())
    }

    #[tokio::test]
    async fn load_returns_zero_when_no_file() {
        let dir = TempDir::new().unwrap();
        let cursor = make_cursor(&dir);
        assert_eq!(cursor.load().await, 0);
    }

    #[tokio::test]
    async fn save_and_reload() {
        let dir = TempDir::new().unwrap();
        let cursor = make_cursor(&dir);

        cursor.save(150).await.unwrap();
        assert_eq!(cursor.load().await, 150);
    }

    #[tokio::test]
    async fn save_overwrites_previous() {
        let dir = TempDir::new().unwrap();
        let cursor = make_cursor(&dir);

        cursor.save(100).await.unwrap();
        cursor.save(200).await.unwrap();
        assert_eq!(cursor.load().await, 200);
    }

    #[tokio::test]
    async fn clear_resets_to_zero() {
        let dir = TempDir::new().unwrap();
        let cursor = make_cursor(&dir);

        cursor.save(99).await.unwrap();
        cursor.clear().await;
        assert_eq!(cursor.load().await, 0, "after clear, load should return 0");
    }

    #[tokio::test]
    async fn clear_is_safe_when_no_file() {
        let dir = TempDir::new().unwrap();
        let cursor = make_cursor(&dir);
        // Should not panic or error when file does not exist.
        cursor.clear().await;
    }

    #[tokio::test]
    async fn corrupt_file_is_handled_gracefully() {
        let dir = TempDir::new().unwrap();
        let cursor = make_cursor(&dir);

        // Write garbage to the cursor file.
        let path = dir.path().join("reconciliation_cursor.json");
        fs::write(&path, b"not valid json").await.unwrap();

        // load() should fall back to 0 instead of panicking.
        assert_eq!(cursor.load().await, 0);
    }
}
