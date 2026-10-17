//! Lichess-specific fetcher logic for the oracle service.
//!
//! This module contains everything that is specific to the Lichess platform:
//! game-id validation, result fetching, and result normalization.

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, warn};

use crate::oracle::types::{GameResult, GameStatus, Platform};

/// Base URL for the Lichess public API.
const LICHESS_API_BASE: &str = "https://lichess.org";

/// Validates that the given string looks like a Lichess game id.
///
/// Lichess game ids are 8 or 12 character alphanumeric strings.
pub fn is_valid_game_id(game_id: &str) -> bool {
    let len = game_id.len();
    if len != 8 && len != 12 {
        return false;
    }
    game_id.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Raw response shape from the Lichess `/api/games/{id}` endpoint.
///
/// Only the fields we care about are deserialized.
#[derive(Debug, Deserialize)]
struct LichessGameResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    winner: Option<String>,
    #[serde(default)]
    players: Option<LichessPlayers>,
}

#[derive(Debug, Deserialize)]
struct LichessPlayers {
    #[serde(default)]
    white: Option<LichessPlayer>,
    #[serde(default)]
    black: Option<LichessPlayer>,
}

#[derive(Debug, Deserialize)]
struct LichessPlayer {
    #[serde(default)]
    user: Option<LichessUser>,
}

#[derive(Debug, Deserialize)]
struct LichessUser {
    #[serde(default)]
    name: Option<String>,
}

/// Fetches the result of a Lichess game by its id.
///
/// Returns a normalized [`GameResult`] describing the outcome of the game.
pub async fn fetch_game_result(client: &Client, game_id: &str) -> Result<GameResult> {
    if !is_valid_game_id(game_id) {
        return Err(anyhow!("invalid Lichess game id: {game_id}"));
    }

    let url = format!("{LICHESS_API_BASE}/api/games/{game_id}");
    debug!(url = %url, "fetching Lichess game result");

    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("failed to send Lichess request for game {game_id}"))?;

    if !response.status().is_success() {
        let status = response.status();
        warn!(%status, game_id, "Lichess API returned non-success status");
        return Err(anyhow!(
            "Lichess API returned status {status} for game {game_id}"
        ));
    }

    let game: LichessGameResponse = response
        .json()
        .await
        .with_context(|| format!("failed to parse Lichess response for game {game_id}"))?;

    Ok(normalize_game_result(game_id, game))
}

/// Normalizes a raw Lichess game response into a [`GameResult`].
fn normalize_game_result(game_id: &str, game: LichessGameResponse) -> GameResult {
    let status = game.status.as_deref().unwrap_or("unknown");
    let winner = game.winner.as_deref();

    let (game_status, winner_name) = match status {
        "created" | "started" => (GameStatus::InProgress, None),
        "resign" | "mate" | "outoftime" | "timeout" | "draw" | "stalemate"
        | "cheat" | "noStart" | "unknownFinish" | "variantEnd" => {
            let winner_name = winner.and_then(|w| player_name(&game.players, w));
            (GameStatus::Finished, winner_name)
        }
        other => {
            warn!(game_id, status = other, "unrecognized Lichess game status");
            (GameStatus::Unknown, None)
        }
    };

    GameResult {
        platform: Platform::Lichess,
        game_id: game_id.to_string(),
        status: game_status,
        winner: winner_name,
    }
}

/// Resolves the player name for the given side ("white" / "black").
fn player_name(players: &Option<LichessPlayers>, side: &str) -> Option<String> {
    let players = players.as_ref()?;
    let player = match side {
        "white" => players.white.as_ref(),
        "black" => players.black.as_ref(),
        _ => None,
    }?;
    player.user.as_ref()?.name.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_game_ids() {
        assert!(is_valid_game_id("abcd1234"));
        assert!(is_valid_game_id("abcd1234wxyz"));
    }

    #[test]
    fn invalid_game_ids() {
        assert!(!is_valid_game_id(""));
        assert!(!is_valid_game_id("short"));
        assert!(!is_valid_game_id("has-dash"));
        assert!(!is_valid_game_id("toolonggameid123"));
    }

    #[test]
    fn normalizes_finished_game_with_winner() {
        let game = LichessGameResponse {
            status: Some("mate".to_string()),
            winner: Some("white".to_string()),
            players: Some(LichessPlayers {
                white: Some(LichessPlayer {
                    user: Some(LichessUser {
                        name: Some("alice".to_string()),
                    }),
                }),
                black: Some(LichessPlayer {
                    user: Some(LichessUser {
                        name: Some("bob".to_string()),
                    }),
                }),
            }),
        };

        let result = normalize_game_result("abcd1234", game);
        assert_eq!(result.platform, Platform::Lichess);
        assert_eq!(result.status, GameStatus::Finished);
        assert_eq!(result.winner.as_deref(), Some("alice"));
    }

    #[test]
    fn normalizes_in_progress_game() {
        let game = LichessGameResponse {
            status: Some("started".to_string()),
            winner: None,
            players: None,
        };

        let result = normalize_game_result("abcd1234", game);
        assert_eq!(result.status, GameStatus::InProgress);
        assert!(result.winner.is_none());
    }

    #[test]
    fn normalizes_draw_without_winner() {
        let game = LichessGameResponse {
            status: Some("draw".to_string()),
            winner: None,
            players: None,
        };

        let result = normalize_game_result("abcd1234", game);
        assert_eq!(result.status, GameStatus::Finished);
        assert!(result.winner.is_none());
    }
}