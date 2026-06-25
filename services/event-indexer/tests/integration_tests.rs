use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use event_indexer::{
    api::{build_router, ApiResponse},
    cache::EventCache,
    db::Database,
    rpc::SorobanRpcClient,
};

fn test_app() -> axum::Router {
    let db = Arc::new(Database::new(":memory:").unwrap());
    db.init_schema().unwrap();
    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:8000").unwrap());
    build_router(db, cache, rpc)
}

#[test]
fn test_event_indexing() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        assert!(true, "Event indexing test placeholder");
    });
}

#[test]
fn test_event_filtering() {
    assert!(true, "Event filtering test placeholder");
}

#[test]
fn test_cache_operations() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        assert!(true, "Cache operations test placeholder");
    });
}

#[tokio::test]
async fn test_get_match_info_404_returns_error_body() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/match/9999")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: ApiResponse<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    assert!(!parsed.success);
    assert_eq!(parsed.error.as_deref(), Some("Match 9999 not found"));
}
