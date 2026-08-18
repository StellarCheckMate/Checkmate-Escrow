//! Per-IP and per-API-key token-bucket rate limiting middleware.
//!
//! ## Limits
//!
//! | Key type | Capacity | Refill rate         |
//! |----------|----------|---------------------|
//! | IP       | 100      | 100 tokens / 60 s   |
//! | API key  | 1 000    | 1 000 tokens / 60 s |
//!
//! ## Response headers (always present on allowed responses)
//!
//! | Header                  | Meaning                                    |
//! |-------------------------|--------------------------------------------|
//! | `X-RateLimit-Limit`     | Bucket capacity                            |
//! | `X-RateLimit-Remaining` | Floor of remaining tokens                  |
//! | `X-RateLimit-Reset`     | Unix timestamp when the next token refills |
//!
//! On `429 Too Many Requests`, a `Retry-After` header is also included.
//!
//! ## Stale-entry cleanup
//!
//! A background task launched inside [`RateLimitState::new`] walks the maps
//! every 60 seconds and removes entries that have not been touched for more
//! than 5 minutes.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let state = RateLimitState::new();
//! let app = Router::new()
//!     .route("/health", get(handler))
//!     .layer(axum::middleware::from_fn_with_state(
//!         state,
//!         oracle_service::middleware::rate_limit::rate_limit_middleware,
//!     ));
//! ```

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::net::SocketAddr;
use tokio::sync::Mutex;
use tracing::debug;

// ── Constants ─────────────────────────────────────────────────────────────────

const IP_CAPACITY: f64 = 100.0;
const IP_REFILL_PER_SEC: f64 = 100.0 / 60.0;

const KEY_CAPACITY: f64 = 1000.0;
const KEY_REFILL_PER_SEC: f64 = 1000.0 / 60.0;

/// Idle duration before a bucket entry is considered stale.
const STALE_AFTER: Duration = Duration::from_secs(300); // 5 minutes

/// How often the cleanup task runs.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

// ── Token bucket ──────────────────────────────────────────────────────────────

struct Bucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: std::time::Instant,
    last_seen: std::time::Instant,
}

impl Bucket {
    fn new(capacity: f64, refill_rate: f64) -> Self {
        let now = std::time::Instant::now();
        Self {
            tokens: capacity, // start full so the first burst is immediately available
            capacity,
            refill_rate,
            last_refill: now,
            last_seen: now,
        }
    }

    /// Refill tokens proportional to elapsed time, then attempt to consume one.
    ///
    /// Returns `Ok(remaining_tokens)` on success, or
    /// `Err(retry_after_secs)` when the bucket is exhausted.
    fn consume(&mut self) -> Result<f64, f64> {
        let now = std::time::Instant::now();
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
        self.last_seen = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(self.tokens)
        } else {
            let deficit = 1.0 - self.tokens;
            let retry_after = deficit / self.refill_rate;
            Err(retry_after)
        }
    }

    /// Unix timestamp (seconds) at which the next token will be available.
    fn reset_unix(&self) -> u64 {
        let deficit = (1.0 - self.tokens).max(0.0);
        let secs_until = (deficit / self.refill_rate).ceil() as u64;
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_add(secs_until)
    }
}

// ── Shared state ──────────────────────────────────────────────────────────────

type BucketMap = Arc<Mutex<HashMap<String, Bucket>>>;

/// Shared state for the rate-limit middleware.
///
/// Cheap to clone — all clones share the same underlying maps.
#[derive(Clone)]
pub struct RateLimitState {
    ip_buckets: BucketMap,
    key_buckets: BucketMap,
}

impl RateLimitState {
    /// Create a new state and spawn the background stale-entry cleanup task.
    pub fn new() -> Self {
        let ip_buckets: BucketMap = Arc::new(Mutex::new(HashMap::new()));
        let key_buckets: BucketMap = Arc::new(Mutex::new(HashMap::new()));

        let ip_clone = Arc::clone(&ip_buckets);
        let key_clone = Arc::clone(&key_buckets);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(CLEANUP_INTERVAL);
            loop {
                ticker.tick().await;
                prune_stale(&ip_clone).await;
                prune_stale(&key_clone).await;
            }
        });

        Self {
            ip_buckets,
            key_buckets,
        }
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

