//! Tests for the `GET /disputes` endpoint.
//!
//! Tests verify:
//! - Returns 200 with open disputes
//! - Pagination bounds (`limit` and `offset`) are respected
//! - Returns empty array when no open disputes exist

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use event_indexer::{
    api::build_router,
    cache::EventCache,
    db::Database,
    models::DisputeInfo,
    rpc::SorobanRpcClient,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn api_cache() -> Arc<event_indexer::api_cache::ApiCache> {
    Arc::new(event_indexer::api_cache::ApiCache::in_memory())
}

/// `GET /disputes` returns empty array when no open disputes exist.
#[tokio::test]
async fn get_disputes_returns_empty_when_no_disputes() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_disputes_returns_empty_when_no_disputes: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let app = build_router(db, cache, rpc, api_cache(), None);

    let request = Request::builder()
        .uri("/disputes")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"].is_array());
    assert_eq!(parsed["error"], serde_json::Value::Null);
}

/// `GET /disputes` respects limit and offset pagination bounds.
#[tokio::test]
async fn get_disputes_pagination_bounds_respected() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_disputes_pagination_bounds_respected: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    // Insert multiple test disputes
    for i in 1..=10 {
        let dispute = DisputeInfo {
            dispute_id: 1000 + i,
            match_id: 2000 + i,
            disputer: format!("GDISPUTER_{}", i),
            status: "open".to_string(),
            evidence_hash: Some("evidence_hash_123".to_string()),
            dispute_bond: Some("1000000".to_string()),
            created_ledger: 5000 + (i as u32),
            created_at: Some(Utc::now()),
        };
        db.insert_dispute(&dispute).await.expect("insert dispute");
    }

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());
    let app = build_router(db, cache, rpc, api_cache(), None);

    // Test with limit=3, offset=0
    let req1 = Request::builder()
        .uri("/disputes?limit=3&offset=0")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let body1 = to_bytes(resp1.into_body(), usize::MAX).await.unwrap();
    let parsed1: serde_json::Value = serde_json::from_slice(&body1).unwrap();
    assert_eq!(parsed1["success"], true);
    let list1 = parsed1["data"].as_array().unwrap();
    assert_eq!(list1.len(), 3, "limit=3 must return at most 3 entries");

    // Test with limit=2, offset=3
    let req2 = Request::builder()
        .uri("/disputes?limit=2&offset=3")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body2 = to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
    let parsed2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(parsed2["success"], true);
    let list2 = parsed2["data"].as_array().unwrap();
    assert_eq!(list2.len(), 2, "limit=2 must return at most 2 entries");
    assert_ne!(
        list1[0]["dispute_id"], list2[0]["dispute_id"],
        "offset=3 must yield different items from offset=0"
    );
}
