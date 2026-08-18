//! Lightweight Web Application Firewall (WAF) middleware.
//!
//! Detects and blocks three categories of suspicious traffic before they
//! reach any handler:
//!
//! | Check                     | Trigger                              | Status |
//! |---------------------------|--------------------------------------|--------|
//! | Burst rate                | > 20 requests/second from one IP     | 429    |
//! | Oversized body            | `Content-Length` > 1 MiB             | 413    |
//! | Long URI                  | URI path > 2 048 characters          | 400    |
//! | Null byte in URI          | `\0` anywhere in the URI             | 400    |
//!
//! ## Usage
//!
//! ```rust,ignore
//! let waf_state = WafState::new();
//! let app = Router::new()
//!     .route("/health", get(handler))
//!     .layer(axum::middleware::from_fn_with_state(
//!         waf_state,
//!         oracle_service::middleware::waf::waf_middleware,
//!     ));
//! ```

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::net::SocketAddr;
use tokio::sync::Mutex;
use tracing::warn;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum allowed requests per second from a single IP before blocking.
const MAX_RPS: u64 = 20;

/// Maximum allowed `Content-Length` in bytes (1 MiB).
const MAX_BODY_BYTES: u64 = 1_048_576;

/// Maximum allowed URI length in bytes.
const MAX_URI_LEN: usize = 2048;

/// Window size for burst tracking.
const BURST_WINDOW: Duration = Duration::from_secs(1);

/// How long to keep a per-IP burst entry after it was last seen.
const BURST_STALE_AFTER: Duration = Duration::from_secs(30);

/// How often the cleanup task runs.
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

// ── Per-IP burst tracking ─────────────────────────────────────────────────────

struct BurstEntry {
    /// Number of requests seen in the current window.
    count: u64,
    /// When the current 1-second window started.
    window_start: Instant,
    /// Last time this entry was touched (for stale pruning).
    last_seen: Instant,
}

impl BurstEntry {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            count: 0,
            window_start: now,
            last_seen: now,
        }
    }

    /// Increment the counter, resetting the window if it has elapsed.
    ///
    /// Returns `true` if the request should be blocked (> MAX_RPS).
    fn record_and_check(&mut self) -> bool {
        let now = Instant::now();
        if now.saturating_duration_since(self.window_start) >= BURST_WINDOW {
            // New window — reset counter.
            self.count = 0;
            self.window_start = now;
        }
        self.count += 1;
        self.last_seen = now;
        self.count > MAX_RPS
    }
}

// ── Shared state ──────────────────────────────────────────────────────────────

/// Shared state for the WAF middleware.
///
/// Cheap to clone — all clones reference the same inner map.
#[derive(Clone)]
pub struct WafState {
    burst: Arc<Mutex<HashMap<String, BurstEntry>>>,
}

impl WafState {
    /// Create a new state and spawn the background cleanup task.
    pub fn new() -> Self {
        let burst: Arc<Mutex<HashMap<String, BurstEntry>>> = Arc::new(Mutex::new(HashMap::new()));

        let burst_clone = Arc::clone(&burst);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(CLEANUP_INTERVAL);
            loop {
                ticker.tick().await;
                let mut guard = burst_clone.lock().await;
                let now = Instant::now();
                guard.retain(|_, e| now.saturating_duration_since(e.last_seen) < BURST_STALE_AFTER);
            }
        });

        Self { burst }
    }
}

impl Default for WafState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Middleware ────────────────────────────────────────────────────────────────

/// Axum middleware function for WAF checks.
pub async fn waf_middleware(
    State(state): State<WafState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // ── 1. URI sanity checks (no I/O, very cheap) ─────────────────────────
    let uri = req.uri().to_string();

    if uri.len() > MAX_URI_LEN {
        warn!(uri_len = uri.len(), "WAF: URI too long");
        return bad_request(
            "uri_too_long",
            "Request URI exceeds maximum allowed length.",
        );
    }

    if uri.contains('\0') {
        warn!("WAF: null byte in URI");
        return bad_request("invalid_uri", "Request URI contains invalid characters.");
    }

    // ── 2. Body size check ────────────────────────────────────────────────
    if let Some(content_length) = req
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
    {
        if content_length > MAX_BODY_BYTES {
            warn!(content_length, "WAF: request body too large");
            return payload_too_large();
        }
    }

    // ── 3. Burst check (requires shared state) ────────────────────────────
    let ip = extract_ip(&req);
    {
        let mut guard = state.burst.lock().await;
        let entry = guard.entry(ip.clone()).or_insert_with(BurstEntry::new);
        if entry.record_and_check() {
            warn!(ip = %ip, "WAF: burst rate exceeded");
            drop(guard);
            return burst_blocked();
        }
    }

    next.run(req).await
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn extract_ip(req: &Request<Body>) -> String {
    if let Some(xff) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
    {
        return xff.to_string();
    }
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

fn bad_request(code: &str, message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({
            "error": code,
            "message": message,
        })),
    )
        .into_response()
}

fn payload_too_large() -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(serde_json::json!({
            "error": "payload_too_large",
            "message": format!(
                "Request body exceeds the maximum allowed size of {} bytes.",
                MAX_BODY_BYTES
            ),
        })),
    )
        .into_response()
}

fn burst_blocked() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(serde_json::json!({
            "error": "burst_rate_exceeded",
            "message": format!(
                "Too many requests from this IP. Maximum burst is {} requests/second.",
                MAX_RPS
            ),
        })),
    )
        .into_response()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_entry_allows_up_to_max_rps() {
        let mut e = BurstEntry::new();
        for _ in 0..MAX_RPS {
            assert!(!e.record_and_check(), "should not block within limit");
        }
        // The (MAX_RPS + 1)-th request in the same window should be blocked.
        assert!(e.record_and_check(), "should block when over limit");
    }

    #[test]
    fn burst_entry_resets_after_window() {
        let mut e = BurstEntry::new();
        // Fill the window.
        for _ in 0..=MAX_RPS {
            e.record_and_check();
        }
        // Simulate a new window by backdating the window_start.
        e.window_start = Instant::now() - Duration::from_secs(2);
        // First request in the new window should be allowed.
        assert!(!e.record_and_check(), "should reset counter after window");
    }
}
