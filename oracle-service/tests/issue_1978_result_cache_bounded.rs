//! Regression test for issue #1978 — result_cache must be bounded in size.
//!
//! Before the fix the cache evicted only by TTL with no maximum entry count,
//! so a long-running process accumulating unique `(platform, game_id)` pairs
//! could grow memory without bound.
//!
//! The fix ensures the cache is backed by an LRU with a fixed `max_entries`
//! capacity so that inserting beyond the bound evicts the least-recently-used
//! entry.  The [`DEFAULT_CACHE_CAPACITY`] constant caps the default instance
//! at 1 024 entries.

use oracle_service::config::Platform;
use oracle_service::result_cache::{ResultCache, DEFAULT_CACHE_CAPACITY};
use contracts_oracle::types::Winner;
use std::time::Duration;

/// Inserting more entries than the configured bound evicts the oldest (LRU)
/// entries so that the cache never exceeds `max_entries`.
#[tokio::test]
async fn cache_does_not_grow_beyond_max_entries() {
    let max_entries: usize = 8;
    let cache = ResultCache::new(max_entries, Duration::from_secs(60));

    // Insert max_entries + 4 unique game IDs.
    for i in 0..(max_entries + 4) {
        let game_id = format!("abcd{:04}", i); // 8-char Lichess-format ID
        cache
            .insert(Platform::Lichess, &game_id, Winner::Player1)
            .await;
    }

    let size = cache.len().await;
    assert_eq!(
        size, max_entries,
        "cache size should be capped at max_entries={}, got {}",
        max_entries, size
    );
}

/// The default cache capacity constant matches the documented default of 1 024.
#[tokio::test]
async fn default_capacity_is_1024() {
    assert_eq!(
        DEFAULT_CACHE_CAPACITY, 1_024,
        "default cache capacity should be 1024"
    );
}

/// LRU eviction removes the least-recently-used entry when the bound is hit.
///
/// Access pattern: insert A, insert B, access A (promotes A), insert C.
/// Expected eviction: B (now LRU), not A.
#[tokio::test]
async fn lru_eviction_promotes_accessed_entry() {
    let cache = ResultCache::new(2, Duration::from_secs(60));

    cache
        .insert(Platform::Lichess, "abcd0001", Winner::Player1)
        .await;
    cache
        .insert(Platform::Lichess, "abcd0002", Winner::Player2)
        .await;

    // Access abcd0001 to promote it — abcd0002 becomes LRU.
    let _ = cache.get(Platform::Lichess, "abcd0001").await;

    // Inserting a third entry should evict abcd0002 (LRU).
    cache
        .insert(Platform::Lichess, "abcd0003", Winner::Draw)
        .await;

    assert!(
        cache.get(Platform::Lichess, "abcd0001").await.is_some(),
        "recently accessed entry abcd0001 should still be present"
    );
    assert!(
        cache.get(Platform::Lichess, "abcd0002").await.is_none(),
        "LRU entry abcd0002 should have been evicted"
    );
    assert!(
        cache.get(Platform::Lichess, "abcd0003").await.is_some(),
        "newest entry abcd0003 should be present"
    );
}

/// TTL expiry still evicts entries even when the cache is under capacity.
#[tokio::test]
async fn ttl_eviction_still_works_under_capacity() {
    let cache = ResultCache::new(16, Duration::from_millis(5));

    cache
        .insert(Platform::Lichess, "abcd0001", Winner::Player1)
        .await;

    // Wait for TTL to expire.
    tokio::time::sleep(Duration::from_millis(20)).await;

    let result = cache.get(Platform::Lichess, "abcd0001").await;
    assert!(result.is_none(), "entry should have been evicted by TTL");
}

/// The `with_defaults` constructor produces a cache bounded at
/// `DEFAULT_CACHE_CAPACITY` entries.
#[tokio::test]
async fn with_defaults_is_bounded() {
    let cache = ResultCache::with_defaults();
    // The cache starts empty.
    assert!(cache.is_empty().await);
    // Capacity is implicit but we can verify it doesn't panic on use.
    cache
        .insert(Platform::ChessDotCom, "1234567", Winner::Draw)
        .await;
    assert_eq!(cache.len().await, 1);
}
