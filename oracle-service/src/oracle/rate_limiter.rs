/// Token-bucket rate limiter with configurable burst and sustained-rate limits.
///
/// ## Algorithm
///
/// Tokens refill at `rate` tokens-per-second continuously.  At most `capacity`
/// tokens may be stored at once (burst ceiling).  Each call to
/// [`TokenBucket::acquire`] consumes one token, sleeping until one is
/// available when the bucket is empty.
///
/// This is **shared** and **clone-safe**: every clone of a [`RateLimiter`]
/// points at the same inner state, so it is correct to hand one `RateLimiter`
/// to many concurrent tasks.
///
/// ## Example
///
/// ```rust
/// use oracle_service::oracle::rate_limiter::{RateLimiter, RateLimiterConfig};
/// use std::time::Duration;
///
/// // 10 req/s sustained, burst of 20
/// let cfg = RateLimiterConfig { capacity: 20, refill_rate: 10.0 };
/// let limiter = RateLimiter::new(cfg);
/// ```
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;
use tokio::time::sleep;

/// Configuration knobs for a [`RateLimiter`].
#[derive(Debug, Clone, Copy)]
pub struct RateLimiterConfig {
    /// Maximum number of tokens that may accumulate (burst ceiling).
    pub capacity: u32,
    /// Tokens added per second (sustained throughput ceiling).
    pub refill_rate: f64,
}

impl RateLimiterConfig {
    /// 30 req/min (0.5 req/s), burst of 1 — matches Chess.com public API.
    pub fn chess_com_default() -> Self {
        Self {
            capacity: 5,
            refill_rate: 0.5,
        }
    }

    /// Lichess has no documented hard cap; we apply a conservative 60 req/min
    /// (1 req/s) with a small burst allowance.
    pub fn lichess_default() -> Self {
        Self {
            capacity: 10,
            refill_rate: 1.0,
        }
    }
}

struct BucketState {
    /// Current token count (fractional so sub-second refills are tracked).
    tokens: f64,
    /// When the bucket was last refilled.
    last_refill: Instant,
    config: RateLimiterConfig,
}

impl BucketState {
    fn new(config: RateLimiterConfig) -> Self {
        // Start full so the first burst is immediately available.
        Self {
            tokens: config.capacity as f64,
            last_refill: Instant::now(),
            config,
        }
    }

    /// Refill based on elapsed real time, then either consume a token
    /// immediately or return how long until one is available.
    fn refill_and_next_available(&mut self) -> Duration {
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_refill).as_secs_f64();
        self.tokens =
            (self.tokens + elapsed * self.config.refill_rate).min(self.config.capacity as f64);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Duration::ZERO
        } else {
            // How long until we have a full token?  This is the precise
            // amount of time to sleep before the next request can proceed.
            let deficit = 1.0 - self.tokens;
            let wait_secs = deficit / self.config.refill_rate;
            Duration::from_secs_f64(wait_secs)
        }
    }
}

/// A cloneable, async-safe token-bucket rate limiter.
///
/// Cloning is cheap — all clones share the same underlying bucket.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<BucketState>>,
}

impl RateLimiter {
    /// Create a new limiter with the given configuration.
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BucketState::new(config))),
        }
    }

    /// Block (async-sleep) until a token is available, then consume it.
    ///
    /// This does **not** spin; it sleeps the exact amount needed so the
    /// calling task yields to the runtime while waiting.
    pub async fn acquire(&self) {
        loop {
            let wait = {
                let mut state = self.inner.lock().await;
                state.refill_and_next_available()
            };
            if wait.is_zero() {
                return;
            }
            sleep(wait).await;
        }
    }

    /// Try to acquire a token without waiting.
    ///
    /// Returns `true` if a token was available and consumed, `false` if the
    /// bucket is empty (i.e. the caller is being rate-limited right now).
    pub async fn try_acquire(&self) -> bool {
        let mut state = self.inner.lock().await;
        let wait = state.refill_and_next_available();
        wait.is_zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Instant as TokioInstant;

    #[tokio::test]
    async fn initial_burst_is_immediately_available() {
        let cfg = RateLimiterConfig {
            capacity: 5,
            refill_rate: 1.0,
        };
        let limiter = RateLimiter::new(cfg);
        // All 5 burst tokens should be available without sleeping.
        for _ in 0..5 {
            assert!(
                limiter.try_acquire().await,
                "burst token should be available"
            );
        }
        // 6th should not be — bucket is empty.
        assert!(
            !limiter.try_acquire().await,
            "bucket should be empty after burst"
        );
    }

    #[tokio::test]
    async fn acquire_sleeps_until_token_available() {
        let cfg = RateLimiterConfig {
            capacity: 1,
            refill_rate: 10.0, // 1 token per 100 ms
        };
        let limiter = RateLimiter::new(cfg);
        // Drain the single burst token.
        assert!(limiter.try_acquire().await);

        let start = TokioInstant::now();
        limiter.acquire().await; // must wait ~100 ms
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(90),
            "should have waited for refill, elapsed={elapsed:?}"
        );
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let cfg = RateLimiterConfig {
            capacity: 2,
            refill_rate: 0.1,
        };
        let a = RateLimiter::new(cfg);
        let b = a.clone();

        assert!(a.try_acquire().await);
        assert!(b.try_acquire().await); // both drain the shared bucket
        assert!(!a.try_acquire().await, "bucket should be shared and empty");
    }

    #[tokio::test]
    async fn rate_does_not_exceed_capacity() {
        let cfg = RateLimiterConfig {
            capacity: 3,
            refill_rate: 1_000.0, // very fast refill
        };
        let limiter = RateLimiter::new(cfg);
        // Sleep a bit so the bucket has plenty of time to overfill — it
        // must be clamped to capacity.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut acquired = 0usize;
        for _ in 0..10 {
            if limiter.try_acquire().await {
                acquired += 1;
            }
        }
        // After sleeping the bucket is at `capacity` (3), not more.
        assert!(acquired <= 3 + 1, "acquired={acquired} should be ≤ capacity+1 (off-by-refill)");
    }
}
