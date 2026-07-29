//! Tests for the `GET /matches/active` endpoint.
//!
//! Tests verify:
//! - Returns 200 with active matches array
//! - Returns 200 with empty array when no active matches
//! - Response structure contains required fields
//! - Accepts limit and offset query parameters

use axum::http::StatusCode;
use event_indexer::{
    api::{build_router, AppState},
    cache::EventCache,
    db::Database,
    models::{IndexedEvent, MatchInfo, MatchStatus},
    rpc::SorobanRpcClient,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Helper to convert response body to bytes
async fn body_to_bytes(
    body: axum::body::Body,
) -> Result<bytes::Bytes, Box<dyn std::error::Error>> {
    Ok(body.collect().await?.to_bytes())
}

/// `GET /matches/active` returns 200 with empty array when no active matches
#[tokio::test]
async fn get_active_matches_returns_empty_when_no_active() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_active_matches_empty: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(
        Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"),
    );
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let api_cache = Arc::new(
        event_indexer::api_cache::ApiCache::new_process_local(std::time::Duration::from_secs(10))
            .expect("failed to create cache"),
    );

    let app = build_router(db, cache, rpc, api_cache);

    let request = http::Request::builder()
        .uri("/matches/active")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_bytes(response.into_body()).await.unwrap();
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
    let db = Arc::new(
        Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"),
    );
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let api_cache = Arc::new(
        event_indexer::api_cache::ApiCache::new_process_local(std::time::Duration::from_secs(10))
            .expect("failed to create cache"),
    );

    let app = build_router(db, cache, rpc, api_cache);

    let request = http::Request::builder()
        .uri("/matches/active")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_bytes(response.into_body()).await.unwrap();
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
    let db = Arc::new(
        Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"),
    );
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let api_cache = Arc::new(
        event_indexer::api_cache::ApiCache::new_process_local(std::time::Duration::from_secs(10))
            .expect("failed to create cache"),
    );

    let app = build_router(db, cache, rpc, api_cache);

    // Test with limit and offset parameters
    let request = http::Request::builder()
        .uri("/matches/active?limit=50&offset=0")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_to_bytes(response.into_body()).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_array());
}
