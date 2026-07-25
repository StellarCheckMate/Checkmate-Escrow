use oracle_service::oracle::{LichessClient};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use std::time::{Duration, Instant};

#[tokio::test]
async fn test_lichess_rate_limiter_spacing() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/game/123"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    // configure a short spacing for the test to keep it fast
    let min_spacing = Duration::from_millis(200);

    let client = LichessClient::new_with_base_timeout_and_spacing(
        server.uri(),
        Duration::from_secs(30),
        min_spacing,
    )
    .unwrap();

    let start = Instant::now();
    client.fetch_result("123").await.unwrap();
    client.fetch_result("123").await.unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed >= min_spacing, "elapsed={:?}, min_spacing={:?}", elapsed, min_spacing);
}
