//! Oracle service entry point.
//!
//! Starts two concurrent tasks:
//!
//! 1. **Health HTTP endpoint** on `0.0.0.0:8000` — exposes `/health` and
//!    `/metrics` for liveness probes and Prometheus scraping.
//!
//! 2. **Pipeline poller** — wakes every `ORACLE_POLL_INTERVAL_SECS` seconds,
//!    processes all due pending-verification entries, and submits results
//!    on-chain via Soroban RPC.

use axum::{extract::State, routing::get, Json, Router};
use chrono::Utc;
use serde::Serialize;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use oracle_service::{config, poller::Poller};

// ── Health endpoint types ─────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
struct HealthStatus {
    status: String,
    network: String,
    contract_address: String,
    oracle_address: String,
    last_checked_at: String,
}

#[derive(Clone)]
struct AppState {
    health: HealthStatus,
}

async fn health_check(State(state): State<AppState>) -> Json<HealthStatus> {
    let mut h = state.health.clone();
    h.last_checked_at = Utc::now().to_rfc3339();
    Json(h)
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
    let network = std::env::var("STELLAR_NETWORK").unwrap_or_else(|_| "testnet".to_string());
    let contract_escrow = cfg.contract_escrow.clone();
    let oracle_address = cfg.oracle_address.clone();

    // ── Pipeline poller ───────────────────────────────────────────────────
    let poller = match Poller::new(&cfg) {
        Ok(p) => p,
        Err(e) => {
            error!("failed to initialise pipeline poller: {}", e);
            std::process::exit(1);
        }
    };

    // ── Health server ─────────────────────────────────────────────────────
    let state = AppState {
        health: HealthStatus {
            status: "healthy".to_string(),
            network,
            contract_address: contract_escrow,
            oracle_address,
            last_checked_at: Utc::now().to_rfc3339(),
        },
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .with_state(state);

    let listener = match tokio::net::TcpListener::bind("0.0.0.0:8000").await {
        Ok(l) => l,
        Err(e) => {
            error!("failed to bind to port 8000: {}", e);
            std::process::exit(1);
        }
    };

    info!("oracle service listening on http://0.0.0.0:8000");

    // ── Run both tasks concurrently ───────────────────────────────────────
    tokio::select! {
        res = axum::serve(listener, app) => {
            if let Err(e) = res {
                error!("HTTP server error: {}", e);
            }
        }
        _ = poller.run_loop(poll_interval) => {
            // run_loop never returns normally
        }
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
