use oracle_service::oracle::{ChessComError, LichessClient, LichessGameResult};

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
}

#[tokio::test]
async fn validate_game_id_accepts_8_alphanumeric() {
    LichessClient::validate_game_id("abcd1234").unwrap();
    LichessClient::validate_game_id("ABCD1234").unwrap();
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

    let client = LichessClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
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

    let client = LichessClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
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

    let client = LichessClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
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

    let client = LichessClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();

    let err = client.fetch_result("notfound").await.unwrap_err();
    assert!(matches!(err, ChessComError::GameNotFound));
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

    let client = LichessClient::new_with_base_and_timeout(
        server.uri(),
        std::time::Duration::from_secs(30),
    )
    .unwrap();

    let err = client.fetch_result("unk12345").await.unwrap_err();
    assert!(matches!(err, ChessComError::InvalidResponse));
}
