//! REST API server.
//!
//! All query handlers use `db.query_*` methods which route through the
//! **read pool** (`read_pool` in `Database`).  If a read-replica DSN is
//! configured (`DATABASE_READ_URL`), read traffic is automatically spread
//! across replicas without any code changes here.

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
    cache::EventCache,
    db::Database,
    models::{AnalyticsOverview, IndexedEvent, MatchInfo, MatchStatus, PlayerAnalytics, QueryFilters, TokenAnalytics},
    rpc::SorobanRpcClient,
};

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub cache: Arc<RwLock<EventCache>>,
    pub rpc: Arc<SorobanRpcClient>,
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

/// Query parameters accepted by all three analytics endpoints.
#[derive(Deserialize)]
pub struct AnalyticsQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub start_date: Option<chrono::DateTime<chrono::Utc>>,
    pub end_date: Option<chrono::DateTime<chrono::Utc>>,
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn build_router(
    db: Arc<Database>,
    cache: Arc<RwLock<EventCache>>,
    rpc: Arc<SorobanRpcClient>,
) -> Router {
    let state = AppState { db, cache, rpc };
    Router::new()
        .route("/health", get(health_check))
        .route("/events", get(get_events))
        .route("/events/:match_id", get(get_match_events))
        .route("/matches", get(get_matches))
        .route("/match/:match_id", get(get_match_info))
        .route("/stats", get(get_stats))
        .route("/analytics/overview", get(analytics_overview))
        .route("/analytics/player/:player_address", get(analytics_player))
        .route("/analytics/token/:token_address", get(analytics_token))
        .with_state(state)
}

pub async fn start_server(
    bind_addr: &str,
    bind_port: u16,
    db: Arc<Database>,
    cache: Arc<RwLock<EventCache>>,
    rpc: Arc<SorobanRpcClient>,
) -> anyhow::Result<()> {
    let app = build_router(db, cache, rpc);

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
async fn get_matches(
    State(state): State<AppState>,
    TypedQuery(query): TypedQuery<MatchQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<MatchInfo>>>) {
    let status = query.status;

    match state.db.get_matches_by_status(status.as_ref()).await {
        Ok(matches) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(matches),
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

/// `GET /match/:match_id` – full match summary with all events.
async fn get_match_info(
    State(state): State<AppState>,
    Path(match_id): Path<u64>,
) -> (StatusCode, Json<ApiResponse<MatchInfo>>) {
    match state.db.build_match_info(match_id).await {
        Ok(Some(match_info)) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(match_info),
                error: None,
            }),
        ),
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

// ── Stats ─────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct Stats {
    pub total_events: i64,
    pub cache_size: usize,
}

/// `GET /stats` – service-level statistics.
async fn get_stats(State(state): State<AppState>) -> Json<ApiResponse<Stats>> {
    let cache_lock = state.cache.read().await;
    let cache_size = cache_lock.size();
    drop(cache_lock);

    let total_events = state.db.total_event_count().await.unwrap_or(0);

    Json(ApiResponse {
        success: true,
        data: Some(Stats {
            total_events,
            cache_size,
        }),
        error: None,
    })
}

// ── Analytics ─────────────────────────────────────────────────────────────────

/// `GET /analytics/overview` — platform-wide aggregate statistics.
///
/// Optional query params:
/// - `start_date` / `end_date` — ISO-8601 timestamps for time-range filtering.
async fn analytics_overview(
    State(state): State<AppState>,
    TypedQuery(query): TypedQuery<AnalyticsQuery>,
) -> (StatusCode, Json<ApiResponse<AnalyticsOverview>>) {
    match state.db.analytics_overview(query.start_date, query.end_date).await {
        Ok(overview) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(overview),
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

/// `GET /analytics/player/:player_address` — per-player statistics.
///
/// Optional query params:
/// - `limit` / `offset` — pagination (default limit: 50).
/// - `start_date` / `end_date` — time-range filter.
async fn analytics_player(
    State(state): State<AppState>,
    Path(player_address): Path<String>,
    TypedQuery(query): TypedQuery<AnalyticsQuery>,
) -> (StatusCode, Json<ApiResponse<PlayerAnalytics>>) {
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0).max(0);

    match state
        .db
        .analytics_player(&player_address, limit, offset, query.start_date, query.end_date)
        .await
    {
        Ok(analytics) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(analytics),
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

/// `GET /analytics/token/:token_address` — per-token statistics.
///
/// Optional query params:
/// - `limit` / `offset` — pagination (default limit: 50).
/// - `start_date` / `end_date` — time-range filter.
async fn analytics_token(
    State(state): State<AppState>,
    Path(token_address): Path<String>,
    TypedQuery(query): TypedQuery<AnalyticsQuery>,
) -> (StatusCode, Json<ApiResponse<TokenAnalytics>>) {
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = query.offset.unwrap_or(0).max(0);

    match state
        .db
        .analytics_token(&token_address, limit, offset, query.start_date, query.end_date)
        .await
    {
        Ok(analytics) => (
            StatusCode::OK,
            Json(ApiResponse {
                success: true,
                data: Some(analytics),
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
