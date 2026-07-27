//! REST API server.
//!
//! All query handlers use `db.query_*` methods which route through the
//! **read pool** (`read_pool` in `Database`).  If a read-replica DSN is
//! configured (`DATABASE_READ_URL`), read traffic is automatically spread
//! across replicas without any code changes here.
//!
//! ## Response caching
//! The high-traffic read endpoints are memoised in [`crate::api_cache`]:
//! the match list for 10 s, a single match for 5 s and analytics for 60 s.
//! Match entries are invalidated eagerly by the poller whenever a contract
//! event for that match is ingested, so a state change never waits for a TTL.
//! Cache failures are logged and treated as misses — a cache outage costs
//! latency, never correctness.
//!
//! ## Input validation
//! Every request passes through [`crate::validation::validate_request`] before
//! reaching a handler, so malformed addresses, amounts, ids, dates and page
//! windows are answered with a `400` and a field-scoped message instead of
//! reaching the database.  Handlers re-validate the values they need to parse;
//! validation is idempotent.

use axum::{
    async_trait,
    extract::{rejection::QueryRejection, FromRequestParts, Path, Query, State},
    http::{request::Parts, StatusCode},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::{
    api_cache::{self, ApiCache},
    cache::EventCache,
    db::Database,
    models::{IndexedEvent, MatchInfo, MatchStatus, QueryFilters},
    rpc::SorobanRpcClient,
    transactions::{
        SortOrder, TransactionHistoryFilters, TransactionPage, TransactionSortField,
        DEFAULT_PAGE_LIMIT,
    },
    validation::{self, ValidationError},
};

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub cache: Arc<RwLock<EventCache>>,
    pub rpc: Arc<SorobanRpcClient>,
    /// Shared TTL response cache (Redis, or process-local when unconfigured).
    pub api_cache: Arc<ApiCache>,
}

// ── Response envelope ─────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

// ── Custom query extractor ────────────────────────────────────────────────────

/// A custom `Query` extractor that returns a `400 ApiResponse` on
/// deserialization failure instead of axum's default 422 plain-text body.
pub struct TypedQuery<T>(pub T);

#[async_trait]
impl<T, S> FromRequestParts<S> for TypedQuery<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiResponse<()>>);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        axum::extract::Query::<T>::from_request_parts(parts, state)
            .await
            .map(|axum::extract::Query(inner)| TypedQuery(inner))
            .map_err(|e: QueryRejection| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(format!("Invalid query parameter: {}", e.body_text())),
                    }),
                )
            })
    }
}

// ── Query param types ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct EventQuery {
    pub player_address: Option<String>,
    pub status: Option<MatchStatus>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct MatchQuery {
    pub status: Option<MatchStatus>,
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Transaction-history query parameters.
///
/// Every field is received as a raw string so the validators can produce a
/// field-scoped message instead of serde's generic "failed to deserialize"
/// rejection.
#[derive(Deserialize, Default)]
pub struct TransactionHistoryQuery {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
    /// Accepted as either `type` (documented) or `tx_type`.
    #[serde(alias = "type")]
    pub tx_type: Option<String>,
    pub token: Option<String>,
    pub sort_by: Option<String>,
    #[serde(alias = "order")]
    pub sort_order: Option<String>,
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn build_router(
    db: Arc<Database>,
    cache: Arc<RwLock<EventCache>>,
    rpc: Arc<SorobanRpcClient>,
    api_cache: Arc<ApiCache>,
) -> Router {
    let state = AppState {
        db,
        cache,
        rpc,
        api_cache,
    };
    Router::new()
        .route("/health", get(health_check))
        .route("/events", get(get_events))
        .route("/events/:match_id", get(get_match_events))
        .route("/matches", get(get_matches))
        .route("/match/:match_id", get(get_match_info))
        .route(
            "/transactions/player/:player_address",
            get(get_player_transactions),
        )
        .route("/stats", get(get_stats))
        // Validation runs before routing dispatches to a handler, so malformed
        // input never reaches the database.
        .layer(axum::middleware::from_fn(validation::validate_request))
        .with_state(state)
}

pub async fn start_server(
    bind_addr: &str,
    bind_port: u16,
    db: Arc<Database>,
    cache: Arc<RwLock<EventCache>>,
    rpc: Arc<SorobanRpcClient>,
    api_cache: Arc<ApiCache>,
) -> anyhow::Result<()> {
    let app = build_router(db, cache, rpc, api_cache);

    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", bind_addr, bind_port)).await?;

    info!("API server listening on {}:{}", bind_addr, bind_port);

    axum::serve(listener, app).await?;

    Ok(())
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.db.ping().await {
        Ok(_) => Json(serde_json::json!({"db": "ok"})),
        Err(e) => Json(serde_json::json!({"db": "error", "detail": e.to_string()})),
    }
}

/// `GET /events` – query events with optional filters.
///
/// All filtering and sorting happens via the **read pool** so this endpoint
/// scales horizontally when `DATABASE_READ_URL` points to a replica.
async fn get_events(
    State(state): State<AppState>,
    TypedQuery(query): TypedQuery<EventQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<IndexedEvent>>>) {
    let filters = QueryFilters {
        player_address: query.player_address,
        status: query.status,
        start_date: None,
        end_date: None,
        limit: query.limit.or(Some(100)),
        offset: query.offset,
    };

    match state.db.query_events(&filters).await {
        Ok(events) => {
            if events.is_empty() {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some("No events found".to_string()),
                    }),
                )
            } else {
                (
                    StatusCode::OK,
                    Json(ApiResponse {
                        success: true,
                        data: Some(events),
                        error: None,
                    }),
                )
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            }),
        ),
    }
}

