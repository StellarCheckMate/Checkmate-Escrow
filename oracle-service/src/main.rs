//! Oracle service entry point.
//!
//! Starts three concurrent tasks:
//!
//! 1. **Health HTTP endpoint** on `0.0.0.0:8000` — exposes `/health` with
//!    real-time dependency checks and `/metrics` for Prometheus scraping.
//!
//! 2. **Health check poller** — runs comprehensive liveness checks every 30
//!    seconds, updating the health status with real RPC, contract, and API
//!    connectivity information.
//!
//! 3. **Pipeline poller** — wakes every `ORACLE_POLL_INTERVAL_SECS` seconds,
//!    processes all due pending-verification entries, and submits results
//!    on-chain via Soroban RPC.

use axum::{extract::State, routing::get, Json, Router};
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use oracle_service::{
    config, health::HealthChecker, oracle::{ChessComClient, LichessClient}, poller::Poller,
    soroban_client::SorobanClient,
};

// ── Application state ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    health_checker: Arc<HealthChecker>,
}

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    let status = state.health_checker.status().await;
    Json(serde_json::to_value(&status).unwrap_or(serde_json::json!(null)))
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    // ── Logging ───────────────────────────────────────────────────────────
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    // ── Config ────────────────────────────────────────────────────────────
    // Load .env if present (development convenience).
    #[cfg(debug_assertions)]
    {
        let _ = load_dotenv();
    }

    let cfg = match config::load() {
        Ok(c) => {
            info!("oracle config loaded: {:?}", c);
            c
        }
        Err(e) => {
            error!("failed to load oracle config: {}", e);
            std::process::exit(1);
        }
    };

    let poll_interval = cfg.poll_interval_secs;

    // ── Initialize dependencies ───────────────────────────────────────────
    let soroban = match SorobanClient::new(
        cfg.rpc_url.clone(),
        cfg.network_passphrase.clone(),
        &cfg.contract_escrow,
    ) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            error!("failed to initialize Soroban client: {}", e);
            std::process::exit(1);
        }
    };

    let chess_com = match ChessComClient::new() {
        Ok(c) => Arc::new(c),
        Err(e) => {
            error!("failed to initialize Chess.com client: {}", e);
            std::process::exit(1);
        }
    };

    let lichess = match LichessClient::new() {
        Ok(l) => Arc::new(l),
        Err(e) => {
            error!("failed to initialize Lichess client: {}", e);
            std::process::exit(1);
        }
    };

    // ── Health checker ────────────────────────────────────────────────────
    let health_checker = Arc::new(HealthChecker::new(cfg.clone(), soroban, chess_com, lichess));

    // Perform initial health check
    info!("performing initial health check");
    health_checker.check_all().await;

    // ── Pipeline poller ───────────────────────────────────────────────────
    let poller = match Poller::new(&cfg) {
        Ok(p) => p,
        Err(e) => {
            error!("failed to initialise pipeline poller: {}", e);
            std::process::exit(1);
        }
    };

    // ── HTTP server state ─────────────────────────────────────────────────
    let app_state = AppState {
        health_checker: health_checker.clone(),
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .with_state(app_state);

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8000").await {
        Ok(l) => l,
        Err(e) => {
            error!("failed to bind to port 8000: {}", e);
            std::process::exit(1);
        }
    };

    info!("oracle service listening on http://0.0.0.0:8000");

    // ── Run all three tasks concurrently ───────────────────────────────────
    tokio::select! {
        res = axum::serve(listener, app) => {
            if let Err(e) = res {
                error!("HTTP server error: {}", e);
            }
        }
        _ = run_health_check_loop(health_checker) => {
            // health loop never returns normally
        }
        _ = poller.run_loop(poll_interval) => {
            // run_loop never returns normally
        }
    }
}

/// Periodically run comprehensive health checks.
async fn run_health_check_loop(health_checker: Arc<HealthChecker>) {
    let check_interval = std::time::Duration::from_secs(30);
    let mut ticker = tokio::time::interval(check_interval);

    loop {
        ticker.tick().await;
        health_checker.check_all().await;
    }
}

/// Load a `.env` file from the current directory (dev only, best-effort).
#[cfg(debug_assertions)]
fn load_dotenv() -> std::io::Result<()> {
    let path = std::path::Path::new(".env");
    if !path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(path)?;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            // Only set if not already present in the environment.
            if std::env::var(key.trim()).is_err() {
                std::env::set_var(key.trim(), val.trim());
            }
        }
    }
    Ok(())
}
