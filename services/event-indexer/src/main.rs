use event_indexer::{api, api_cache, cache, config, db, leader, rpc};

use anyhow::Result;
use config::Config;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;

    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("event_indexer={}", config.log_level).parse()?),
        )
        .init();

    info!(
        "Event Indexer starting — instance_id={}",
        config.instance_id
    );

    // The admin-like endpoints (analytics, transaction history, stats) are
    // gated by a shared secret.  Fail closed: without a configured key they
    // answer 401 rather than leak sensitive balance data.
    match config.api_key.as_deref() {
        Some(_) => info!(
            "Admin-like endpoints (/analytics/*, /transactions/*, /stats) require an X-Api-Key header"
        ),
        None => warn!(
            "EVENT_INDEXER_API_KEY is not set — admin-like endpoints \
             (/analytics/*, /transactions/*, /stats) are refusing all requests \
             with 401 until a key is configured"
        ),
    }

    // ── PostgreSQL pools ──────────────────────────────────────────────────
    let database = Arc::new(db::Database::from_dsns(
        &config.database_url,
        &config.database_read_url,
        config.db_pool_size,
        config.db_read_pool_size,
    )?);
    database.init_schema().await?;
    info!("Database schema initialised");

    // ── In-process LRU cache ──────────────────────────────────────────────
    let cache = Arc::new(RwLock::new(cache::EventCache::new(config.cache_size)));

    // ── Shared API response cache ─────────────────────────────────────────
    // Falls back to a process-local TTL cache when REDIS_URL is unset or the
    // server is unreachable, so start-up never depends on Redis being up.
    let response_cache =
        Arc::new(api_cache::ApiCache::from_config(config.redis_url.as_deref()).await);
    info!(
        "API response cache backend: {}",
        response_cache.backend_name()
    );

    // ── Soroban RPC client ────────────────────────────────────────────────
    let rpc_client = Arc::new(rpc::SorobanRpcClient::new(&config.rpc_url)?);

    // ── Leader election ───────────────────────────────────────────────────
    let election = leader::LeaderElection::new(
        database.write_pool().clone(),
        config.instance_id.clone(),
        config.leader_ttl_secs,
        config.leader_heartbeat_secs,
    );

    // ── API server task ───────────────────────────────────────────────────
    let api_handle = {
        let db = database.clone();
        let cache = cache.clone();
        let rpc = rpc_client.clone();
        let response_cache = response_cache.clone();
        let bind_addr = config.bind_addr.clone();
        let bind_port = config.bind_port;
        let api_key = config.api_key.clone();
        tokio::spawn(async move {
            if let Err(e) = api::start_server(
                &bind_addr,
                bind_port,
                db,
                cache,
                rpc,
                response_cache,
                api_key,
            )
            .await
            {
                error!("API server error: {}", e);
            }
        })
    };

    // ── Poller task ───────────────────────────────────────────────────────
    let poller_handle = {
        let db = database.clone();
        let cache = cache.clone();
        let rpc = rpc_client.clone();
        let response_cache = response_cache.clone();
        let contract = config.contract_escrow.clone();
        let interval = config.poll_interval_secs;
        tokio::spawn(async move {
            if let Err(e) = rpc::event_poller(
                rpc,
                db,
                cache,
                response_cache,
                election,
                &contract,
                interval,
            )
            .await
            {
                error!("Event poller error: {}", e);
            }
        })
    };

    info!(
        "Event Indexer running on {}:{}",
        config.bind_addr, config.bind_port
    );

    tokio::select! {
        _ = api_handle => {
            error!("API server stopped unexpectedly");
        }
        _ = poller_handle => {
            error!("Event poller stopped unexpectedly");
        }
    }

    Ok(())
}
