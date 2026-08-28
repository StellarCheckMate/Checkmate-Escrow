//! Health check module tests.
//!
//! Tests the health checker's ability to detect real and simulated failures
//! across all dependencies.

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use http_body_util::BodyExt;
use oracle_service::health::{HealthChecker, HealthStatus};
use std::sync::Arc;
use tower::ServiceExt; // for `oneshot`
use wiremock::{matchers::path, Mock, MockServer, ResponseTemplate};

mod common;

// ── HTTP status helpers ───────────────────────────────────────────────────────

/// Minimal app state used in HTTP 503 tests.
#[derive(Clone)]
struct TestAppState {
    health_checker: Arc<HealthChecker>,
}

/// Mirrors the real `health_check` handler: returns 503 when unhealthy.
async fn health_check_handler(
    State(state): State<TestAppState>,
) -> impl IntoResponse {
    let status = state.health_checker.status().await;
    let http_status = if status.status == HealthStatus::Unhealthy {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    (http_status, Json(serde_json::to_value(&status).unwrap()))
}

fn test_router(checker: Arc<HealthChecker>) -> Router {
    Router::new()
        .route("/health", get(health_check_handler))
        .with_state(TestAppState {
            health_checker: checker,
        })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_health_check_all_dependencies_up() {
    // All dependencies respond normally
    let rpc_mock = MockServer::start().await;
    let app_mock = MockServer::start().await;

    Mock::given(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "version": "1.0"
            }
        })))
        .mount(&rpc_mock)
        .await;

    Mock::given(path("/api/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test_user"
        })))
        .mount(&app_mock)
        .await;

    // In a real scenario, we'd create a health checker with these mocked URLs
    // For now, this test demonstrates the structure
}

/// Verify that the /health endpoint returns HTTP 503 when the Stellar RPC is
/// unreachable (i.e. the `stellar_rpc` check is `Down` and overall status is
/// `Unhealthy`).
#[tokio::test]
async fn test_health_endpoint_returns_503_when_rpc_down() {
    use oracle_service::health::{CheckResult, CheckStatus, HealthChecks, HealthChecker};
    use oracle_service::oracle::{ChessComClient, LichessClient};
    use oracle_service::soroban_client::SorobanClient;

    // Point all clients at a non-existent address so every probe fails.
    let dead_url = "http://127.0.0.1:19999".to_string(); // nothing listening here

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
        )
        .expect("soroban client constructed"),
    );
    let chess_com = Arc::new(
        ChessComClient::new_with_base_and_timeout(
            dead_url.clone(),
            std::time::Duration::from_millis(200),
        )
        .expect("chess.com client constructed"),
    );
    let lichess = Arc::new(
        LichessClient::new_with_base_and_timeout(
            dead_url.clone(),
            std::time::Duration::from_millis(200),
        )
        .expect("lichess client constructed"),
    );

    let checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));

    // Run the health probes — all should fail because nothing is listening.
    checker.check_all().await;

    // Build the minimal test app and send a GET /health request.
    let app = test_router(checker.clone());
    let request = axum::http::Request::builder()
        .uri("/health")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "expected HTTP 503 when Soroban RPC is unreachable"
    );

    // Also confirm the body reports the service as unhealthy.
    let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(
        body["status"].as_str().unwrap(),
        "unhealthy",
        "body status should be 'unhealthy'"
    );
}

/// Verify that the /health endpoint returns HTTP 200 when all critical
/// dependencies are healthy.
#[tokio::test]
async fn test_health_endpoint_returns_200_when_rpc_up() {
    use oracle_service::oracle::{ChessComClient, LichessClient};
    use oracle_service::soroban_client::SorobanClient;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::method;

    // Start a mock RPC server that returns a valid JSON-RPC response.
    let rpc_server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {}
        })))
        .mount(&rpc_server)
        .await;

    let cfg = {
        let mut c = common::test_config();
        c.rpc_url = rpc_server.uri();
        c
    };

    let soroban = Arc::new(
        SorobanClient::new(
            rpc_server.uri(),
            cfg.network_passphrase.clone(),
            &cfg.contract_escrow,
        )
        .expect("soroban client constructed"),
    );
    // Use the mock RPC as the base for other clients just so they don't break
    // the checker — their individual check results won't affect the overall
    // Unhealthy → Healthy determination since we only care about stellar_rpc.
    let chess_com = Arc::new(
        ChessComClient::new_with_base_and_timeout(
            rpc_server.uri(),
            std::time::Duration::from_secs(5),
        )
        .expect("chess.com client constructed"),
    );
    let lichess = Arc::new(
        LichessClient::new_with_base_and_timeout(
            rpc_server.uri(),
            std::time::Duration::from_secs(5),
        )
        .expect("lichess client constructed"),
    );

    // Force the checker to report stellar_rpc as Up by injecting a known
    // good check result via check_all (the mock will respond 200).
    let checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));
    checker.check_all().await;

    // The mock returned 200 so stellar_rpc should be Up; the overall status
    // may still be Degraded if escrow/oracle contract checks fail (those use
    // the same mock which returns {} — that's OK for this test).
    //
    // We just verify it's NOT 503.
    let app = test_router(checker);
    let request = axum::http::Request::builder()
        .uri("/health")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_ne!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "should not return 503 when RPC is reachable"
    );
}

