use std::sync::Arc;
use std::time::{Duration, Instant};

use contracts_oracle::types::Winner;
use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::errors::ChessComError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessGameResult {
    pub winner: Winner,
}

/// Lichess off-chain client.
///
/// - Validates Lichess game IDs (exactly 8 alphanumeric characters).
/// - Applies a per-request timeout (30s).
#[derive(Clone)]
pub struct LichessClient {
    http: Client,
    api_base: String,
    min_spacing: Duration,
    last_request: Arc<Mutex<Instant>>,
}

impl Default for LichessClient {
    fn default() -> Self {
        Self::new().expect("failed to construct LichessClient")
    }
}

impl LichessClient {
    pub fn new() -> Result<Self, ChessComError> {
        Self::new_with_base_and_timeout(
            "https://lichess.org".to_string(),
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

        let min_spacing = Duration::from_secs(2);

        Ok(Self {
            http,
            api_base,
            min_spacing,
            last_request: Arc::new(Mutex::new(Instant::now() - min_spacing)),
        })
    }

    /// Validates that a Lichess game ID is exactly 8 alphanumeric characters.
    pub fn validate_game_id(game_id: &str) -> Result<(), ChessComError> {
        if game_id.len() != 8 || !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(ChessComError::InvalidGameId);
        }
        Ok(())
    }

    async fn enforce_rate_limit(&self) -> Result<(), ChessComError> {
        let mut last = self.last_request.lock().await;
        let elapsed = Instant::now().saturating_duration_since(*last);
        if elapsed < self.min_spacing {
            tokio::time::sleep(self.min_spacing - elapsed).await;
        }
        *last = Instant::now();
        Ok(())
    }

    pub async fn fetch_result(&self, game_id: &str) -> Result<LichessGameResult, ChessComError> {
        Self::validate_game_id(game_id)?;

        self.enforce_rate_limit().await?;

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

        let body: LichessGame = resp.json().await.map_err(ChessComError::Http)?;

        // Lichess: "winner" field is "white", "black", or absent (draw).
        let winner = match body.winner.as_deref() {
            Some("white") => Winner::Player1,
            Some("black") => Winner::Player2,
            None => Winner::Draw,
            _ => return Err(ChessComError::InvalidResponse),
        };

        Ok(LichessGameResult { winner })
    }
}

#[derive(Debug, Deserialize)]
struct LichessGame {
    winner: Option<String>,
}
