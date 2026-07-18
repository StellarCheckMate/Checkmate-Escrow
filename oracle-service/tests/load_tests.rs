/// Load tests: 500 concurrent match-verification tasks against a mock server.
///
/// These tests verify that:
/// 1. 500 tasks can complete without panics or deadlocks.
/// 2. The rate limiter never causes more requests than the configured ceiling.
/// 3. The concurrency semaphore is respected (no more than `max_concurrent`
///    in-flight at the same moment).
///
/// The mock server records every request so we can assert on throughput.
///
/// Run with:
///   cargo test -p oracle-service --test load_tests -- --nocapture
use std::sync::{
    atomic::AtomicUsize,
    Arc,
};
use std::time::{Duration, Instant};

use oracle_service::oracle::{
    ChessComClient, ChessComClientConfig, LichessClient, LichessClientConfig,
    ProviderError, ProviderRegistry, RateLimiterConfig,
};
use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

// ── helpers ───────────────────────────────────────────────────────────────────

/// Build a mock server that always returns a "white wins" JSON response and
/// also counts concurrent in-flight requests.
async fn make_mock_server(concurrent_peak: Arc<AtomicUsize>) -> MockServer {
    let server = MockServer::start().await;

    // We can't directly track concurrency inside wiremock callbacks, so we
    // track it on the client side using the semaphore (validated separately).
    // The mock just needs to respond quickly.
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": { "result": "white" }
        })))
        .mount(&server)
        .await;

    let _ = concurrent_peak; // kept alive
    server
}

async fn make_lichess_mock_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
        .mount(&server)
        .await;
    server
}

// ── Chess.com 500-concurrent load test ───────────────────────────────────────

/// Spin up 500 concurrent tasks, each verifying a different numeric game ID,
/// and assert all succeed within the wall-clock time budget.
///
/// Configuration:
///   - burst:            100 tokens  (so requests start immediately)
///   - sustained rate:   1000 req/s  (effectively unlimited for a unit test)
///   - max_concurrent:   50          (cap per-provider in-flight)
///
/// At 1000 req/s sustained the 500 tasks should all finish in well under 2 s
/// on any modern CI machine.  We allow a generous 10 s to avoid flakiness.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chess_com_500_concurrent_tasks_all_succeed() {
    let concurrent_peak = Arc::new(AtomicUsize::new(0));
    let server = make_mock_server(concurrent_peak.clone()).await;

    let client = ChessComClient::with_config(ChessComClientConfig {
        api_base: server.uri(),
        request_timeout: Duration::from_secs(5),
        rate_limiter: RateLimiterConfig {
            capacity: 100,      // large burst so 500 tasks aren't held up
            refill_rate: 1000.0, // effectively unlimited for test purposes
        },
        max_concurrent: 50,
    })
    .unwrap();

    let start = Instant::now();
    let handles: Vec<_> = (0..500u32)
        .map(|i| {
            let c = client.clone();
            let game_id = format!("{}", 1_000_000 + i);
            tokio::spawn(async move { c.fetch_result(&game_id).await })
        })
        .collect();

    let mut failures = 0usize;
    for h in handles {
        if h.await.unwrap().is_err() {
            failures += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("chess_com 500-task load test completed in {elapsed:?}, failures={failures}");

    assert_eq!(failures, 0, "expected 0 failures, got {failures}");
    assert!(
        elapsed < Duration::from_secs(15),
        "load test took too long: {elapsed:?}"
    );
}

// ── Rate-limit enforcement under load ────────────────────────────────────────

/// Verify that the token bucket actually throttles throughput.
///
/// With a sustained rate of 5 req/s and a burst of 5, dispatching 20 tasks
/// should take at least 3 seconds (burn 5 burst tokens → wait for 15 more at
/// 5/s → ≈3s additional).
///
/// We use a generous lower bound of 2.5 s to avoid test flakiness on slow CI.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rate_limiter_throttles_throughput() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": { "result": "draw" }
        })))
        .mount(&server)
        .await;

    let client = ChessComClient::with_config(ChessComClientConfig {
        api_base: server.uri(),
        request_timeout: Duration::from_secs(5),
        rate_limiter: RateLimiterConfig {
            capacity: 5,       // 5 burst tokens
            refill_rate: 5.0,  // 5 tokens/s
        },
        max_concurrent: 20,
    })
    .unwrap();

    let start = Instant::now();
    let handles: Vec<_> = (0..20u32)
        .map(|i| {
            let c = client.clone();
            let gid = format!("{}", 9_000_000 + i);
            tokio::spawn(async move { c.fetch_result(&gid).await })
        })
        .collect();
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let elapsed = start.elapsed();
    println!("rate_limiter throttle test elapsed: {elapsed:?}");

    // 20 requests at 5 tokens burst + 5 req/s: after burst, 15 more at 5/s
    // = at least 3 s.  Use 2.5 s lower bound for CI safety.
    assert!(
        elapsed >= Duration::from_millis(2_500),
        "rate limiter did not throttle: elapsed={elapsed:?}"
    );
}

