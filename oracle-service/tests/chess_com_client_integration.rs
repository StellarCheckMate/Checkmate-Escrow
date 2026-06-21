#[ignore]
#[tokio::test]
async fn chess_com_integration_fetch_known_game() {
    // Provide a known live/sandbox game id via env var.
    // Example:
    //   CHESSCOM_GAME_ID=123456789 cargo test -p oracle-service -- --ignored
    let game_id = match std::env::var("CHESSCOM_GAME_ID") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("CHESSCOM_GAME_ID not set; skipping integration test");
            return;
        }
    };

    let client = oracle_service::oracle::ChessComClient::new().unwrap();
    let res = client.fetch_result(&game_id).await.unwrap();

    // We only assert that we can parse into a valid winner.
    let _winner = res.winner;
}
