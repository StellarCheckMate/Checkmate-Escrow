//! Integration tests for the `oracle-replay` binary (src/bin/replay.rs).
//!
//! These tests exercise the CLI flags introduced in #1359:
//! - `--dry-run`: should print a preview without writing to disk.
//! - `--max-retries <N>`: should skip entries whose `total_attempts >= N`.
//!
//! The tests call the binary as a subprocess so that arg-parsing, flag
//! interactions, stdout/stderr output, and exit codes are all covered
//! end-to-end rather than through unit stubs.

use oracle_service::config::Platform;
use oracle_service::queue::PendingEntry;
use oracle_service::{dead_letter::DeadLetterStore, queue::PendingQueue};
use tempfile::TempDir;

/// Build a `DeadLetterEntry` with the given match_id and attempt count, then
/// push it into `store`. We set `attempts` directly so that `total_attempts`
/// in the dead-letter entry reflects a desired value.
async fn push_entry(store: &DeadLetterStore, match_id: u64, attempts: u32) {
    let mut entry = PendingEntry::new(match_id, format!("game{}", match_id), Platform::Lichess);
    entry.attempts = attempts;
    entry.last_error = Some("simulated error".into());
    store.push(entry).await.unwrap();
}

/// Run the `oracle-replay` binary with the given extra args and the provided
/// `ORACLE_QUEUE_DIR`. Returns `(stdout, stderr, exit_status_success)`.
fn run_replay(queue_dir: &str, extra_args: &[&str]) -> (String, String, bool) {
    // Locate the binary via cargo's OUT_DIR / target path heuristic.
    // `cargo test` compiles binaries in the same profile directory.
    let mut cmd = std::process::Command::new(env!("CARGO_BIN_EXE_oracle-replay"));
    cmd.env("ORACLE_QUEUE_DIR", queue_dir);
    for arg in extra_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("failed to run oracle-replay binary");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.success())
}

// ── --dry-run ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn dry_run_all_prints_preview_and_does_not_write() {
    let dir = TempDir::new().unwrap();
    let queue_dir = dir.path().to_str().unwrap();
    let store = DeadLetterStore::new(queue_dir);
    let queue = PendingQueue::new(queue_dir);

    push_entry(&store, 10, 2).await;
    push_entry(&store, 20, 3).await;

    let (stdout, _stderr, success) = run_replay(queue_dir, &["--all", "--dry-run"]);

    assert!(success, "dry-run must exit 0");
    assert!(
        stdout.contains("DRY-RUN"),
        "dry-run output must contain DRY-RUN: {}",
        stdout
    );
    assert!(
        stdout.contains("match_id=10"),
        "dry-run output must mention match_id=10: {}",
        stdout
    );
    assert!(
        stdout.contains("match_id=20"),
        "dry-run output must mention match_id=20: {}",
        stdout
    );

    // Dead-letter store must be unchanged — entries must still be there.
    let dl_entries = store.load().await.unwrap();
    assert_eq!(
        dl_entries.len(),
        2,
        "dry-run must not remove entries from the dead-letter store"
    );

    // Pending queue must be empty — nothing was actually enqueued.
    let pending = queue.load().await.unwrap();
    assert!(
        pending.is_empty(),
        "dry-run must not write to the pending queue"
    );
}

#[tokio::test]
async fn dry_run_single_match_prints_preview_and_does_not_write() {
    let dir = TempDir::new().unwrap();
    let queue_dir = dir.path().to_str().unwrap();
    let store = DeadLetterStore::new(queue_dir);
    let queue = PendingQueue::new(queue_dir);

    push_entry(&store, 42, 1).await;

    let (stdout, _stderr, success) = run_replay(queue_dir, &["--match-id", "42", "--dry-run"]);

    assert!(success, "dry-run single-match must exit 0");
    assert!(
        stdout.contains("DRY-RUN"),
        "dry-run output must contain DRY-RUN: {}",
        stdout
    );
    assert!(
        stdout.contains("match_id=42"),
        "output must mention match_id=42: {}",
        stdout
    );

    // Store and queue must be unchanged.
    assert_eq!(store.load().await.unwrap().len(), 1);
    assert!(queue.load().await.unwrap().is_empty());
}

// ── --max-retries ─────────────────────────────────────────────────────────

