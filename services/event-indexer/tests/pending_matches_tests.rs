//! Tests for the `GET /matches/pending` endpoint.
//!
//! Tests verify:
//! - Returns 200 with pending matches array
//! - Returns 200 with empty array when no pending matches
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

/// `GET /matches/pending` returns 200 with empty array when no pending matches
#[tokio::test]
async fn get_pending_matches_returns_empty_when_no_pending() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_empty: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/pending")
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

/// `GET /matches/pending` response has correct structure
#[tokio::test]
async fn get_pending_matches_response_structure() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_structure: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/pending")
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

/// `GET /matches/pending` accepts limit and offset query parameters
#[tokio::test]
async fn get_pending_matches_accepts_pagination_params() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_pagination: DATABASE_URL not set");
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
        .uri("/matches/pending?limit=50&offset=0")
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

/// `GET /matches/pending` respects the requested limit bound
#[tokio::test]
async fn get_pending_matches_respects_limit_bound() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_limit_bound: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/pending?limit=2&offset=0")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    let data = parsed["data"].as_array().expect("data must be an array");
    assert!(
        data.len() <= 2,
        "expected at most 2 pending matches, got {}",
        data.len()
    );
}

/// `GET /matches/pending` applies a default limit when no params are given
#[tokio::test]
async fn get_pending_matches_applies_default_limit() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_default_limit: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/pending")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    let data = parsed["data"].as_array().expect("data must be an array");
    assert!(
        data.len() <= 100,
        "default limit should cap results at 100, got {}",
        data.len()
    );
}

/// `GET /matches/pending` shifts the page when an offset is supplied
#[tokio::test]
async fn get_pending_matches_offset_shifts_page() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_offset_shifts: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let first_page = Request::builder()
        .uri("/matches/pending?limit=1&offset=0")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.clone().oneshot(first_page).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let first: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let first_data = first["data"].as_array().expect("data must be an array");

    let second_page = Request::builder()
        .uri("/matches/pending?limit=1&offset=1")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(second_page).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let second: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let second_data = second["data"].as_array().expect("data must be an array");

    assert!(first_data.len() <= 1);
    assert!(second_data.len() <= 1);
    if !first_data.is_empty() && !second_data.is_empty() {
        assert_ne!(
            first_data[0], second_data[0],
            "offset should shift the returned page"
        );
    }
}

/// `GET /matches/pending` accepts only offset parameter
#[tokio::test]
async fn get_pending_matches_accepts_offset_only() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_pending_matches_offset_only: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/matches/pending?offset=10")
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
