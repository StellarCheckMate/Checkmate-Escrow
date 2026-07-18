/// Unified error type for the pluggable provider layer.
///
/// The key design goal is **backpressure signaling**: the oracle pipeline must
/// be able to tell the difference between:
///
/// - [`ProviderError::RateLimited`] — the provider is healthy but we are
///   sending requests too fast.  The pipeline should **back off and retry
///   later** (the provider will eventually accept the request).
///
/// - [`ProviderError::Unavailable`] — the provider is down or returning
///   persistent server errors.  The pipeline should **fail over to the next
///   provider** immediately rather than waiting.
///
/// - All other variants — logical errors (bad game ID, game not finished,
///   etc.) that should **not** trigger a failover.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProviderError {
    // ── Backpressure signals ──────────────────────────────────────────────

    /// The client-side token bucket is exhausted, **or** the upstream
    /// provider returned HTTP 429.  The caller should wait before retrying.
    ///
    /// `retry_after` is the minimum recommended wait derived from the
    /// client-side rate limiter or a `Retry-After` response header.
    #[error("rate limited by {provider}; retry after {retry_after:?}")]
    RateLimited {
        provider: &'static str,
        retry_after: std::time::Duration,
    },

    /// The provider is unreachable (connection error, persistent 5xx, or
    /// timeout).  Fail over to the next provider.
    #[error("provider {provider} unavailable: {reason}")]
    Unavailable {
        provider: &'static str,
        reason: String,
    },

    // ── Concurrency gate ─────────────────────────────────────────────────

    /// The per-provider concurrency semaphore is exhausted.  Too many
    /// in-flight requests right now; the caller should queue or back off.
    #[error("concurrency limit reached for {provider}")]
    ConcurrencyLimitReached { provider: &'static str },

    // ── Logical / validation errors ───────────────────────────────────────

    /// The supplied game ID did not pass format validation.
    #[error("invalid game id: {0}")]
    InvalidGameId(String),

    /// The game exists but has not yet reached a terminal state.
    #[error("game not finished yet")]
    GameNotFinished,

    /// The game was not found on the provider (invalid or deleted ID).
    #[error("game not found")]
    GameNotFound,

    /// The provider returned a response that could not be parsed.
    #[error("invalid response from {provider}: {detail}")]
    InvalidResponse {
        provider: &'static str,
        detail: String,
    },

    // ── Multi-provider failover exhaustion ────────────────────────────────

    /// All providers in the registry were tried and all failed.  The inner
    /// `Vec` holds one error per provider in precedence order.
    #[error("all providers exhausted ({count} tried)")]
    AllProvidersFailed {
        count: usize,
        errors: Vec<ProviderError>,
    },
}

impl ProviderError {
    /// Returns `true` for errors that should trigger an immediate failover
    /// to the next provider rather than a backoff-then-retry on the same one.
    pub fn should_failover(&self) -> bool {
        matches!(
            self,
            ProviderError::Unavailable { .. } | ProviderError::ConcurrencyLimitReached { .. }
        )
    }

    /// Returns `true` when the client should back off before retrying on
    /// the same provider.
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, ProviderError::RateLimited { .. })
    }

    /// Returns `true` when the provider is busy but healthy; the caller
    /// should queue or back off rather than treating it as a terminal failure.
    pub fn is_concurrency_limited(&self) -> bool {
        matches!(self, ProviderError::ConcurrencyLimitReached { .. })
    }

    /// Returns `true` for terminal errors that should not be retried at all
    /// (wrong game ID, game not found, etc.).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ProviderError::InvalidGameId(_)
                | ProviderError::GameNotFound
                | ProviderError::InvalidResponse { .. }
        )
    }
}

// ── Conversion helpers ────────────────────────────────────────────────────────

use crate::oracle::errors::ChessComError;

impl From<ChessComError> for ProviderError {
    fn from(e: ChessComError) -> Self {
        match e {
            ChessComError::InvalidGameId => {
                ProviderError::InvalidGameId("chess.com".to_string())
            }
            ChessComError::GameNotFound => ProviderError::GameNotFound,
            ChessComError::GameNotFinished => ProviderError::GameNotFinished,
            ChessComError::InvalidResponse => ProviderError::InvalidResponse {
                provider: "chess.com",
                detail: "missing or unrecognised result field".to_string(),
            },
            ChessComError::Timeout => ProviderError::Unavailable {
                provider: "chess.com",
                reason: "request timed out".to_string(),
            },
            ChessComError::RateLimited { retry_after } => ProviderError::RateLimited {
                provider: "chess.com",
                retry_after,
            },
            ChessComError::ConcurrencyLimitReached => ProviderError::ConcurrencyLimitReached {
                provider: "chess.com",
            },
            ChessComError::HttpStatus { status } if status.as_u16() == 429 => {
                ProviderError::RateLimited {
                    provider: "chess.com",
                    retry_after: std::time::Duration::from_secs(60),
                }
            }
            ChessComError::HttpStatus { status }
                if status.is_server_error() || status.as_u16() == 503 =>
            {
                ProviderError::Unavailable {
                    provider: "chess.com",
                    reason: format!("HTTP {status}"),
                }
            }
            ChessComError::HttpStatus { status } => ProviderError::Unavailable {
                provider: "chess.com",
                reason: format!("HTTP {status}"),
            },
            ChessComError::Http(e) => ProviderError::Unavailable {
                provider: "chess.com",
                reason: e.to_string(),
            },
        }
    }
}
