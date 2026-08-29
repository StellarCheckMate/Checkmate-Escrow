//! Oracle pipeline poller.
//!
//! The poller runs as two background tasks that share the same durable
//! queue: reconciliation (discovery) and the pipeline tick (verification).
//!
//! ## Reconciliation — how matches enter the queue
//!
//! [`Poller::reconcile`] periodically pages through the escrow contract's
//! `Active` matches via `get_active_matches_paginated` and, for each one that
//! the oracle contract's `has_result` reports as not yet resolved, calls
//! [`Poller::enqueue`]. `PendingQueue::enqueue` dedups on `match_id`, so
//! running this on every cycle is safe: a match that's already queued (mid
//! retry backoff or otherwise) is left untouched.  This is what makes a
//! match that becomes `Active` — or a queue that's lost its persisted state
//! entirely — get (re)discovered without needing an external caller to push
//! it in.
//!
//! ## Pipeline tick — verification
//!
//! On every tick, [`Poller::tick`]:
//!
//! 1. Reads all active matches from the queue that are due for a retry.
//! 2. For each due entry, calls the appropriate chess platform client to
//!    fetch the game result.
//! 3. On success: signs and submits `submit_result` to Soroban, then removes
//!    the entry from the queue.
//! 4. On a *transient* failure (network, rate-limit, game not finished yet):
//!    records the failure and advances the retry schedule.
//! 5. On exhaustion: moves the entry to the dead-letter store.

use std::sync::Arc;

use chrono::Utc;
use ed25519_dalek::SigningKey;
use tracing::{error, info, info_span, warn, Instrument};
use zeroize::Zeroizing;

use crate::config::{OracleConfig, Platform};
use crate::dead_letter::DeadLetterStore;
use crate::metrics;
use crate::oracle::errors::{ChessComError, LichessError, OracleServiceError};
use crate::oracle::{ChessComClient, LichessClient, Winner};
use crate::queue::{PendingEntry, PendingQueue};
use crate::reconciliation_cursor::ReconciliationCursor;
use crate::result_cache::ResultCache;
use crate::soroban_client::SorobanClient;
use crate::submission_log::{SubmissionLog, SubmissionRecord};

/// The pipeline poller.  Clone-cheap — the inner state is reference-counted.
#[derive(Clone)]
pub struct Poller {
    inner: Arc<PollerInner>,
}

struct PollerInner {
    queue: PendingQueue,
    dead_letter: DeadLetterStore,
    cursor: ReconciliationCursor,
    soroban: SorobanClient,
    chess_com: ChessComClient,
    lichess: LichessClient,
    signing_key: Zeroizing<[u8; 32]>,
    max_retries: u32,
    retry_base_delay_secs: u64,
    /// How often (seconds) the Chess.com-specific poll loop wakes.
    chessdotcom_poll_interval_secs: u64,
    /// Strkey of the oracle contract — used by reconciliation to check
    /// `has_result` before enqueuing a discovered match.
    contract_oracle: String,
    /// Raw ed25519 public key derived from `signing_key`, used as the source
    /// account for read-only simulation calls (`get_active_matches_paginated`,
    /// `has_result`). Never used to sign anything.
    pubkey: [u8; 32],
    /// In-memory LRU cache for oracle results, keyed by (platform, game_id).
    /// Avoids redundant API calls on retry within the TTL window.
    result_cache: ResultCache,
    /// Persistent audit log of all successfully-submitted oracle results.
    /// Appended to by `submit_and_complete` so `oracle-export-csv` can read it.
    submission_log: SubmissionLog,
}

impl Poller {
    /// Construct a poller from the oracle configuration.
    pub fn new(cfg: &OracleConfig) -> Result<Self, OracleServiceError> {
        let soroban = SorobanClient::new(
            cfg.rpc_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
        )?;

        let chess_com =
            ChessComClient::new().map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let lichess =
            LichessClient::new().map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let pubkey = pubkey_from_signing_key(&cfg.oracle_signing_key);

        Ok(Self {
            inner: Arc::new(PollerInner {
                queue: PendingQueue::new(&cfg.queue_dir),
                dead_letter: DeadLetterStore::new(&cfg.queue_dir, cfg.dead_letter_max_entries),
                cursor: ReconciliationCursor::new(&cfg.queue_dir),
                soroban,
                chess_com,
                lichess,
                signing_key: Zeroizing::new(*cfg.oracle_signing_key),
                max_retries: cfg.max_retries,
                retry_base_delay_secs: cfg.retry_base_delay_secs,
                chessdotcom_poll_interval_secs: cfg.chessdotcom_poll_interval_secs,
                contract_oracle: cfg.contract_oracle.clone(),
                pubkey,
                result_cache: ResultCache::with_defaults(),
                submission_log: SubmissionLog::new(&cfg.queue_dir),
            }),
        })
    }

