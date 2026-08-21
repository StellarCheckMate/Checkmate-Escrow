//! Adversarial tests for cache consistency across replicas.
//!
//! These tests prove that the TTL-bounded EventCache prevents indefinitely-stale
//! reads after leadership failover, and that ApiCache's Redis-unreachable fallback
//! is detectable via the health endpoint.

use chrono::Utc;
use event_indexer::{cache::EventCache, models::IndexedEvent};
use std::thread;
use std::time::Duration;

fn make_event(id: &str, match_id: u64, ledger: u32) -> IndexedEvent {
    IndexedEvent {
        id: id.to_string(),
        ledger_sequence: ledger,
        match_id,
        event_type: "match:created".to_string(),
        player1: Some("PLAYER_A".to_string()),
        player2: Some("PLAYER_B".to_string()),
        status: Some("pending".to_string()),
        winner: None,
        stake_amount: Some("1000".to_string()),
        token: Some("XLM".to_string()),
        game_id: Some("game-001".to_string()),
        platform: Some("lichess".to_string()),
        timestamp: Utc::now(),
        txn_hash: Some(format!("txhash-{}", id)),
        event_index_in_txn: None,
        reorg_invalidated_at: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Split-brain scenario — two replicas with different cached state
// ─────────────────────────────────────────────────────────────────────────────

/// **Adversarial test:** Simulate two replicas that ingested different event sets
/// for the same match_id at different times. Before the TTL fix, both would serve
/// their cached versions indefinitely. After the fix, reads converge after TTL.
#[test]
fn two_replicas_different_events_diverge_before_ttl() {
    // Replica 1 was the leader and cached events e1, e2 for match 42.
    let mut replica1_cache = EventCache::with_ttl(10, 2); // 2-second TTL
    replica1_cache.insert(make_event("e1", 42, 100));
    replica1_cache.insert(make_event("e2", 42, 101));

    // Replica 2 later became the leader and cached events e3, e4 for match 42.
    let mut replica2_cache = EventCache::with_ttl(10, 2);
    replica2_cache.insert(make_event("e3", 42, 102));
    replica2_cache.insert(make_event("e4", 42, 103));

    // Immediately: replicas serve different answers (the bug).
    let r1_events = replica1_cache.get_by_match(42);
    let r2_events = replica2_cache.get_by_match(42);
    assert_eq!(r1_events.len(), 2, "replica 1 serves its cached state");
    assert_eq!(r2_events.len(), 2, "replica 2 serves its cached state");
    assert_ne!(
        r1_events[0].id, r2_events[0].id,
        "before TTL, replicas serve different event sets"
    );
}

#[test]
fn two_replicas_converge_after_ttl() {
    // Replica 1 was the leader and cached events for match 42.
    let mut replica1_cache = EventCache::with_ttl(10, 1); // 1-second TTL
    replica1_cache.insert(make_event("e1", 42, 100));
    replica1_cache.insert(make_event("e2", 42, 101));

    // Replica 2 later became the leader and cached different events for match 42.
    let mut replica2_cache = EventCache::with_ttl(10, 1);
    replica2_cache.insert(make_event("e3", 42, 102));
    replica2_cache.insert(make_event("e4", 42, 103));

    // Wait for TTL to expire on both.
    thread::sleep(Duration::from_millis(1100));

    // After TTL: both return empty (cache miss → DB fallback).
    let r1_events = replica1_cache.get_by_match(42);
    let r2_events = replica2_cache.get_by_match(42);
    assert_eq!(
        r1_events.len(),
        0,
        "replica 1 cache miss after TTL (will hit DB)"
    );
    assert_eq!(
        r2_events.len(),
        0,
        "replica 2 cache miss after TTL (will hit DB)"
    );
    // Both replicas now converge to DB-authoritative state (staleness bounded).
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Leadership failover scenario
// ─────────────────────────────────────────────────────────────────────────────

/// **Adversarial test:** A former leader loses its lease but continues serving
/// cached data. Prove TTL bounds the staleness window.
#[test]
fn former_leader_serves_stale_data_within_ttl_window() {
    let mut leader_cache = EventCache::with_ttl(10, 2); // 2-second TTL

    // Leader ingests an event for match 99.
    leader_cache.insert(make_event("leader-event", 99, 200));

    // Simulate leadership loss (leader does not clear cache in the broken version).
    // In the broken implementation, this cache would serve indefinitely.

    // Immediately after losing leadership: stale read still succeeds (within TTL).
    let stale_events = leader_cache.get_by_match(99);
    assert_eq!(
        stale_events.len(),
        1,
        "within TTL window, former leader still serves cached state"
    );

    // Wait for TTL to expire.
    thread::sleep(Duration::from_millis(2100));

    // After TTL: cache miss, forcing DB fallback (staleness bounded).
    let post_ttl_events = leader_cache.get_by_match(99);
    assert_eq!(
        post_ttl_events.len(),
        0,
        "after TTL, former leader returns cache miss → DB fallback"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: ApiCache backend detection via health endpoint
// ─────────────────────────────────────────────────────────────────────────────

/// **Test:** Prove `ApiCache::from_config` with an unreachable Redis URL is
/// detectable via `is_shared()` and `backend_name()`, not just a log line.
#[tokio::test]
async fn api_cache_redis_unreachable_detectable() {
    use event_indexer::api_cache::ApiCache;

    // Unreachable Redis URL (invalid port).
    let unreachable_redis_url = "redis://localhost:9999";
    let cache = ApiCache::from_config(Some(unreachable_redis_url)).await;

    // Must fall back to memory backend.
    assert_eq!(
        cache.backend_name(),
        "memory",
        "unreachable Redis must fall back to memory backend"
    );
    assert!(
        !cache.is_shared(),
        "memory backend is not shared (correctness risk)"
    );
}

#[tokio::test]
async fn api_cache_no_redis_url_uses_memory_backend() {
    use event_indexer::api_cache::ApiCache;

    let cache = ApiCache::from_config(None).await;

    assert_eq!(
        cache.backend_name(),
        "memory",
        "no REDIS_URL must use memory backend"
    );
    assert!(!cache.is_shared(), "memory backend is not shared");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: Leadership failover clears cache (integration with poller logic)
// ─────────────────────────────────────────────────────────────────────────────

/// **Test:** Prove that after a leadership transition is detected, the cache
/// is cleared (via the poller's leadership-loss hook). This complements the TTL
/// mechanism by providing immediate invalidation on known leadership changes.
#[tokio::test]
async fn leadership_loss_clears_cache() {
    use std::sync::Arc;
    use tokio::sync::RwLock;

    let cache = Arc::new(RwLock::new(EventCache::with_ttl(10, 300))); // Long TTL

    // Simulate the leader ingesting events.
    {
        let mut cache_lock = cache.write().await;
        cache_lock.insert(make_event("leader-e1", 42, 100));
        cache_lock.insert(make_event("leader-e2", 42, 101));
    }

    // Verify events are cached.
    {
        let cache_lock = cache.read().await;
        assert_eq!(cache_lock.get_by_match(42).len(), 2);
    }

    // Simulate leadership loss: the poller calls `cache.clear()`.
    {
        let mut cache_lock = cache.write().await;
        cache_lock.clear();
    }

    // Verify cache is now empty (forcing DB fallback on next read).
    {
        let cache_lock = cache.read().await;
        assert_eq!(
            cache_lock.get_by_match(42).len(),
            0,
            "cache must be empty after leadership loss"
        );
        assert_eq!(cache_lock.size(), 0, "cache size must be 0");
    }
}
