use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Client;
use tokio::sync::Mutex;

use super::errors::ChessComError;

/// Minimal result type for tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessGameResult {
    // keep minimal — tests only care about timing
    pub ok: bool,
}

#[derive(Clone)]
pub struct LichessClient {
    http: Client,
    api_base: String,
    min_spacing: Duration,
    last_request: Arc<Mutex<Instant>>,
}

impl LichessClient {
    pub fn new() -> Result<Self, ChessComError> {
        Self::new_with_base_and_timeout("https://lichess.org".to_string(), Duration::from_secs(30))
    }

    pub fn new_with_base_and_timeout(
        api_base: String,
        request_timeout: Duration,
    ) -> Result<Self, ChessComError> {
        // default spacing 2 seconds to be conservative
        Self::new_with_base_timeout_and_spacing(api_base, request_timeout, Duration::from_secs(2))
    }

    pub fn new_with_base_timeout_and_spacing(
        api_base: String,
        request_timeout: Duration,
        min_spacing: Duration,
    ) -> Result<Self, ChessComError> {
        let http = Client::builder()
            .timeout(request_timeout)
            .build()
            .map_err(ChessComError::Http)?;

        Ok(Self {
            http,
            api_base,
            min_spacing,
            last_request: Arc::new(Mutex::new(Instant::now() - min_spacing)),
        })
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

    /// Minimal fetch_result used by unit tests — returns Ok on 2xx and maps
    /// errors similarly to the Chess.com client.
    pub async fn fetch_result(&self, game_id: &str) -> Result<LichessGameResult, ChessComError> {
        self.enforce_rate_limit().await?;

        let url = format!("{}/api/game/{}", self.api_base.trim_end_matches('/'), game_id);

        let resp = self.http.get(url).send().await.map_err(|e| {
            if e.is_timeout() {
                ChessComError::Timeout
            } else {
                ChessComError::Http(e)
            }
        })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(ChessComError::HttpStatus { status });
        }

        Ok(LichessGameResult { ok: true })
    }
}
