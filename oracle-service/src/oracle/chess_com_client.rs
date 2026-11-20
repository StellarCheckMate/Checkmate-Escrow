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

/// Maximum number of game IDs sent in a single batch request.
pub const MAX_BATCH_SIZE: usize = 50;

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

            if resp.status() != reqwest::StatusCode::TOO_MANY_REQUESTS || retry_count >= MAX_RETRIES
            {
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

        // Guard: reject non-JSON responses (e.g. captive-portal HTML) before
        // attempting to deserialize.  A missing or non-JSON Content-Type is
        // treated as an invalid response rather than a panic from serde.
        if !is_json_content_type(resp.headers()) {
            return Err(ChessComError::InvalidResponse);
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

    /// Fetch results for many games in as few HTTP requests as possible.
    ///
    /// Game IDs are grouped into chunks of at most [`MAX_BATCH_SIZE`] and each
    /// chunk is dispatched as a single batch request.  One rate-limit token and
    /// one concurrency permit are acquired per batch request (not per game),
    /// which keeps the polling cycle well within Chess.com's rate limit.
    ///
    /// The returned vector is aligned with `game_ids`: each entry is either the
    /// game result or the error encountered for that specific game.  If the
    /// batch endpoint responds with `429 Too Many Requests`, the whole batch is
    /// reported as [`ChessComError::RateLimited`] so the caller can apply
    /// back-pressure.
    pub async fn fetch_results_batch(
        &self,
        game_ids: &[String],
    ) -> Vec<Result<ChessComGameResult, ChessComError>> {
        let mut results: Vec<Result<ChessComGameResult, ChessComError>> =
            Vec::with_capacity(game_ids.len());

        for chunk in game_ids.chunks(MAX_BATCH_SIZE) {
            // Validate up-front so invalid IDs never reach the network.
            let mut valid: Vec<&String> = Vec::with_capacity(chunk.len());
            let mut chunk_results: Vec<Option<Result<ChessComGameResult, ChessComError>>> =
                Vec::with_capacity(chunk.len());
            for id in chunk {
                match Self::validate_game_id(id) {
                    Ok(()) => {
                        valid.push(id);
                        chunk_results.push(None);
                    }
                    Err(e) => chunk_results.push(Some(Err(e))),
                }
            }

            if !valid.is_empty() {
                match self.fetch_batch_chunk(&valid).await {
                    Ok(mut mapped) => {
                        // `mapped` is aligned with `valid`; splice into place.
                        let mut mapped_iter = mapped.drain(..);
                        for slot in chunk_results.iter_mut() {
                            if slot.is_none() {
                                *slot = Some(
                                    mapped_iter
                                        .next()
                                        .unwrap_or(Err(ChessComError::InvalidResponse)),
                                );
                            }
                        }
                    }
                    Err(e) => {
                        for slot in chunk_results.iter_mut() {
                            if slot.is_none() {
                                *slot = Some(Err(e.clone()));
                            }
                        }
                    }
                }
            }

            results.extend(
                chunk_results
                    .into_iter()
                    .map(|slot| slot.unwrap_or(Err(ChessComError::InvalidResponse))),
            );
        }

        results
    }

    /// Perform a single batch HTTP request for a validated, non-empty chunk of
    /// game IDs.  Returns results aligned with `game_ids`.
    async fn fetch_batch_chunk(
        &self,
        game_ids: &[&String],
    ) -> Result<Vec<Result<ChessComGameResult, ChessComError>>, ChessComError> {
        // 1. One rate-limit token for the whole batch.
        self.rate_limiter.acquire().await;

        // 2. One concurrency permit for the whole batch.
        let _permit: OwnedSemaphorePermit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("semaphore closed");

        let ids_csv = game_ids
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let url = format!(
            "{}/pub/games/{}",
            self.api_base.trim_end_matches('/'),
            ids_csv
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

            if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
                // Surface rate limiting to the caller for back-pressure.
                return Err(ChessComError::RateLimited);
            }

            if resp.status() != reqwest::StatusCode::TOO_MANY_REQUESTS || retry_count >= MAX_RETRIES
            {
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
        if !status.is_success() {
            return Err(ChessComError::HttpStatus { status });
        }

        // Guard: reject non-JSON responses before attempting to deserialize.
        if !is_json_content_type(resp.headers()) {
            return Err(ChessComError::InvalidResponse);
        }

        let body: ChessComGamesBatch = resp.json().await.map_err(ChessComError::Http)?;

        // Index returned games by their URL-derived id so we can map back to
        // the requested order.
        let mut by_id: std::collections::HashMap<String, ChessComGame> =
            std::collections::HashMap::with_capacity(body.games.len());
        for game in body.games {
            if let Some(id) = game.id_from_url() {
                by_id.insert(id, game);
            }
        }

        let mut out = Vec::with_capacity(game_ids.len());
        for id in game_ids {
            match by_id.remove(id.as_str()) {
                Some(game) => out.push(Self::game_to_result(game)),
                None => out.push(Err(ChessComError::GameNotFound)),
            }
        }
        Ok(out)
    }

    fn game_to_result(game: ChessComGame) -> Result<ChessComGameResult, ChessComError> {
        let result_str = game
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

// ── Content-type guard ────────────────────────────────────────────────────────

/// Return `true` only when the response `Content-Type` header indicates JSON.
///
/// Accepts `application/json` and `application/json; charset=utf-8` (and any
/// other parameter suffix).  Rejects HTML pages returned by captive portals or
/// intermediary proxies, which would otherwise cause a serde panic.
fn is_json_content_type(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            let ct = ct.trim().to_ascii_lowercase();
            ct.starts_with("application/json")
        })
        .unwrap_or(false)
}

// ── Response shapes ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChessComGame {
    #[serde(default)]
    url: Option<String>,
    end: Option<ChessComEnd>,
}

impl ChessComGame {
    /// Extract the numeric game id from the game's `url` field, if present.
    fn id_from_url(&self) -> Option<String> {
        let url = self.url.as_deref()?;
        let trimmed = url.trim_end_matches('/');
        let id = trimmed.rsplit('/').next()?;
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            Some(id.to_string())
        } else {
            None
        }
    }
}

#[derive(Debug, Deserialize)]
struct ChessComEnd {
    result: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChessComGamesBatch {
    #[serde(default)]
    games: Vec<ChessComGame>,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with_content_type(ct: &str) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        map.insert(
            reqwest::header::CONTENT_TYPE,
            ct.parse().unwrap(),
        );
        map
    }

    #[test]
    fn is_json_content_type_accepts_application_json() {
        let headers = headers_with_content_type("application/json");
        assert!(is_json_content_type(&headers));
    }

    #[test]
    fn is_json_content_type_accepts_with_charset() {
        let headers = headers_with_content_type("application/json; charset=utf-8");
        assert!(is_json_content_type(&headers));
    }

    #[test]
    fn is_json_content_type_rejects_html() {
        let headers = headers_with_content_type("text/html; charset=utf-8");
        assert!(!is_json_content_type(&headers));
    }

    #[test]
    fn is_json_content_type_rejects_missing_header() {
        let headers = reqwest::header::HeaderMap::new();
        assert!(!is_json_content_type(&headers));
    }

    #[test]
    fn is_json_content_type_rejects_plain_text() {
        let headers = headers_with_content_type("text/plain");
        assert!(!is_json_content_type(&headers));
    }
}
