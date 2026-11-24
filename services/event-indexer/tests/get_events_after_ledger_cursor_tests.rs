//! Tests for `GET /events` with `after_ledger` and `after_index` cursor parameters.
//!
//! Tests verify:
//! - `after_ledger` parameter filters events to those with ledger_sequence > value
//! - `after_index` parameter works with `after_ledger` for stable pagination
//! - Events are ordered ascending by ledger_sequence, then by event_index_in_txn
//! - Cursor-based pagination is stable across requests
//! - Response cursor echoes the last consumed ledger sequence

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use event_indexer::{api::build_router, cache::EventCache, db::Database, rpc::SorobanRpcClient};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

fn api_cache() -> Arc<event_indexer::api_cache::ApiCache> {
    Arc::new(event_indexer::api_cache::ApiCache::in_memory())
}

async fn app() -> axum::Router {
    let db_url = std::env::var("DATABASE_URL").unwrap();
    let db = Arc::new(Database::from_dsns(&db_url, &db_url, 2, 2).expect("failed to create db"));
    db.init_schema().await.expect("failed to init schema");

    let cache = Arc::new(RwLock::new(EventCache::new(100)));
    let rpc = Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap());

    build_router(db, cache, rpc, api_cache(), None)
}

/// `GET /events?after_ledger=<n>` returns only events with ledger_sequence > n.
#[tokio::test]
async fn get_events_accepts_after_ledger_cursor() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_events_accepts_after_ledger_cursor: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    let request = Request::builder()
        .uri("/events?after_ledger=0&limit=50")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);
    assert!(parsed["data"]["events"].is_array());

    if let Some(events) = parsed["data"]["events"].as_array() {
        // All events should have ledger_sequence > 0
        for event in events {
            let ledger = event["ledger_sequence"].as_i64().unwrap();
            assert!(ledger > 0, "Expected ledger_sequence > 0, got {}", ledger);
        }
    }
}

/// `GET /events?after_ledger=<n>&after_index=<i>` uses both cursor values for stable pagination.
#[tokio::test]
async fn get_events_accepts_after_ledger_and_after_index() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_events_accepts_after_ledger_and_after_index: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    // First request to establish a baseline
    let request1 = Request::builder()
        .uri("/events?limit=1")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response1 = app.clone().oneshot(request1).await.unwrap();
    let body1 = to_bytes(response1.into_body(), usize::MAX).await.unwrap();
    let parsed1: serde_json::Value = serde_json::from_slice(&body1).unwrap();

    // Extract ledger and index from first event
    if let Some(events) = parsed1["data"]["events"].as_array() {
        if events.len() > 0 {
            let first_ledger = events[0]["ledger_sequence"].as_i64().unwrap();
            let first_index = events[0]["event_index_in_txn"].as_i64().unwrap_or(0);

            // Request events after the first one using cursor pagination
            let uri = format!("/events?after_ledger={}&after_index={}&limit=50", first_ledger, first_index);
            let request2 = Request::builder()
                .uri(&uri)
                .method("GET")
                .body(axum::body::Body::empty())
                .unwrap();

            let response2 = app.oneshot(request2).await.unwrap();
            assert_eq!(response2.status(), StatusCode::OK);

            let body2 = to_bytes(response2.into_body(), usize::MAX).await.unwrap();
            let parsed2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
            assert_eq!(parsed2["success"], true);

            if let Some(events2) = parsed2["data"]["events"].as_array() {
                // All returned events should be strictly after the cursor
                for event in events2 {
                    let ledger = event["ledger_sequence"].as_i64().unwrap();
                    let index = event["event_index_in_txn"].as_i64().unwrap_or(0);

                    if ledger == first_ledger {
                        assert!(index > first_index, "Expected index > {}, got {}", first_index, index);
                    } else {
                        assert!(ledger > first_ledger, "Expected ledger > {}, got {}", first_ledger, ledger);
                    }
                }
            }
        }
    }
}