/// `GET /events/:match_id` – events for a single match with cache-first lookup.
async fn get_match_events(
    State(state): State<AppState>,
    Path(match_id): Path<u64>,
    Query(pagination): Query<PaginationQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<IndexedEvent>>>) {
    let limit = pagination.limit.unwrap_or(100);
    let offset = pagination.offset.unwrap_or(0);

    // Cache-first: only bypass cache when explicit pagination params are given.
    if pagination.limit.is_none() && pagination.offset.is_none() {
        let cache_lock = state.cache.read().await;
        let cached_events = cache_lock.get_by_match(match_id);
        drop(cache_lock);

        if !cached_events.is_empty() {
            return (
                StatusCode::OK,
                Json(ApiResponse {
                    success: true,
                    data: Some(cached_events),
                    error: None,
                }),
            );
        }
    }

    match state
        .db
        .get_events_by_match_paginated(match_id, limit, offset)
        .await
    {
        Ok(events) => {
            if events.is_empty() {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse {
                        success: false,
                        data: None,
                        error: Some(format!("No events found for match {}", match_id)),
                    }),
                )
            } else {
                (
                    StatusCode::OK,
                    Json(ApiResponse {
                        success: true,
                        data: Some(events),
                        error: None,
                    }),
                )
            }
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            }),
        ),
    }
}

/// `GET /matches` – list matches, optionally filtered by status.
///
/// Cached for 10 seconds (`get_pending_matches` is the hot caller). Building the
/// list fans out one `build_match_info` query per match, so this is the most
/// expensive read the service serves and the one that benefits most from
/// memoisation.
async fn get_matches(
    State(state): State<AppState>,
    TypedQuery(query): TypedQuery<MatchQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<MatchInfo>>>) {
    let status = query.status;
    let cache_key = api_cache::matches_key(status.as_ref());

    if let Some(cached) = state.api_cache.get_json::<Vec<MatchInfo>>(&cache_key).await {
        return (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(cached),
                error: None,
            }),
        );
    }

    match state.db.get_matches_by_status(status.as_ref()).await {
        Ok(matches) => {
            state
                .api_cache
                .set_json(&cache_key, &matches, api_cache::pending_matches_ttl())
                .await;
            (
                StatusCode::OK,
                Json(ApiResponse {
                    success: true,
                    data: Some(matches),
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            }),
        ),
    }
}

/// `GET /match/:match_id` – full match summary with all events.
///
/// Cached for 5 seconds and invalidated eagerly on any contract event for this
/// match, so the TTL only bounds staleness for matches nothing is happening to.
/// Only successful lookups are cached — a `404` for a match that is about to be
/// indexed must not be sticky.
async fn get_match_info(
    State(state): State<AppState>,
    Path(match_id): Path<u64>,
) -> (StatusCode, Json<ApiResponse<MatchInfo>>) {
    let cache_key = api_cache::match_key(match_id);

    if let Some(cached) = state.api_cache.get_json::<MatchInfo>(&cache_key).await {
        return (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(cached),
                error: None,
            }),
        );
    }

    match state.db.build_match_info(match_id).await {
        Ok(Some(match_info)) => {
            state
                .api_cache
                .set_json(&cache_key, &match_info, api_cache::match_ttl())
                .await;
            (
                StatusCode::OK,
                Json(ApiResponse {
                    success: true,
                    data: Some(match_info),
                    error: None,
                }),
            )
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Match {} not found", match_id)),
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            }),
        ),
    }
}

// ── Transaction history ───────────────────────────────────────────────────────

