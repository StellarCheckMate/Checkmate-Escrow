use std::sync::Arc;
use std::time::{Duration, Instant};

use contracts_oracle::types::Winner;

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::errors::LichessError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LichessGameResult {
    pub winner: Winner,
}

#[derive(Clone)]
pub struct LichessClient {
    http: Client,
    api_base: String,
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
            .timeout(request_timeout)
            .build()
            .map_err(LichessError::Http)?;

        Ok(Self {
            http,
            api_base,
        })
    }

    pub fn validate_game_id(game_id: &str) -> Result<(), LichessError> {
        if game_id.len() != 8 {
            return Err(LichessError::InvalidGameId);
        }
        if !game_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(LichessError::InvalidGameId);
        }
        Ok(())
    }

    pub async fn fetch_result(&self, game_id: &str) -> Result<LichessGameResult, LichessError> {
        Self::validate_game_id(game_id)?;

        let url = format!(
            "{}/game/export/{}",
            self.api_base.trim_end_matches('/'),
            game_id
        );

        let resp = self.http.get(url).send().await.map_err(|e| {
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

#[derive(Debug, Deserialize)]
struct LichessGame {
    winner: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[test]
    fn test_validate_game_id_valid() {
        assert!(LichessClient::validate_game_id("abc12345").is_ok());
    }

    #[test]
    fn test_validate_game_id_invalid_length() {
        assert!(LichessClient::validate_game_id("abc123").is_err());
    }

    #[test]
    fn test_validate_game_id_invalid_chars() {
        assert!(LichessClient::validate_game_id("abc!2345").is_err());
    }

    #[tokio::test]
    async fn test_fetch_result_white_wins() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/game/export/abc12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"winner":"white"}"#)
            .create();

        let client = LichessClient::new_with_base_and_timeout(server.url(), Duration::from_secs(30)).unwrap();
        let result = client.fetch_result("abc12345").await.unwrap();

        assert_eq!(result.winner, Winner::Player1);
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_result_black_wins() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/game/export/abc12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"winner":"black"}"#)
            .create();

        let client = LichessClient::new_with_base_and_timeout(server.url(), Duration::from_secs(30)).unwrap();
        let result = client.fetch_result("abc12345").await.unwrap();

        assert_eq!(result.winner, Winner::Player2);
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_result_draw() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/game/export/abc12345")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"winner":null}"#)
            .create();

        let client = LichessClient::new_with_base_and_timeout(server.url(), Duration::from_secs(30)).unwrap();
        let result = client.fetch_result("abc12345").await.unwrap();

        assert_eq!(result.winner, Winner::Draw);
        mock.assert();
    }

    #[tokio::test]
    async fn test_fetch_result_not_found() {
        let mut server = Server::new();
        let mock = server
            .mock("GET", "/game/export/abc12345")
            .with_status(404)
            .create();

        let client = LichessClient::new_with_base_and_timeout(server.url(), Duration::from_secs(30)).unwrap();
        let result = client.fetch_result("abc12345").await;

        assert!(matches!(result, Err(LichessError::GameNotFound)));
        mock.assert();
    }
}
