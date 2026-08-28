//! End-to-end tests for the API-key gate on the admin-like endpoints.
//!
//! `/stats`, `/analytics/*` and `/transactions/player/*` expose sensitive
//! financial data or expensive whole-table aggregates, so they are gated by a
//! shared secret presented in the `X-Api-Key` header.
//!
//! ## No database required
//! The router is built against a DSN that parses but cannot connect.  That is
//! enough to exercise the middleware: authentication happens before routing,
//! validation and any database I/O, so the tests below only ever assert on
//! `401` vs "anything else".  A request that passes the gate reaches the
//! handler and fails with `500` from the unreachable database — so "not 401"
//! is the positive signal that authentication succeeded.

use std::sync::Arc;

use axum::body::to_bytes;
use axum::http::{header, Request, StatusCode};
use tokio::sync::RwLock;
use tower::ServiceExt;

use event_indexer::api::{build_router, ApiResponse};
use event_indexer::api_cache::ApiCache;
use event_indexer::auth::API_KEY_HEADER;
use event_indexer::cache::EventCache;
use event_indexer::db::Database;
use event_indexer::rpc::SorobanRpcClient;

const UNREACHABLE_DSN: &str = "postgres://nobody:nobody@127.0.0.1:1/nowhere";
/// Checksum-valid account address, used to reach `/transactions/player/*`.
const VALID_ACCOUNT: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";

/// Shared secret configured on the router, at least `MIN_API_KEY_LENGTH` chars.
const TEST_API_KEY: &str = "test-api-key-0123456789abcdef";

/// Build a router whose database is unreachable, with an optional API key.
fn app(api_key: Option<&str>) -> axum::Router {
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
        api_key.map(str::to_string),
    )
}

/// Router with **no** key configured — the fail-closed default.
async fn get_unconfigured(uri: &str) -> axum::response::Response {
    app(None)
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Router with [`TEST_API_KEY`] configured; `request_key` is what the client
/// sends in the `X-Api-Key` header (`None` sends no header at all).
async fn get_with_key(uri: &str, request_key: Option<&str>) -> axum::response::Response {
    let mut builder = Request::builder().uri(uri);
    if let Some(key) = request_key {
        builder = builder.header(API_KEY_HEADER, key);
    }
    app(Some(TEST_API_KEY))
        .oneshot(builder.body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn unauthorized_envelope(
    response: axum::response::Response,
) -> (StatusCode, ApiResponse<serde_json::Value>) {
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed = serde_json::from_slice(&body).expect("401 body must be valid ApiResponse JSON");
    (status, parsed)
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Fail-closed default
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn protected_endpoints_fail_closed_without_a_configured_key() {
    for uri in [
        "/stats",
        "/analytics/overview",
        &format!("/transactions/player/{VALID_ACCOUNT}"),
    ] {
        let response = get_unconfigured(uri).await;
        let (status, parsed) = unauthorized_envelope(response).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "expected 401 for {uri}");
        assert!(!parsed.success);
        assert!(
            parsed.error.is_some(),
            "401 for {uri} must explain that no key is configured"
        );
        assert!(
            parsed
                .error
                .as_deref()
                .unwrap_or("")
                .contains("not configured"),
            "401 for {uri} must mention the missing configuration"
        );
    }
}

#[tokio::test]
async fn fail_closed_response_advertises_the_api_key_scheme() {
    let response = get_unconfigured("/stats").await;
    let challenge = response
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok());
    assert_eq!(challenge, Some("ApiKey"));
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Key configured: header required and verified
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn protected_endpoints_require_the_header_when_a_key_is_configured() {
    for uri in [
        "/stats",
        "/analytics/overview",
        &format!("/transactions/player/{VALID_ACCOUNT}"),
    ] {
        let response = get_with_key(uri, None).await;
        let (status, parsed) = unauthorized_envelope(response).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "expected 401 for {uri} without header"
        );
        assert!(!parsed.success);
        assert!(parsed.error.is_some());
    }
}

#[tokio::test]
async fn wrong_key_is_rejected() {
    let response = get_with_key("/stats", Some("definitely-not-the-key-9876543210")).await;
    let (status, parsed) = unauthorized_envelope(response).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!parsed.success);
    assert!(
        parsed.error.as_deref().unwrap_or("").contains("X-Api-Key"),
        "wrong key should be reported as a bad header"
    );
}

#[tokio::test]
async fn correct_key_passes_the_gate() {
    // A non-401 status proves the request reached the handler (which then
    // fails on the unreachable database, hence 500) — authentication passed.
    let response = get_with_key("/stats", Some(TEST_API_KEY)).await;
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "valid key must pass authentication"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Public endpoints stay public
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn public_endpoints_never_require_a_key() {
    for uri in [
        "/health",
        "/events",
        "/events?limit=0", // malformed input must yield 400, not 401
        "/matches",
        "/matches/active",
        "/matches/pending",
        "/match/1",
        "/players/GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN/matches",
        "/api/docs",
        "/api/openapi.yaml",
    ] {
        let response = get_unconfigured(uri).await;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} must stay public"
        );
    }
}

#[tokio::test]
async fn paths_that_merely_start_with_a_protected_prefix_stay_public() {
    // The gate matches `/analytics/`, `/transactions/` and `/stats` exactly;
    // a bare prefix or a suffix must not be treated as protected.
    for uri in ["/analytics", "/transactions", "/stats/everything"] {
        let response = get_unconfigured(uri).await;
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} must not be caught by the prefix match"
        );
    }
}