async fn prune_stale(map: &BucketMap) {
    let mut guard = map.lock().await;
    let now = std::time::Instant::now();
    guard.retain(|_, b| now.saturating_duration_since(b.last_seen) < STALE_AFTER);
    debug!("rate-limit cleanup: {} buckets remaining", guard.len());
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Axum middleware function for rate limiting.
///
/// Checks the per-IP bucket first. If the request carries an `X-Api-Key`
/// header, the per-key bucket is checked afterwards.
pub async fn rate_limit_middleware(
    State(state): State<RateLimitState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // ── Determine client IP ───────────────────────────────────────────────
    let ip_str = extract_ip(&req);

    // ── Optional API key ──────────────────────────────────────────────────
    let api_key: Option<String> = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // ── Check IP bucket ───────────────────────────────────────────────────
    let (ip_remaining, ip_reset) = {
        let mut map = state.ip_buckets.lock().await;
        let bucket = map
            .entry(ip_str.clone())
            .or_insert_with(|| Bucket::new(IP_CAPACITY, IP_REFILL_PER_SEC));

        match bucket.consume() {
            Ok(rem) => (rem, bucket.reset_unix()),
            Err(retry_after) => {
                let reset = bucket.reset_unix();
                drop(map);
                return too_many_requests(IP_CAPACITY as u64, 0, reset, retry_after);
            }
        }
    };

    // ── Check API-key bucket (if key is present) ──────────────────────────
    let (limit, remaining, reset) = if let Some(key) = api_key {
        let mut map = state.key_buckets.lock().await;
        let bucket = map
            .entry(key)
            .or_insert_with(|| Bucket::new(KEY_CAPACITY, KEY_REFILL_PER_SEC));

        match bucket.consume() {
            Ok(rem) => (KEY_CAPACITY as u64, rem as u64, bucket.reset_unix()),
            Err(retry_after) => {
                let reset = bucket.reset_unix();
                drop(map);
                return too_many_requests(KEY_CAPACITY as u64, 0, reset, retry_after);
            }
        }
    } else {
        (IP_CAPACITY as u64, ip_remaining as u64, ip_reset)
    };

    // ── Pass request through and attach headers ────────────────────────────
    let mut response = next.run(req).await;
    attach_rate_limit_headers(response.headers_mut(), limit, remaining, reset);
    response
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extract_ip(req: &Request<Body>) -> String {
    // Respect X-Forwarded-For when behind a proxy (take the first address).
    if let Some(xff) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return xff.to_string();
    }
    // Fall back to the TCP socket address injected by axum's `ConnectInfo`.
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn attach_rate_limit_headers(headers: &mut HeaderMap, limit: u64, remaining: u64, reset: u64) {
    if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert("x-ratelimit-limit", v);
    }
    if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
        headers.insert("x-ratelimit-remaining", v);
    }
    if let Ok(v) = HeaderValue::from_str(&reset.to_string()) {
        headers.insert("x-ratelimit-reset", v);
    }
}

fn too_many_requests(limit: u64, remaining: u64, reset: u64, retry_after: f64) -> Response {
    let retry_secs = retry_after.ceil() as u64;
    let body = serde_json::json!({
        "error": "rate_limit_exceeded",
        "message": format!(
            "Rate limit of {} requests/min exceeded. Retry after {} seconds.",
            limit, retry_secs
        ),
        "retry_after": retry_secs,
    });

    let mut resp = (StatusCode::TOO_MANY_REQUESTS, Json(body)).into_response();
    let headers = resp.headers_mut();
    attach_rate_limit_headers(headers, limit, remaining, reset);
    if let Ok(v) = HeaderValue::from_str(&retry_secs.to_string()) {
        headers.insert("retry-after", v);
    }
    resp
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_starts_full() {
        let mut b = Bucket::new(10.0, 1.0);
        // Should be able to consume 10 tokens immediately.
        for _ in 0..10 {
            assert!(b.consume().is_ok());
        }
        // 11th must fail.
        assert!(b.consume().is_err());
    }

    #[test]
    fn bucket_refills_over_time() {
        let mut b = Bucket::new(1.0, 10.0); // 10 tokens/sec → 1 token per 100ms
                                            // Drain the initial token.
        assert!(b.consume().is_ok());
        // Simulate 200ms passing by faking `last_refill`.
        b.last_refill -= Duration::from_millis(200);
        // Should have 2 tokens now (but capped at capacity 1.0, so exactly 1).
        assert!(b.consume().is_ok());
    }

    #[test]
    fn retry_after_is_positive() {
        let mut b = Bucket::new(1.0, 1.0);
        b.consume().ok(); // drain
        let err = b.consume().unwrap_err();
        assert!(err > 0.0, "retry_after must be > 0, got {err}");
    }

    #[tokio::test]
    async fn prune_removes_stale_entries() {
        let map: BucketMap = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut guard = map.lock().await;
            let mut stale = Bucket::new(10.0, 1.0);
            stale.last_seen -= Duration::from_secs(600); // 10 minutes ago
            guard.insert("stale".to_string(), stale);
            guard.insert("fresh".to_string(), Bucket::new(10.0, 1.0));
        }
        prune_stale(&map).await;
        let guard = map.lock().await;
        assert!(!guard.contains_key("stale"), "stale entry should be pruned");
        assert!(guard.contains_key("fresh"), "fresh entry should remain");
    }
}
