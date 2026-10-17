//! Chess.com-specific fetcher logic for the oracle service.
//!
//! This module contains everything that is specific to the Chess.com
//! platform: game-id validation, result fetching and result normalization.
//! The generic polling loop lives in `poller.rs`, which calls into the
//! functions exposed here.

use reqwest::Client;
use serde::Deserialize;

use crate::oracle::OracleError;

/// Base URL for the public Chess.com API.
const CHESS_COM_API_BASE: &str = "https://api.chess.com/pub";

/// Outcome of a Chess.com game, normalized to the oracle's vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChessComResult {
    /// White won the game.
    WhiteWin,
    /// Black won the game.
    BlackWin,
    /// The game ended in a draw.
    Draw,
}

/// Subset of the Chess.com game payload that the oracle cares about.
#[derive(Debug, Deserialize)]
struct ChessComGame {
    /// Result reported by the API, e.g. `"win"`, `"checkmated"`, `"draw"`.
    #[serde(default)]
    white: ChessComPlayer,
    /// Black player information.
    #[serde(default)]
    black: ChessComPlayer,
}

/// Per-player information returned by the Chess.com API.
#[derive(Debug, Default, Deserialize)]
struct ChessComPlayer {
    /// The result string for this player, e.g. `"win"`, `"resigned"`.
    #[serde(default)]
    result: String,
}

/// Returns `true` if `game_id` is a syntactically valid Chess.com game id.
///
/// Chess.com game ids are numeric and are typically 7 to 12 digits long.
/// We accept any non-empty string composed solely of ASCII digits to stay
/// permissive with historical ids while still rejecting obviously malformed
/// input.
pub fn is_valid_game_id(game_id: &str) -> bool {
    !game_id.is_empty() && game_id.chars().all(|c| c.is_ascii_digit())
}

/// Builds the Chess.com API URL for the given game id.
fn game_url(game_id: &str) -> String {
    format!("{CHESS_COM_API_BASE}/game/{game_id}")
}

/// Normalizes a Chess.com result payload into a [`ChessComResult`].
///
/// The Chess.com API reports results per player. A player with `"win"`
/// means that player won; otherwise the game is treated as a draw unless
/// the opponent explicitly won.
fn normalize_result(game: &ChessComGame) -> ChessComResult {
    let white_won = game.white.result.eq_ignore_ascii_case("win");
    let black_won = game.black.result.eq_ignore_ascii_case("win");

    match (white_won, black_won) {
        (true, false) => ChessComResult::WhiteWin,
        (false, true) => ChessComResult::BlackWin,
        _ => ChessComResult::Draw,
    }
}

/// Fetches and normalizes the result of a Chess.com game.
///
/// Returns an error if the game id is invalid, the HTTP request fails, or
/// the response body cannot be parsed.
pub async fn fetch_result(client: &Client, game_id: &str) -> Result<ChessComResult, OracleError> {
    if !is_valid_game_id(game_id) {
        return Err(OracleError::InvalidGameId(game_id.to_string()));
    }

    let url = game_url(game_id);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| OracleError::Fetch(format!("chess.com request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(OracleError::Fetch(format!(
            "chess.com returned status {} for game {game_id}",
            response.status()
        )));
    }

    let game: ChessComGame = response
        .json()
        .await
        .map_err(|e| OracleError::Fetch(format!("chess.com response parse failed: {e}")))?;

    Ok(normalize_result(&game))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_game_ids_are_accepted() {
        assert!(is_valid_game_id("1234567890"));
        assert!(is_valid_game_id("1"));
    }

    #[test]
    fn invalid_game_ids_are_rejected() {
        assert!(!is_valid_game_id(""));
        assert!(!is_valid_game_id("abc123"));
        assert!(!is_valid_game_id("12-34"));
        assert!(!is_valid_game_id("12 34"));
    }

    #[test]
    fn normalizes_white_win() {
        let game = ChessComGame {
            white: ChessComPlayer {
                result: "win".to_string(),
            },
            black: ChessComPlayer {
                result: "resigned".to_string(),
            },
        };
        assert_eq!(normalize_result(&game), ChessComResult::WhiteWin);
    }

    #[test]
    fn normalizes_black_win() {
        let game = ChessComGame {
            white: ChessComPlayer {
                result: "checkmated".to_string(),
            },
            black: ChessComPlayer {
                result: "win".to_string(),
            },
        };
        assert_eq!(normalize_result(&game), ChessComResult::BlackWin);
    }

    #[test]
    fn normalizes_draw() {
        let game = ChessComGame {
            white: ChessComPlayer {
                result: "agreed".to_string(),
            },
            black: ChessComPlayer {
                result: "agreed".to_string(),
            },
        };
        assert_eq!(normalize_result(&game), ChessComResult::Draw);
    }
}