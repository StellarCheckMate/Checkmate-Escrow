//! Oracle pipeline poller.
//!
//! The poller runs as a background task.  On every tick it:
//!
//! 1. Reads all active matches from the queue that are due for a retry.
//! 2. For each due entry, calls the appropriate chess platform client to
//!    fetch the game result.
//! 3. On success: signs and submits `submit_result` to Soroban, then removes
//!    the entry from the queue.
//! 4. On a *transient* failure (network, rate-limit, game not finished yet):
//!    records the failure and advances the retry schedule.
//! 5. On exhaustion: moves the entry to the dead-letter store.
//!
//! The poller does **not** discover new matches — that is the responsibility
//! of the match-source adapter passed in via `MatchSource`.  In the current
//! implementation, matches are enqueued externally (e.g. by reading the
//! on-chain active-match list via the event-indexer or a manual CLI call).

use std::sync::Arc;

use tracing::{error, info, warn};
use zeroize::Zeroizing;

use crate::config::{OracleConfig, Platform};
use crate::dead_letter::DeadLetterStore;
use crate::oracle::errors::{ChessComError, LichessError, OracleServiceError};
use crate::oracle::{
    ChessComClient, LichessClient, Winner,
};
use crate::queue::{PendingEntry, PendingQueue};
use crate::soroban_client::SorobanClient;

/// The pipeline poller.  Clone-cheap — the inner state is reference-counted.
#[derive(Clone)]
pub struct Poller {
    inner: Arc<PollerInner>,
}

struct PollerInner {
    queue: PendingQueue,
    dead_letter: DeadLetterStore,
    soroban: SorobanClient,
    chess_com: ChessComClient,
    lichess: LichessClient,
    signing_key: Zeroizing<[u8; 32]>,
    max_retries: u32,
    retry_base_delay_secs: u64,
}

impl Poller {
    /// Construct a poller from the oracle configuration.
    pub fn new(cfg: &OracleConfig) -> Result<Self, OracleServiceError> {
        let soroban = SorobanClient::new(
            cfg.rpc_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
        )?;

        let chess_com = ChessComClient::new()
            .map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let lichess = LichessClient::new()
            .map_err(|e| OracleServiceError::Config(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(PollerInner {
                queue: PendingQueue::new(&cfg.queue_dir),
                dead_letter: DeadLetterStore::new(&cfg.queue_dir),
                soroban,
                chess_com,
                lichess,
                signing_key: Zeroizing::new(*cfg.oracle_signing_key),
                max_retries: cfg.max_retries,
                retry_base_delay_secs: cfg.retry_base_delay_secs,
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

        let chess_com = ChessComClient::new()
            .map_err(|e| OracleServiceError::Config(e.to_string()))?;
        let lichess = LichessClient::new_with_base_and_timeout(
            lichess_base,
            std::time::Duration::from_secs(30),
        )
        .map_err(|e| OracleServiceError::Config(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(PollerInner {
                queue: PendingQueue::new(&cfg.queue_dir),
                dead_letter: DeadLetterStore::new(&cfg.queue_dir),
                soroban,
                chess_com,
                lichess,
                signing_key: Zeroizing::new(*cfg.oracle_signing_key),
                max_retries: cfg.max_retries,
                retry_base_delay_secs: cfg.retry_base_delay_secs,
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
        let lichess = LichessClient::new()
            .map_err(|e| OracleServiceError::Config(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(PollerInner {
                queue: PendingQueue::new(&cfg.queue_dir),
                dead_letter: DeadLetterStore::new(&cfg.queue_dir),
                soroban,
                chess_com,
                lichess,
                signing_key: Zeroizing::new(*cfg.oracle_signing_key),
                max_retries: cfg.max_retries,
                retry_base_delay_secs: cfg.retry_base_delay_secs,
            }),
        })
    }

    /// Run a single polling tick: process all due queue entries.
    ///
    /// This is `pub` so that tests can call it directly without spawning a
    /// background task.
    pub async fn tick(&self) -> Result<(), OracleServiceError> {
        let due = self.inner.queue.due_entries().await?;
        if due.is_empty() {
            return Ok(());
        }
        info!(count = due.len(), "poller tick: processing due entries");

        for entry in due {
            self.process_entry(entry).await;
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

    /// Enqueue a new match for verification if not already queued.
    pub async fn enqueue(
        &self,
        match_id: u64,
        game_id: String,
        platform: Platform,
    ) -> Result<bool, OracleServiceError> {
        self.inner.queue.enqueue(match_id, game_id, platform).await
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

        // ── Fetch result from chess platform ─────────────────────────────
        let winner_result = match entry.platform {
            Platform::Lichess => {
                self.inner
                    .lichess
                    .fetch_result(&game_id)
                    .await
                    .map(|r| r.winner)
                    .map_err(|e| classify_lichess_error(e))
            }
            Platform::ChessDotCom => {
                self.inner
                    .chess_com
                    .fetch_result(&game_id)
                    .await
                    .map(|r| r.winner)
                    .map_err(|e| classify_chess_com_error(e))
            }
        };

        match winner_result {
            Ok(winner) => {
                info!(match_id, ?winner, "result fetched; submitting to Soroban");
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
                    error!(match_id, "failed to remove completed entry from queue: {}", e);
                }
            }
            Err(e) => {
                warn!(match_id, "Soroban submission failed (transient?): {}", e);
                self.handle_transient(entry, e.to_string()).await;
            }
        }
    }

    async fn handle_transient(&self, mut entry: PendingEntry, reason: String) {
        let exhausted = entry.record_failure(
            reason,
            self.inner.retry_base_delay_secs,
            self.inner.max_retries,
        );

        if exhausted {
            self.exhaust_entry(entry).await;
        } else {
            if let Err(e) = self.inner.queue.update_entry(entry).await {
                error!("failed to update queue entry: {}", e);
            }
        }
    }

    async fn exhaust_entry(&self, entry: PendingEntry) {
        let match_id = entry.match_id;
        if let Err(e) = self.inner.dead_letter.push(entry).await {
            error!(match_id, "failed to move entry to dead-letter store: {}", e);
        }
        if let Err(e) = self.inner.queue.remove(match_id).await {
            error!(match_id, "failed to remove exhausted entry from queue: {}", e);
        }
    }
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
        LichessError::GameNotFinished => FetchError::Transient(e.to_string()),
        LichessError::Http(_)
        | LichessError::Timeout
        | LichessError::HttpStatus { .. }
        | LichessError::InvalidResponse => FetchError::Transient(e.to_string()),
    }
}

fn classify_chess_com_error(e: ChessComError) -> FetchError {
    match e {
        ChessComError::InvalidGameId => FetchError::Permanent(e.to_string()),
        ChessComError::GameNotFound => FetchError::Permanent(e.to_string()),
        ChessComError::GameNotFinished => FetchError::Transient(e.to_string()),
        ChessComError::Http(_)
        | ChessComError::Timeout
        | ChessComError::HttpStatus { .. }
        | ChessComError::InvalidResponse => FetchError::Transient(e.to_string()),
    }
}
