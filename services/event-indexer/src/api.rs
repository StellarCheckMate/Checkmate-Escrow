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
    models::{IndexedEvent, MatchInfo, MatchStatus, QueryFilters},
    rpc::SorobanRpcClient,
};

#[derive(Clone)]
pub struct AppState {
    db: Arc<Database>,
    cache: Arc<RwLock<EventCache>>,
    rpc: Arc<SorobanRpcClient>,
}

#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// A custom `Query` extractor that returns a `400 ApiResponse` on deserialization failure
/// instead of axum's default 422 with a plain-text body.
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

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", bind_addr, bind_port)).await?;

    info!("API server listening on {}:{}", bind_addr, bind_port);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    match state.db.ping() {
        Ok(_) => Json(serde_json::json!({"db": "ok"})),
        Err(e) => Json(serde_json::json!({"db": "error", "detail": e.to_string()})),
    }
}

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

    match state.db.query_events(&filters) {
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

async fn get_match_events(
    State(state): State<AppState>,
    Path(match_id): Path<u64>,
    Query(pagination): Query<PaginationQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<IndexedEvent>>>) {
    let limit = pagination.limit.unwrap_or(100);
    let offset = pagination.offset.unwrap_or(0);

    // Only use cache when no pagination is requested (default first page, default limit)
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

    match state.db.get_events_by_match_paginated(match_id, limit, offset) {
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

async fn get_matches(
    State(state): State<AppState>,
    TypedQuery(query): TypedQuery<MatchQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<MatchInfo>>>) {
    let status = query.status;

    match state.db.get_matches_by_status(status.as_ref()) {
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

async fn get_match_info(
    State(state): State<AppState>,
    Path(match_id): Path<u64>,
) -> (StatusCode, Json<ApiResponse<MatchInfo>>) {
    match state.db.build_match_info(match_id) {
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

#[derive(Serialize)]
pub struct Stats {
    pub total_events: i64,
    pub cache_size: usize,
}

async fn get_stats(State(state): State<AppState>) -> Json<ApiResponse<Stats>> {
    let cache_lock = state.cache.read().await;
    let cache_size = cache_lock.size();
    drop(cache_lock);

    let total_events = state.db.total_event_count().unwrap_or(0);

    Json(ApiResponse {
        success: true,
        data: Some(Stats {
            total_events,
            cache_size,
        }),
        error: None,
    })
}
