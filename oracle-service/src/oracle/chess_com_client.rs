use std::sync::Arc;
use std::time::{Duration, Instant};

use contracts_oracle::types::Winner;

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::errors::ChessComError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessComGameResult {
    pub winner: Winner,
}

/// Chess.com off-chain client.
///
/// - Validates Chess.com game IDs (numeric ASCII).
/// - Enforces 30 req/min using a local rate limiter.
/// - Applies a per-request timeout (30s).
///
/// Note: governor configuration differs slightly across governor versions.
/// We use a conservative approach: a simple async mutex-based spacing
/// (at least 2 seconds between requests) which is compatible and
/// satisfies the 30 req/min policy.
#[derive(Clone)]
pub struct ChessComClient {
    http: Client,
    api_base: String,
    min_spacing: Duration,
    last_request: Arc<Mutex<Instant>>,
}

/// Implements [`Default`] by delegating to [`ChessComClient::new`].
///
/// # Panics
/// Panics if the underlying HTTP client cannot be constructed (e.g. TLS
/// initialisation failure). Use [`ChessComClient::new`] directly when you
/// need to handle that error explicitly.
impl Default for ChessComClient {
    fn default() -> Self {
        Self::new().expect("failed to construct ChessComClient")
    }
}

impl ChessComClient {
    pub fn new() -> Result<Self, ChessComError> {
        Self::new_with_base_and_timeout(
            "https://api.chess.com".to_string(),
            Duration::from_secs(30),
        )
    }

    pub fn new_with_base_and_timeout(
        api_base: String,
        request_timeout: Duration,
    ) -> Result<Self, ChessComError> {
        let http = Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(ChessComError::Http)?;

        // 30 req/min => 1 req / 2 seconds.
        let min_spacing = Duration::from_secs(2);

        Ok(Self {
            http,
            api_base,
            min_spacing,
            last_request: Arc::new(Mutex::new(Instant::now() - min_spacing)),
        })
    }

    pub fn validate_game_id(game_id: &str) -> Result<(), ChessComError> {
        if game_id.is_empty() {
            return Err(ChessComError::InvalidGameId);
        }
        if !game_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(ChessComError::InvalidGameId);
        }
        Ok(())
    }

    async fn enforce_rate_limit(&self) -> Result<(), ChessComError> {
        let mut last = self.last_request.lock().await;
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(*last);
        if elapsed < self.min_spacing {
            tokio::time::sleep(self.min_spacing - elapsed).await;
        }
        *last = Instant::now();
        Ok(())
    }

    pub async fn fetch_result(&self, game_id: &str) -> Result<ChessComGameResult, ChessComError> {
        Self::validate_game_id(game_id)?;

        self.enforce_rate_limit().await?;

        let url = format!(
            "{}/pub/game/{}",
            self.api_base.trim_end_matches('/'),
            game_id
        );

        let resp = self.http.get(url).send().await.map_err(|e| {
            if e.is_timeout() {
                ChessComError::Timeout
            } else {
                ChessComError::Http(e)
            }
        })?;

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ChessComError::GameNotFound);
        }
        if !status.is_success() {
            return Err(ChessComError::HttpStatus { status });
        }

        let body: ChessComGame = resp.json().await.map_err(ChessComError::Http)?;

        let end_state = body.end.and_then(|e| e.result);
        let result_str = end_state.ok_or(ChessComError::InvalidResponse)?;

        // Chess.com result values: "white", "black", "draw", ...
        let winner = match result_str.as_str() {
            "draw" => Winner::Draw,
            "white" => Winner::Player1,
            "black" => Winner::Player2,
            _ => return Err(ChessComError::InvalidResponse),
        };

        Ok(ChessComGameResult { winner })
    }
}

// Minimal response shape needed for result mapping.
#[derive(Debug, Deserialize)]
struct ChessComGame {
    end: Option<ChessComEnd>,
}

#[derive(Debug, Deserialize)]
struct ChessComEnd {
    result: Option<String>,
}
