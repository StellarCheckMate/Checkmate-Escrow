use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use contracts_oracle::types::Winner;
use reqwest::{Client, Response};
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::errors::LichessError;
use super::provider::GameProvider;
use super::provider_error::ProviderError;
use super::rate_limiter::{RateLimiter, RateLimiterConfig};

/// Maximum number of game IDs Lichess accepts per `POST /api/games/export/_ids`.
pub const LICHESS_BATCH_MAX_IDS: usize = 300;

/// Lichess asks clients to wait a full minute after a 429 when no
/// `Retry-After` header is provided.
const DEFAULT_RATE_LIMIT_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessGameResult {
    pub winner: Winner,
}

/// Configuration for [`LichessClient`].
#[derive(Debug, Clone)]
pub struct LichessClientConfig {
    /// API base URL (override in tests with a mock server address).
    pub api_base: String,
    /// Per-request HTTP timeout.
    pub request_timeout: Duration,
    /// Token-bucket settings (burst + sustained rate).
    pub rate_limiter: RateLimiterConfig,
    /// Maximum number of in-flight HTTP requests at any one time.
    pub max_concurrent: usize,
}

impl Default for LichessClientConfig {
    fn default() -> Self {
        Self {
            api_base: "https://lichess.org".to_string(),
            request_timeout: Duration::from_secs(30),
            rate_limiter: RateLimiterConfig::lichess_default(),
            max_concurrent: 8,
        }
    }
}

/// Lichess off-chain client.
///
/// - Validates Lichess game IDs: exactly 8 or 12 alphanumeric characters.
///   Standard games use 8-character IDs; some tournament formats use 12.
/// - Applies a per-request timeout.
/// - Token-bucket rate limiting (60 req/min by default).
/// - Concurrency semaphore (8 in-flight requests by default).
///
/// ## Sharing
///
/// [`LichessClient`] is `Clone` and cheap to clone — all clones share the
/// same token bucket and semaphore.
#[derive(Clone)]
pub struct LichessClient {
    http: Client,
    api_base: String,
    rate_limiter: RateLimiter,
    semaphore: Arc<Semaphore>,
}

impl Default for LichessClient {
    fn default() -> Self {
        Self::new().expect("failed to construct LichessClient")
    }
}

impl LichessClient {
    /// Create a client with production defaults.
    pub fn new() -> Result<Self, LichessError> {
        Self::with_config(LichessClientConfig::default())
    }

    /// Create a client with fully custom configuration (useful in tests).
    pub fn with_config(cfg: LichessClientConfig) -> Result<Self, LichessError> {
        let http = Client::builder()
            .timeout(cfg.request_timeout)
            .build()
            .map_err(LichessError::Http)?;

        Ok(Self {
            http,
            api_base: cfg.api_base,
            rate_limiter: RateLimiter::new(cfg.rate_limiter),
            semaphore: Arc::new(Semaphore::new(cfg.max_concurrent)),
        })
    }

    /// Convenience constructor: accepts only base URL and timeout, uses
    /// default rate-limit and concurrency settings.
    pub fn new_with_base_and_timeout(
        api_base: String,
        request_timeout: Duration,
    ) -> Result<Self, LichessError> {
        Self::with_config(LichessClientConfig {
            api_base,
            request_timeout,
            ..Default::default()
        })
    }