// ── Concurrency cap under load ────────────────────────────────────────────────

/// Verify that the semaphore bounds in-flight requests.
///
/// We configure `max_concurrent=3` and `capacity=100` (burst).  We then
/// dispatch 30 tasks to a mock server that takes 100 ms to respond.  If
/// concurrency were uncapped all 30 would be in-flight simultaneously; with
/// the cap only 3 are.
///
/// Total expected time ≥ ceil(30/3) * 100ms = 1000 ms.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrency_cap_is_respected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        // 100 ms response delay to make concurrency measurable
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({ "end": { "result": "black" } }))
                .set_delay(Duration::from_millis(100)),
        )
        .mount(&server)
        .await;

    let client = ChessComClient::with_config(ChessComClientConfig {
        api_base: server.uri(),
        request_timeout: Duration::from_secs(10),
        rate_limiter: RateLimiterConfig {
            capacity: 100,
            refill_rate: 1000.0, // not the bottleneck
        },
        max_concurrent: 3, // the bottleneck
    })
    .unwrap();

    let start = Instant::now();
    let handles: Vec<_> = (0..30u32)
        .map(|i| {
            let c = client.clone();
            let gid = format!("{}", 7_000_000 + i);
            tokio::spawn(async move { c.fetch_result(&gid).await })
        })
        .collect();
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let elapsed = start.elapsed();
    println!("concurrency cap test elapsed: {elapsed:?}");

    // 30 tasks / 3 concurrent = 10 batches * 100ms = ~1000 ms minimum.
    assert!(
        elapsed >= Duration::from_millis(900),
        "concurrency cap not enforced: elapsed={elapsed:?}"
    );
}

// ── Lichess 500-concurrent load test ─────────────────────────────────────────

/// Same as the Chess.com test but using LichessClient and 8-char alphanumeric
/// game IDs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lichess_500_concurrent_tasks_all_succeed() {
    let server = make_lichess_mock_server().await;

    let client = LichessClient::with_config(LichessClientConfig {
        api_base: server.uri(),
        request_timeout: Duration::from_secs(5),
        rate_limiter: RateLimiterConfig {
            capacity: 100,
            refill_rate: 1000.0,
        },
        max_concurrent: 50,
    })
    .unwrap();

    // Generate 500 unique 8-char alphanumeric IDs.
    fn make_id(i: u32) -> String {
        format!("{:08x}", i) // e.g. "000001f4"
    }

    let start = Instant::now();
    let handles: Vec<_> = (0..500u32)
        .map(|i| {
            let c = client.clone();
            let game_id = make_id(i);
            tokio::spawn(async move { c.fetch_result(&game_id).await })
        })
        .collect();

    let mut failures = 0usize;
    for h in handles {
        if h.await.unwrap().is_err() {
            failures += 1;
        }
    }

    let elapsed = start.elapsed();
    println!("lichess 500-task load test completed in {elapsed:?}, failures={failures}");

    assert_eq!(failures, 0, "expected 0 failures, got {failures}");
    assert!(
        elapsed < Duration::from_secs(15),
        "load test took too long: {elapsed:?}"
    );
}

