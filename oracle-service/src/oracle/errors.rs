use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChessComError {
    #[error("invalid chess.com game id")]
    InvalidGameId,

    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("request timed out")]
    Timeout,

    #[error("rate limited by chess.com; retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },

    #[error("concurrency limit reached for chess.com")]
    ConcurrencyLimitReached,

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

    #[error("rate limited by lichess; retry after {retry_after:?}")]
    RateLimited { retry_after: Duration },

    #[error("concurrency limit reached for lichess")]
    ConcurrencyLimitReached,

    #[error("lichess returned non-success status: {status}")]
    HttpStatus { status: reqwest::StatusCode },

    #[error("game not found")]
    GameNotFound,

    #[error("game result is not available yet")]
    GameNotFinished,

    #[error("game is missing result fields or is in an unknown state")]
    InvalidResponse,
}

/// Top-level oracle service error, encompassing config, transport, XDR
/// construction, simulation, and transaction submission failures.
#[derive(Debug, Error)]
pub enum OracleServiceError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("RPC error: {0}")]
    RpcError(String),

    #[error("simulate transaction error: {0}")]
    SimulateError(String),

    #[error("send transaction error: {0}")]
    SendError(String),

    #[error("transaction failed on-chain: {0}")]
    TxFailed(String),

    #[error("XDR encoding/decoding error: {0}")]
    XdrError(String),

    #[error("queue I/O error: {0}")]
    QueueIo(String),
}