    /// Validates that a Lichess game ID is 8 or 12 alphanumeric characters.
    ///
    /// Standard Lichess games use 8-character IDs.  Lichess recently began
    /// supporting extended 12-character IDs for some tournament formats.
    pub fn validate_game_id(game_id: &str) -> Result<(), LichessError> {
        let len = game_id.len();
        if (len != 8 && len != 12) || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(LichessError::InvalidGameId);
        }
        Ok(())
    }

    /// Fetch the result for `game_id` from the Lichess API.
    pub async fn fetch_result(&self, game_id: &str) -> Result<LichessGameResult, LichessError> {
        Self::validate_game_id(game_id)?;

        // 1. Acquire a rate-limit token (may sleep until one is available).
        self.rate_limiter.acquire().await;

        // 2. Acquire a concurrency permit.
        let _permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        // 3. Issue the HTTP request.
        let url = format!(
            "{}/game/export/{}",
            self.api_base.trim_end_matches('/'),
            game_id
        );

        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LichessError::Timeout
                } else {
                    LichessError::Http(e)
                }
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(LichessError::GameNotFound);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LichessError::RateLimited {
                retry_after: retry_after(&resp),
            });
        }
        if !status.is_success() {
            return Err(LichessError::HttpStatus { status });
        }

        let body: LichessGame = resp.json().await.map_err(LichessError::Http)?;
        game_result(&body)
    }

    /// Fetch the results of many games with one request per
    /// [`LICHESS_BATCH_MAX_IDS`] IDs (`POST /api/games/export/_ids`).
    ///
    /// Returns one entry per distinct input ID.  Per-game problems (invalid
    /// ID, game missing from the response, game still in progress) are
    /// reported in that game's entry.  A failure of the request itself —
    /// including [`LichessError::RateLimited`] — fails the whole call so the
    /// caller can back off instead of retrying each game individually.
    pub async fn fetch_results_batch(
        &self,
        game_ids: &[String],
    ) -> Result<HashMap<String, Result<LichessGameResult, LichessError>>, LichessError> {
        let mut results = HashMap::new();
        let mut valid: Vec<&str> = Vec::new();
        for id in game_ids {
            if results.contains_key(id) || valid.contains(&id.as_str()) {
                continue;
            }
            match Self::validate_game_id(id) {
                Ok(()) => valid.push(id),
                Err(e) => {
                    results.insert(id.clone(), Err(e));
                }
            }
        }

        for chunk in valid.chunks(LICHESS_BATCH_MAX_IDS) {
            let body = self.export_games(chunk).await?;
            let mut games: HashMap<String, LichessGame> = HashMap::new();
            for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let game: LichessGame =
                    serde_json::from_str(line).map_err(|_| LichessError::InvalidResponse)?;
                if let Some(id) = game.id.clone() {
                    games.insert(id, game);
                }
            }
            for id in chunk {
                // Lichess silently omits IDs it does not know.
                let result = games
                    .get(*id)
                    .map_or(Err(LichessError::GameNotFound), game_result);
                results.insert(id.to_string(), result);
            }
        }

        Ok(results)
    }

    /// One `POST /api/games/export/_ids` round trip; returns the NDJSON body.
    async fn export_games(&self, ids: &[&str]) -> Result<String, LichessError> {
        self.rate_limiter.acquire().await;
        let _permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        let url = format!(
            "{}/api/games/export/_ids",
            self.api_base.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .header("Accept", "application/x-ndjson")
            .header("Content-Type", "text/plain")
            .body(ids.join(","))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LichessError::Timeout
                } else {
                    LichessError::Http(e)
                }
            })?;

        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(LichessError::RateLimited {
                retry_after: retry_after(&resp),
            });
        }
        if !status.is_success() {
            return Err(LichessError::HttpStatus { status });
        }
        resp.text().await.map_err(LichessError::Http)
    }

    /// Health check: test connectivity to the Lichess API.
    pub async fn health_check(&self) -> Result<(), LichessError> {
        let _permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        let url = format!("{}/api/account", self.api_base.trim_end_matches('/'));

        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    LichessError::Timeout
                } else {
                    LichessError::Http(e)
                }
            })?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(LichessError::HttpStatus {
                status: resp.status(),
            })
        }
    }
}

/// `Retry-After` in whole seconds, defaulting to Lichess's one-minute guidance.
fn retry_after(resp: &Response) -> Duration {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_RATE_LIMIT_BACKOFF)
}

/// Map a Lichess game record onto a settled result.
///
/// Lichess omits `winner` for draws, but also for games still being played
/// (`status` `created` / `started`); those are reported as
/// [`LichessError::GameNotFinished`] rather than settled as a draw.
fn game_result(game: &LichessGame) -> Result<LichessGameResult, LichessError> {
    let winner = match (game.winner.as_deref(), game.status.as_deref()) {
        (Some("white"), _) => Winner::Player1,
        (Some("black"), _) => Winner::Player2,
        (Some(_), _) => return Err(LichessError::InvalidResponse),
        (None, Some("created" | "started")) => return Err(LichessError::GameNotFinished),
        (None, _) => Winner::Draw,
    };
    Ok(LichessGameResult { winner })
}

// ── GameProvider impl ─────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl GameProvider for LichessClient {
    fn name(&self) -> &'static str {
        "lichess"
    }

    async fn fetch_result(&self, game_id: &str) -> Result<Winner, ProviderError> {
        // Lichess game IDs are 8 or 12 alphanumeric chars; fail fast otherwise.
        let len = game_id.len();
        if (len != 8 && len != 12) || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ProviderError::InvalidGameId(format!(
                "lichess expects 8 or 12 alphanumeric chars, got {:?}",
                game_id
            )));
        }
        self.fetch_result(game_id)
            .await
            .map(|r| r.winner)
            .map_err(|e| {
                // Re-label the provider field on the conversion.
                match ProviderError::from(e) {
                    ProviderError::Unavailable { reason, .. } => ProviderError::Unavailable {
                        provider: "lichess",
                        reason,
                    },
                    ProviderError::RateLimited { retry_after, .. } => ProviderError::RateLimited {
                        provider: "lichess",
                        retry_after,
                    },
                    ProviderError::InvalidResponse { detail, .. } => {
                        ProviderError::InvalidResponse {
                            provider: "lichess",
                            detail,
                        }
                    }
                    other => other,
                }
            })
    }
}

// ── Response shape ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct LichessGame {
    id: Option<String>,
    status: Option<String>,
    winner: Option<String>,
}
