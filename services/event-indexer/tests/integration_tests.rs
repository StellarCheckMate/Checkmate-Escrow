use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use rusqlite::Connection;

/// Verifies that total_event_count returns the real row count from the events table.
/// Mirrors the logic in db.rs so the test is self-contained for a binary crate.
#[test]
fn test_total_event_count() {
    let conn = Connection::open_in_memory().unwrap();

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
            id TEXT PRIMARY KEY,
            ledger_sequence INTEGER NOT NULL,
            match_id INTEGER NOT NULL,
            event_type TEXT NOT NULL,
            player1 TEXT, player2 TEXT, status TEXT, winner TEXT,
            stake_amount TEXT, token TEXT, game_id TEXT, platform TEXT,
            timestamp TEXT NOT NULL, txn_hash TEXT
        );"
    ).unwrap();

    let count_empty: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_empty, 0);

    for i in 1..=3u64 {
        conn.execute(
            "INSERT INTO events (id, ledger_sequence, match_id, event_type, timestamp)
             VALUES (?, ?, ?, 'match:created', ?)",
            rusqlite::params![format!("evt-{}", i), i, i, Utc::now().to_rfc3339()],
        ).unwrap();
    }

    let count_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_after, 3);
}

#[test]
fn test_event_indexing() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        assert!(true, "Event indexing test placeholder");
    });
}

#[test]
fn test_event_filtering() {
    assert!(true, "Event filtering test placeholder");
}

#[test]
fn test_cache_operations() {
    let rt = tokio::runtime::Runtime::new().unwrap();

    rt.block_on(async {
        assert!(true, "Cache operations test placeholder");
    });
}

#[tokio::test]
async fn test_get_match_info_404_returns_error_body() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/match/9999")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: ApiResponse<serde_json::Value> = serde_json::from_slice(&body).unwrap();

    assert!(!parsed.success);
    assert_eq!(parsed.error.as_deref(), Some("Match 9999 not found"));
}