    /// Construct a poller with a custom Lichess API base URL.
    ///
    /// Used in tests to point the Lichess client at a mock server.
    pub fn new_with_lichess_base(
        cfg: &OracleConfig,
        lichess_base: String,
    ) -> Result<Self, OracleServiceError> {
        let soroban = SorobanClient::new(
            cfg.rpc_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
        )?;

        let chess_com =
            ChessComClient::new().map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let lichess = LichessClient::new_with_base_and_timeout(
            lichess_base,
            std::time::Duration::from_secs(30),
        )
        .map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let pubkey = pubkey_from_signing_key(&cfg.oracle_signing_key);

        Ok(Self {
            inner: Arc::new(PollerInner {
                queue: PendingQueue::new(&cfg.queue_dir),
                dead_letter: DeadLetterStore::new(&cfg.queue_dir, cfg.dead_letter_max_entries),
                cursor: ReconciliationCursor::new(&cfg.queue_dir),
                soroban,
                chess_com,
                lichess,
                signing_key: Zeroizing::new(*cfg.oracle_signing_key),
                max_retries: cfg.max_retries,
                retry_base_delay_secs: cfg.retry_base_delay_secs,
                chessdotcom_poll_interval_secs: cfg.chessdotcom_poll_interval_secs,
                contract_oracle: cfg.contract_oracle.clone(),
                pubkey,
                result_cache: ResultCache::with_defaults(),
                submission_log: SubmissionLog::new(&cfg.queue_dir),
            }),
        })
    }

    /// Construct a poller with a custom Chess.com API base URL.
    ///
    /// Used in tests to point the Chess.com client at a mock server.
    pub fn new_with_chess_com_base(
        cfg: &OracleConfig,
        chess_com_base: String,
    ) -> Result<Self, OracleServiceError> {
        let soroban = SorobanClient::new(
            cfg.rpc_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
        )?;

        let chess_com = ChessComClient::new_with_base_and_timeout(
            chess_com_base,
            std::time::Duration::from_secs(30),
        )
        .map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let lichess =
            LichessClient::new().map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let pubkey = pubkey_from_signing_key(&cfg.oracle_signing_key);

        Ok(Self {
            inner: Arc::new(PollerInner {
                queue: PendingQueue::new(&cfg.queue_dir),
                dead_letter: DeadLetterStore::new(&cfg.queue_dir, cfg.dead_letter_max_entries),
                cursor: ReconciliationCursor::new(&cfg.queue_dir),
                soroban,
                chess_com,
                lichess,
                signing_key: Zeroizing::new(*cfg.oracle_signing_key),
                max_retries: cfg.max_retries,
                retry_base_delay_secs: cfg.retry_base_delay_secs,
                chessdotcom_poll_interval_secs: cfg.chessdotcom_poll_interval_secs,
                contract_oracle: cfg.contract_oracle.clone(),
                pubkey,
                result_cache: ResultCache::with_defaults(),
                submission_log: SubmissionLog::new(&cfg.queue_dir),
            }),
        })
    }

    /// Return the configured Chess.com poll interval in seconds.
    ///
    /// Exposed so that the main loop and tests can read it without reaching
    /// into the `Arc<PollerInner>`.
    pub fn chessdotcom_poll_interval_secs(&self) -> u64 {
        self.inner.chessdotcom_poll_interval_secs
    }

    /// Read the currently persisted reconciliation cursor offset from disk.
    ///
    /// Returns `0` if no cursor file exists (i.e. no cycle is in progress or
    /// the last cycle completed cleanly).  Exposed primarily for integration
    /// tests that simulate a mid-reconciliation restart.
    pub async fn reconciliation_cursor_offset(&self) -> u32 {
        self.inner.cursor.load().await
    }

