//! Integration tests for the incoming rate-limiting and WAF middleware.
//!
//! Each test spins up a real Axum server bound to an OS-assigned port, sends
//! HTTP requests via `reqwest`, and asserts on the status codes / headers.
//!
//! Rate-limit tests use a server with only the rate-limit layer (no WAF) so
//! that burst-rate WAF checks don't interfere with the 100 req/min IP bucket
//! tests. WAF tests use a server with only the WAF layer.

use axum::{routing::get, Router};
use tokio::net::TcpListener;

use oracle_service::middleware::{rate_limit::RateLimitState, waf::WafState};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Spawn a server with only the rate-limit middleware (no WAF).
async fn spawn_rate_limit_only_server(rate_state: RateLimitState) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .layer(axum::middleware::from_fn_with_state(
            rate_state,
            oracle_service::middleware::rate_limit::rate_limit_middleware,
        ));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{}", addr.port())
}

/// Spawn a server with only the WAF middleware (no rate limiter).
async fn spawn_waf_only_server(waf_state: WafState) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .layer(axum::middleware::from_fn_with_state(
            waf_state,
            oracle_service::middleware::waf::waf_middleware,
        ));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{}", addr.port())
}

/// Spawn a server with both middleware layers (WAF outer, rate-limit inner).
async fn spawn_full_server(rate_state: RateLimitState, waf_state: WafState) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .layer(axum::middleware::from_fn_with_state(
            waf_state,
            oracle_service::middleware::waf::waf_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            rate_state,
            oracle_service::middleware::rate_limit::rate_limit_middleware,
        ));
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://127.0.0.1:{}", addr.port())
}

// ── Rate-limit tests ──────────────────────────────────────────────────────────

/// After 100 successful requests the 101st from the same IP must return 429.
///
/// Uses rate-limit-only server to avoid WAF burst interference.
/// Uses a unique `X-Forwarded-For` IP to isolate from other tests.
#[tokio::test]
async fn test_ip_rate_limit_enforced() {
    let base = spawn_rate_limit_only_server(RateLimitState::new()).await;
    let client = reqwest::Client::new();
    // Each test uses a unique IP so parallel runs don't share buckets.
    let test_ip = "10.1.0.1";

    // First 100 requests must all succeed.
    for i in 1..=100 {
        let resp = client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", test_ip)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 200, "request {i} must succeed");
    }

    // The 101st must be rate-limited.
    let resp = client
        .get(format!("{}/ping", base))
        .header("x-forwarded-for", test_ip)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 429, "request 101 must return 429");

    // The 429 response must include Retry-After.
    assert!(
        resp.headers().contains_key("retry-after"),
        "429 response must include Retry-After header"
    );
}

/// Requests with an `X-Api-Key` header use the higher 1,000 req/min bucket.
/// Sending 100 requests must all succeed (well within 1,000/min).
#[tokio::test]
async fn test_api_key_rate_limit_higher() {
    let base = spawn_rate_limit_only_server(RateLimitState::new()).await;
    let client = reqwest::Client::new();
    let test_ip = "10.1.0.2";

    for i in 1..=100 {
        let resp = client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", test_ip)
            .header("x-api-key", "test-key-abc")
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status().as_u16(),
            200,
            "keyed request {i} should succeed"
        );
    }
}

/// Every successful response must include the three `X-RateLimit-*` headers.
#[tokio::test]
async fn test_rate_limit_headers_present() {
    let base = spawn_rate_limit_only_server(RateLimitState::new()).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/ping", base))
        .header("x-forwarded-for", "10.1.0.3")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);

    let headers = resp.headers();
    assert!(
        headers.contains_key("x-ratelimit-limit"),
        "missing X-RateLimit-Limit"
    );
    assert!(
        headers.contains_key("x-ratelimit-remaining"),
        "missing X-RateLimit-Remaining"
    );
    assert!(
        headers.contains_key("x-ratelimit-reset"),
        "missing X-RateLimit-Reset"
    );

    let remaining: u64 = headers["x-ratelimit-remaining"]
        .to_str()
        .unwrap()
        .parse()
        .expect("X-RateLimit-Remaining must be a non-negative integer");
    assert!(
        remaining < 100,
        "remaining={remaining} must be less than capacity after one request"
    );
}

