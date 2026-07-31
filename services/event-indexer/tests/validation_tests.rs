//! End-to-end tests for the API validation middleware.
//!
//! ## No database required
//! The router is built against a DSN that parses but cannot connect.  A rejected
//! request therefore returns `400` *without* the database being reachable, which
//! is exactly the property under test: validation happens before any I/O.  A
//! request that passes validation reaches the handler and fails with `500` from
//! the unreachable database — so `500` is the positive signal that input was
//! accepted.

use std::sync::Arc;

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use tokio::sync::RwLock;
use tower::ServiceExt;

use event_indexer::api::{build_router, ApiResponse};
use event_indexer::api_cache::ApiCache;
use event_indexer::cache::EventCache;
use event_indexer::db::Database;
use event_indexer::rpc::SorobanRpcClient;

const UNREACHABLE_DSN: &str = "postgres://nobody:nobody@127.0.0.1:1/nowhere";

/// Checksum-valid account address.
const VALID_ACCOUNT: &str = "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN";
/// Checksum-valid contract address.
const VALID_CONTRACT: &str = "CAAACAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB6N4O";

fn app() -> axum::Router {
    let db = Arc::new(
        Database::from_dsns(UNREACHABLE_DSN, UNREACHABLE_DSN, 1, 1).expect("DSN must parse"),
    );
    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    build_router(db, cache, rpc, Arc::new(ApiCache::disabled()))
}

/// Issue a GET and return the status plus the decoded error message (if any).
async fn get(uri: &str) -> (StatusCode, String) {
    let response = app()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let message = serde_json::from_slice::<ApiResponse<serde_json::Value>>(&body)
        .ok()
        .and_then(|r| r.error)
        .unwrap_or_default();

    (status, message)
}

/// Assert a request is rejected with 400 and that the message names the field.
async fn assert_rejected(uri: &str, field: &str) {
    let (status, message) = get(uri).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400 for {uri}");
    assert!(
        message.contains(field),
        "error for {uri} should name {field:?}, got {message:?}"
    );
}

