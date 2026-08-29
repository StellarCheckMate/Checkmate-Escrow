//! Tests for cursor-based pagination on `GET /matches`.
//!
//! Tests verify:
//! - `after`/`before` cursor params are accepted and return 200
//! - Offset-based pagination still works for backwards compatibility
//! - `limit` bounds the page size in both modes

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use event_indexer::{api::build_router, cache::EventCache, db::Database, rpc::SorobanRpcClient};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn api_cache() -> Arc<event_indexer::api_cache::ApiCache> {
    Arc::new(event_indexer::api_cache::ApiCache::in_memory())
}

async fn app() -> axum::Router {
    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    build_router(db, cache, rpc, api_cache(), None)
}

/// `GET /matches?after=<id>` returns 200 with a matches array, ordered
/// ascending by `match_id` and filtered to ids greater than the cursor.
#[tokio::test]
async fn get_matches_accepts_after_cursor() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_matches_accepts_after_cursor: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    let request = Request::builder()
        .uri("/matches?after=0&limit=10")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_array());

    let ids: Vec<i64> = parsed["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["match_id"].as_i64().unwrap())
        .collect();
    // Every returned id must be strictly greater than the cursor, and the
    // page must be sorted ascending (a requirement for cursor pagination to
    // be stable across requests).
    for id in &ids {
        assert!(*id > 0);
    }
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

/// `GET /matches?before=<id>` returns only ids less than the cursor.
#[tokio::test]
async fn get_matches_accepts_before_cursor() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_matches_accepts_before_cursor: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    let request = Request::builder()
        .uri("/matches?before=1000000")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);

    for m in parsed["data"].as_array().unwrap() {
        assert!(m["match_id"].as_i64().unwrap() < 1_000_000);
    }
}

/// `GET /matches?offset=&limit=` (legacy mode) still works when no cursor is
/// supplied, preserving backwards compatibility.
#[tokio::test]
async fn get_matches_offset_pagination_still_works() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_matches_offset_pagination_still_works: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    let request = Request::builder()
        .uri("/matches?offset=0&limit=5")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].as_array().unwrap().len() <= 5);
}
