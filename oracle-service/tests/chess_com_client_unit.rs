use oracle_service::oracle::{ChessComClient, ChessComError, ChessComGameResult};

use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn validate_game_id_rejects_empty() {
    assert!(ChessComClient::validate_game_id("").is_err());
}

#[tokio::test]
async fn validate_game_id_rejects_non_numeric() {
    assert!(ChessComClient::validate_game_id("abc").is_err());
    assert!(ChessComClient::validate_game_id("12a").is_err());
}

#[tokio::test]
async fn validate_game_id_accepts_numeric() {
    ChessComClient::validate_game_id("123456789").unwrap();
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

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
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

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();

    let res: ChessComGameResult = client.fetch_result("555").await.unwrap();
    assert_eq!(res.winner, contracts_oracle::types::Winner::Player1);
}

#[tokio::test]
async fn fetch_result_404_maps_to_game_not_found() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/404"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();

    let err = client.fetch_result("404").await.unwrap_err();
    assert!(matches!(err, ChessComError::GameNotFound));
}

#[tokio::test]
async fn test_chess_com_request_timeout() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/pub/game/999"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(500)),
        )
        .mount(&server)
        .await;

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_millis(1),
    )
    .unwrap();

    let err = client.fetch_result("999").await.unwrap_err();
    assert!(matches!(err, ChessComError::Timeout));
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

    let client = ChessComClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();

    let err = client.fetch_result("777").await.unwrap_err();
    assert!(matches!(err, ChessComError::InvalidResponse));
}

