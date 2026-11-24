//! Tests for the `GET /matches/active` endpoint.
//!
//! Tests verify:
//! - Returns 200 with active matches array
//! - Returns 200 with empty array when no active matches
//! - Response structure contains required fields
//! - Accepts limit and offset query parameters

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use event_indexer::{api::build_router, cache::EventCache, db::Database, rpc::SorobanRpcClient};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn api_cache() -> Arc<event_indexer::api_cache::ApiCache> {
    Arc::new(event_indexer::api_cache::ApiCache::in_memory())
}

/// `GET /matches/active` returns 200 with empty array when no active matches
#[tokio::test]
async fn get_active_matches_returns_empty_when_no_active() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_active_matches_empty: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/active")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_array());
    assert_eq!(parsed["data"].as_array().unwrap().len(), 0);
    assert_eq!(parsed["error"], serde_json::Value::Null);
}

/// `GET /matches/active` response has correct structure for active matches
#[tokio::test]
async fn get_active_matches_response_structure() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_active_matches_structure: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/active")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_array(), "data must be an array");
    assert_eq!(parsed["error"], serde_json::Value::Null);
}

/// `GET /matches/active` accepts limit and offset query parameters
#[tokio::test]
async fn get_active_matches_accepts_pagination_params() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_active_matches_pagination: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    // Test with limit and offset parameters
    let request = Request::builder()
        .uri("/matches/active?limit=50&offset=0")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_array());
}

/// `GET /matches/active` respects the `limit` bound and shifts pages with `offset`
#[tokio::test]
async fn get_active_matches_respects_pagination_bounds() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_active_matches_pagination_bounds: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    // Seed more active matches than the requested limit.
    let seeded = 5usize;
    for i in 0..seeded {
        let match_id = format!("pagination-active-match-{}", i);
        let sql = format!(
            "INSERT INTO matches (match_id, player1, player2, status, created_at) \
             VALUES ('{}', 'player1', 'player2', 'active', NOW()) \
             ON CONFLICT (match_id) DO NOTHING",
            match_id
        );
        if let Err(e) = sqlx::query(&sql).execute(db.pool()).await {
            println!("Skipping pagination bounds test: could not seed matches ({e})");
            return;
        }
    }

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    // Requested limit must bound the returned page size.
    let limit = 2usize;
    let request = Request::builder()
        .uri(format!("/matches/active?limit={}&offset=0", limit))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    let first_page = parsed["data"].as_array().unwrap();
    assert!(
        first_page.len() <= limit,
        "returned {} matches, expected at most {}",
        first_page.len(),
        limit
    );

    // Offset must shift the page.
    let request = Request::builder()
        .uri(format!("/matches/active?limit={}&offset={}", limit, limit))
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    let second_page = parsed["data"].as_array().unwrap();
    assert!(
        second_page.len() <= limit,
        "returned {} matches, expected at most {}",
        second_page.len(),
        limit
    );
    assert_ne!(
        first_page, second_page,
        "offset should shift the returned page"
    );

    // Default limit applies when no pagination params are given.
    let request = Request::builder()
        .uri("/matches/active")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    let default_page = parsed["data"].as_array().unwrap();
    assert!(
        default_page.len() <= 50,
        "default page size should be bounded, got {}",
        default_page.len()
    );
}
