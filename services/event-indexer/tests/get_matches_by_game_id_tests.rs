//! Tests for the `GET /matches?game_id=` endpoint filter (#1431).
//!
//! These tests verify:
//! - Querying by game_id returns the correct match when it exists.
//! - Querying by an unknown game_id returns an empty list (not a 404).
//! - An empty game_id value returns a 400 Bad Request.
//! - The response always has the correct `ApiResponse` envelope shape.
//!
//! Tests that require a live PostgreSQL instance are gated behind
//! `DATABASE_URL` and skip gracefully when the env var is absent.

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use event_indexer::{
    api::{build_router, ApiResponse},
    api_cache::ApiCache,
    cache::EventCache,
    db::Database,
    models::{IndexedEvent, MatchInfo},
    rpc::SorobanRpcClient,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

// ── helpers ───────────────────────────────────────────────────────────────────

fn no_cache() -> Arc<ApiCache> {
    Arc::new(ApiCache::disabled())
}

const UNREACHABLE_DSN: &str = "postgres://nobody:nobody@127.0.0.1:1/nowhere";

fn unreachable_app() -> axum::Router {
    let db = Arc::new(
        Database::from_dsns(UNREACHABLE_DSN, UNREACHABLE_DSN, 1, 1)
            .expect("DSN must parse without connecting"),
    );
    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    build_router(db, cache, rpc, no_cache(), None)
}

fn make_event(game_id: &str, match_id: u64) -> IndexedEvent {
    IndexedEvent {
        id: format!("evt-{}-{}", match_id, game_id),
        ledger_sequence: 100,
        match_id,
        event_type: "match:created".to_string(),
        player1: Some("GAAA".to_string()),
        player2: Some("GBBB".to_string()),
        status: Some("pending".to_string()),
        winner: None,
        stake_amount: Some("1000".to_string()),
        token: Some("XLM".to_string()),
        game_id: Some(game_id.to_string()),
        platform: Some("lichess".to_string()),
        timestamp: Utc::now(),
        txn_hash: Some("txhash-abc".to_string()),
        event_index_in_txn: None,
        reorg_invalidated_at: None,
    }
}

// ── structural / routing tests (no DB needed) ─────────────────────────────────

/// `GET /matches?game_id=` with an empty value returns 400 (not 500).
#[tokio::test]
async fn get_matches_empty_game_id_returns_400() {
    let response = unreachable_app()
        .oneshot(
            Request::builder()
                .uri("/matches?game_id=")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Empty game_id must be rejected with 400"
    );

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let resp: ApiResponse<Vec<MatchInfo>> =
        serde_json::from_slice(&body).expect("body must be valid ApiResponse JSON");
    assert!(!resp.success);
    assert!(resp.error.is_some());
}

/// `GET /matches?game_id=<value>` routes correctly (status ≠ 404 means the
/// endpoint accepted the parameter even though the DB is unreachable).
#[tokio::test]
async fn get_matches_game_id_param_is_accepted_by_router() {
    let response = unreachable_app()
        .oneshot(
            Request::builder()
                .uri("/matches?game_id=lichess-abc123")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // With an unreachable DB we expect 500, but definitely not 400 (bad param)
    // or 404 (unknown route).
    assert_ne!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "game_id param must be accepted by the router without a 400"
    );
    assert_ne!(
        response.status(),
        StatusCode::NOT_FOUND,
        "route /matches must exist when game_id is provided"
    );
}

// ── integration tests (require DATABASE_URL) ──────────────────────────────────

/// Create a match event with a known `game_id`, then query `/matches?game_id=`
/// and verify the match is returned.
#[tokio::test]
async fn get_matches_by_game_id_returns_match() {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            println!("Skipping get_matches_by_game_id_returns_match: DATABASE_URL not set");
            return;
        }
    };

    let db = Arc::new(
        Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"),
    );
    db.init_schema().await.expect("failed to init schema");

    // Use a unique game_id to avoid interference from other tests.
    let unique_game_id = format!("test-game-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let match_id: u64 = 88_001;

    let event = make_event(&unique_game_id, match_id);
    db.insert_event(&event).await.expect("insert_event failed");

    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    let app = build_router(
        db,
        cache,
        rpc,
        Arc::new(ApiCache::in_memory()),
        None,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/matches?game_id={}", unique_game_id))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let resp: ApiResponse<Vec<MatchInfo>> =
        serde_json::from_slice(&body).expect("body must be valid ApiResponse JSON");

    assert!(resp.success, "success must be true");
    let matches = resp.data.expect("data must be present");
    assert!(
        matches.iter().any(|m| m.match_id == match_id),
        "expected match_id {} in response, got: {:?}",
        match_id,
        matches.iter().map(|m| m.match_id).collect::<Vec<_>>()
    );
}

/// Querying a `game_id` that has no associated events returns an empty list
/// (not a 404 or an error).
#[tokio::test]
async fn get_matches_by_unknown_game_id_returns_empty_list() {
    let db_url = match std::env::var("DATABASE_URL") {
        Ok(v) => v,
        Err(_) => {
            println!(
                "Skipping get_matches_by_unknown_game_id_returns_empty_list: DATABASE_URL not set"
            );
            return;
        }
    };

    let db = Arc::new(
        Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"),
    );
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    let app = build_router(db, cache, rpc, Arc::new(ApiCache::in_memory()), None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/matches?game_id=no-such-game-id-xyz-99999")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let resp: ApiResponse<Vec<MatchInfo>> =
        serde_json::from_slice(&body).expect("body must be valid ApiResponse JSON");

    assert!(resp.success, "success must be true even for an empty result");
    let matches = resp.data.expect("data field must be present");
    assert!(
        matches.is_empty(),
        "unknown game_id must return an empty list, got {} match(es)",
        matches.len()
    );
}