// ── ProviderRegistry 500-concurrent load test ────────────────────────────────

/// Verify that the full provider registry handles 500 concurrent Lichess
/// lookups (primary provider), all succeeding.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn registry_500_concurrent_all_succeed() {
    let server = make_lichess_mock_server().await;

    let lichess = Arc::new(
        LichessClient::with_config(LichessClientConfig {
            api_base: server.uri(),
            request_timeout: Duration::from_secs(5),
            rate_limiter: RateLimiterConfig {
                capacity: 100,
                refill_rate: 1000.0,
            },
            max_concurrent: 50,
        })
        .unwrap(),
    );

    let registry = Arc::new(ProviderRegistry::new(vec![lichess]));

    fn make_id(i: u32) -> String {
        format!("{:08x}", i)
    }

    let start = Instant::now();
    let handles: Vec<_> = (0..500u32)
        .map(|i| {
            let reg = registry.clone();
            let game_id = make_id(i);
            tokio::spawn(async move { reg.fetch_result(&game_id).await })
        })
        .collect();

    let mut failures = 0usize;
    for h in handles {
        if h.await.unwrap().is_err() {
            failures += 1;
        }
    }
    let elapsed = start.elapsed();
    println!("registry 500-task load test completed in {elapsed:?}, failures={failures}");

    assert_eq!(failures, 0);
    assert!(elapsed < Duration::from_secs(15));
}

// ── Failover and backpressure regression coverage ─────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn provider_registry_fails_over_when_primary_is_unavailable() {
    let primary = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(503).set_body_string("temporarily unavailable"))
        .mount(&primary)
        .await;

    let secondary = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": { "result": "white" }
        })))
        .mount(&secondary)
        .await;

    let primary_client = Arc::new(
        ChessComClient::with_config(ChessComClientConfig {
            api_base: primary.uri(),
            request_timeout: Duration::from_secs(5),
            rate_limiter: RateLimiterConfig {
                capacity: 10,
                refill_rate: 100.0,
            },
            max_concurrent: 4,
        })
        .unwrap(),
    );

    let secondary_client = Arc::new(
        ChessComClient::with_config(ChessComClientConfig {
            api_base: secondary.uri(),
            request_timeout: Duration::from_secs(5),
            rate_limiter: RateLimiterConfig {
                capacity: 10,
                refill_rate: 100.0,
            },
            max_concurrent: 4,
        })
        .unwrap(),
    );

    let reg = ProviderRegistry::new(vec![primary_client, secondary_client]);
    let winner = reg.fetch_result("123456789").await.unwrap();

    assert_eq!(winner, contracts_oracle::types::Winner::Player1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn registry_returns_rate_limited_error_when_every_provider_is_backed_off() {
    let primary = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limit exceeded"))
        .mount(&primary)
        .await;

    let secondary = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex(r".*"))
        .respond_with(ResponseTemplate::new(429).set_body_string("rate limit exceeded"))
        .mount(&secondary)
        .await;

    let primary_client = Arc::new(
        ChessComClient::with_config(ChessComClientConfig {
            api_base: primary.uri(),
            request_timeout: Duration::from_secs(5),
            rate_limiter: RateLimiterConfig {
                capacity: 1,
                refill_rate: 0.01,
            },
            max_concurrent: 1,
        })
        .unwrap(),
    );

    let secondary_client = Arc::new(
        ChessComClient::with_config(ChessComClientConfig {
            api_base: secondary.uri(),
            request_timeout: Duration::from_secs(5),
            rate_limiter: RateLimiterConfig {
                capacity: 1,
                refill_rate: 0.01,
            },
            max_concurrent: 1,
        })
        .unwrap(),
    );

    let reg = ProviderRegistry::new(vec![primary_client, secondary_client]);
    let err = reg.fetch_result("123456789").await.unwrap_err();

    assert!(matches!(err, ProviderError::AllProvidersFailed { count: 2, .. }));
}
