//! Health check module for the oracle service.
//!
//! Provides real-time liveness checks for all critical dependencies:
//! - Stellar RPC connectivity
//! - Escrow contract reachability
//! - Oracle contract reachability
//! - Chess platform API health
//!
//! The health endpoint distinguishes between:
//! - **Healthy**: All critical dependencies operational
//! - **Degraded**: Some non-critical dependencies failing but service operational
//! - **Unhealthy**: Critical dependencies down, service cannot function

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::OracleConfig;
use crate::oracle::{ChessComClient, LichessClient};
use crate::soroban_client::SorobanClient;

/// Status of a single dependency.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Up,
    Degraded,
    Down,
    RateLimited,
    Unknown,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Up => write!(f, "up"),
            CheckStatus::Degraded => write!(f, "degraded"),
            CheckStatus::Down => write!(f, "down"),
            CheckStatus::RateLimited => write!(f, "rate_limited"),
            CheckStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Result of a single health check probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub status: CheckStatus,
    pub latency_ms: u64,
    pub last_checked_at: DateTime<Utc>,
    pub consecutive_failures: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

/// Overall system health status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
        }
    }
}

/// Complete health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResponse {
    pub status: HealthStatus,
    pub service_ready: bool,
    pub network: String,
    pub contract_escrow: String,
    pub contract_oracle: String,
    pub oracle_address: String,

    // Per-dependency checks
    pub checks: HealthChecks,

    // Canary synthetic test
    pub canary_status: CanaryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canary_details: Option<String>,

    // Metadata
    pub last_full_check_at: DateTime<Utc>,
    pub uptime_seconds: u64,
}

/// Per-dependency health checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthChecks {
    pub stellar_rpc: CheckResult,
    pub escrow_contract: CheckResult,
    pub oracle_contract: CheckResult,
    pub lichess_api: CheckResult,
    pub chess_com_api: CheckResult,
}

/// Status of the synthetic canary check.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CanaryStatus {
    Pending,
    Passed,
    Failed,
}

impl std::fmt::Display for CanaryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CanaryStatus::Pending => write!(f, "pending"),
            CanaryStatus::Passed => write!(f, "passed"),
            CanaryStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Health checker that maintains and updates health state.
pub struct HealthChecker {
    inner: Arc<HealthCheckerInner>,
}

struct HealthCheckerInner {
    config: OracleConfig,
    soroban: Arc<SorobanClient>,
    chess_com: Arc<ChessComClient>,
    lichess: Arc<LichessClient>,
    state: RwLock<HealthCheckerState>,
}

struct HealthCheckerState {
    checks: HealthChecks,
    canary_status: CanaryStatus,
    canary_details: Option<String>,
    last_check_at: DateTime<Utc>,
    started_at: DateTime<Utc>,
}

impl HealthChecker {
    /// Create a new health checker from the oracle configuration.
    pub fn new(
        cfg: OracleConfig,
        soroban: Arc<SorobanClient>,
        chess_com: Arc<ChessComClient>,
        lichess: Arc<LichessClient>,
    ) -> Self {
        let now = Utc::now();
        let unknown = CheckResult {
            status: CheckStatus::Unknown,
            latency_ms: 0,
            last_checked_at: now,
            consecutive_failures: 0,
            details: Some("not yet checked".to_string()),
        };

        Self {
            inner: Arc::new(HealthCheckerInner {
                config: cfg,
                soroban,
                chess_com,
                lichess,
                state: RwLock::new(HealthCheckerState {
                    checks: HealthChecks {
                        stellar_rpc: unknown.clone(),
                        escrow_contract: unknown.clone(),
                        oracle_contract: unknown.clone(),
                        lichess_api: unknown.clone(),
                        chess_com_api: unknown,
                    },
                    canary_status: CanaryStatus::Pending,
                    canary_details: None,
                    last_check_at: now,
                    started_at: now,
                }),
            }),
        }
    }

