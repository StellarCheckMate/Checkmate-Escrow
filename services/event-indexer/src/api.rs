use axum::{
    extract::{Query, Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::{
    cache::EventCache,
    db::Database,
    models::{IndexedEvent, MatchInfo, QueryFilters, MatchStatus},
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

#[derive(Deserialize)]
pub struct EventQuery {
    pub player_address: Option<String>,
    pub status: Option<String>,
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

async fn health_check() -> Json<ApiResponse<String>> {
    Json(ApiResponse {
        success: true,
        data: Some("Event Indexer is healthy".to_string()),
        error: None,
    })
}

async fn get_events(
    State(state): State<AppState>,
    Query(query): Query<EventQuery>,
) -> (StatusCode, Json<ApiResponse<Vec<IndexedEvent>>>) {
    let filters = QueryFilters {
        player_address: query.player_address,
        status: query.status.as_ref().map(|s| match s.as_str() {
            "pending" => MatchStatus::Pending,
            "active" => MatchStatus::Active,
            "completed" => MatchStatus::Completed,
            "cancelled" => MatchStatus::Cancelled,
            "expired" => MatchStatus::Expired,
            _ => MatchStatus::Pending,
        }),
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
) -> (StatusCode, Json<ApiResponse<Vec<IndexedEvent>>>) {
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

    match state.db.get_events_by_match(match_id) {
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