/// `GET /transactions/player/:player_address` – a player's financial history.
///
/// Query parameters: `from_date`, `to_date`, `type` (`deposit`|`payout`|`fee`),
/// `token`, `limit` (default 100, max 1000), `offset`, `sort_by`
/// (`timestamp`|`amount`|`match_id`|`type`) and `sort_order` (`asc`|`desc`).
///
/// Not cached: the response is per-player and per-filter, so the hit rate would
/// be poor while the staleness would be user-visible.
async fn get_player_transactions(
    State(state): State<AppState>,
    Path(player_address): Path<String>,
    TypedQuery(query): TypedQuery<TransactionHistoryQuery>,
) -> (StatusCode, Json<ApiResponse<TransactionPage>>) {
    let filters = match build_transaction_filters(&player_address, &query) {
        Ok(filters) => filters,
        Err(e) => {
            let (status, Json(body)) = e.into_response_tuple();
            return (
                status,
                Json(ApiResponse {
                    success: false,
                    data: None,
                    error: body.error,
                }),
            );
        }
    };

    match state.db.query_player_transactions(&filters).await {
        // An empty history is a valid answer, not a 404: the player may simply
        // have no transactions in the requested window.
        Ok((transactions, total)) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(TransactionPage::new(
                    transactions,
                    total,
                    filters.limit,
                    filters.offset,
                )),
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse {
                success: false,
                data: None,
                error: Some(format!("Database error: {}", e)),
            }),
        ),
    }
}

/// Validate and convert raw history query parameters into typed filters.
///
/// Exposed within the crate so the tests can exercise the conversion (including
/// every rejection message) without standing up a database.
pub fn build_transaction_filters(
    player_address: &str,
    query: &TransactionHistoryQuery,
) -> Result<TransactionHistoryFilters, ValidationError> {
    let player = validation::validate_account_address("player_address", player_address)?;

    let mut filters = TransactionHistoryFilters::new(player);

    if let Some(raw) = non_empty(&query.tx_type) {
        filters.tx_type = Some(validation::validate_transaction_type("type", raw)?);
    }

    if let Some(raw) = non_empty(&query.token) {
        filters.token = Some(validation::validate_token("token", raw)?);
    }

    if let Some(raw) = non_empty(&query.from_date) {
        filters.from_date = Some(validation::validate_date("from_date", raw)?);
    }

    if let Some(raw) = non_empty(&query.to_date) {
        filters.to_date = Some(validation::validate_date("to_date", raw)?);
    }

    validation::validate_date_range(filters.from_date, filters.to_date)?;

    filters.limit = match non_empty(&query.limit) {
        Some(raw) => validation::validate_limit("limit", raw)?,
        None => DEFAULT_PAGE_LIMIT,
    };

    filters.offset = match non_empty(&query.offset) {
        Some(raw) => validation::validate_offset("offset", raw)?,
        None => 0,
    };

    filters.sort_by = match non_empty(&query.sort_by) {
        Some(raw) => validation::validate_sort_by("sort_by", raw)?,
        None => TransactionSortField::Timestamp,
    };

    filters.sort_order = match non_empty(&query.sort_order) {
        Some(raw) => validation::validate_sort_order("sort_order", raw)?,
        None => SortOrder::Desc,
    };

    Ok(filters)
}

/// Treat a blank query value as absent, matching how an omitted field behaves.
fn non_empty(value: &Option<String>) -> Option<&str> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

// ── Stats ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct Stats {
    pub total_events: i64,
    pub cache_size: usize,
}

/// `GET /stats` – service-level statistics.
///
/// Cached for 60 seconds: the counters are aggregates over the whole event table
/// and no caller needs them fresher than a minute.  The cached value is
/// invalidated whenever a contract event is ingested, so an idle service still
/// reports accurate numbers immediately after new activity.
async fn get_stats(State(state): State<AppState>) -> Json<ApiResponse<Stats>> {
    let cache_key = api_cache::analytics_key(api_cache::ANALYTICS_STATS);

    if let Some(cached) = state.api_cache.get_json::<Stats>(&cache_key).await {
        return Json(ApiResponse {
            success: true,
            data: Some(cached),
            error: None,
        });
    }

    let cache_lock = state.cache.read().await;
    let cache_size = cache_lock.size();
    drop(cache_lock);

    let total_events = state.db.total_event_count().await.unwrap_or(0);

    let stats = Stats {
        total_events,
        cache_size,
    };

    state
        .api_cache
        .set_json(&cache_key, &stats, api_cache::analytics_ttl())
        .await;

    Json(ApiResponse {
        success: true,
        data: Some(stats),
        error: None,
    })
}
