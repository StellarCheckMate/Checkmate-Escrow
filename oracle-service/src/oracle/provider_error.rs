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

/// Backoff for an outage ([`ProviderError::Unavailable`]): retry sooner than a
/// rate limit, since the provider may recover at any moment and the registry
/// already fails over to the next provider.
const UNAVAILABLE_BACKOFF: std::time::Duration = std::time::Duration::from_secs(5);

/// Backoff when the per-provider concurrency gate is full
/// ([`ProviderError::ConcurrencyLimitReached`]): retry quickly, the gate
/// clears as in-flight requests complete.
const CONCURRENCY_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);

/// Backoff when a game has not reached a terminal state yet
/// ([`ProviderError::GameNotFinished`]): give the game time to complete before
/// polling the provider again.
const NOT_FINISHED_BACKOFF: std::time::Duration = std::time::Duration::from_secs(30);

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

    /// Recommended backoff before retrying on the **same** provider.
    ///
    /// Different error classes warrant different delays, which is exactly
    /// what lets a caller tell a rate limit apart from an outage and apply
    /// the correct backoff instead of a single one-size-fits-all value
    /// (issue #1974):
    ///
    /// | variant                  | backoff                            |
    /// |--------------------------|------------------------------------|
    /// | `RateLimited`            | the server's `retry_after`         |
    /// | `Unavailable`            | [`UNAVAILABLE_BACKOFF`]            |
    /// | `ConcurrencyLimitReached`| [`CONCURRENCY_BACKOFF`]            |
    /// | `GameNotFinished`        | [`NOT_FINISHED_BACKOFF`]            |
    /// | `AllProvidersFailed`     | the largest of the inner backoffs   |
    /// | terminal errors          | [`Duration::ZERO`] (do not retry)  |
    ///
    /// In particular, HTTP 429 (`RateLimited`) and a generic network error
    /// (`Unavailable`) map to **distinct** backoffs, so retry logic can
    /// distinguish them.
    pub fn recommended_backoff(&self) -> std::time::Duration {
        match self {
            ProviderError::RateLimited { retry_after, .. } => *retry_after,
            ProviderError::Unavailable { .. } => UNAVAILABLE_BACKOFF,
            ProviderError::ConcurrencyLimitReached { .. } => CONCURRENCY_BACKOFF,
            ProviderError::GameNotFinished => NOT_FINISHED_BACKOFF,
            ProviderError::AllProvidersFailed { errors, .. } => errors
                .iter()
                .map(ProviderError::recommended_backoff)
                .max()
                .unwrap_or(UNAVAILABLE_BACKOFF),
            // Terminal errors: retrying yields the same failure, so no backoff.
            ProviderError::InvalidGameId(_)
            | ProviderError::GameNotFound
            | ProviderError::InvalidResponse { .. } => std::time::Duration::ZERO,
        }
    }
}

// ── Conversion helpers ────────────────────────────────────────────────────────

use crate::oracle::errors::{ChessComError, LichessError};

