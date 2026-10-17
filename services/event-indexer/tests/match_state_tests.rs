//! Materialised match state (`matches` table) tests.
//!
//! `/match/:id` reads status, winner and creation fields from the `matches`
//! row maintained by `Database::insert_event` instead of replaying the event
//! log.  These tests need PostgreSQL and skip themselves when `DATABASE_URL` is
//! unset:
//! ```
//! DATABASE_URL=postgres://postgres:postgres@localhost:5432/test_event_indexer \
//!   cargo test -p event-indexer --test match_state_tests
//! ```
//!
//! Each test uses its own match ids and a ledger range no other test touches,
//! because `mark_events_as_invalid` invalidates by ledger range across matches.

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use event_indexer::api::{build_router, ApiResponse};
use event_indexer::api_cache::ApiCache;
use event_indexer::cache::EventCache;
use event_indexer::db::{build_pool, Database};
use event_indexer::models::{IndexedEvent, MatchInfo, MatchStatus, Winner};
use event_indexer::rpc::SorobanRpcClient;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower::ServiceExt;

async fn test_db() -> Option<(Arc<Database>, String)> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let db = Arc::new(Database::from_dsns(&url, &url, 2, 2).expect("db"));
    db.init_schema().await.expect("schema");
    Some((db, url))
}

/// Remove every row the test wrote, including any left by an aborted run.
async fn cleanup(url: &str, match_ids: &[i64]) {
    let conn = build_pool(url, 1).unwrap().get().await.unwrap();
    conn.execute("DELETE FROM events WHERE match_id = ANY($1)", &[&match_ids])
        .await
        .unwrap();
    conn.execute("DELETE FROM matches WHERE match_id = ANY($1)", &[&match_ids])
        .await
        .unwrap();
}

fn event(id: &str, match_id: u64, ledger: u32, event_type: &str, status: &str) -> IndexedEvent {
    let created = event_type.contains("created");
    IndexedEvent {
        id: id.to_string(),
        ledger_sequence: ledger,
        match_id,
        event_type: event_type.to_string(),
        player1: created.then(|| "GPLAYER1".to_string()),
        player2: created.then(|| "GPLAYER2".to_string()),
        status: Some(status.to_string()),
        winner: None,
        stake_amount: created.then(|| "5000".to_string()),
        token: created.then(|| "XLM".to_string()),
        game_id: created.then(|| "abcd1234".to_string()),
        platform: created.then(|| "lichess".to_string()),
        timestamp: Utc::now(),
        txn_hash: None,
        event_index_in_txn: None,
        reorg_invalidated_at: None,
    }
}

/// created → deposited (active) → completed (player1 wins).
fn lifecycle(match_id: u64, first_ledger: u32) -> Vec<IndexedEvent> {
    let mut completed = event(
        &format!("ms-{match_id}-3"),
        match_id,
        first_ledger + 2,
        "match:completed",
        "completed",
    );
    completed.winner = Some("player1".to_string());
    vec![
        event(&format!("ms-{match_id}-1"), match_id, first_ledger, "match:created", "pending"),
        event(&format!("ms-{match_id}-2"), match_id, first_ledger + 1, "match:deposit", "active"),
        completed,
    ]
}

async fn get_match(db: Arc<Database>, match_id: u64) -> (StatusCode, Option<MatchInfo>) {
    let app = build_router(
        db,
        Arc::new(RwLock::new(EventCache::new(10))),
        Arc::new(SorobanRpcClient::new("http://localhost:1").unwrap()),
        Arc::new(ApiCache::disabled()),
        None,
    );
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/match/{match_id}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: ApiResponse<MatchInfo> = serde_json::from_slice(&body).unwrap();
    (status, parsed.data)
}

#[tokio::test]
async fn match_endpoint_reflects_incrementally_ingested_state() {
    let Some((db, url)) = test_db().await else {
        println!("Skipping match_endpoint_reflects_incrementally_ingested_state: DATABASE_URL not set");
        return;
    };
    let match_id = 1_723_001u64;
    cleanup(&url, &[match_id as i64]).await;

    let events = lifecycle(match_id, 17_230_100);
    db.insert_event(&events[0]).await.unwrap();
    let (_, info) = get_match(db.clone(), match_id).await;
    assert_eq!(info.unwrap().status, MatchStatus::Pending);

    for e in &events[1..] {
        db.insert_event(e).await.unwrap();
    }
    let (status, info) = get_match(db.clone(), match_id).await;
    assert_eq!(status, StatusCode::OK);
    let info = info.unwrap();
    assert_eq!(info.status, MatchStatus::Completed);
    assert_eq!(info.winner, Some(Winner::Player1));
    assert_eq!(info.player1, "GPLAYER1");
    assert_eq!(info.player2, "GPLAYER2");
    assert_eq!(info.stake_amount, "5000");
    assert_eq!(info.game_id, "abcd1234");
    assert_eq!(info.created_ledger, 17_230_100);
    assert_eq!(info.completed_ledger, Some(17_230_102));
    assert_eq!(info.events.len(), 3);

    cleanup(&url, &[match_id as i64]).await;
}

