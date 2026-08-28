use oracle_service::oracle::{ChessComClient, ChessComError, ChessComGameResult};

use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn validate_game_id_rejects_empty() {
    let err = ChessComClient::validate_game_id("").unwrap_err();
    assert!(matches!(err, ChessComError::InvalidGameId));
}

#[tokio::test]
async fn validate_game_id_rejects_non_numeric() {
    let err = ChessComClient::validate_game_id("abc").unwrap_err();
    assert!(matches!(err, ChessComError::InvalidGameId));
    let err = ChessComClient::validate_game_id("12a").unwrap_err();
    assert!(matches!(err, ChessComError::InvalidGameId));
}

#[tokio::test]
async fn validate_game_id_accepts_numeric() {
    ChessComClient::validate_game_id("123456789").unwrap();
}

#[tokio::test]
async fn validate_game_id_accepts_very_long_numeric_string() {
    let long_id = "1".repeat(1000);
    ChessComClient::validate_game_id(&long_id).unwrap();
}

#[tokio::test]
async fn validate_game_id_rejects_very_long_non_numeric_string() {
    let long_invalid = "1".repeat(999) + "x";
    let err = ChessComClient::validate_game_id(&long_invalid).unwrap_err();
    assert!(matches!(err, ChessComError::InvalidGameId));
}

#[tokio::test]
async fn fetch_result_maps_draw() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": {"result": "draw"}
        })))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res = client.fetch_result("123").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Draw);
}

#[tokio::test]
async fn fetch_result_maps_white_to_player1() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/555"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": {"result": "white"}
        })))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res: ChessComGameResult = client.fetch_result("555").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Player1);
}

#[tokio::test]
async fn fetch_result_retries_rate_limited_request_after_header_delay() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/429"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", "0")
                .set_body_string("rate limited"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/pub/game/429"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": {"result": "draw"}
        })))
        .mount(&server)
        .await;

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();

    let result = client.fetch_result("429").await.unwrap();

    assert_eq!(result.winner, contracts_oracle::types::Winner::Draw);
}

#[tokio::test]
async fn fetch_result_reloads_rotated_api_key_after_unauthorized() {
    let server = MockServer::start().await;
    std::env::set_var("CHESSDOTCOM_API_KEY", "old-key");

    Mock::given(method("GET"))
        .and(path("/pub/game/401"))
        .and(header("authorization", "Bearer old-key"))
        .respond_with(ResponseTemplate::new(401))
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/pub/game/401"))
        .and(header("authorization", "Bearer new-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": {"result": "white"}
        })))
        .mount(&server)
        .await;

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    std::env::set_var("CHESSDOTCOM_API_KEY", "new-key");

    let result = client.fetch_result("401").await.unwrap();

    std::env::remove_var("CHESSDOTCOM_API_KEY");
    assert_eq!(result.winner, contracts_oracle::types::Winner::Player1);
}

#[tokio::test]
async fn test_chess_com_game_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/999"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("999").await.unwrap_err();
    assert!(matches!(err, ChessComError::GameNotFound));
}

#[tokio::test]
async fn fetch_result_404_maps_to_game_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/404"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("404").await.unwrap_err();
    assert!(matches!(err, ChessComError::GameNotFound));
}

#[tokio::test]
async fn test_chess_com_draw_result() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": {"result": "draw"}
        })))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let res: ChessComGameResult = client.fetch_result("42").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Draw);
}

#[tokio::test]
async fn fetch_result_invalid_response_errors() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/777"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "end": {}
        })))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("777").await.unwrap_err();
    assert!(matches!(err, ChessComError::InvalidResponse));
}

#[tokio::test]
async fn test_chess_com_503_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/503"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let client =
        ChessComClient::new_with_base_and_timeout(server.uri(), std::time::Duration::from_secs(30))
            .unwrap();

    let err = client.fetch_result("503").await.unwrap_err();
    match err {
        ChessComError::HttpStatus { status } => {
            assert_eq!(status, reqwest::StatusCode::SERVICE_UNAVAILABLE);
        }
        ChessComError::Http(_) => {
            // network-level error is also acceptable
        }
        _ => panic!(
            "expected service unavailable or network error, got: {:?}",
            err
        ),
    }
}
