use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChessComError {
    #[error("invalid chess.com game id")]
    InvalidGameId,

    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("request timed out")]
    Timeout,

    #[error("chess.com returned non-success status: {status}")]
    HttpStatus { status: reqwest::StatusCode },

    #[error("game not found")]
    GameNotFound,

    #[error("game is missing result fields or is in an unknown state")]
    InvalidResponse,

    #[error("game result is not available yet")]
    GameNotFinished,
}

#[derive(Debug, Error)]
pub enum LichessError {
    #[error("invalid lichess game id")]
    InvalidGameId,

    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("request timed out")]
    Timeout,

    #[error("lichess returned non-success status: {status}")]
    HttpStatus { status: reqwest::StatusCode },

    #[error("game not found")]
    GameNotFound,

    #[error("game result is not available yet")]
    GameNotFinished,

    #[error("game is missing result fields or is in an unknown state")]
    InvalidResponse,
}