/// Two different IPs should each get their own independent bucket.
#[tokio::test]
async fn test_different_ips_independent() {
    let base = spawn_rate_limit_only_server(RateLimitState::new()).await;
    let client = reqwest::Client::new();
    let ip_a = "10.2.0.1";
    let ip_b = "10.2.0.2";

    // Exhaust the IP-A bucket.
    for _ in 0..100 {
        client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", ip_a)
            .send()
            .await
            .unwrap();
    }

    // 101st request from ip_a should be rate-limited.
    let blocked = client
        .get(format!("{}/ping", base))
        .header("x-forwarded-for", ip_a)
        .send()
        .await
        .unwrap();
    assert_eq!(blocked.status().as_u16(), 429, "{ip_a} should be blocked");

    // First request from ip_b must be allowed (fresh bucket).
    let allowed = client
        .get(format!("{}/ping", base))
        .header("x-forwarded-for", ip_b)
        .send()
        .await
        .unwrap();
    assert_eq!(
        allowed.status().as_u16(),
        200,
        "{ip_b} must not be affected by {ip_a}'s exhausted bucket"
    );
}

// ── WAF tests ─────────────────────────────────────────────────────────────────

/// A POST with Content-Length > 1 MiB must be blocked by the WAF with 413.
#[tokio::test]
async fn test_waf_blocks_large_body() {
    let base = spawn_waf_only_server(WafState::new()).await;
    let client = reqwest::Client::new();

    let oversized: u64 = 1_048_577; // 1 MiB + 1 byte
    let resp = client
        .post(format!("{}/ping", base))
        .header("x-forwarded-for", "10.3.0.1")
        .header("content-length", oversized.to_string())
        .body("x")
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    assert_eq!(status, 413, "oversized body must return 413, got {status}");
}

/// A request with a URI longer than 2 048 chars must be rejected with 400.
#[tokio::test]
async fn test_waf_blocks_long_uri() {
    let base = spawn_waf_only_server(WafState::new()).await;
    let client = reqwest::Client::new();

    let long_path = "a".repeat(2049);
    let resp = client
        .get(format!("{}/{}", base, long_path))
        .header("x-forwarded-for", "10.3.0.2")
        .send()
        .await
        .unwrap();

    let status = resp.status().as_u16();
    assert_eq!(
        status, 400,
        "WAF must return 400 for overly long URI, got {status}"
    );
}

/// Rapid burst from one IP (>20 req/s) should be blocked by WAF with 429.
#[tokio::test]
async fn test_waf_blocks_burst() {
    let base = spawn_waf_only_server(WafState::new()).await;
    let client = reqwest::Client::new();
    let burst_ip = "10.3.0.3";

    // Send 21 requests as fast as possible — the 21st should be blocked.
    let mut got_blocked = false;
    for _ in 0..30 {
        let resp = client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", burst_ip)
            .send()
            .await
            .unwrap();
        if resp.status().as_u16() == 429 {
            got_blocked = true;
            break;
        }
    }
    assert!(got_blocked, "WAF must block burst traffic > 20 req/s");
}

/// Full stack: both WAF and rate-limiter active. Normal traffic must succeed.
#[tokio::test]
async fn test_full_stack_normal_traffic_passes() {
    let base = spawn_full_server(RateLimitState::new(), WafState::new()).await;
    let client = reqwest::Client::new();

    // A few normal requests should pass through both layers.
    for _ in 0..5 {
        let resp = client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", "10.4.0.1")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status().as_u16(), 200);
    }
}
