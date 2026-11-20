//! Regression test for issue #1980 — `/health/ready` readiness probe.
//!
//! Before the fix, the service exposed only `/health` (liveness), which meant
//! Kubernetes could not gate traffic on dependency readiness separately from
//! process liveness.
//!
//! The fix adds `/health/ready` that checks dependency readiness (RPC reachable,
//! queue directory accessible) and returns HTTP 503 when any critical dependency
//! is not ready.  This lets Kubernetes configure independent liveness and
//! readiness probes.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;
use oracle_service::health::{CheckResult, CheckStatus, HealthChecker};
use oracle_service::oracle::{ChessComClient, LichessClient};
use oracle_service::soroban_client::SorobanClient;

mod common;

// ── Minimal test app ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct ReadyState {
    checker: Arc<HealthChecker>,
    queue_dir_accessible: bool,
}

/// Mirrors the `/health/ready` handler added in main.rs (#1980).
async fn ready_handler(State(state): State<ReadyState>) -> impl IntoResponse {
    let status = state.checker.status().await;

    let rpc_ready = matches!(
        status.checks.stellar_rpc.status,
        CheckStatus::Up | CheckStatus::Degraded
    );
    let escrow_ready = matches!(
        status.checks.escrow_contract.status,
        CheckStatus::Up | CheckStatus::Degraded
    );
    let oracle_ready = matches!(
        status.checks.oracle_contract.status,
        CheckStatus::Up | CheckStatus::Degraded
    );
    let queue_dir_ready = state.queue_dir_accessible;

    let ready = rpc_ready && escrow_ready && oracle_ready && queue_dir_ready;

    let http_status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let body = serde_json::json!({
        "ready": ready,
        "checks": {
            "stellar_rpc": rpc_ready,
            "escrow_contract": escrow_ready,
            "oracle_contract": oracle_ready,
            "queue_dir": queue_dir_ready,
        },
        "overall_health": status.status.to_string(),
    });
    (http_status, Json(body))
}

fn test_router(checker: Arc<HealthChecker>, queue_dir_accessible: bool) -> Router {
    Router::new()
        .route("/health/ready", get(ready_handler))
        .with_state(ReadyState {
            checker,
            queue_dir_accessible,
        })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// `/health/ready` returns HTTP 503 when RPC is unreachable (not ready).
#[tokio::test]
async fn ready_returns_503_when_rpc_down() {
    let dead_url = "http://127.0.0.1:19998".to_string();

    let cfg = {
        let mut c = common::test_config();
        c.rpc_url = dead_url.clone();
        c
    };

    let soroban = Arc::new(
        SorobanClient::new(
            dead_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
            cfg.max_fee_stroops,
        )
        .unwrap(),
    );
    let chess_com = Arc::new(
        ChessComClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(200))
            .unwrap(),
    );
    let lichess = Arc::new(
        LichessClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(200))
            .unwrap(),
    );

    let checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));
    checker.check_all().await;

    let app = test_router(checker, true);
    let req = axum::http::Request::builder()
        .uri("/health/ready")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "/health/ready must return 503 when RPC is unreachable"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(
        body["ready"].as_bool(),
        Some(false),
        "body.ready must be false when RPC is down"
    );
    assert_eq!(
        body["checks"]["stellar_rpc"].as_bool(),
        Some(false),
        "checks.stellar_rpc must be false"
    );
}

/// `/health/ready` returns HTTP 503 when the queue directory is not accessible.
#[tokio::test]
async fn ready_returns_503_when_queue_dir_inaccessible() {
    let dead_url = "http://127.0.0.1:19997".to_string();

    let cfg = {
        let mut c = common::test_config();
        c.rpc_url = dead_url.clone();
        c
    };

    let soroban = Arc::new(
        SorobanClient::new(
            dead_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
            cfg.max_fee_stroops,
        )
        .unwrap(),
    );
    let chess_com = Arc::new(
        ChessComClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(200))
            .unwrap(),
    );
    let lichess = Arc::new(
        LichessClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(200))
            .unwrap(),
    );

    let checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));

    // queue_dir_accessible = false simulates an inaccessible queue directory.
    let app = test_router(checker, false);
    let req = axum::http::Request::builder()
        .uri("/health/ready")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "/health/ready must return 503 when queue directory is inaccessible"
    );

    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(
        body["ready"].as_bool(),
        Some(false),
        "body.ready must be false when queue dir is inaccessible"
    );
    assert_eq!(
        body["checks"]["queue_dir"].as_bool(),
        Some(false),
        "checks.queue_dir must be false"
    );
}

/// The readiness response body always contains the expected JSON keys.
#[tokio::test]
async fn ready_response_shape_is_correct() {
    let dead_url = "http://127.0.0.1:19996".to_string();

    let cfg = {
        let mut c = common::test_config();
        c.rpc_url = dead_url.clone();
        c
    };

    let soroban = Arc::new(
        SorobanClient::new(
            dead_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
            cfg.max_fee_stroops,
        )
        .unwrap(),
    );
    let chess_com = Arc::new(
        ChessComClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(100))
            .unwrap(),
    );
    let lichess = Arc::new(
        LichessClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(100))
            .unwrap(),
    );

    let checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));

    let app = test_router(checker, true);
    let req = axum::http::Request::builder()
        .uri("/health/ready")
        .body(axum::body::Body::empty())
        .unwrap();

    let resp = app.oneshot(req).await.unwrap();
    let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(body.get("ready").is_some(), "response must have 'ready' field");
    assert!(body.get("checks").is_some(), "response must have 'checks' field");
    assert!(body.get("overall_health").is_some(), "response must have 'overall_health' field");

    let checks = &body["checks"];
    assert!(checks.get("stellar_rpc").is_some(), "checks must have stellar_rpc");
    assert!(checks.get("escrow_contract").is_some(), "checks must have escrow_contract");
    assert!(checks.get("oracle_contract").is_some(), "checks must have oracle_contract");
    assert!(checks.get("queue_dir").is_some(), "checks must have queue_dir");
}

/// The readiness probe (`/health/ready`) is distinct from the liveness probe
/// (`/health`): readiness reflects dependency state while liveness reflects
/// process state.
///
/// This test asserts that a fresh checker (no checks run yet, all Unknown) is
/// not reported as ready, since Unknown dependencies cannot gate traffic.
#[tokio::test]
async fn readiness_reflects_dependency_state_not_just_liveness() {
    let dead_url = "http://127.0.0.1:19995".to_string();

    let cfg = {
        let mut c = common::test_config();
        c.rpc_url = dead_url.clone();
        c
    };

    let soroban = Arc::new(
        SorobanClient::new(
            dead_url.clone(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
            cfg.max_fee_stroops,
        )
        .unwrap(),
    );
    let chess_com = Arc::new(
        ChessComClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(100))
            .unwrap(),
    );
    let lichess = Arc::new(
        LichessClient::new_with_base_and_timeout(dead_url.clone(), std::time::Duration::from_millis(100))
            .unwrap(),
    );

    // Do NOT call check_all() — all checks are in Unknown state.
    let checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));

    let status = checker.status().await;
    // A fresh checker has all checks as Unknown, so service_ready is false.
    assert!(
        !status.service_ready,
        "service_ready should be false when no checks have been run"
    );
}
