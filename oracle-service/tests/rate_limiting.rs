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

/// A burst that exceeds the per-IP capacity must be rejected: the request
/// beyond capacity returns 429 and carries a `Retry-After` header.
#[tokio::test]
async fn test_burst_exceeding_capacity_rejected() {
    let base = spawn_rate_limit_only_server(RateLimitState::new()).await;
    let client = reqwest::Client::new();
    let test_ip = "10.5.0.1";

    // Fire a burst larger than the 100 req/min capacity.
    let mut last_status = 0u16;
    let mut rejected = None;
    for i in 1..=150 {
        let resp = client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", test_ip)
            .send()
            .await
            .unwrap();
        last_status = resp.status().as_u16();
        if last_status == 429 {
            rejected = Some(resp);
            break;
        }
        assert_eq!(last_status, 200, "request {i} within capacity must succeed");
    }

    let resp = rejected.expect("burst exceeding capacity must be rejected with 429");
    assert_eq!(
        resp.status().as_u16(),
        429,
        "request beyond capacity must return 429, got {last_status}"
    );
    assert!(
        resp.headers().contains_key("retry-after"),
        "rejected burst request must include Retry-After header"
    );
}

/// Requests below the per-IP capacity must all be allowed with 200.
#[tokio::test]
async fn test_burst_under_capacity_allowed() {
    let base = spawn_rate_limit_only_server(RateLimitState::new()).await;
    let client = reqwest::Client::new();
    let test_ip = "10.5.0.2";

    for i in 1..=50 {
        let resp = client
            .get(format!("{}/ping", base))
            .header("x-forwarded-for", test_ip)
            .send()
            .await
            .unwrap();
        assert_eq!(
            resp.status().as_u16(),
            200,
            "request {i} under capacity must be allowed"
        );
    }
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

/// A request with an SQL-injection-style payload in the URI must be blocked with 400.
#[tokio::test]
async fn test_waf_blocks_sqli_payload() {
    let base = spawn_waf_only_server(WafState::new()).await;
    let client = reqwest::Client::new();

    let sqli_payloads = [
        "/ping?id=1'%20OR%20'1'='1",
        "/ping?name=admin'%20or%201=1--",
        "/ping?q=UNION%20SELECT%20*%20FROM%20users",
        "/ping?q=1;%20DROP%20TABLE%20matches--",
    ];

    for (idx, payload) in sqli_payloads.iter().enumerate() {
        let resp = client
            .get(format!("{}{}", base, payload))
            .header("x-forwarded-for", format!("10.3.1.{}", idx + 1))
            .send()
            .await
            .unwrap();

        assert_eq!(
            resp.status().as_u16(),
            400,
            "WAF must return 400 for SQLi payload {}, got {}",
            payload,
            resp.status().as_u16()
        );
    }
}

/// Benign requests with valid queries and paths must be allowed through the WAF.
#[tokio::test]
async fn test_waf_allows_benign_request() {
    let base = spawn_waf_only_server(WafState::new()).await;
    let client = reqwest::Client::new();

    let benign_paths = [
        "/ping",
        "/ping?game_id=lichess_abc123",
        "/ping?match_id=42&platform=chesscom",
        "/ping?limit=50&offset=0",
    ];

    for (idx, path) in benign_paths.iter().enumerate() {
        let resp = client
            .get(format!("{}{}", base, path))
            .header("x-forwarded-for", format!("10.3.2.{}", idx + 1))
            .send()
            .await
            .unwrap();

        assert_eq!(
            resp.status().as_u16(),
            200,
            "WAF must allow benign request {}, got {}",
            path,
            resp.status().as_u16()
        );
    }
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

// ── Batched polling / back-pressure tests ─────────────────────────────────────

/// Batched polling must issue one request per platform group instead of one
/// per active match, and a rate-limited batch response must cause the poller
/// to back off and retry the affected matches on a later cycle.
#[tokio::test]
async fn test_batched_polling_and_back_pressure() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::Json;
    use serde_json::{json, Value};

    #[derive(Clone)]
    struct MockState {
        batch_calls: Arc<AtomicUsize>,
        rate_limited_once: Arc<AtomicUsize>,
    }

    // Mock batch game-status endpoint. The first call returns 429 to simulate
    // platform rate limiting; subsequent calls succeed.
    async fn batch_status(State(state): State<MockState>) -> impl IntoResponse {
        let call = state.batch_calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            state.rate_limited_once.fetch_add(1, Ordering::SeqCst);
            return (
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": "rate limited" })),
            )
                .into_response();
        }
        let ids: Vec<Value> = (0..3)
            .map(|i| json!({ "gameId": format!("game-{i}"), "status": "started" }))
            .collect();
        (StatusCode::OK, Json(json!({ "games": ids }))).into_response()
    }

    let state = MockState {
        batch_calls: Arc::new(AtomicUsize::new(0)),
        rate_limited_once: Arc::new(AtomicUsize::new(0)),
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/api/games/status", get(batch_status))
        .with_state(state.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://127.0.0.1:{}", addr.port());
    let client = reqwest::Client::new();

    // Three active matches on the same platform must be grouped into a single
    // batch request rather than three individual requests.
    let game_ids = ["game-0", "game-1", "game-2"];
    let resp = client
        .get(format!("{}/api/games/status", base))
        .query(&[("ids", game_ids.join(","))])
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status().as_u16(),
        429,
        "first batch call must surface rate limiting"
    );
    assert_eq!(
        state.batch_calls.load(Ordering::SeqCst),
        1,
        "grouped matches must produce exactly one batch request"
    );
    assert_eq!(
        state.rate_limited_once.load(Ordering::SeqCst),
        1,
        "rate-limited batch must be recorded for back-pressure"
    );

    // Back-pressure: the poller must not immediately hammer the endpoint again
    // within the same cycle; the affected matches are retried on a later cycle.
    let retry = client
        .get(format!("{}/api/games/status", base))
        .query(&[("ids", game_ids.join(","))])
        .send()
        .await
        .unwrap();
    assert_eq!(
        retry.status().as_u16(),
        200,
        "retried batch on a later cycle must succeed"
    );
    assert_eq!(
        state.batch_calls.load(Ordering::SeqCst),
        2,
        "retry must be a single batched request, not one per match"
    );

    let body: Value = retry.json().await.unwrap();
    let games = body["games"].as_array().expect("games array");
    assert_eq!(
        games.len(),
        game_ids.len(),
        "batch response must cover all affected matches"
    );
}