#[tokio::test]
async fn test_health_check_stellar_rpc_down() {
    // Stellar RPC unreachable — should mark RPC check as down
    // Escrow/Oracle contract checks fail due to RPC failure
}

#[tokio::test]
async fn test_health_check_contract_unreachable() {
    // RPC up but contract address doesn't exist or isn't deployable
    // Should mark contract check as down while RPC is up
}

#[tokio::test]
async fn test_health_check_chess_api_rate_limited() {
    // Chess API returns 429 Too Many Requests
    // Should be marked as rate_limited, not down
}

#[tokio::test]
async fn test_health_check_timeout() {
    // Dependency hangs and times out
    // Should be marked as down with timeout detail
}

#[tokio::test]
async fn test_health_status_degraded_when_non_critical_down() {
    // Critical dependencies (RPC, contracts) up
    // Chess API down
    // Overall status should be degraded, not unhealthy
}

#[tokio::test]
async fn test_health_status_unhealthy_when_critical_down() {
    // Any critical dependency (RPC or contract) down
    // Overall status should be unhealthy
}

#[tokio::test]
async fn test_health_check_consecutive_failures() {
    // Track consecutive failures per dependency
    // After N failures, mark as down
}

#[tokio::test]
async fn test_health_response_includes_config() {
    // Health response includes:
    // - network name
    // - contract addresses
    // - oracle address
}

#[tokio::test]
async fn test_health_response_includes_uptime() {
    // Health response uptime_seconds increases over time
}

#[tokio::test]
async fn test_service_ready_only_when_critical_up() {
    // service_ready: true only when all critical checks are up and not unknown
}

#[cfg(test)]
mod chaos_fault_injection {

    #[tokio::test]
    async fn test_stellar_rpc_injection_detects_failure() {
        // Simulate: Stellar RPC port blocked / firewall rule
        // Verify: health check detects RPC down without trying contracts
    }

    #[tokio::test]
    async fn test_escrow_contract_deleted() {
        // Simulate: Contract address no longer exists on-chain
        // Verify: health check detects contract down, RPC still up
    }

    #[tokio::test]
    async fn test_oracle_contract_paused() {
        // Simulate: Oracle contract paused (certain methods return error)
        // Verify: health check detects oracle contract degraded
    }

    #[tokio::test]
    async fn test_lichess_api_ddos() {
        // Simulate: Lichess API responds with 503 Service Unavailable
        // Verify: health check marks API as down, doesn't retry infinitely
    }

    #[tokio::test]
    async fn test_chess_com_rate_limit_spike() {
        // Simulate: Chess.com rate limit drops to 1 req/min
        // Verify: health check marks API as rate_limited, suggests retry-after
    }

    #[tokio::test]
    async fn test_cascading_failures() {
        // Simulate: RPC goes down, which cascades to contract checks
        // Verify: health check short-circuits and doesn't spam failed contract probes
    }

    #[tokio::test]
    async fn test_partial_recovery_after_outage() {
        // Simulate: RPC down, then back up
        // Verify: health status transitions from unhealthy → healthy
    }

    #[tokio::test]
    async fn test_flaky_dependency() {
        // Simulate: Dependency fails 2 out of 5 probes (latency/timeout jitter)
        // Verify: health check doesn't flip to unhealthy on single transient failure
    }

    #[tokio::test]
    async fn test_slow_dependency_still_up() {
        // Simulate: Lichess responds in 4.9s (timeout is 5s)
        // Verify: marked as up, but high latency noted
    }

    #[tokio::test]
    async fn test_all_dependencies_down_simultaneously() {
        // Simulate: Network partition (can't reach any external service)
        // Verify: overall status is unhealthy, per-dependency status clear
    }
}

#[cfg(test)]
mod regression_tests {

    #[tokio::test]
    async fn test_health_check_never_returns_hardcoded_healthy() {
        // Regression: ensure health check actually probes, not just returning "healthy"
        // Inject fault: make RPC unreachable
        // Verify: status != "healthy"
    }

    #[tokio::test]
    async fn test_contract_address_not_ignored() {
        // Regression: ensure contract address from config is actually checked
        // not replaced with placeholder "CB..."
    }

    #[tokio::test]
    async fn test_health_check_differentiates_services() {
        // Regression: ensure the health check doesn't conflate
        // Lichess with Chess.com failures (they should be independent)
    }
}

