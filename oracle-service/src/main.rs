//! Oracle service entry point.
//!
//! Starts four concurrent tasks:
//!
//! 1. **Health HTTP endpoint** on `0.0.0.0:8000` — exposes `/health` with
//!    real-time dependency checks and `/metrics` for Prometheus scraping.
//!    Also exposes `/api/docs` (Swagger UI) and `/api/openapi.yaml` (raw schema).
//!
//! 2. **Health check poller** — runs comprehensive liveness checks every 30
//!    seconds, updating the health status with real RPC, contract, and API
//!    connectivity information.
//!
//! 3. **Reconciliation task** — wakes every `ORACLE_RECONCILIATION_INTERVAL_SECS`
//!    seconds, pages through the escrow contract's `Active` matches, and
//!    enqueues any that the oracle contract doesn't yet have a result for.
//!    This is how a match actually enters the pipeline — see
//!    `oracle_service::poller` for details.
//!
//! 4. **Pipeline poller** — wakes every `ORACLE_POLL_INTERVAL_SECS` seconds,
//!    processes all due pending-verification entries, and submits results
//!    on-chain via Soroban RPC.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use std::sync::Arc;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use oracle_service::{
    config,
    health::HealthChecker,
    metrics,
    middleware::{
        rate_limit::{self, RateLimitState},
        waf::{self, WafState},
    },
    oracle::{ChessComClient, LichessClient},
    poller::Poller,
    soroban_client::SorobanClient,
};

// ── Application state ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    health_checker: Arc<HealthChecker>,
    rate_limiter: RateLimitState,
    waf: WafState,
    /// Optional HTTP Basic Auth credentials for the `/metrics` endpoint.
    /// When `None`, the endpoint is left open for backward compatibility.
    metrics_auth: Option<Arc<MetricsAuth>>,
}

/// Credentials guarding the `/metrics` scrape endpoint.
struct MetricsAuth {
    username: String,
    password: String,
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    use oracle_service::health::HealthStatus;

    let status = state.health_checker.status().await;
    let http_status = if status.status == HealthStatus::Unhealthy {
        StatusCode::SERVICE_UNAVAILABLE // 503
    } else {
        StatusCode::OK // 200
    };
    let body = serde_json::to_value(&status).unwrap_or(serde_json::json!(null));
    (http_status, Json(body))
}

/// `GET /metrics` — Prometheus text-format metrics scrape endpoint.
///
/// Exposes `oracle_queue_depth` and `oracle_dead_letter_count` gauges (plus
/// standard process metrics when the `process` feature is enabled) in the
/// text/plain; version=0.0.4 format expected by Prometheus.
///
/// When `ORACLE_METRICS_AUTH_USERNAME` / `ORACLE_METRICS_AUTH_PASSWORD` are
/// configured, requests must carry a valid `Authorization: Basic ...` header;
/// otherwise a `401 Unauthorized` with a `WWW-Authenticate` challenge is
/// returned. When unset, the endpoint stays open for backward compatibility.
async fn metrics_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(auth) = state.metrics_auth.as_ref() {
        let authorized = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .map(|value| metrics::check_basic_auth(value, &auth.username, &auth.password))
            .unwrap_or(false);

        if !authorized {
            return (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Basic realm=\"metrics\"")],
                "Unauthorized",
            )
                .into_response();
        }
    }

    let body = metrics::render();
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        body,
    )
        .into_response()
}

// ── API documentation handlers ────────────────────────────────────────────────

/// `GET /api/docs` — serves the Swagger UI HTML page.
///
/// The HTML page loads `swagger-ui-dist` from unpkg CDN and points it at
/// `/api/openapi.yaml` so the interactive docs always reflect the canonical
/// schema bundled with the service binary.
async fn api_docs_ui() -> Html<&'static str> {
    Html(include_str!("../../docs/swagger-ui.html"))
}

/// `GET /api/openapi.yaml` — serves the raw OpenAPI 3.0 schema.
///
/// Swagger UI (and any other tooling) fetches this file to render the
/// interactive documentation. Clients can also download it directly for
/// code-generation or schema validation.
async fn api_openapi_yaml() -> Response {
    (
        [(header::CONTENT_TYPE, "application/yaml")],
        include_str!("../../docs/openapi.yaml"),
    )
        .into_response()
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
    let reconciliation_interval = cfg.reconciliation_interval_secs;
    let http_addr = cfg.http_addr.clone();

    // ── Optional Basic Auth for /metrics ──────────────────────────────────
    // Both vars must be set for auth to be enabled; a partial configuration
    // is treated as disabled (with a warning) to avoid locking out scrapers.
    let metrics_auth = match (
        cfg.metrics_auth_username.clone(),
        cfg.metrics_auth_password.clone(),
    ) {
        (Some(username), Some(password)) => {
            info!("metrics endpoint authentication enabled (user: {})", username);
            Some(Arc::new(MetricsAuth { username, password }))
        }
        (None, None) => {
            info!("metrics endpoint authentication disabled");
            None
        }
        _ => {
            warn!(
                "only one of ORACLE_METRICS_AUTH_USERNAME / ORACLE_METRICS_AUTH_PASSWORD is set; \
                 metrics endpoint authentication disabled"
            );
            None
        }
    };

    // ── Initialize dependencies ───────────────────────────────────────────
    let soroban = match SorobanClient::new(
        cfg.rpc_url.clone(),
        cfg.network_passphrase.clone(),
        &cfg.contract_escrow,
        cfg.max_fee_stroops,
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

    // ── Pipeline poller (constructed before cfg is moved) ─────────────────
    let poller = match Poller::new(&cfg) {
        Ok(p) => p,
        Err(e) => {
            error!("failed to initialise pipeline poller: {}", e);
            std::process::exit(1);
        }
    };

    // ── Health checker (consumes cfg) ─────────────────────────────────────
    let health_checker = Arc::new(HealthChecker::new(cfg, soroban, chess_com, lichess));

    // Perform initial health check
    info!("performing initial health check");
    health_checker.check_all().await;

    // ── HTTP server state ─────────────────────────────────────────────────
    let app_state = AppState {
        health_checker: health_checker.clone(),
        rate_limiter: RateLimitState::new(),
        waf: WafState::new(),
        metrics_auth,
    };

    // WAF is layered outermost so its burst/body/URI checks reject bad
    // traffic before the token-bucket rate limiter does any bookkeeping.
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics_handler))
        .route("/api/docs", get(api_docs_ui))
        .route("/api/openapi.yaml", get(api_openapi_yaml))
        .with_state(app_state.clone())
        .layer(axum::middleware::from_fn_with_state(
            app_state.rate_limiter.clone(),
            rate_limit::rate_limit_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            app_state.waf.clone(),
            waf::waf_middleware,
        ));

    let listener = match tokio::net::TcpListener::bind(&http_addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("failed to bind to {}: {}", http_addr, e);
            std::process::exit(1);
        }
    };

    info!("oracle service listening on http://{}", http_addr);
    info!("API docs available at http://{}/api/docs", http_addr);
    info!("Prometheus metrics available at http://{}/metrics", http_addr);
    if cfg_dry_run {
        warn!("DRY_RUN enabled: settlements will be logged but not submitted");
    }

    // ── Run all four tasks concurrently ─────────────────────────────────────
    tokio::select! {
        res = axum::serve(listener, app) => {
            if let Err(e) = res {
                error!("HTTP server error: {}", e);
            }
        }
        _ = run_health_check_loop(health_checker) => {
            // health loop never returns normally
        }
        _ = poller.clone().run_reconciliation_loop(reconciliation_interval) => {
            // run_reconciliation_loop never returns normally
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
