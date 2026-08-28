//! Oracle result cache — in-memory LRU cache with per-entry TTL.
//!
//! ## Purpose
//!
//! The oracle calls the chess platform API on every retry attempt.  For a
//! match that just ended, subsequent retries within the same minute will get
//! the same answer.  A local TTL cache keyed by `(platform, game_id)` avoids
//! redundant outbound API calls and reduces the risk of hitting per-IP rate
//! limits on Lichess or Chess.com.
//!
//! ## Design
//!
//! * **Key**: `(Platform, game_id)` — a `(String, String)` pair internally.
//! * **Value**: `Winner` + the instant the entry expires.
//! * **Eviction**: LRU up to `capacity` entries; additionally, entries whose
//!   TTL has elapsed are treated as misses and removed on access.
//! * **TTL default**: 60 seconds (configurable at construction time).
//! * **Capacity default**: 1 024 entries.
//!
//! The cache is wrapped in a `tokio::sync::Mutex` so it can be shared across
//! async tasks without blocking the executor thread.
//!
//! ## Thread safety
//!
//! [`ResultCache`] is `Clone` — all clones share the same underlying `Arc<Mutex<…>>`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lru::LruCache;
use tokio::sync::Mutex;

use crate::config::Platform;
use crate::oracle::Winner;

/// Default TTL for cached results (60 seconds).
pub const DEFAULT_CACHE_TTL_SECS: u64 = 60;

/// Default maximum number of entries held in the LRU cache.
pub const DEFAULT_CACHE_CAPACITY: usize = 1_024;

/// Cache key: `(platform_name, game_id)`.
type CacheKey = (String, String);

/// A single cached value: the winner plus the instant the entry expires.
#[derive(Debug, Clone)]
struct CacheEntry {
    winner: Winner,
    expires_at: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        Instant::now() >= self.expires_at
    }
}

/// Shared, clone-cheap oracle result cache.
///
/// Use [`ResultCache::get`] before making a platform API call and
/// [`ResultCache::insert`] after a successful fetch.
#[derive(Clone)]
pub struct ResultCache {
    inner: Arc<Mutex<LruCache<CacheKey, CacheEntry>>>,
    ttl: Duration,
}

impl ResultCache {
    /// Create a cache with the given capacity and TTL.
    ///
    /// * `capacity` — maximum number of `(platform, game_id)` entries to keep.
    /// * `ttl` — how long a cached result is considered valid.
    pub fn new(capacity: usize, ttl: Duration) -> Self {
        let cap = std::num::NonZeroUsize::new(capacity.max(1)).expect("capacity must be >= 1");
        Self {
            inner: Arc::new(Mutex::new(LruCache::new(cap))),
            ttl,
        }
    }

    /// Create a cache with default settings (1 024 entries, 60 s TTL).
    pub fn with_defaults() -> Self {
        Self::new(
            DEFAULT_CACHE_CAPACITY,
            Duration::from_secs(DEFAULT_CACHE_TTL_SECS),
        )
    }

    /// Look up a cached result for `(platform, game_id)`.
    ///
    /// Returns `Some(winner)` if there is a non-expired entry, `None` otherwise.
    /// Expired entries are removed on access.
    pub async fn get(&self, platform: Platform, game_id: &str) -> Option<Winner> {
        let key = cache_key(platform, game_id);
        let mut guard = self.inner.lock().await;
        if let Some(entry) = guard.peek(&key) {
            if entry.is_expired() {
                // Remove the stale entry and report a miss.
                guard.pop(&key);
                return None;
            }
            // `peek` doesn't update LRU order; use `get` to promote the entry.
            return guard.get(&key).map(|e| e.winner.clone());
        }
        None
    }

    /// Store a `winner` for `(platform, game_id)` with the configured TTL.
    pub async fn insert(&self, platform: Platform, game_id: &str, winner: Winner) {
        let key = cache_key(platform, game_id);
        let entry = CacheEntry {
            winner,
            expires_at: Instant::now() + self.ttl,
        };
        self.inner.lock().await.put(key, entry);
    }

    /// Current number of (possibly expired) entries in the cache.
    ///
    /// Primarily useful in tests.
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    /// Return `true` if the cache is empty.
    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
}

fn cache_key(platform: Platform, game_id: &str) -> CacheKey {
    let platform_str = match platform {
        Platform::Lichess => "lichess",
        Platform::ChessDotCom => "chess.com",
    };
    (platform_str.to_string(), game_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts_oracle::types::Winner;

    #[tokio::test]
    async fn cache_hit_within_ttl() {
        let cache = ResultCache::new(16, Duration::from_secs(60));
        cache
            .insert(Platform::Lichess, "abcd1234", Winner::Player1)
            .await;

        let result = cache.get(Platform::Lichess, "abcd1234").await;
        assert_eq!(result, Some(Winner::Player1));
    }

    #[tokio::test]
    async fn cache_miss_for_unknown_key() {
        let cache = ResultCache::with_defaults();
        let result = cache.get(Platform::Lichess, "zzzzzzzz").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cache_miss_after_ttl_expiry() {
        // Very short TTL so the entry expires before we read it.
        let cache = ResultCache::new(16, Duration::from_millis(1));
        cache
            .insert(Platform::Lichess, "abcd1234", Winner::Player1)
            .await;

        // Sleep past the TTL.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = cache.get(Platform::Lichess, "abcd1234").await;
        assert!(result.is_none(), "entry should have expired");
    }

    #[tokio::test]
    async fn different_platforms_are_separate_keys() {
        let cache = ResultCache::new(16, Duration::from_secs(60));
        cache
            .insert(Platform::Lichess, "abcd1234", Winner::Player1)
            .await;
        cache
            .insert(Platform::ChessDotCom, "abcd1234", Winner::Player2)
            .await;

        assert_eq!(
            cache.get(Platform::Lichess, "abcd1234").await,
            Some(Winner::Player1)
        );
        assert_eq!(
            cache.get(Platform::ChessDotCom, "abcd1234").await,
            Some(Winner::Player2)
        );
    }

    #[tokio::test]
    async fn lru_eviction_respects_capacity() {
        let cache = ResultCache::new(2, Duration::from_secs(60));
        cache
            .insert(Platform::Lichess, "game0001", Winner::Player1)
            .await;
        cache
            .insert(Platform::Lichess, "game0002", Winner::Draw)
            .await;
        // Inserting a third entry should evict the LRU entry (game0001).
        cache
            .insert(Platform::Lichess, "game0003", Winner::Player2)
            .await;

        assert_eq!(cache.len().await, 2);
        // game0001 was evicted (LRU).
        assert!(cache.get(Platform::Lichess, "game0001").await.is_none());
    }
}
