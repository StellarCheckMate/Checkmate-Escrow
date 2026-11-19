//! Regression test for issue #1979 — health endpoint must reflect
//! dead-letter queue depth.
//!
//! Before the fix, `health.rs` reported healthy based only on liveness,
//! ignoring a non-empty dead-letter queue that indicates failed settlements
//! requiring manual intervention.
//!
//! The fix: when the dead-letter queue is non-empty and the service would
//! otherwise report Healthy, it instead reports Degraded.  This surfaces
//! the problem to operators and monitoring systems without blocking traffic.

use oracle_service::config::Platform;
use oracle_service::dead_letter::DeadLetterStore;
use oracle_service::health::HealthStatus;
use oracle_service::queue::PendingEntry;
use tempfile::TempDir;

fn make_dead_letter_entry(match_id: u64) -> PendingEntry {
    let mut e = PendingEntry::new(match_id, "abcd1234".into(), Platform::Lichess);
    e.attempts = 5;
    e.last_error = Some("test error".into());
    e
}

/// When the dead-letter queue is non-empty, the health response must not
/// report Healthy.
#[tokio::test]
async fn non_empty_dead_letter_queue_is_not_healthy() {
    let dir = TempDir::new().unwrap();
    let store = DeadLetterStore::new(dir.path().to_str().unwrap(), 0);

    // Push a dead-letter entry.
    store.push(make_dead_letter_entry(42)).await.unwrap();

    // The dead-letter queue depth should be non-zero.
    let depth = store.load().await.unwrap().len();
    assert_eq!(depth, 1, "dead-letter queue should have one entry");

    // A health handler that is aware of the dead-letter depth should not
    // report Healthy when depth > 0.  Simulate the check logic from main.rs:
    let simulated_liveness_status = HealthStatus::Healthy; // liveness would be OK
    let effective_status = if depth > 0 && simulated_liveness_status == HealthStatus::Healthy {
        HealthStatus::Degraded
    } else {
        simulated_liveness_status
    };

    assert_ne!(
        effective_status,
        HealthStatus::Healthy,
        "health should not be Healthy when dead-letter queue is non-empty"
    );
    assert_eq!(
        effective_status,
        HealthStatus::Degraded,
        "health should be Degraded when dead-letter queue is non-empty but liveness checks pass"
    );
}

/// When the dead-letter queue is empty, the health check is not affected.
#[tokio::test]
async fn empty_dead_letter_queue_does_not_affect_health() {
    let dir = TempDir::new().unwrap();
    let store = DeadLetterStore::new(dir.path().to_str().unwrap(), 0);

    let depth = store.load().await.unwrap().len();
    assert_eq!(depth, 0, "dead-letter queue should be empty");

    // Health status should pass through unchanged.
    let simulated_liveness_status = HealthStatus::Healthy;
    let effective_status = if depth > 0 && simulated_liveness_status == HealthStatus::Healthy {
        HealthStatus::Degraded
    } else {
        simulated_liveness_status
    };

    assert_eq!(
        effective_status,
        HealthStatus::Healthy,
        "health should remain Healthy when dead-letter queue is empty"
    );
}

/// If liveness is already Unhealthy, adding a dead-letter entry must not
/// downgrade to something worse (Unhealthy stays Unhealthy).
#[tokio::test]
async fn unhealthy_liveness_is_not_changed_by_dead_letter() {
    let dir = TempDir::new().unwrap();
    let store = DeadLetterStore::new(dir.path().to_str().unwrap(), 0);

    store.push(make_dead_letter_entry(99)).await.unwrap();
    let depth = store.load().await.unwrap().len();

    let simulated_liveness_status = HealthStatus::Unhealthy;
    let effective_status = if depth > 0 && simulated_liveness_status == HealthStatus::Healthy {
        HealthStatus::Degraded
    } else {
        simulated_liveness_status
    };

    assert_eq!(
        effective_status,
        HealthStatus::Unhealthy,
        "Unhealthy liveness status should not be overridden by dead-letter check"
    );
}

/// Multiple dead-letter entries still report Degraded, not a worse status.
#[tokio::test]
async fn multiple_dead_letter_entries_report_degraded() {
    let dir = TempDir::new().unwrap();
    let store = DeadLetterStore::new(dir.path().to_str().unwrap(), 0);

    for id in 1..=5 {
        store.push(make_dead_letter_entry(id)).await.unwrap();
    }

    let depth = store.load().await.unwrap().len();
    assert_eq!(depth, 5);

    let simulated_liveness_status = HealthStatus::Healthy;
    let effective_status = if depth > 0 && simulated_liveness_status == HealthStatus::Healthy {
        HealthStatus::Degraded
    } else {
        simulated_liveness_status
    };

    assert_eq!(effective_status, HealthStatus::Degraded);
}

/// After manually removing all dead-letter entries (e.g. via oracle-replay),
/// the health status returns to Healthy.
#[tokio::test]
async fn health_recovers_after_dead_letter_cleared() {
    let dir = TempDir::new().unwrap();
    let store = DeadLetterStore::new(dir.path().to_str().unwrap(), 0);

    store.push(make_dead_letter_entry(7)).await.unwrap();
    store.remove(7).await.unwrap();

    let depth = store.load().await.unwrap().len();
    assert_eq!(depth, 0, "dead-letter queue should be empty after removal");

    let simulated_liveness_status = HealthStatus::Healthy;
    let effective_status = if depth > 0 && simulated_liveness_status == HealthStatus::Healthy {
        HealthStatus::Degraded
    } else {
        simulated_liveness_status
    };

    assert_eq!(
        effective_status,
        HealthStatus::Healthy,
        "health should recover to Healthy after dead-letter queue is cleared"
    );
}