/// Events returned are ordered ascending by ledger_sequence, then event_index_in_txn.
#[tokio::test]
async fn get_events_returns_ordered_results() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_events_returns_ordered_results: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    let request = Request::builder()
        .uri("/events?limit=100")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed["success"], true);

    if let Some(events) = parsed["data"]["events"].as_array() {
        let mut prev_ledger = 0i64;
        let mut prev_index = 0i64;

        for event in events {
            let ledger = event["ledger_sequence"].as_i64().unwrap();
            let index = event["event_index_in_txn"].as_i64().unwrap_or(0);

            // Verify ordering
            if ledger == prev_ledger {
                assert!(index >= prev_index, "Events with same ledger should be ordered by index");
            } else {
                assert!(ledger > prev_ledger, "Ledger sequence should increase");
            }

            prev_ledger = ledger;
            prev_index = index;
        }
    }
}

/// Cursor pagination is stable: requesting with the same cursor yields no duplicate events.
#[tokio::test]
async fn get_events_cursor_pagination_stable() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_events_cursor_pagination_stable: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    // First request: get first page
    let request1 = Request::builder()
        .uri("/events?limit=10")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response1 = app.clone().oneshot(request1).await.unwrap();
    let body1 = to_bytes(response1.into_body(), usize::MAX).await.unwrap();
    let parsed1: serde_json::Value = serde_json::from_slice(&body1).unwrap();

    if let Some(events1) = parsed1["data"]["events"].as_array() {
        if events1.len() > 0 {
            let last_event = &events1[events1.len() - 1];
            let last_ledger = last_event["ledger_sequence"].as_i64().unwrap();
            let last_index = last_event["event_index_in_txn"].as_i64().unwrap_or(0);

            // Second request: get events after the last event from first page
            let uri = format!("/events?after_ledger={}&after_index={}&limit=10", last_ledger, last_index);
            let request2 = Request::builder()
                .uri(&uri)
                .method("GET")
                .body(axum::body::Body::empty())
                .unwrap();

            let response2 = app.oneshot(request2).await.unwrap();
            assert_eq!(response2.status(), StatusCode::OK);

            let body2 = to_bytes(response2.into_body(), usize::MAX).await.unwrap();
            let parsed2: serde_json::Value = serde_json::from_slice(&body2).unwrap();

            if let Some(events2) = parsed2["data"]["events"].as_array() {
                // Verify no duplicates between pages
                for event2 in events2 {
                    let ledger2 = event2["ledger_sequence"].as_i64().unwrap();
                    let index2 = event2["event_index_in_txn"].as_i64().unwrap_or(0);

                    let is_duplicate = events1.iter().any(|e1| {
                        let ledger1 = e1["ledger_sequence"].as_i64().unwrap();
                        let index1 = e1["event_index_in_txn"].as_i64().unwrap_or(0);
                        ledger1 == ledger2 && index1 == index2
                    });

                    assert!(!is_duplicate, "Found duplicate event in second page");
                }
            }
        }
    }
}

/// The `/events` response cursor echoes the ledger sequence of the last returned event.
///
/// This is a database-backed test: it fetches one event page and asserts that
/// `data.cursor` equals the `ledger_sequence` of the last event in `data.events`.
#[tokio::test]
async fn get_events_response_cursor_reflects_last_consumed_ledger() {
    if std::env::var("DATABASE_URL").is_err() {
        println!(
            "Skipping get_events_response_cursor_reflects_last_consumed_ledger: DATABASE_URL not set"
        );
        return;
    }

    let app = app().await;

    // Fetch up to 1 event so the last-event check is deterministic.
    let request = Request::builder()
        .uri("/events?limit=1")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    // No events in the DB → nothing to assert.
    if response.status() == StatusCode::NOT_FOUND {
        println!("Skipping cursor assertion: no events in the database");
        return;
    }

    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(parsed["success"], true);

    // cursor must be present and must equal the ledger_sequence of the last event.
    let cursor = parsed["data"]["cursor"]
        .as_u64()
        .expect("cursor must be a non-null integer when events are returned");

    let events = parsed["data"]["events"]
        .as_array()
        .expect("events must be an array");
    assert!(!events.is_empty(), "events array must not be empty on 200");

    let last_ledger = events
        .last()
        .unwrap()["ledger_sequence"]
        .as_u64()
        .expect("ledger_sequence must be an integer");

    assert_eq!(
        cursor, last_ledger,
        "cursor must equal the ledger_sequence of the last returned event"
    );
}
