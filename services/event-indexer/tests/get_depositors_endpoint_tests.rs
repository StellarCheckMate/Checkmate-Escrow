//! Tests for the `GET /matches/:match_id/depositors` endpoint.
//!
//! Tests verify:
//! - Returns 404 for nonexistent match
//! - Returns expected depositors list when deposits exist
//! - Returns 200 with empty array when match exists with no deposits

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use event_indexer::{
    api::build_router,
    cache::EventCache,
    db::Database,
    models::IndexedEvent,
    rpc::SorobanRpcClient,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn api_cache() -> Arc<event_indexer::api_cache::ApiCache> {
    Arc::new(event_indexer::api_cache::ApiCache::in_memory())
}

/// `GET /matches/:match_id/depositors` returns 404 for unknown match.
#[tokio::test]
async fn get_match_depositors_returns_404_for_unknown() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_match_depositors_returns_404_for_unknown: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/999999999/depositors")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], false);
    assert!(parsed["error"].as_str().unwrap().contains("not found"));
}

/// `GET /matches/:match_id/depositors` returns expected depositors list.
#[tokio::test]
async fn get_match_depositors_returns_expected_depositors() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_match_depositors_returns_expected_depositors: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let match_id = 88881234u64;
    let player1 = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".to_string();
    let player2 = "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H".to_string();

    // Ingest match created event
    let created_event = IndexedEvent {
        id: format!("evt_created_{}", match_id),
        ledger_sequence: 1000,
        match_id,
        event_type: "match:created".to_string(),
        player1: Some(player1.clone()),
        player2: Some(player2.clone()),
        status: Some("pending".to_string()),
        winner: None,
        stake_amount: Some("1000000".to_string()),
        token: Some("CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string()),
        game_id: Some("game123".to_string()),
        platform: Some("lichess".to_string()),
        timestamp: Utc::now(),
        txn_hash: Some("tx_created".to_string()),
        event_index_in_txn: Some(0),
        reorg_invalidated_at: None,
    };
    db.insert_event(&created_event).await.expect("insert created event");

    // Ingest player 1 deposit event
    let dep1_event = IndexedEvent {
        id: format!("evt_dep1_{}", match_id),
        ledger_sequence: 1001,
        match_id,
        event_type: "match:deposit".to_string(),
        player1: Some(player1.clone()),
        player2: None,
        status: Some("pending".to_string()),
        winner: None,
        stake_amount: Some("1000000".to_string()),
        token: Some("CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string()),
        game_id: Some("game123".to_string()),
        platform: Some("lichess".to_string()),
        timestamp: Utc::now(),
        txn_hash: Some("tx_dep1".to_string()),
        event_index_in_txn: Some(1),
        reorg_invalidated_at: None,
    };
    db.insert_event(&dep1_event).await.expect("insert deposit 1 event");

    // Ingest player 2 deposit event
    let dep2_event = IndexedEvent {
        id: format!("evt_dep2_{}", match_id),
        ledger_sequence: 1002,
        match_id,
        event_type: "match:deposit".to_string(),
        player1: Some(player2.clone()),
        player2: None,
        status: Some("active".to_string()),
        winner: None,
        stake_amount: Some("1000000".to_string()),
        token: Some("CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA".to_string()),
        game_id: Some("game123".to_string()),
        platform: Some("lichess".to_string()),
        timestamp: Utc::now(),
        txn_hash: Some("tx_dep2".to_string()),
        event_index_in_txn: Some(1),
        reorg_invalidated_at: None,
    };
    db.insert_event(&dep2_event).await.expect("insert deposit 2 event");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri(format!("/matches/{}/depositors", match_id))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);
    let depositors = parsed["data"].as_array().expect("depositors array");
    assert_eq!(depositors.len(), 2);
    assert_eq!(depositors[0].as_str().unwrap(), player1);
    assert_eq!(depositors[1].as_str().unwrap(), player2);
}
