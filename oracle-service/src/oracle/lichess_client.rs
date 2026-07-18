use std::sync::Arc;
use std::time::Duration;

use contracts_oracle::types::Winner;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::errors::LichessError;

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
/// - Validates Lichess game IDs (exactly 8 alphanumeric characters).
/// - Applies a per-request timeout (30s).
/// - Enforces 2-second spacing between requests to stay within rate limits.
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
    pub fn new() -> Result<Self, LichessError> {
        Self::new_with_base_and_timeout(
            "https://lichess.org".to_string(),
            Duration::from_secs(30),
        )
    }

    pub fn new_with_base_and_timeout(
        api_base: String,
        request_timeout: Duration,
    ) -> Result<Self, LichessError> {
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

    /// Convenience constructor used by existing tests.
    pub fn new_with_base_and_timeout(
        api_base: String,
        request_timeout: Duration,
    ) -> Result<Self, ChessComError> {
        Self::with_config(LichessClientConfig {
            api_base,
            request_timeout,
            ..Default::default()
        })
    }

    /// Validates that a Lichess game ID is exactly 8 alphanumeric characters.
    pub fn validate_game_id(game_id: &str) -> Result<(), LichessError> {
        if game_id.len() != 8 || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(LichessError::InvalidGameId);
        }
        Ok(())
    }

    async fn enforce_rate_limit(&self) -> Result<(), LichessError> {
        let mut last = self.last_request.lock().await;
        let elapsed = Instant::now().saturating_duration_since(*last);
        if elapsed < self.min_spacing {
            tokio::time::sleep(self.min_spacing - elapsed).await;
        }
        *last = Instant::now();
        Ok(())
    }

    pub async fn fetch_result(&self, game_id: &str) -> Result<LichessGameResult, LichessError> {
        Self::validate_game_id(game_id)?;

        // 1. Acquire a rate-limit token.
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
        if !status.is_success() {
            return Err(LichessError::HttpStatus { status });
        }

        let body: LichessGame = resp.json().await.map_err(LichessError::Http)?;

        let winner = match body.winner.as_deref() {
            Some("white") => Winner::Player1,
            Some("black") => Winner::Player2,
            None => Winner::Draw,
            _ => return Err(LichessError::InvalidResponse),
        };

        Ok(LichessGameResult { winner })
    }
}

// ── GameProvider impl ─────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl GameProvider for LichessClient {
    fn name(&self) -> &'static str {
        "lichess"
    }

    async fn fetch_result(&self, game_id: &str) -> Result<Winner, ProviderError> {
        // Lichess game IDs are always 8 chars; chess.com IDs are numeric.
        // If the game_id doesn't look like a Lichess ID, fail fast so the
        // registry can try the next provider.
        if game_id.len() != 8 || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ProviderError::InvalidGameId(format!(
                "lichess expects 8 alphanumeric chars, got {:?}",
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
                    ProviderError::RateLimited { retry_after, .. } => {
                        ProviderError::RateLimited {
                            provider: "lichess",
                            retry_after,
                        }
                    }
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
    winner: Option<String>,
}