#[tokio::test]
async fn match_state_is_served_from_materialised_table_not_event_replay() {
    let Some((db, url)) = test_db().await else {
        println!("Skipping match_state_is_served_from_materialised_table: DATABASE_URL not set");
        return;
    };
    let match_id = 1_723_002u64;
    cleanup(&url, &[match_id as i64]).await;

    for e in lifecycle(match_id, 17_230_200) {
        db.insert_event(&e).await.unwrap();
    }

    // Remove the event log behind the indexer's back.  A replaying handler
    // would now 404; the materialised row still answers.
    let conn = build_pool(&url, 1).unwrap().get().await.unwrap();
    conn.execute("DELETE FROM events WHERE match_id = $1", &[&(match_id as i64)])
        .await
        .unwrap();

    let (status, info) = get_match(db.clone(), match_id).await;
    assert_eq!(status, StatusCode::OK);
    let info = info.unwrap();
    assert_eq!(info.status, MatchStatus::Completed);
    assert_eq!(info.winner, Some(Winner::Player1));
    assert!(info.events.is_empty());

    cleanup(&url, &[match_id as i64]).await;
}

#[tokio::test]
async fn out_of_order_and_duplicate_events_do_not_regress_state() {
    let Some((db, url)) = test_db().await else {
        println!("Skipping out_of_order_and_duplicate_events: DATABASE_URL not set");
        return;
    };
    let match_id = 1_723_003u64;
    cleanup(&url, &[match_id as i64]).await;

    let events = lifecycle(match_id, 17_230_300);
    // Latest first, then the older ones, then a replay of everything.
    for i in [2, 0, 1, 0, 1, 2] {
        db.insert_event(&events[i]).await.unwrap();
    }

    let (_, info) = get_match(db.clone(), match_id).await;
    let info = info.unwrap();
    assert_eq!(info.status, MatchStatus::Completed);
    assert_eq!(info.winner, Some(Winner::Player1));
    assert_eq!(info.player1, "GPLAYER1");
    assert_eq!(info.created_ledger, 17_230_300);
    assert_eq!(info.completed_ledger, Some(17_230_302));
    assert_eq!(info.events.len(), 3);

    cleanup(&url, &[match_id as i64]).await;
}

#[tokio::test]
async fn reorg_rebuilds_state_from_surviving_events() {
    let Some((db, url)) = test_db().await else {
        println!("Skipping reorg_rebuilds_state_from_surviving_events: DATABASE_URL not set");
        return;
    };
    let rolled_back = 1_723_004u64;
    let wiped = 1_723_005u64;
    cleanup(&url, &[rolled_back as i64, wiped as i64]).await;

    for e in lifecycle(rolled_back, 17_230_400) {
        db.insert_event(&e).await.unwrap();
    }
    db.insert_event(&event("ms-wiped-1", wiped, 17_230_402, "match:created", "pending"))
        .await
        .unwrap();

    // Ledger 17_230_402 holds the completion of one match and the creation of
    // the other.
    let invalidated = db
        .mark_events_as_invalid("test-contract-1723", 17_230_402, 17_230_402, "test_reorg")
        .await
        .unwrap();
    assert_eq!(invalidated, 2);

    let (_, info) = get_match(db.clone(), rolled_back).await;
    let info = info.unwrap();
    assert_eq!(info.status, MatchStatus::Active);
    assert_eq!(info.winner, None);
    assert_eq!(info.completed_ledger, Some(17_230_401));

    let (status, _) = get_match(db.clone(), wiped).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    cleanup(&url, &[rolled_back as i64, wiped as i64]).await;
    let conn = build_pool(&url, 1).unwrap().get().await.unwrap();
    conn.execute("DELETE FROM reorg_events WHERE contract_id = 'test-contract-1723'", &[])
        .await
        .unwrap();
}

#[tokio::test]
async fn init_schema_backfills_matches_from_existing_events() {
    let Some((db, url)) = test_db().await else {
        println!("Skipping init_schema_backfills_matches_from_existing_events: DATABASE_URL not set");
        return;
    };
    let match_id = 1_723_006u64;
    cleanup(&url, &[match_id as i64]).await;

    for e in lifecycle(match_id, 17_230_600) {
        db.insert_event(&e).await.unwrap();
    }
    // Simulate events indexed by a version that predates the `matches` table.
    let conn = build_pool(&url, 1).unwrap().get().await.unwrap();
    conn.execute("DELETE FROM matches WHERE match_id = $1", &[&(match_id as i64)])
        .await
        .unwrap();
    let (status, _) = get_match(db.clone(), match_id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    db.init_schema().await.unwrap();

    let (status, info) = get_match(db.clone(), match_id).await;
    assert_eq!(status, StatusCode::OK);
    let info = info.unwrap();
    assert_eq!(info.status, MatchStatus::Completed);
    assert_eq!(info.winner, Some(Winner::Player1));
    assert_eq!(info.player1, "GPLAYER1");
    assert_eq!(info.created_ledger, 17_230_600);
    assert_eq!(info.completed_ledger, Some(17_230_602));

    cleanup(&url, &[match_id as i64]).await;
}