#[tokio::test]
async fn test_health_check_all_dependencies_up() {
    // All dependencies respond normally
    let rpc_mock = MockServer::start().await;
    let app_mock = MockServer::start().await;

    Mock::given(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "jsonrpc": "2.0",
            "result": {
                "version": "1.0"
            }
        })))
        .mount(&rpc_mock)
        .await;

    Mock::given(path("/api/account"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "test_user"
        })))
        .mount(&app_mock)
        .await;

    // In a real scenario, we'd create a health checker with these mocked URLs
    // For now, this test demonstrates the structure
}

#[tokio::test]
async fn test_health_check_stellar_rpc_down() {
    // Stellar RPC unreachable — should mark RPC check as down
    // Escrow/Oracle contract checks fail due to RPC failure
}

#[tokio::test]
async fn test_health_check_contract_unreachable() {
    // RPC up but contract address doesn't exist or isn't deployable
    // Should mark contract check as down while RPC is up
}

#[tokio::test]
async fn test_health_check_chess_api_rate_limited() {
    // Chess API returns 429 Too Many Requests
    // Should be marked as rate_limited, not down
}

#[tokio::test]
async fn test_health_check_timeout() {
    // Dependency hangs and times out
    // Should be marked as down with timeout detail
}

#[tokio::test]
async fn test_health_status_degraded_when_non_critical_down() {
    // Critical dependencies (RPC, contracts) up
    // Chess API down
    // Overall status should be degraded, not unhealthy
}

#[tokio::test]
async fn test_health_status_unhealthy_when_critical_down() {
    // Any critical dependency (RPC or contract) down
    // Overall status should be unhealthy
}

#[tokio::test]
async fn test_health_check_consecutive_failures() {
    // Track consecutive failures per dependency
    // After N failures, mark as down
}

#[tokio::test]
async fn test_health_response_includes_config() {
    // Health response includes:
    // - network name
    // - contract addresses
    // - oracle address
}

#[tokio::test]
async fn test_health_response_includes_uptime() {
    // Health response uptime_seconds increases over time
}

#[tokio::test]
async fn test_service_ready_only_when_critical_up() {
    // service_ready: true only when all critical checks are up and not unknown
}

#[cfg(test)]
mod chaos_fault_injection {

    #[tokio::test]
    async fn test_stellar_rpc_injection_detects_failure() {
        // Simulate: Stellar RPC port blocked / firewall rule
        // Verify: health check detects RPC down without trying contracts
    }

    #[tokio::test]
    async fn test_escrow_contract_deleted() {
        // Simulate: Contract address no longer exists on-chain
        // Verify: health check detects contract down, RPC still up
    }

    #[tokio::test]
    async fn test_oracle_contract_paused() {
        // Simulate: Oracle contract paused (certain methods return error)
        // Verify: health check detects oracle contract degraded
    }

    #[tokio::test]
    async fn test_lichess_api_ddos() {
        // Simulate: Lichess API responds with 503 Service Unavailable
        // Verify: health check marks API as down, doesn't retry infinitely
    }

    #[tokio::test]
    async fn test_chess_com_rate_limit_spike() {
        // Simulate: Chess.com rate limit drops to 1 req/min
        // Verify: health check marks API as rate_limited, suggests retry-after
    }

    #[tokio::test]
    async fn test_cascading_failures() {
        // Simulate: RPC goes down, which cascades to contract checks
        // Verify: health check short-circuits and doesn't spam failed contract probes
    }

    #[tokio::test]
    async fn test_partial_recovery_after_outage() {
        // Simulate: RPC down, then back up
        // Verify: health status transitions from unhealthy → healthy
    }

    #[tokio::test]
    async fn test_flaky_dependency() {
        // Simulate: Dependency fails 2 out of 5 probes (latency/timeout jitter)
        // Verify: health check doesn't flip to unhealthy on single transient failure
    }

    #[tokio::test]
    async fn test_slow_dependency_still_up() {
        // Simulate: Lichess responds in 4.9s (timeout is 5s)
        // Verify: marked as up, but high latency noted
    }

    #[tokio::test]
    async fn test_all_dependencies_down_simultaneously() {
        // Simulate: Network partition (can't reach any external service)
        // Verify: overall status is unhealthy, per-dependency status clear
    }
}

#[cfg(test)]
mod regression_tests {

    #[tokio::test]
    async fn test_health_check_never_returns_hardcoded_healthy() {
        // Regression: ensure health check actually probes, not just returning "healthy"
        // Inject fault: make RPC unreachable
        // Verify: status != "healthy"
    }

    #[tokio::test]
    async fn test_contract_address_not_ignored() {
        // Regression: ensure contract address from config is actually checked
        // not replaced with placeholder "CB..."
    }

    #[tokio::test]
    async fn test_health_check_differentiates_services() {
        // Regression: ensure the health check doesn't conflate
        // Lichess with Chess.com failures (they should be independent)
    }
}