    /// Run a single polling tick: process all due queue entries.
    ///
    /// Each entry is processed inside a tracing span that carries `match_id`
    /// as a structured field, so all log lines emitted during verification of
    /// that entry (including inside `lichess_client` and `soroban_client`) are
    /// automatically correlated by `match_id` in structured log aggregators.
    ///
    /// This is `pub` so that tests can call it directly without spawning a
    /// background task.
    pub async fn tick(&self) -> Result<(), OracleServiceError> {
        let due = self.inner.queue.due_entries().await?;
        // Update queue-depth gauge on every tick so Prometheus reflects live state.
        let total_depth = self.inner.queue.load().await?.len();
        metrics::set_queue_depth(total_depth);
        if due.is_empty() {
            return Ok(());
        }
        info!(count = due.len(), "poller tick: processing due entries");

        for entry in due {
            let match_id = entry.match_id;
            let span = info_span!(
                "oracle.verify",
                match_id = match_id,
                game_id = %entry.game_id,
                platform = %entry.platform,
            );
            self.process_entry(entry).instrument(span).await;
        }
        Ok(())
    }

    /// Run the polling loop forever, sleeping `interval_secs` between ticks.
    pub async fn run_loop(self, interval_secs: u64) {
        let interval = tokio::time::Duration::from_secs(interval_secs);
        loop {
            if let Err(e) = self.tick().await {
                error!("poller tick error: {}", e);
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Run a Chess.com-specific polling loop that wakes every
    /// `chessdotcom_poll_interval_secs` seconds.
    ///
    /// This allows Chess.com games (which finish more slowly, especially
    /// classical time controls) to be polled at an independent rate from the
    /// general Lichess pipeline tick.
    pub async fn run_chess_com_loop(self) {
        let interval =
            tokio::time::Duration::from_secs(self.inner.chessdotcom_poll_interval_secs);
        loop {
            // Only process due Chess.com entries.
            match self.tick_chess_com().await {
                Ok(n) if n > 0 => {
                    info!(count = n, "chess.com poller tick: processed due entries");
                }
                Ok(_) => {}
                Err(e) => {
                    error!("chess.com poller tick error: {}", e);
                }
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Process only Chess.com entries that are due for a retry.
    ///
    /// Returns the number of entries processed.  `pub` so tests can call it
    /// directly without spawning a background loop.
    pub async fn tick_chess_com(&self) -> Result<usize, OracleServiceError> {
        let due = self
            .inner
            .queue
            .due_entries()
            .await?
            .into_iter()
            .filter(|e| e.platform == Platform::ChessDotCom)
            .collect::<Vec<_>>();
        let count = due.len();
        for entry in due {
            self.process_entry(entry).await;
        }
        Ok(count)
    }

    /// Enqueue a new match for verification if not already queued.
    pub async fn enqueue(
        &self,
        match_id: u64,
        game_id: String,
        platform: Platform,
    ) -> Result<bool, OracleServiceError> {
        let added = self
            .inner
            .queue
            .enqueue(match_id, game_id, platform)
            .await?;
        if added {
            let depth = self.inner.queue.load().await?.len();
            metrics::set_queue_depth(depth);
        }
        Ok(added)
    }

    /// Run one reconciliation pass: page through the escrow contract's
    /// `Active` matches and enqueue any that the oracle contract does not
    /// yet have a result for.
    ///
    /// Safe to call on a fresh queue (nothing enqueued yet), after the queue
    /// file has been lost entirely, or against a queue that already has
    /// entries mid-retry — `PendingQueue::enqueue` dedups on `match_id` and
    /// never touches an existing entry's retry state.
    ///
    /// ## Cursor persistence
    ///
    /// The current page offset is saved to disk after every page so that a
    /// service restart mid-reconciliation resumes from where it left off rather
    /// than restarting from offset 0.  The cursor is cleared when the cycle
    /// completes successfully.
    pub async fn reconcile(&self) -> Result<(), OracleServiceError> {
        const PAGE_SIZE: u32 = 50;

        // Resume from a previously persisted offset (0 if starting fresh).
        let mut offset = self.inner.cursor.load().await;
        if offset > 0 {
            info!(
                offset,
                "reconciliation resuming from persisted cursor offset"
            );
        }

        let mut discovered = 0u32;

        loop {
            let page = self
                .inner
                .soroban
                .get_active_matches_paginated(offset, PAGE_SIZE, &self.inner.pubkey)
                .await?;
            let page_len = page.len() as u32;
            if page.is_empty() {
                break;
            }

            for m in page {
                let has_result = self
                    .inner
                    .soroban
                    .has_result(&self.inner.contract_oracle, m.match_id, &self.inner.pubkey)
                    .await?;
                if has_result {
                    continue;
                }

                match self
                    .inner
                    .queue
                    .enqueue(m.match_id, m.game_id.clone(), m.platform)
                    .await
                {
                    Ok(true) => {
                        discovered += 1;
                        info!(
                            match_id = m.match_id,
                            game_id = %m.game_id,
                            platform = %m.platform,
                            "reconciliation discovered new match",
                        );
                    }
                    Ok(false) => {
                        // Already queued (fresh entry or mid-backoff) — leave it alone.
                    }
                    Err(e) => {
                        error!(
                            match_id = m.match_id,
                            "reconciliation failed to enqueue discovered match: {}", e
                        );
                    }
                }
            }

            offset += page_len;

            // Persist the cursor after each successfully processed page so a
            // restart can resume from here instead of offset 0.
            if let Err(e) = self.inner.cursor.save(offset).await {
                // Non-fatal: log and continue — correctness is maintained because
                // PendingQueue::enqueue is idempotent (already-queued matches are
                // silently skipped on re-discovery).
                error!(
                    offset,
                    "failed to persist reconciliation cursor (non-fatal): {}", e
                );
            }

            if page_len < PAGE_SIZE {
                break;
            }
        }

        // Cycle complete — remove the cursor file so the next cycle starts
        // from offset 0 (fresh scan of all active matches).
        self.inner.cursor.clear().await;

        if discovered > 0 {
            info!(count = discovered, "reconciliation cycle complete");
        }
        Ok(())
    }

    /// Run the reconciliation loop forever, sleeping `interval_secs` between
    /// passes.
    pub async fn run_reconciliation_loop(self, interval_secs: u64) {
        let interval = tokio::time::Duration::from_secs(interval_secs);
        loop {
            if let Err(e) = self.reconcile().await {
                error!("reconciliation cycle error: {}", e);
            }
            tokio::time::sleep(interval).await;
        }
    }

    // ── private ───────────────────────────────────────────────────────────────

    async fn process_entry(&self, mut entry: PendingEntry) {
        let match_id = entry.match_id;
        let game_id = entry.game_id.clone();

        info!(
            match_id,
            game_id = %game_id,
            attempt = entry.attempts + 1,
            platform = %entry.platform,
            "attempting result verification",
        );

        // ── Pre-validation: check game_id format before calling the API ──
        // If the game_id stored on-chain is structurally invalid (e.g. due to
        // a contract-level validation bug or corrupted data), every API call
        // will fail unconditionally.  We short-circuit here to avoid exhausting
        // all retries on an ID that can never succeed, and send the entry
        // directly to the dead-letter store.
        if let Err(reason) = validate_game_id(entry.platform, &game_id) {
            warn!(
                match_id,
                game_id = %game_id,
                platform = %entry.platform,
                %reason,
                "game_id failed pre-validation; dead-lettering without API call",
            );
            entry.last_error = Some(reason);
            entry.attempts = self.inner.max_retries;
            self.exhaust_entry(entry).await;
            return;
        }

        // ── Cache check ──────────────────────────────────────────────────
        // Skip the platform API call if there is a non-expired cached result.
        if let Some(cached_winner) = self.inner.result_cache.get(entry.platform, &game_id).await {
            info!(
                match_id,
                game_id = %game_id,
                "cache hit — skipping API call, using cached result",
            );
            self.submit_and_complete(entry, cached_winner).await;
            return;
        }

        // ── Fetch result from chess platform ─────────────────────────────
        let winner_result = match entry.platform {
            Platform::Lichess => self
                .inner
                .lichess
                .fetch_result(&game_id)
                .await
                .map(|r| r.winner)
                .map_err(classify_lichess_error),
            Platform::ChessDotCom => self
                .inner
                .chess_com
                .fetch_result(&game_id)
                .await
                .map(|r| r.winner)
                .map_err(classify_chess_com_error),
        };

        match winner_result {
            Ok(winner) => {
                info!(
                    match_id,
                    ?winner,
                    "result fetched; caching and submitting to Soroban"
                );
                // Populate the cache so subsequent retries (if submission fails)
                // don't need to call the platform API again within the TTL window.
                self.inner
                    .result_cache
                    .insert(entry.platform, &game_id, winner.clone())
                    .await;
                self.submit_and_complete(entry, winner).await;
            }
            Err(FetchError::Permanent(reason)) => {
                warn!(match_id, %reason, "permanent fetch error; dead-lettering immediately");
                entry.last_error = Some(reason);
                // Count as exhausted right away — permanent errors should not
                // consume retries pointlessly.
                entry.attempts = self.inner.max_retries;
                self.exhaust_entry(entry).await;
            }
            Err(FetchError::Transient(reason)) => {
                warn!(match_id, %reason, "transient fetch error; scheduling retry");
                self.handle_transient(entry, reason).await;
            }
        }
    }

    async fn submit_and_complete(&self, entry: PendingEntry, winner: Winner) {
        let match_id = entry.match_id;
        match self
            .inner
            .soroban
            .submit_result(match_id, &winner, &self.inner.signing_key)
            .await
        {
            Ok(tx_hash) => {
                info!(match_id, %tx_hash, "submit_result confirmed on-chain; removing from queue");
                if let Err(e) = self.inner.queue.remove(match_id).await {
                    error!(
                        match_id,
                        "failed to remove completed entry from queue: {}", e
                    );
                } else {
                    // Update queue-depth gauge after successful removal.
                    if let Ok(entries) = self.inner.queue.load().await {
                        metrics::set_queue_depth(entries.len());
                    }
                }

                // Append to the persistent audit log so oracle-export-csv can
                // produce a full CSV trail (#1362).
                let record = SubmissionRecord::new(
                    match_id,
                    entry.game_id,
                    entry.platform,
                    &winner,
                    Utc::now(),
                    tx_hash,
                );
                if let Err(e) = self.inner.submission_log.append(record).await {
                    error!(match_id, "failed to append to submission log: {}", e);
                }
            }
            Err(e) => {
                warn!(match_id, "Soroban submission failed (transient?): {}", e);
                self.handle_transient(entry, e.to_string()).await;
            }
        }
    }

    async fn handle_transient(&self, mut entry: PendingEntry, reason: String) {
        let match_id = entry.match_id;
        let exhausted = entry.record_failure(
            reason,
            self.inner.retry_base_delay_secs,
            self.inner.max_retries,
        );

        if exhausted {
            self.exhaust_entry(entry).await;
        } else {
            if let Err(e) = self.inner.queue.update_entry(entry).await {
                error!(match_id, "failed to update queue entry: {}", e);
            }
        }
    }

    async fn exhaust_entry(&self, entry: PendingEntry) {
        let match_id = entry.match_id;
        if let Err(e) = self.inner.dead_letter.push(entry).await {
            error!(match_id, "failed to move entry to dead-letter store: {}", e);
        }
        if let Err(e) = self.inner.queue.remove(match_id).await {
            error!(
                match_id,
                "failed to remove exhausted entry from queue: {}", e
            );
        }
    }
}

/// Derive the raw ed25519 public key bytes for the oracle's signing key, used
/// only as the source account for read-only simulation calls.
fn pubkey_from_signing_key(seed: &[u8; 32]) -> [u8; 32] {
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

// ── Error classification ──────────────────────────────────────────────────────

enum FetchError {
    /// Permanent errors (invalid ID, game deleted) — do not retry.
    Permanent(String),
    /// Transient errors (network, game not finished, rate-limit) — retry.
    Transient(String),
}

fn classify_lichess_error(e: LichessError) -> FetchError {
    match e {
        LichessError::InvalidGameId => FetchError::Permanent(e.to_string()),
        LichessError::GameNotFound => FetchError::Permanent(e.to_string()),
        LichessError::GameNotFinished
        | LichessError::Http(_)
        | LichessError::Timeout
        | LichessError::HttpStatus { .. }
        | LichessError::RateLimited { .. }
        | LichessError::ConcurrencyLimitReached
        | LichessError::InvalidResponse => FetchError::Transient(e.to_string()),
    }
}

fn classify_chess_com_error(e: ChessComError) -> FetchError {
    match e {
        ChessComError::InvalidGameId => FetchError::Permanent(e.to_string()),
        ChessComError::GameNotFound => FetchError::Permanent(e.to_string()),
        ChessComError::GameNotFinished
        | ChessComError::Http(_)
        | ChessComError::Timeout
        | ChessComError::HttpStatus { .. }
        | ChessComError::RateLimited { .. }
        | ChessComError::ConcurrencyLimitReached
        | ChessComError::InvalidResponse => FetchError::Transient(e.to_string()),
    }
}

// ── Pre-flight game_id validation ─────────────────────────────────────────────

/// Validate a `game_id` against the expected format for `platform` **before**
/// making any API call.
///
/// This guard sits at the pipeline level so that a structurally invalid ID
/// stored on-chain (e.g. due to a contract-level validation bug or data
/// corruption) is caught immediately, without exhausting all retry attempts
/// on HTTP calls that will always fail.
///
/// Returns `Ok(())` when the ID is valid, or `Err(reason)` with a human-
/// readable description when it is not.
///
/// ## Validation rules (mirrors `validate_game_id_format` in the escrow contract)
///
/// - **Lichess**: exactly 8 or 12 ASCII alphanumeric characters.
/// - **Chess.com**: 7–12 ASCII digit characters.
/// - Both platforms: non-empty, ≤ 64 bytes (shared contract constant).
pub(crate) fn validate_game_id(platform: Platform, game_id: &str) -> Result<(), String> {
    let len = game_id.len();

    if len == 0 {
        return Err("game_id is empty".to_string());
    }
    if len > 64 {
        return Err(format!(
            "game_id is too long ({} bytes, max 64)",
            len
        ));
    }

    match platform {
        Platform::Lichess => {
            if (len != 8 && len != 12) || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(format!(
                    "invalid Lichess game_id '{}': must be 8 or 12 ASCII alphanumeric characters",
                    game_id
                ));
            }
        }
        Platform::ChessDotCom => {
            if !(7..=12).contains(&len) || !game_id.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!(
                    "invalid Chess.com game_id '{}': must be 7–12 ASCII digit characters",
                    game_id
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod validate_game_id_tests {
    use super::*;

    // ── Lichess ───────────────────────────────────────────────────────────────

    #[test]
    fn lichess_valid_8_char() {
        assert!(validate_game_id(Platform::Lichess, "abcd1234").is_ok());
    }

    #[test]
    fn lichess_valid_12_char() {
        assert!(validate_game_id(Platform::Lichess, "abcdef123456").is_ok());
    }

    #[test]
    fn lichess_empty_rejected() {
        assert!(validate_game_id(Platform::Lichess, "").is_err());
    }

    #[test]
    fn lichess_wrong_length_rejected() {
        for len in [1, 7, 9, 10, 11, 13, 63, 64] {
            let id: String = "a".repeat(len);
            assert!(
                validate_game_id(Platform::Lichess, &id).is_err(),
                "expected rejection for Lichess id of length {len}"
            );
        }
    }

    #[test]
    fn lichess_non_alphanumeric_rejected() {
        assert!(validate_game_id(Platform::Lichess, "abcd12!@").is_err());
        assert!(validate_game_id(Platform::Lichess, "abcd-123").is_err());
        assert!(validate_game_id(Platform::Lichess, "abcd 123").is_err());
    }

    #[test]
    fn lichess_oversized_rejected() {
        let id: String = "a".repeat(65);
        assert!(validate_game_id(Platform::Lichess, &id).is_err());
    }

    // ── Chess.com ─────────────────────────────────────────────────────────────

    #[test]
    fn chess_com_valid_7_digits() {
        assert!(validate_game_id(Platform::ChessDotCom, "1234567").is_ok());
    }

    #[test]
    fn chess_com_valid_12_digits() {
        assert!(validate_game_id(Platform::ChessDotCom, "123456789012").is_ok());
    }

    #[test]
    fn chess_com_empty_rejected() {
        assert!(validate_game_id(Platform::ChessDotCom, "").is_err());
    }

    #[test]
    fn chess_com_too_short_rejected() {
        for len in [1, 2, 3, 4, 5, 6] {
            let id: String = "1".repeat(len);
            assert!(
                validate_game_id(Platform::ChessDotCom, &id).is_err(),
                "expected rejection for Chess.com id of length {len}"
            );
        }
    }

    #[test]
    fn chess_com_too_long_rejected() {
        let id: String = "1".repeat(13);
        assert!(validate_game_id(Platform::ChessDotCom, &id).is_err());
    }

    #[test]
    fn chess_com_non_digits_rejected() {
        assert!(validate_game_id(Platform::ChessDotCom, "1234abc8").is_err());
        assert!(validate_game_id(Platform::ChessDotCom, "1234567!").is_err());
    }

    #[test]
    fn chess_com_oversized_rejected() {
        let id: String = "1".repeat(65);
        assert!(validate_game_id(Platform::ChessDotCom, &id).is_err());
    }
}
