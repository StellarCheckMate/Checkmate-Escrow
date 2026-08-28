use oracle_service::oracle::{LichessClient, LichessError, LichessGameResult};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn validate_game_id_rejects_empty() {
    assert!(LichessClient::validate_game_id("").is_err());
}

#[tokio::test]
async fn validate_game_id_rejects_non_alphanumeric() {
    assert!(LichessClient::validate_game_id("abc!defg").is_err());
    assert!(LichessClient::validate_game_id("abc def1").is_err());
}

#[tokio::test]
async fn validate_game_id_rejects_wrong_length() {
    // 7 chars
    assert!(LichessClient::validate_game_id("abcdefg").is_err());
    // 9 chars
    assert!(LichessClient::validate_game_id("abcdefghi").is_err());
    // 10 chars (not 8 or 12)
    assert!(LichessClient::validate_game_id("abcdefghij").is_err());
    // 11 chars (not 8 or 12)
    assert!(LichessClient::validate_game_id("abcdefghijk").is_err());
    // 13 chars (not 8 or 12)
    assert!(LichessClient::validate_game_id("abcdefghijklm").is_err());
}

#[tokio::test]
async fn validate_game_id_accepts_8_alphanumeric() {
    LichessClient::validate_game_id("abcd1234").unwrap();
    LichessClient::validate_game_id("ABCD1234").unwrap();
}

/// #1355: 12-character extended Lichess game IDs (tournament format) should be accepted.
#[tokio::test]
async fn validate_game_id_accepts_12_alphanumeric() {
    LichessClient::validate_game_id("abcd12345678").unwrap();
    LichessClient::validate_game_id("ABCD12345678").unwrap();
    LichessClient::validate_game_id("a1B2c3D4e5F6").unwrap();
}

/// #1355: 12-character IDs with non-alphanumeric characters should be rejected.
#[tokio::test]
async fn validate_game_id_rejects_12_chars_non_alphanumeric() {
    // dash in the middle
    assert!(LichessClient::validate_game_id("abcd1234-678").is_err());
    // underscore in the middle
    assert!(LichessClient::validate_game_id("abcd1234_678").is_err());
}

#[tokio::test]
async fn fetch_result_maps_white_to_player1() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/abcd1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res = client.fetch_result("abcd1234").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Player1);
}

#[tokio::test]
async fn fetch_result_maps_black_to_player2() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/abcd5678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "black"
        })))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res: LichessGameResult = client.fetch_result("abcd5678").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Player2);
}

#[tokio::test]
async fn fetch_result_maps_absent_winner_to_draw() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/draw1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res = client.fetch_result("draw1234").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Draw);
}

#[tokio::test]
async fn fetch_result_404_maps_to_game_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/notfound"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("notfound").await.unwrap_err();
    assert!(matches!(err, LichessError::GameNotFound));
}

#[tokio::test]
async fn fetch_result_unknown_winner_errors() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/unk12345"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "unknown_value"
        })))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("unk12345").await.unwrap_err();
    assert!(matches!(err, LichessError::InvalidResponse));
}

#[tokio::test]
async fn fetch_result_non_2xx_maps_to_http_status() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/err12345"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("err12345").await.unwrap_err();
    assert!(matches!(err, LichessError::HttpStatus { .. }));
}

#[tokio::test]
async fn test_lichess_missing_winner_field() {
    // Lichess omits the "winner" key entirely for draws (already covered by
    // fetch_result_maps_absent_winner_to_draw) -- an unrelated "status" key
    // being present alongside the missing "winner" doesn't change that, so
    // this must map to Draw too, not InvalidResponse.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/miss1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "finished"
        })))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let result = client.fetch_result("miss1234").await.unwrap();
    assert_eq!(result.winner, contracts_oracle::types::Winner::Draw);
}

#[tokio::test]
async fn test_lichess_rate_limit_retry() {
    // Per provider.rs's documented fallback design, a 429 is surfaced as
    // LichessError::RateLimited (with the Retry-After delay) so the
    // multi-provider orchestrator can fail over to the next provider --
    // fetch_result itself doesn't retry against the same rate-limited host.
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/rate1234"))
        .respond_with(ResponseTemplate::new(429).append_header("Retry-After", "1"))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("rate1234").await.unwrap_err();
    match err {
        LichessError::RateLimited { retry_after } => {
            assert_eq!(retry_after, std::time::Duration::from_secs(1));
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
}

#[tokio::test]
async fn test_lichess_game_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/notfnd12"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("notfnd12").await.unwrap_err();
    assert!(matches!(err, LichessError::GameNotFound));
}

// ── #1355: 12-character extended Lichess game ID fetch ────────────────────────

/// A 12-character Lichess game ID should pass validation and result in a
/// successful API call when the mock server returns a valid response.
#[tokio::test]
async fn fetch_result_accepts_12_char_game_id() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/abcd12345678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "winner": "white"
        })))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res = client.fetch_result("abcd12345678").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Player1);
}

/// A 12-character Lichess game ID with a draw result should map to Winner::Draw.
#[tokio::test]
async fn fetch_result_12_char_draw() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/game/export/draw12345678"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
        .mount(&server)
        .await;

    let client =
        LichessClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res = client.fetch_result("draw12345678").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Draw);
}