    /// Perform all health checks (typically called periodically).
    pub async fn check_all(&self) {
        debug!("starting full health check");
        let start = std::time::Instant::now();

        // Run all checks in parallel
        let (rpc_check, escrow_check, oracle_check, lichess_check, chess_com_check) = tokio::join!(
            self.check_stellar_rpc(),
            self.check_escrow_contract(),
            self.check_oracle_contract(),
            self.check_lichess_api(),
            self.check_chess_com_api(),
        );

        let mut state = self.inner.state.write().await;
        state.checks = HealthChecks {
            stellar_rpc: rpc_check,
            escrow_contract: escrow_check,
            oracle_contract: oracle_check,
            lichess_api: lichess_check,
            chess_com_api: chess_com_check,
        };
        state.last_check_at = Utc::now();

        let elapsed = start.elapsed().as_millis();
        debug!("health check completed in {}ms", elapsed);
    }

    /// Get the current health status snapshot.
    pub async fn status(&self) -> HealthCheckResponse {
        let state = self.inner.state.read().await;

        let critical_checks = [
            state.checks.stellar_rpc.status,
            state.checks.escrow_contract.status,
            state.checks.oracle_contract.status,
        ];

        let overall_status = if critical_checks.iter().any(|s| *s == CheckStatus::Down) {
            HealthStatus::Unhealthy
        } else if critical_checks
            .iter()
            .any(|s| *s == CheckStatus::Degraded)
        {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        let service_ready = overall_status == HealthStatus::Healthy
            && state.checks.stellar_rpc.status != CheckStatus::Unknown
            && state.checks.escrow_contract.status != CheckStatus::Unknown
            && state.checks.oracle_contract.status != CheckStatus::Unknown;

        let uptime = (Utc::now() - state.started_at)
            .num_seconds()
            .max(0) as u64;

        HealthCheckResponse {
            status: overall_status,
            service_ready,
            network: std::env::var("STELLAR_NETWORK")
                .unwrap_or_else(|_| "testnet".to_string()),
            contract_escrow: self.inner.config.contract_escrow.clone(),
            contract_oracle: self.inner.config.contract_oracle.clone(),
            oracle_address: self.inner.config.oracle_address.clone(),
            checks: state.checks.clone(),
            canary_status: state.canary_status,
            canary_details: state.canary_details.clone(),
            last_full_check_at: state.last_check_at,
            uptime_seconds: uptime,
        }
    }

    // ─ Individual health checks ─────────────────────────────────────────────

    async fn check_stellar_rpc(&self) -> CheckResult {
        let start = std::time::Instant::now();
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.inner.soroban.health_check(),
        )
        .await
        {
            Ok(Ok(_)) => {
                let latency = start.elapsed().as_millis() as u64;
                CheckResult {
                    status: CheckStatus::Up,
                    latency_ms: latency,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 0,
                    details: None,
                }
            }
            Ok(Err(e)) => {
                warn!("Stellar RPC health check failed: {}", e);
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: start.elapsed().as_millis() as u64,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some(format!("RPC error: {}", e)),
                }
            }
            Err(_) => {
                warn!("Stellar RPC health check timed out");
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: 5000,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some("timeout".to_string()),
                }
            }
        }
    }

    async fn check_escrow_contract(&self) -> CheckResult {
        let start = std::time::Instant::now();
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.inner.soroban.contract_health_check(&self.inner.config.contract_escrow),
        )
        .await
        {
            Ok(Ok(_)) => {
                let latency = start.elapsed().as_millis() as u64;
                CheckResult {
                    status: CheckStatus::Up,
                    latency_ms: latency,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 0,
                    details: None,
                }
            }
            Ok(Err(e)) => {
                warn!("Escrow contract health check failed: {}", e);
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: start.elapsed().as_millis() as u64,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some(format!("Contract error: {}", e)),
                }
            }
            Err(_) => {
                warn!("Escrow contract health check timed out");
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: 5000,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some("timeout".to_string()),
                }
            }
        }
    }

    async fn check_oracle_contract(&self) -> CheckResult {
        let start = std::time::Instant::now();
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.inner.soroban.contract_health_check(&self.inner.config.contract_oracle),
        )
        .await
        {
            Ok(Ok(_)) => {
                let latency = start.elapsed().as_millis() as u64;
                CheckResult {
                    status: CheckStatus::Up,
                    latency_ms: latency,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 0,
                    details: None,
                }
            }
            Ok(Err(e)) => {
                warn!("Oracle contract health check failed: {}", e);
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: start.elapsed().as_millis() as u64,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some(format!("Contract error: {}", e)),
                }
            }
            Err(_) => {
                warn!("Oracle contract health check timed out");
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: 5000,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some("timeout".to_string()),
                }
            }
        }
    }

    async fn check_lichess_api(&self) -> CheckResult {
        let start = std::time::Instant::now();
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.inner.lichess.health_check(),
        )
        .await
        {
            Ok(Ok(_)) => {
                let latency = start.elapsed().as_millis() as u64;
                CheckResult {
                    status: CheckStatus::Up,
                    latency_ms: latency,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 0,
                    details: None,
                }
            }
            Ok(Err(e)) => {
                let status = if e.to_string().contains("rate") {
                    CheckStatus::RateLimited
                } else {
                    CheckStatus::Down
                };
                warn!("Lichess API health check failed: {}", e);
                CheckResult {
                    status,
                    latency_ms: start.elapsed().as_millis() as u64,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some(format!("API error: {}", e)),
                }
            }
            Err(_) => {
                warn!("Lichess API health check timed out");
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: 5000,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some("timeout".to_string()),
                }
            }
        }
    }

    async fn check_chess_com_api(&self) -> CheckResult {
        let start = std::time::Instant::now();
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.inner.chess_com.health_check(),
        )
        .await
        {
            Ok(Ok(_)) => {
                let latency = start.elapsed().as_millis() as u64;
                CheckResult {
                    status: CheckStatus::Up,
                    latency_ms: latency,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 0,
                    details: None,
                }
            }
            Ok(Err(e)) => {
                let status = if e.to_string().contains("rate") {
                    CheckStatus::RateLimited
                } else {
                    CheckStatus::Down
                };
                warn!("Chess.com API health check failed: {}", e);
                CheckResult {
                    status,
                    latency_ms: start.elapsed().as_millis() as u64,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some(format!("API error: {}", e)),
                }
            }
            Err(_) => {
                warn!("Chess.com API health check timed out");
                CheckResult {
                    status: CheckStatus::Down,
                    latency_ms: 5000,
                    last_checked_at: Utc::now(),
                    consecutive_failures: 1,
                    details: Some("timeout".to_string()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_status_ordering() {
        assert!(HealthStatus::Unhealthy > HealthStatus::Degraded);
        assert!(HealthStatus::Degraded > HealthStatus::Healthy);
    }

    #[test]
    fn check_status_display() {
        assert_eq!(CheckStatus::Up.to_string(), "up");
        assert_eq!(CheckStatus::Down.to_string(), "down");
        assert_eq!(CheckStatus::RateLimited.to_string(), "rate_limited");
    }

    #[test]
    fn canary_status_display() {
        assert_eq!(CanaryStatus::Pending.to_string(), "pending");
        assert_eq!(CanaryStatus::Passed.to_string(), "passed");
        assert_eq!(CanaryStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn health_status_display() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }

    #[test]
    fn serialization_check_result() {
        let now = Utc::now();
        let result = CheckResult {
            status: CheckStatus::Up,
            latency_ms: 42,
            last_checked_at: now,
            consecutive_failures: 0,
            details: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"up\""));
        assert!(json.contains("\"latency_ms\":42"));
    }
}
