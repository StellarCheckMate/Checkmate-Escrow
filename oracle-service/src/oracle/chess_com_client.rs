use std::sync::Arc;
use std::time::Duration;

use contracts_oracle::types::Winner;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore};

use super::errors::ChessComError;
use super::provider::GameProvider;
use super::provider_error::ProviderError;
use super::rate_limiter::{RateLimiter, RateLimiterConfig};

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessComGameResult {
    pub winner: Winner,
}

/// Configuration for [`ChessComClient`].
#[derive(Debug, Clone)]
pub struct ChessComClientConfig {
    /// API base URL (override in tests with a mock server address).
    pub api_base: String,
    /// Per-request HTTP timeout.
    pub request_timeout: Duration,
    /// Token-bucket settings (burst + sustained rate).
    pub rate_limiter: RateLimiterConfig,
    /// Maximum number of in-flight HTTP requests at any one time.
    pub max_concurrent: usize,
}

impl Default for ChessComClientConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.chess.com".to_string(),
            request_timeout: Duration::from_secs(30),
            rate_limiter: RateLimiterConfig::chess_com_default(),
            max_concurrent: 4,
        }
    }
}

/// Chess.com off-chain client.
///
/// ## Rate limiting
///
/// Uses a **token-bucket** limiter (see [`RateLimiter`]) rather than a simple
/// mutex gate.  The default configuration matches Chess.com's public API limit
/// of **30 req/min** (0.5 req/s sustained) with a burst ceiling of **5
/// tokens**, meaning up to 5 requests may be dispatched back-to-back before
/// the sustained rate kicks in.
///
/// ## Concurrency
///
/// A [`Semaphore`] caps the number of in-flight HTTP requests at
/// `max_concurrent` (default 4) regardless of how many async tasks are
/// sharing the client.
///
/// ## Sharing
///
/// [`ChessComClient`] is `Clone` and cheap to clone — all clones share the
/// same token bucket and semaphore, so they collectively respect the same
/// limits.
#[derive(Clone)]
pub struct ChessComClient {
    http: Client,
    api_base: String,
    rate_limiter: RateLimiter,
    semaphore: Arc<Semaphore>,
    api_key: Arc<RwLock<Option<String>>>,
}

impl Default for ChessComClient {
    fn default() -> Self {
        Self::new().expect("failed to construct ChessComClient")
    }
}

impl ChessComClient {
    /// Create a client with production defaults.
    pub fn new() -> Result<Self, ChessComError> {
        Self::with_config(ChessComClientConfig::default())
    }

    /// Create a client with fully custom configuration (useful in tests).
    pub fn with_config(cfg: ChessComClientConfig) -> Result<Self, ChessComError> {
        let http = Client::builder()
            .timeout(cfg.request_timeout)
            .build()
            .map_err(ChessComError::Http)?;

        Ok(Self {
            http,
            api_base: cfg.api_base,
            rate_limiter: RateLimiter::new(cfg.rate_limiter),
            semaphore: Arc::new(Semaphore::new(cfg.max_concurrent)),
            api_key: Arc::new(RwLock::new(Self::load_api_key())),
        })
    }

    /// Convenience constructor used by existing tests: accepts only base URL
    /// and timeout, uses default rate-limit and concurrency settings.
    pub fn new_with_base_and_timeout(
        api_base: String,
        request_timeout: Duration,
    ) -> Result<Self, ChessComError> {
        Self::with_config(ChessComClientConfig {
            api_base,
            request_timeout,
            ..Default::default()
        })
    }

    /// Validate that `game_id` is a non-empty numeric string.

    fn load_api_key() -> Option<String> {
        std::env::var("CHESSDOTCOM_API_KEY")
            .ok()
            .filter(|key| !key.is_empty())
    }

    async fn reload_api_key(&self) {
        *self.api_key.write().await = Self::load_api_key();
    }

    pub fn validate_game_id(game_id: &str) -> Result<(), ChessComError> {
        if game_id.is_empty() || !game_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(ChessComError::InvalidGameId);
        }
        Ok(())
    }

    /// Acquire a rate-limit token and a concurrency permit, then perform the
    /// HTTP request.
    pub async fn fetch_result(&self, game_id: &str) -> Result<ChessComGameResult, ChessComError> {
        Self::validate_game_id(game_id)?;

        // 1. Acquire a rate-limit token (may sleep until one is available).
        self.rate_limiter.acquire().await;

        // 2. Acquire a concurrency permit (blocks if max_concurrent in-flight).
        let _permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        // 3. Issue the HTTP request.
        let url = format!(
            "{}/pub/game/{}",
            self.api_base.trim_end_matches('/'),
            game_id
        );

        let mut retry_count = 0;
        let mut auth_retry_count = 0;
        let resp = loop {
            let api_key = self.api_key.read().await.clone();
            let request = self.http.get(&url);
            let request = match api_key {
                Some(api_key) => request.bearer_auth(api_key),
                None => request,
            };
            let resp = request.send().await.map_err(|e| {
                if e.is_timeout() {
                    ChessComError::Timeout
                } else {
                    ChessComError::Http(e)
                }
            })?;

            if resp.status() == reqwest::StatusCode::UNAUTHORIZED && auth_retry_count == 0 {
                auth_retry_count += 1;
                self.reload_api_key().await;
                continue;
            }

            if resp.status() != reqwest::StatusCode::TOO_MANY_REQUESTS || retry_count >= MAX_RETRIES {
                break resp;
            }

            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs)
                .unwrap_or_else(|| INITIAL_BACKOFF * 2u32.pow(retry_count));

            retry_count += 1;
            tokio::time::sleep(retry_after).await;
        };

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ChessComError::GameNotFound);
        }
        if !status.is_success() {
            return Err(ChessComError::HttpStatus { status });
        }

        let body: ChessComGame = resp.json().await.map_err(ChessComError::Http)?;

        let result_str = body
            .end
            .and_then(|e| e.result)
            .ok_or(ChessComError::InvalidResponse)?;

        let winner = match result_str.as_str() {
            "draw" => Winner::Draw,
            "white" => Winner::Player1,
            "black" => Winner::Player2,
            _ => return Err(ChessComError::InvalidResponse),
        };

        Ok(ChessComGameResult { winner })
    }

    /// Health check: test connectivity to the Chess.com API.
    pub async fn health_check(&self) -> Result<(), ChessComError> {
        let _permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        let url = format!(
            "{}/pub/player/profiles",
            self.api_base.trim_end_matches('/')
        );

        let resp = self.http.get(&url).send().await.map_err(|e| {
            if e.is_timeout() {
                ChessComError::Timeout
            } else {
                ChessComError::Http(e)
            }
        })?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(ChessComError::HttpStatus {
                status: resp.status(),
            })
        }
    }
}

// ── GameProvider impl ─────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl GameProvider for ChessComClient {
    fn name(&self) -> &'static str {
        "chess.com"
    }

    async fn fetch_result(&self, game_id: &str) -> Result<Winner, ProviderError> {
        self.fetch_result(game_id)
            .await
            .map(|r| r.winner)
            .map_err(ProviderError::from)
    }
}

// ── Response shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChessComGame {
    end: Option<ChessComEnd>,
}

#[derive(Debug, Deserialize)]
struct ChessComEnd {
    result: Option<String>,
}