impl From<ChessComError> for ProviderError {
    fn from(e: ChessComError) -> Self {
        match e {
            ChessComError::InvalidGameId => ProviderError::InvalidGameId("chess.com".to_string()),
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

impl From<LichessError> for ProviderError {
    fn from(e: LichessError) -> Self {
        match e {
            LichessError::InvalidGameId => ProviderError::InvalidGameId("lichess".to_string()),
            LichessError::GameNotFound => ProviderError::GameNotFound,
            LichessError::GameNotFinished => ProviderError::GameNotFinished,
            LichessError::InvalidResponse => ProviderError::InvalidResponse {
                provider: "lichess",
                detail: "missing or unrecognised result field".to_string(),
            },
            LichessError::Timeout => ProviderError::Unavailable {
                provider: "lichess",
                reason: "request timed out".to_string(),
            },
            LichessError::RateLimited { retry_after } => ProviderError::RateLimited {
                provider: "lichess",
                retry_after,
            },
            LichessError::ConcurrencyLimitReached => ProviderError::ConcurrencyLimitReached {
                provider: "lichess",
            },
            LichessError::HttpStatus { status } if status.as_u16() == 429 => {
                ProviderError::RateLimited {
                    provider: "lichess",
                    retry_after: std::time::Duration::from_secs(60),
                }
            }
            LichessError::HttpStatus { status } => ProviderError::Unavailable {
                provider: "lichess",
                reason: format!("HTTP {status}"),
            },
            LichessError::Http(e) => ProviderError::Unavailable {
                provider: "lichess",
                reason: e.to_string(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    //! Tests for the backoff mapping that distinguishes a rate limit (HTTP 429)
    //! from a generic network outage (issue #1974).
    use super::*;
    use std::time::Duration;

    /// A rate-limit error carries the server's `retry_after` and the backoff
    /// must honour it rather than applying a fixed outage delay.
    #[test]
    fn rate_limited_backoff_uses_retry_after() {
        let err = ProviderError::RateLimited {
            provider: "lichess",
            retry_after: Duration::from_secs(120),
        };
        assert_eq!(err.recommended_backoff(), Duration::from_secs(120));
        assert!(err.is_rate_limited());
        assert!(!err.should_failover(), "rate limit retries on the same provider");
    }

    /// An outage maps to the short `UNAVAILABLE_BACKOFF`, not the rate-limit
    /// delay, and triggers an immediate failover.
    #[test]
    fn unavailable_backoff_is_short_default() {
        let err = ProviderError::Unavailable {
            provider: "lichess",
            reason: "connection reset".to_string(),
        };
        assert_eq!(err.recommended_backoff(), UNAVAILABLE_BACKOFF);
        assert!(err.should_failover(), "outage triggers failover");
    }

    /// The core #1974 assertion: HTTP 429 and a network error map to distinct
    /// variants *and* distinct backoffs, so retry logic can tell them apart
    /// and apply the correct delay.
    #[test]
    fn http_429_and_network_error_map_to_distinct_backoffs() {
        let rate_limited = ProviderError::RateLimited {
            provider: "lichess",
            retry_after: Duration::from_secs(30),
        };
        let network = ProviderError::Unavailable {
            provider: "lichess",
            reason: "connection reset".to_string(),
        };

        // Distinct variants: a rate limit is retriable on the same provider,
        // an outage triggers failover.
        assert!(!rate_limited.should_failover());
        assert!(network.should_failover());

        // Distinct backoffs: rate limit waits the server's 30s, outage uses the
        // short default.
        assert_eq!(rate_limited.recommended_backoff(), Duration::from_secs(30));
        assert_eq!(network.recommended_backoff(), UNAVAILABLE_BACKOFF);
        assert_ne!(
            rate_limited.recommended_backoff(),
            network.recommended_backoff(),
            "rate-limit and outage must map to different backoffs"
        );
    }

    #[test]
    fn concurrency_backoff_is_short() {
        let err = ProviderError::ConcurrencyLimitReached {
            provider: "lichess",
        };
        assert_eq!(err.recommended_backoff(), CONCURRENCY_BACKOFF);
    }

    #[test]
    fn game_not_finished_backoff() {
        let err = ProviderError::GameNotFinished;
        assert_eq!(err.recommended_backoff(), NOT_FINISHED_BACKOFF);
        assert!(!err.is_terminal(), "not-finished is retriable, not terminal");
    }

    /// Terminal errors must not be retried at all — zero backoff.
    #[test]
    fn terminal_errors_have_zero_backoff() {
        assert_eq!(ProviderError::GameNotFound.recommended_backoff(), Duration::ZERO);
        assert_eq!(
            ProviderError::InvalidGameId("bad".to_string()).recommended_backoff(),
            Duration::ZERO
        );
        assert_eq!(
            ProviderError::InvalidResponse {
                provider: "lichess",
                detail: "unparsable".to_string(),
            }
            .recommended_backoff(),
            Duration::ZERO
        );
        for e in [
            ProviderError::GameNotFound,
            ProviderError::InvalidGameId("bad".to_string()),
            ProviderError::InvalidResponse {
                provider: "lichess",
                detail: "unparsable".to_string(),
            },
        ] {
            assert!(e.is_terminal(), "expected terminal variant");
        }
    }

    /// `AllProvidersFailed` aggregates the inner errors; its backoff is the
    /// largest of theirs so the caller waits long enough for the worst case.
    #[test]
    fn all_providers_failed_uses_max_inner_backoff() {
        let err = ProviderError::AllProvidersFailed {
            count: 2,
            errors: vec![
                ProviderError::Unavailable {
                    provider: "lichess",
                    reason: "down".to_string(),
                },
                ProviderError::RateLimited {
                    provider: "chess.com",
                    retry_after: Duration::from_secs(90),
                },
            ],
        };
        // max(UNAVAILABLE_BACKOFF=5s, 90s) = 90s
        assert_eq!(err.recommended_backoff(), Duration::from_secs(90));
    }

    /// A `RateLimited` whose `retry_after` is shorter than the outage backoff
    /// is still respected verbatim — the mapping never silently inflates the
    /// server's signal.
    #[test]
    fn rate_limited_backoff_respects_short_retry_after() {
        let err = ProviderError::RateLimited {
            provider: "chess.com",
            retry_after: Duration::from_secs(1),
        };
        assert_eq!(err.recommended_backoff(), Duration::from_secs(1));
    }
}
