//! Tests for the X-Request-ID header middleware.
//!
//! Tests include:
//! - Request ID header is present on all responses
//! - Request ID is extracted from client-provided header when present
//! - Request ID is generated (UUID) when not provided by client
//! - Generated request IDs are unique across requests
//! - Request ID is properly formatted as UUID

use std::sync::Arc;

use axum::http::{Request, StatusCode};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

use event_indexer::api::build_router;
use event_indexer::api_cache::ApiCache;
use event_indexer::auth::API_KEY_HEADER;
use event_indexer::cache::EventCache;
use event_indexer::db::Database;
use event_indexer::rpc::SorobanRpcClient;

const UNREACHABLE_DSN: &str = "postgres://nobody:nobody@127.0.0.1:1/nowhere";
const REQUEST_ID_HEADER: &str = "X-Request-ID";

/// Shared secret configured on the test router.  `/stats` is gated by the API
/// key middleware, so every request must carry it to exercise the full chain.
const TEST_API_KEY: &str = "test-api-key-0123456789abcdef";

fn app() -> axum::Router {
    let db = Arc::new(
        Database::from_dsns(UNREACHABLE_DSN, UNREACHABLE_DSN, 1, 1).expect("DSN must parse"),
    );
    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    build_router(
        db,
        cache,
        rpc,
        Arc::new(ApiCache::disabled()),
        Some(TEST_API_KEY.to_string()),
    )
}

/// Issue a GET request to /health and return status and request ID header.
async fn get_with_request_id(
    endpoint: &str,
    client_request_id: Option<&str>,
) -> (StatusCode, Option<String>) {
    let mut builder = Request::builder()
        .uri(endpoint)
        .header(API_KEY_HEADER, TEST_API_KEY);

    if let Some(id) = client_request_id {
        builder = builder.header(REQUEST_ID_HEADER, id);
    }

    let response = app()
        .oneshot(builder.body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();

    let status = response.status();
    let request_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    (status, request_id)
}

#[tokio::test]
async fn request_id_header_present_on_response() {
    let (_status, request_id) = get_with_request_id("/health", None).await;

    assert!(
        request_id.is_some(),
        "X-Request-ID header should be present on response"
    );
}

#[tokio::test]
async fn request_id_is_valid_uuid_when_not_provided() {
    let (_status, request_id) = get_with_request_id("/health", None).await;

    let id = request_id.expect("Request ID should be present");
    assert!(
        Uuid::parse_str(&id).is_ok(),
        "Generated request ID should be a valid UUID: {}",
        id
    );
}

#[tokio::test]
async fn client_request_id_is_echoed_back() {
    let client_id = "my-custom-request-id-12345";
    let (_status, request_id) = get_with_request_id("/health", Some(client_id)).await;

    assert_eq!(
        request_id,
        Some(client_id.to_string()),
        "Client-provided request ID should be echoed back"
    );
}

#[tokio::test]
async fn different_requests_get_different_request_ids() {
    let (_status1, id1) = get_with_request_id("/health", None).await;
    let (_status2, id2) = get_with_request_id("/health", None).await;

    let id1 = id1.expect("First request should have ID");
    let id2 = id2.expect("Second request should have ID");

    assert_ne!(
        id1, id2,
        "Different requests without client-provided ID should get different request IDs"
    );
}

#[tokio::test]
async fn request_id_header_on_events_endpoint() {
    let (_status, request_id) = get_with_request_id("/events", None).await;

    assert!(
        request_id.is_some(),
        "X-Request-ID header should be present on /events endpoint"
    );
}

#[tokio::test]
async fn request_id_header_on_matches_endpoint() {
    let (_status, request_id) = get_with_request_id("/matches", None).await;

    assert!(
        request_id.is_some(),
        "X-Request-ID header should be present on /matches endpoint"
    );
}

#[tokio::test]
async fn request_id_preserves_custom_format() {
    let custom_id = "trace-abc-123-def-456";
    let (_status, request_id) = get_with_request_id("/health", Some(custom_id)).await;

    assert_eq!(
        request_id,
        Some(custom_id.to_string()),
        "Custom request ID format should be preserved"
    );
}

#[tokio::test]
async fn empty_request_id_header_generates_new_uuid() {
    // Test with empty string provided (should be treated as missing)
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .header(REQUEST_ID_HEADER, "")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let request_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    assert!(
        request_id.is_some(),
        "Should generate UUID for empty header"
    );

    let id = request_id.unwrap();
    assert!(
        Uuid::parse_str(&id).is_ok(),
        "Generated ID should be valid UUID: {}",
        id
    );
}

#[tokio::test]
async fn request_id_header_present_on_multiple_endpoints() {
    let endpoints = vec!["/health", "/events", "/matches", "/stats"];

    for endpoint in endpoints {
        let (_status, request_id) = get_with_request_id(endpoint, None).await;

        assert!(
            request_id.is_some(),
            "X-Request-ID header should be present on {} endpoint",
            endpoint
        );
    }
}

/// Issue a request with an explicit API key header (or none) and an optional
/// client-provided request ID, returning the full response.
async fn raw_request(
    endpoint: &str,
    api_key: Option<&str>,
    client_request_id: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder().uri(endpoint);

    if let Some(key) = api_key {
        builder = builder.header(API_KEY_HEADER, key);
    }
    if let Some(id) = client_request_id {
        builder = builder.header(REQUEST_ID_HEADER, id);
    }

    app()
        .oneshot(builder.body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap()
}

/// Extract the `request_id` field from a JSON error body, if present.
async fn body_request_id(response: axum::response::Response) -> Option<String> {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value
        .get("request_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[tokio::test]
async fn error_response_echoes_client_request_id_in_header() {
    let client_id = "error-trace-abc-123";
    // Missing API key on a gated endpoint produces an error response.
    let response = raw_request("/stats", None, Some(client_id)).await;

    assert!(
        response.status().is_client_error(),
        "Expected an error status, got {}",
        response.status()
    );

    let header_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    assert_eq!(
        header_id,
        Some(client_id.to_string()),
        "Error response should echo the client-provided request ID in the header"
    );
}

#[tokio::test]
async fn error_response_includes_request_id_in_body() {
    let client_id = "error-body-trace-456";
    let response = raw_request("/stats", None, Some(client_id)).await;

    assert!(
        response.status().is_client_error(),
        "Expected an error status, got {}",
        response.status()
    );

    let body_id = body_request_id(response).await;

    assert_eq!(
        body_id,
        Some(client_id.to_string()),
        "Error response body should include the client-provided request_id"
    );
}

#[tokio::test]
async fn error_response_generates_request_id_when_absent() {
    let response = raw_request("/stats", None, None).await;

    assert!(
        response.status().is_client_error(),
        "Expected an error status, got {}",
        response.status()
    );

    let header_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .expect("Error response should carry a generated X-Request-ID header");

    assert!(
        Uuid::parse_str(&header_id).is_ok(),
        "Generated request ID on error response should be a valid UUID: {}",
        header_id
    );

    let body_id = body_request_id(response).await;
    assert_eq!(
        body_id,
        Some(header_id),
        "Error response body request_id should match the generated header"
    );
}

#[tokio::test]
async fn request_id_header_case_insensitive_retrieval() {
    let custom_id = "test-id-123";
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/health")
                .header("x-request-id", custom_id) // lowercase
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let request_id = response
        .headers()
        .get("x-request-id") // Try lowercase
        .or_else(|| response.headers().get("X-Request-ID")) // Try uppercase
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());

    assert_eq!(
        request_id,
        Some(custom_id.to_string()),
        "Client-provided request ID should be preserved regardless of case"
    );
}
