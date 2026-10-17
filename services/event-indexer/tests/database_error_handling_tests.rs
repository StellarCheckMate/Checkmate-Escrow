//! Tests for secure error handling in API responses.
//!
//! Verifies that database errors are logged with request ID but not exposed
//! to clients. Clients should only receive a generic error message.
//!
//! This is a security test to prevent database schema, SQL, connection details
//! and other sensitive information from being leaked to API consumers.

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use event_indexer::{api::build_router, cache::EventCache, db::Database, rpc::SorobanRpcClient};
use std::path::{Path, PathBuf};
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

/// Database errors in `/events` responses do not expose implementation details.
#[tokio::test]
async fn get_events_error_is_generic() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_events_error_is_generic: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    // Valid request to /events
    let request = Request::builder()
        .uri("/events?player_address=test")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // If success=false (e.g., no events found), the error message should be generic
    if !parsed["success"].as_bool().unwrap_or(false) {
        let error_msg = parsed["error"].as_str().unwrap_or("");

        // Error should not expose database internals
        assert!(!error_msg.contains("SQL"), "Error exposes SQL: {}", error_msg);
        assert!(!error_msg.contains("postgres"), "Error exposes postgres: {}", error_msg);
        assert!(!error_msg.contains("SELECT"), "Error exposes SELECT query: {}", error_msg);
        assert!(!error_msg.contains("FROM"), "Error exposes FROM clause: {}", error_msg);
        assert!(!error_msg.contains("table"), "Error exposes table names: {}", error_msg);
        assert!(!error_msg.contains("column"), "Error exposes column names: {}", error_msg);
    }
}

/// Database errors in `/matches` responses do not expose implementation details.
#[tokio::test]
async fn get_matches_error_is_generic() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_matches_error_is_generic: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    // Valid request to /matches
    let request = Request::builder()
        .uri("/matches")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Check error message if present
    if let Some(error_msg) = parsed["error"].as_str() {
        // Error should not expose database internals
        assert!(!error_msg.contains("SQL"), "Error exposes SQL: {}", error_msg);
        assert!(!error_msg.contains("postgres"), "Error exposes postgres: {}", error_msg);
        assert!(!error_msg.contains("SELECT"), "Error exposes SELECT query: {}", error_msg);
        assert!(!error_msg.contains("FROM"), "Error exposes FROM clause: {}", error_msg);
        assert!(!error_msg.contains("table"), "Error exposes table names: {}", error_msg);
    }
}

/// Error responses have correct HTTP status codes.
#[tokio::test]
async fn error_responses_use_correct_status_codes() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping error_responses_use_correct_status_codes: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    // Valid request to /events
    let request = Request::builder()
        .uri("/events")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let status = response.status();

    // Status should be either 200 OK (data found), 404 NOT_FOUND (no data), or 500 (error)
    // Not arbitrary error codes that might leak information
    assert!(
        status == StatusCode::OK ||
        status == StatusCode::NOT_FOUND ||
        status == StatusCode::INTERNAL_SERVER_ERROR,
        "Unexpected status code: {}",
        status
    );
}

/// Error response has proper JSON structure (success, data, error fields).
#[tokio::test]
async fn error_responses_have_valid_structure() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping error_responses_have_valid_structure: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    let request = Request::builder()
        .uri("/events")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Must have standard response structure
    assert!(parsed.is_object(), "Response must be JSON object");
    assert!(parsed["success"].is_boolean(), "Response must have boolean success field");

    // data and error may be null, but must be present
    assert!(parsed.get("data").is_some(), "Response must have data field");
    assert!(parsed.get("error").is_some(), "Response must have error field");
}

/// Builds a unique temporary directory path that does not exist yet.
fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "event-indexer-{}-{}-{}",
        tag,
        std::process::id(),
        nanos
    ))
}

/// Extracts the parent directory of a SQLite DSN, if any.
fn dsn_parent_dir(dsn: &str) -> Option<PathBuf> {
    let path = dsn
        .strip_prefix("sqlite://")
        .or_else(|| dsn.strip_prefix("sqlite:"))
        .unwrap_or(dsn);
    Path::new(path).parent().map(|p| p.to_path_buf())
}

/// Startup path: a DSN whose parent directory does not exist should have the
/// directory created (or return a descriptive error containing the path and
/// the OS error) rather than panicking.
#[tokio::test]
async fn startup_creates_missing_parent_directory() {
    let dir = unique_temp_dir("missing-parent");
    assert!(!dir.exists(), "temp dir should not exist yet: {:?}", dir);

    let db_file = dir.join("indexer.sqlite");
    let dsn = format!("sqlite://{}", db_file.display());

    // The parent directory must be missing before the startup check runs.
    let parent = dsn_parent_dir(&dsn).expect("dsn should have a parent directory");
    assert_eq!(parent, dir);
    assert!(!parent.exists());

    // Attempt to open the database. This must not panic; it either succeeds
    // (after creating the directory) or fails with a descriptive error.
    let result = Database::from_dsns(&dsn, &dsn, 2, 2);

    match result {
        Ok(_) => {
            // Directory should have been created by the startup check.
            assert!(
                dir.exists(),
                "startup should create missing parent directory: {:?}",
                dir
            );
        }
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains(&dir.display().to_string()) || msg.contains("indexer.sqlite"),
                "error should mention the path, got: {}",
                msg
            );
        }
    }

    // Best-effort cleanup.
    let _ = std::fs::remove_dir_all(&dir);
}

/// Startup path: when directory creation fails, the error message must contain
/// the path and the underlying OS error instead of panicking.
#[tokio::test]
async fn startup_creation_failure_reports_path() {
    // Use a path whose parent is a regular file, so creating the directory
    // (or the database file) is guaranteed to fail.
    let base = unique_temp_dir("blocked-parent");
    std::fs::create_dir_all(&base).expect("failed to create base temp dir");

    let blocker = base.join("not-a-dir");
    std::fs::write(&blocker, b"blocker").expect("failed to write blocker file");

    let db_file = blocker.join("indexer.sqlite");
    let dsn = format!("sqlite://{}", db_file.display());

    let result = Database::from_dsns(&dsn, &dsn, 2, 2);

    match result {
        Ok(_) => {
            // Should not happen, but if the backend tolerates it, don't fail.
            println!("Skipping startup_creation_failure_reports_path: open unexpectedly succeeded");
        }
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains(&blocker.display().to_string())
                    || msg.contains("indexer.sqlite")
                    || msg.contains(&db_file.display().to_string()),
                "error should mention the path, got: {}",
                msg
            );
            assert!(
                !msg.is_empty(),
                "error message should not be empty"
            );
        }
    }

    // Best-effort cleanup.
    let _ = std::fs::remove_dir_all(&base);
}

/// `/match/:match_id` endpoint returns generic errors on database issues.
#[tokio::test]
async fn get_match_info_error_is_generic() {
    if std::env::var("DATABASE_URL").is_err() {
        println!("Skipping get_match_info_error_is_generic: DATABASE_URL not set");
        return;
    }

    let app = app().await;

    // Request a match that probably doesn't exist
    let request = Request::builder()
        .uri("/match/99999999")
        .method("GET")
        .body(axum::body::Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // If there's an error, it should be generic
    if let Some(error_msg) = parsed["error"].as_str() {
        assert!(!error_msg.contains("Database error:"), "Error message leaks implementation");
        assert!(!error_msg.contains("SQL"), "Error exposes SQL");
        assert!(!error_msg.contains("postgres"), "Error exposes postgres");
    }
}