/// Assert a request passes validation.  The unreachable database means anything
/// other than 400 proves the input was accepted.
async fn assert_accepted(uri: &str) {
    let (status, message) = get(uri).await;
    assert_ne!(
        status,
        StatusCode::BAD_REQUEST,
        "{uri} should have passed validation, got {message:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Stellar addresses
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn valid_address_is_accepted() {
    assert_accepted(&format!("/events?player_address={VALID_ACCOUNT}")).await;
}

#[tokio::test]
async fn wrong_length_address_is_rejected() {
    assert_rejected("/events?player_address=GABC", "player_address").await;
    assert_rejected(
        &format!("/events?player_address={VALID_ACCOUNT}A"),
        "player_address",
    )
    .await;
}

#[tokio::test]
async fn placeholder_address_is_rejected() {
    // The kind of value that used to reach SQL and silently match nothing.
    let (status, message) = get("/events?player_address=PLAYER_ONE").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(message.contains("player_address"), "got {message:?}");
}

#[tokio::test]
async fn ethereum_address_is_rejected() {
    assert_rejected(
        "/events?player_address=0x71C7656EC7ab88b098defB751B7401B5f6d8976F",
        "player_address",
    )
    .await;
}

#[tokio::test]
async fn address_with_a_typo_is_rejected_by_the_checksum() {
    let mut typo = VALID_ACCOUNT.to_string();
    typo.pop();
    typo.push('M');
    let (status, message) = get(&format!("/events?player_address={typo}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(message.contains("checksum"), "got {message:?}");
}

#[tokio::test]
async fn lowercase_address_is_rejected() {
    assert_rejected(
        &format!("/events?player_address={}", VALID_ACCOUNT.to_lowercase()),
        "player_address",
    )
    .await;
}

#[tokio::test]
async fn history_path_requires_an_account_address() {
    assert_accepted(&format!("/transactions/player/{VALID_ACCOUNT}")).await;
    assert_rejected("/transactions/player/not-an-address", "player_address").await;
    assert_rejected(
        &format!("/transactions/player/{VALID_CONTRACT}"),
        "player_address",
    )
    .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Match ids in the path
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn numeric_match_ids_are_accepted() {
    assert_accepted("/match/0").await;
    assert_accepted("/match/12345").await;
    assert_accepted("/events/7").await;
}

#[tokio::test]
async fn non_numeric_match_id_is_rejected_with_400_not_404() {
    // axum's own `Path<u64>` rejection would be a 400 with an opaque body; the
    // middleware answers first with a field-scoped message.
    assert_rejected("/match/abc", "match_id").await;
    assert_rejected("/events/abc", "match_id").await;
}

#[tokio::test]
async fn negative_match_id_is_rejected() {
    assert_rejected("/match/-1", "match_id").await;
}

#[tokio::test]
async fn overflowing_match_id_is_rejected() {
    assert_rejected(&format!("/match/{}", "9".repeat(25)), "match_id").await;
}

#[tokio::test]
async fn sql_injection_attempt_in_the_path_is_rejected() {
    assert_rejected("/match/1%20OR%201=1", "match_id").await;
    assert_rejected("/match/1;DROP%20TABLE%20events", "match_id").await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Pagination
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn valid_pagination_is_accepted() {
    assert_accepted("/events?limit=1&offset=0").await;
    assert_accepted("/events?limit=1000&offset=5000").await;
}

#[tokio::test]
async fn zero_and_oversized_limits_are_rejected() {
    assert_rejected("/events?limit=0", "limit").await;
    assert_rejected("/events?limit=1001", "limit").await;
    assert_rejected("/events?limit=999999", "limit").await;
}

#[tokio::test]
async fn limit_error_states_the_accepted_range() {
    let (_, message) = get("/events?limit=5000").await;
    assert!(
        message.contains("between 1 and 1000"),
        "message must be actionable, got {message:?}"
    );
}

#[tokio::test]
async fn negative_and_non_numeric_pagination_is_rejected() {
    assert_rejected("/events?limit=-5", "limit").await;
    assert_rejected("/events?offset=-1", "offset").await;
    assert_rejected("/events?limit=lots", "limit").await;
    assert_rejected("/events?offset=none", "offset").await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Enumerations
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn valid_statuses_are_accepted() {
    for status in ["pending", "active", "completed", "cancelled", "expired"] {
        assert_accepted(&format!("/matches?status={status}")).await;
    }
}

#[tokio::test]
async fn unknown_status_is_rejected_and_lists_the_options() {
    let (status, message) = get("/matches?status=finished").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(message.contains("status"), "got {message:?}");
    assert!(message.contains("pending"), "got {message:?}");
}

#[tokio::test]
async fn transaction_type_filter_is_validated() {
    assert_accepted(&format!("/transactions/player/{VALID_ACCOUNT}?type=deposit")).await;
    assert_accepted(&format!("/transactions/player/{VALID_ACCOUNT}?type=payout")).await;
    assert_accepted(&format!("/transactions/player/{VALID_ACCOUNT}?type=fee")).await;
    assert_rejected(
        &format!("/transactions/player/{VALID_ACCOUNT}?type=withdrawal"),
        "type",
    )
    .await;
}

#[tokio::test]
async fn sort_parameters_are_whitelisted() {
    assert_accepted(&format!(
        "/transactions/player/{VALID_ACCOUNT}?sort_by=amount&sort_order=asc"
    ))
    .await;
    assert_rejected(
        &format!("/transactions/player/{VALID_ACCOUNT}?sort_by=stake_amount%3B%20DROP"),
        "sort_by",
    )
    .await;
    assert_rejected(
        &format!("/transactions/player/{VALID_ACCOUNT}?sort_order=random"),
        "sort_order",
    )
    .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Amounts
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn negative_amount_is_rejected() {
    assert_rejected("/events?amount=-100", "amount").await;
    assert_rejected("/events?stake_amount=-1", "stake_amount").await;
}

#[tokio::test]
async fn zero_amount_is_rejected() {
    assert_rejected("/events?amount=0", "amount").await;
}

#[tokio::test]
async fn fractional_and_non_numeric_amounts_are_rejected() {
    assert_rejected("/events?amount=1.5", "amount").await;
    assert_rejected("/events?amount=1e10", "amount").await;
    assert_rejected("/events?amount=free", "amount").await;
}

#[tokio::test]
async fn positive_amount_is_accepted() {
    assert_accepted("/events?amount=10000000").await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Game ids and tokens
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn well_formed_game_id_is_accepted() {
    assert_accepted("/events?game_id=abcd1234").await;
    assert_accepted("/events?game_id=game-001").await;
}

#[tokio::test]
async fn game_id_with_dangerous_characters_is_rejected() {
    // Percent-encoded: "abc'; DROP TABLE events;--"
    assert_rejected(
        "/events?game_id=abc%27%3B%20DROP%20TABLE%20events%3B--",
        "game_id",
    )
    .await;
    assert_rejected("/events?game_id=%3Cscript%3E", "game_id").await;
    assert_rejected("/events?game_id=..%2F..%2Fetc%2Fpasswd", "game_id").await;
}

#[tokio::test]
async fn overlong_game_id_is_rejected() {
    assert_rejected(&format!("/events?game_id={}", "a".repeat(65)), "game_id").await;
}

#[tokio::test]
async fn token_filter_accepts_symbols_and_contract_addresses() {
    assert_accepted(&format!("/transactions/player/{VALID_ACCOUNT}?token=XLM")).await;
    assert_accepted(&format!(
        "/transactions/player/{VALID_ACCOUNT}?token={VALID_CONTRACT}"
    ))
    .await;
}

#[tokio::test]
async fn token_filter_rejects_injection_and_bad_checksums() {
    assert_rejected(
        &format!("/transactions/player/{VALID_ACCOUNT}?token=XLM%27%3B--"),
        "token",
    )
    .await;

    let mut typo = VALID_CONTRACT.to_string();
    typo.pop();
    typo.push('A');
    assert_rejected(
        &format!("/transactions/player/{VALID_ACCOUNT}?token={typo}"),
        "token",
    )
    .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Dates
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rfc3339_and_date_only_values_are_accepted() {
    assert_accepted(&format!(
        "/transactions/player/{VALID_ACCOUNT}?from_date=2026-01-01&to_date=2026-12-31"
    ))
    .await;
    assert_accepted(&format!(
        "/transactions/player/{VALID_ACCOUNT}?from_date=2026-01-01T00%3A00%3A00Z"
    ))
    .await;
}

#[tokio::test]
async fn malformed_dates_are_rejected() {
    for bad in ["yesterday", "01-01-2026", "2026-13-45"] {
        assert_rejected(
            &format!("/transactions/player/{VALID_ACCOUNT}?from_date={bad}"),
            "from_date",
        )
        .await;
    }
}

#[tokio::test]
async fn inverted_date_range_is_rejected() {
    let (status, message) = get(&format!(
        "/transactions/player/{VALID_ACCOUNT}?from_date=2026-12-31&to_date=2026-01-01"
    ))
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(message.contains("to_date"), "got {message:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. Shape of the error response
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rejection_uses_the_standard_error_envelope() {
    let response = app()
        .oneshot(
            Request::builder()
                .uri("/events?limit=0")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: ApiResponse<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    assert!(!parsed.success);
    assert!(parsed.data.is_none());
    assert!(parsed.error.is_some(), "a rejection must explain itself");
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. Pass-through cases
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn unvalidated_endpoints_still_work() {
    // /health touches the database (and reports the failure in its body) but
    // must never be rejected by validation.
    let (status, _) = get("/health").await;
    assert_ne!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_query_parameters_are_ignored() {
    assert_accepted("/events?some_future_flag=1&trace=abc").await;
}

#[tokio::test]
async fn empty_parameter_values_are_treated_as_absent() {
    assert_accepted("/events?status=&limit=&player_address=").await;
}

#[tokio::test]
async fn unknown_routes_are_still_404() {
    let (status, _) = get("/no/such/route").await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "validation must not shadow routing"
    );
}