#[tokio::test]
async fn max_retries_skips_entries_at_or_above_limit() {
    let dir = TempDir::new().unwrap();
    let queue_dir = dir.path().to_str().unwrap();
    let store = DeadLetterStore::new(queue_dir);
    let queue = PendingQueue::new(queue_dir);

    // Entry with 3 attempts (below the limit=5 → should be replayed).
    push_entry(&store, 1, 3).await;
    // Entry with 5 attempts (at the limit=5 → should be skipped).
    push_entry(&store, 2, 5).await;
    // Entry with 7 attempts (above the limit=5 → should be skipped).
    push_entry(&store, 3, 7).await;

    let (stdout, _stderr, success) = run_replay(queue_dir, &["--all", "--max-retries", "5"]);

    assert!(success, "max-retries run must exit 0 when no failures");

    // match_id=1 (3 attempts < 5) must be enqueued.
    let pending = queue.load().await.unwrap();
    assert!(
        pending.iter().any(|e| e.match_id == 1),
        "match_id=1 (3 attempts) must be re-enqueued: {:?}",
        pending
    );
    // match_id=2 and 3 must not be enqueued.
    assert!(
        !pending.iter().any(|e| e.match_id == 2),
        "match_id=2 (5 attempts >= 5) must be skipped"
    );
    assert!(
        !pending.iter().any(|e| e.match_id == 3),
        "match_id=3 (7 attempts >= 5) must be skipped"
    );

    // The SKIP output must be present for match_id=2 and 3.
    assert!(
        stdout.contains("SKIP"),
        "output must contain SKIP for skipped entries: {}",
        stdout
    );
}

#[tokio::test]
async fn dry_run_with_max_retries_skips_and_previews() {
    let dir = TempDir::new().unwrap();
    let queue_dir = dir.path().to_str().unwrap();
    let store = DeadLetterStore::new(queue_dir);
    let queue = PendingQueue::new(queue_dir);

    push_entry(&store, 5, 1).await;
    push_entry(&store, 6, 10).await;

    let (stdout, _stderr, success) =
        run_replay(queue_dir, &["--all", "--dry-run", "--max-retries", "5"]);

    assert!(success, "dry-run + max-retries must exit 0");

    // match_id=5 (1 attempt < 5) → DRY-RUN preview.
    assert!(
        stdout.contains("DRY-RUN") && stdout.contains("match_id=5"),
        "match_id=5 must appear as DRY-RUN: {}",
        stdout
    );
    // match_id=6 (10 attempts >= 5) → SKIP.
    assert!(
        stdout.contains("SKIP") && stdout.contains("match_id=6"),
        "match_id=6 must appear as SKIP: {}",
        stdout
    );

    // Nothing written — store unchanged, queue empty.
    assert_eq!(store.load().await.unwrap().len(), 2);
    assert!(queue.load().await.unwrap().is_empty());
}

// ── per-entry success/failure output ─────────────────────────────────────

#[tokio::test]
async fn all_replay_reports_success_per_entry() {
    let dir = TempDir::new().unwrap();
    let queue_dir = dir.path().to_str().unwrap();
    let store = DeadLetterStore::new(queue_dir);

    push_entry(&store, 100, 2).await;
    push_entry(&store, 200, 1).await;

    let (stdout, _stderr, success) = run_replay(queue_dir, &["--all"]);

    assert!(success, "all replay must exit 0 when entries succeed");
    assert!(
        stdout.contains("SUCCESS match_id=100"),
        "must report SUCCESS for match_id=100: {}",
        stdout
    );
    assert!(
        stdout.contains("SUCCESS match_id=200"),
        "must report SUCCESS for match_id=200: {}",
        stdout
    );
    assert!(
        stdout.contains("Summary:"),
        "must print a summary line: {}",
        stdout
    );
}

#[tokio::test]
async fn empty_dead_letter_store_exits_cleanly() {
    let dir = TempDir::new().unwrap();
    let queue_dir = dir.path().to_str().unwrap();

    let (stdout, _stderr, success) = run_replay(queue_dir, &["--all"]);

    assert!(success, "empty replay must exit 0");
    assert!(
        stdout.contains("Nothing to replay"),
        "must print 'Nothing to replay': {}",
        stdout
    );
}
