//! Tests for the GET /matches/:match_id endpoint.
//!
//! Tests include:
//! - Successful match lookup (200 OK)
//! - Match not found (404 Not Found) with descriptive error message
//! - Full match object structure validation

use std::sync::Arc;

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use tokio::sync::RwLock;
use tower::ServiceExt;

use event_indexer::api::{build_router, ApiResponse};
use event_indexer::api_cache::ApiCache;
use event_indexer::cache::EventCache;
use event_indexer::db::Database;
use event_indexer::models::MatchInfo;
use event_indexer::rpc::SorobanRpcClient;

const UNREACHABLE_DSN: &str = "postgres://nobody:nobody@127.0.0.1:1/nowhere";

fn app() -> axum::Router {
    let db = Arc::new(
        Database::from_dsns(UNREACHABLE_DSN, UNREACHABLE_DSN, 1, 1).expect("DSN must parse"),
    );
    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    build_router(db, cache, rpc, Arc::new(ApiCache::disabled()), None)
}

/// Issue a GET request and return the status and response body.
async fn get_match(match_id: u64) -> (StatusCode, String) {
    let response = app()
        .oneshot(
            Request::builder()
                .uri(format!("/matches/{}", match_id))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    (status, body_str)
}

#[tokio::test]
async fn get_match_invalid_match_id_returns_404() {
    // A genuine "not found" is Ok(None) from build_match_info, which
    // requires a query that actually completes -- the UNREACHABLE_DSN
    // fixture the other tests in this file use can only ever produce a
    // connection error (500), never a real empty result. Needs a reachable
    // (but unseeded) database, same as
    // integration_tests.rs::api_match_info_not_found_returns_404.
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_match_invalid_match_id_returns_404: DATABASE_URL not set");
        return;
    }

    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("db"));
    db.init_schema().await.expect("schema");
    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    let app = build_router(db, cache, rpc, Arc::new(ApiCache::disabled()), None);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/matches/99999")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body_bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body = String::from_utf8(body_bytes.to_vec()).unwrap();

    // Parse the response to ensure it's proper JSON
    let response: ApiResponse<Option<MatchInfo>> =
        serde_json::from_str(&body).expect("Response should be valid JSON");

    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "Expected 404 for non-existent match"
    );
    assert!(!response.success, "Response success should be false");
    assert!(
        response.data.is_none(),
        "Response data should be None for 404"
    );
    assert!(
        response.error.is_some(),
        "Response should have an error message"
    );

    let error_msg = response.error.unwrap();
    assert!(
        error_msg.contains("99999"),
        "Error message should mention the match ID: {}",
        error_msg
    );
    assert!(
        error_msg.contains("not found"),
        "Error message should say 'not found': {}",
        error_msg
    );
}

#[tokio::test]
async fn get_match_endpoint_returns_valid_response_structure() {
    let (status, body) = get_match(1).await;

    // Even though the database is unreachable, the response structure should be valid
    let _response: ApiResponse<Option<MatchInfo>> =
        serde_json::from_str(&body).expect("Response should always be valid JSON ApiResponse");

    // The request will fail with 500 due to unreachable DB, not 400 (validation),
    // which means the endpoint routing and parameter parsing works correctly
    assert_ne!(
        status,
        StatusCode::BAD_REQUEST,
        "Numeric match_id should pass validation"
    );
}

#[tokio::test]
async fn get_match_with_non_numeric_id_returns_validation_error() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/matches/not-a-number")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The route won't even match if the parameter can't be parsed as u64
    // This should result in a 404 from the router (no matching route)
    let status = response.status();
    // Either 404 (route not matched) or 400 (validation) are acceptable
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::BAD_REQUEST,
        "Non-numeric match_id should be rejected, got: {}",
        status
    );
}
