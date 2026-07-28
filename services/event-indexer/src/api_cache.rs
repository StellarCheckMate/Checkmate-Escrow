//! Redis-backed response cache for high-traffic read endpoints.
//!
//! ## Why a second cache?
//! [`crate::cache::EventCache`] is an **in-process LRU of raw events** filled by
//! the ingestion path.  It is per-instance and has no notion of TTL, so it
//! cannot be used to memoise *rendered API responses* across the horizontally
//! scaled replicas (`event-indexer-1`, `event-indexer-2`, …).  This module adds
//! a **shared, TTL-bounded response cache** so that repeated calls to
//! `/matches?status=pending`, `/match/:id` and the analytics endpoints do not
//! re-run the same fan-out queries (and, for endpoints that fall through to the
//! chain, the same contract calls) on every request.
//!
//! ## TTLs
//! | Cached response                     | TTL      |
//! |-------------------------------------|----------|
//! | pending-match list (`/matches`)     | 10 s     |
//! | single match (`/match/:id`)         | 5 s      |
//! | analytics (`/stats`)                | 60 s     |
//!
//! Match entries are additionally **invalidated eagerly** whenever the poller
//! ingests a contract event for that match, so a state change is visible well
//! before the 5-second TTL would have expired (see
//! [`ApiCache::invalidate_match`]).
//!
//! ## Fail-open by design
//! The cache is an optimisation, never a source of truth.  Every Redis error is
//! logged at `warn` level and treated as a cache miss, so a Redis outage
//! degrades latency but never correctness or availability.  When `REDIS_URL` is
//! not configured the cache runs in [`Backend::Memory`] mode (per-process,
//! same TTL semantics) so a single-node deployment needs no extra dependency.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{de::DeserializeOwned, Serialize};
use tracing::{debug, warn};

use crate::models::MatchStatus;

// ── TTL constants (seconds) ───────────────────────────────────────────────────

/// TTL for the match-list endpoint, including the pending-match list.
pub const PENDING_MATCHES_TTL_SECS: u64 = 10;
/// TTL for a single match summary.
pub const MATCH_TTL_SECS: u64 = 5;
/// TTL for analytics responses.
pub const ANALYTICS_TTL_SECS: u64 = 60;

pub fn pending_matches_ttl() -> Duration {
    Duration::from_secs(PENDING_MATCHES_TTL_SECS)
}

pub fn match_ttl() -> Duration {
    Duration::from_secs(MATCH_TTL_SECS)
}

pub fn analytics_ttl() -> Duration {
    Duration::from_secs(ANALYTICS_TTL_SECS)
}

// ── Key namespace ─────────────────────────────────────────────────────────────

/// Every key this service owns is prefixed so a shared Redis instance can host
/// other workloads without collisions.
pub const KEY_PREFIX: &str = "cm:api";

/// Analytics key names (kept as constants so invalidation can enumerate them).
pub const ANALYTICS_STATS: &str = "stats";

/// All analytics sub-keys, used by [`ApiCache::invalidate_match`].
pub const ANALYTICS_KEYS: [&str; 1] = [ANALYTICS_STATS];

/// Cache key for a single match summary.
pub fn match_key(match_id: u64) -> String {
    format!("{}:match:{}", KEY_PREFIX, match_id)
}

/// Cache key for the match-list endpoint.  `None` is the unfiltered list.
pub fn matches_key(status: Option<&MatchStatus>) -> String {
    let suffix = match status {
        None => "all",
        Some(MatchStatus::Pending) => "pending",
        Some(MatchStatus::Active) => "active",
        Some(MatchStatus::Completed) => "completed",
        Some(MatchStatus::Cancelled) => "cancelled",
        Some(MatchStatus::Expired) => "expired",
    };
    format!("{}:matches:{}", KEY_PREFIX, suffix)
}

/// Cache key for an analytics response.
pub fn analytics_key(name: &str) -> String {
    format!("{}:analytics:{}", KEY_PREFIX, name)
}

/// Every match-list key.  A single match state change can move a match between
/// any two lists, so all of them are dropped together.
pub fn all_match_list_keys() -> Vec<String> {
    vec![
        matches_key(None),
        matches_key(Some(&MatchStatus::Pending)),
        matches_key(Some(&MatchStatus::Active)),
        matches_key(Some(&MatchStatus::Completed)),
        matches_key(Some(&MatchStatus::Cancelled)),
        matches_key(Some(&MatchStatus::Expired)),
    ]
}

/// Every analytics key.
pub fn all_analytics_keys() -> Vec<String> {
    ANALYTICS_KEYS.iter().map(|n| analytics_key(n)).collect()
}

// ── Metrics ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ApiCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
    pub errors: u64,
}

impl ApiCacheStats {
    /// Hit rate in the range `0.0..=1.0`; `0.0` when no lookups happened yet.
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[derive(Default)]
struct Counters {
    hits: AtomicU64,
    misses: AtomicU64,
    invalidations: AtomicU64,
    errors: AtomicU64,
}

// ── In-memory backend ─────────────────────────────────────────────────────────

/// Process-local TTL map used when no `REDIS_URL` is configured (and in tests).
///
/// Semantics deliberately mirror Redis `SET key value EX ttl`: a value is
/// readable until its deadline, then behaves as absent.
#[derive(Default)]
struct MemoryStore {
    entries: Mutex<HashMap<String, (Instant, String)>>,
}

impl MemoryStore {
    fn get(&self, key: &str) -> Option<String> {
        let mut guard = self.entries.lock().expect("memory cache mutex poisoned");
        match guard.get(key) {
            Some((deadline, payload)) => {
                if Instant::now() >= *deadline {
                    guard.remove(key);
                    None
                } else {
                    Some(payload.clone())
                }
            }
            None => None,
        }
    }

    fn set(&self, key: &str, payload: String, ttl: Duration) {
        let mut guard = self.entries.lock().expect("memory cache mutex poisoned");
        guard.insert(key.to_string(), (Instant::now() + ttl, payload));
    }

    fn delete(&self, keys: &[String]) {
        let mut guard = self.entries.lock().expect("memory cache mutex poisoned");
        for key in keys {
            guard.remove(key);
        }
    }

    fn clear(&self) {
        self.entries
            .lock()
            .expect("memory cache mutex poisoned")
            .clear();
    }

    fn len(&self) -> usize {
        let now = Instant::now();
        self.entries
            .lock()
            .expect("memory cache mutex poisoned")
            .values()
            .filter(|(deadline, _)| now < *deadline)
            .count()
    }
}

// ── Backend ───────────────────────────────────────────────────────────────────

enum Backend {
    /// Caching turned off entirely — every lookup misses, every write is a no-op.
    Disabled,
    /// Process-local TTL map.
    Memory(MemoryStore),
    /// Shared Redis, via a multiplexed connection manager that reconnects itself.
    Redis(redis::aio::ConnectionManager),
}

// ── ApiCache ──────────────────────────────────────────────────────────────────

/// TTL response cache with a Redis backend and a process-local fallback.
pub struct ApiCache {
    backend: Backend,
    counters: Counters,
}

impl ApiCache {
    /// A cache that never stores anything.  Useful for benchmarks and for
    /// reproducing uncached behaviour in tests.
    pub fn disabled() -> Self {
        ApiCache {
            backend: Backend::Disabled,
            counters: Counters::default(),
        }
    }

    /// A process-local cache with the same TTL semantics as the Redis backend.
    pub fn in_memory() -> Self {
        ApiCache {
            backend: Backend::Memory(MemoryStore::default()),
            counters: Counters::default(),
        }
    }

    /// Connect to Redis.  Returns an error if the URL is unparseable or the
    /// server is unreachable at start-up.
    pub async fn connect(redis_url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let manager = redis::aio::ConnectionManager::new(client).await?;
        Ok(ApiCache {
            backend: Backend::Redis(manager),
            counters: Counters::default(),
        })
    }

    /// Build from optional configuration, degrading gracefully:
    /// - `Some(url)` and reachable → Redis backend.
    /// - `Some(url)` but unreachable → warn and fall back to the in-memory backend.
    /// - `None` → in-memory backend.
    pub async fn from_config(redis_url: Option<&str>) -> Self {
        match redis_url {
            Some(url) if !url.trim().is_empty() => match Self::connect(url).await {
                Ok(cache) => {
                    debug!("API response cache using Redis backend");
                    cache
                }
                Err(e) => {
                    warn!(
                        "Redis unavailable ({}) — falling back to a process-local \
                         response cache. Latency will be higher across replicas.",
                        e
                    );
                    Self::in_memory()
                }
            },
            _ => {
                debug!("REDIS_URL not set — using a process-local response cache");
                Self::in_memory()
            }
        }
    }

    /// Whether reads/writes hit a real store.
    pub fn is_enabled(&self) -> bool {
        !matches!(self.backend, Backend::Disabled)
    }

    /// Whether the shared (Redis) backend is in use.
    pub fn is_shared(&self) -> bool {
        matches!(self.backend, Backend::Redis(_))
    }

    /// Short backend name for logs and the analytics endpoint.
    pub fn backend_name(&self) -> &'static str {
        match self.backend {
            Backend::Disabled => "disabled",
            Backend::Memory(_) => "memory",
            Backend::Redis(_) => "redis",
        }
    }

    pub fn stats(&self) -> ApiCacheStats {
        ApiCacheStats {
            hits: self.counters.hits.load(Ordering::Relaxed),
            misses: self.counters.misses.load(Ordering::Relaxed),
            invalidations: self.counters.invalidations.load(Ordering::Relaxed),
            errors: self.counters.errors.load(Ordering::Relaxed),
        }
    }

    /// Number of live (unexpired) entries — only meaningful for the in-memory
    /// backend; Redis reports `None` because `DBSIZE` would count foreign keys.
    pub fn local_len(&self) -> Option<usize> {
        match &self.backend {
            Backend::Memory(store) => Some(store.len()),
            _ => None,
        }
    }

    // ── Read / write ──────────────────────────────────────────────────────

    /// Fetch and deserialize a cached value.  A decode failure is treated as a
    /// miss and the poisoned key is dropped, so a response-shape change during a
    /// rolling deploy cannot wedge the cache.
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let raw = match &self.backend {
            Backend::Disabled => {
                self.counters.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }
            Backend::Memory(store) => store.get(key),
            Backend::Redis(manager) => {
                use redis::AsyncCommands;
                let mut conn = manager.clone();
                match conn.get::<_, Option<String>>(key).await {
                    Ok(value) => value,
                    Err(e) => {
                        self.record_error("GET", key, &e);
                        None
                    }
                }
            }
        };

        match raw {
            Some(payload) => match serde_json::from_str::<T>(&payload) {
                Ok(value) => {
                    self.counters.hits.fetch_add(1, Ordering::Relaxed);
                    debug!(key, "API cache hit");
                    Some(value)
                }
                Err(e) => {
                    warn!(key, "cached payload failed to decode ({}) — dropping", e);
                    self.counters.errors.fetch_add(1, Ordering::Relaxed);
                    self.counters.misses.fetch_add(1, Ordering::Relaxed);
                    self.delete(&[key.to_string()]).await;
                    None
                }
            },
            None => {
                self.counters.misses.fetch_add(1, Ordering::Relaxed);
                debug!(key, "API cache miss");
                None
            }
        }
    }

    /// Store a value under `key` with an expiry of `ttl`.
    pub async fn set_json<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) {
        let payload = match serde_json::to_string(value) {
            Ok(p) => p,
            Err(e) => {
                warn!(key, "failed to serialize value for cache: {}", e);
                self.counters.errors.fetch_add(1, Ordering::Relaxed);
                return;
            }
        };

        // A zero TTL would mean "no expiry" in Redis; refuse to write instead of
        // installing an entry that never goes away.
        let ttl_secs = ttl.as_secs().max(1);

        match &self.backend {
            Backend::Disabled => {}
            Backend::Memory(store) => store.set(key, payload, Duration::from_secs(ttl_secs)),
            Backend::Redis(manager) => {
                use redis::AsyncCommands;
                let mut conn = manager.clone();
                if let Err(e) = conn
                    .set_ex::<_, _, ()>(key, payload, ttl_secs)
                    .await
                {
                    self.record_error("SETEX", key, &e);
                }
            }
        }
    }

    /// Drop the given keys.  Missing keys are not an error.
    pub async fn delete(&self, keys: &[String]) {
        if keys.is_empty() {
            return;
        }
        match &self.backend {
            Backend::Disabled => {}
            Backend::Memory(store) => store.delete(keys),
            Backend::Redis(manager) => {
                use redis::AsyncCommands;
                let mut conn = manager.clone();
                if let Err(e) = conn.del::<_, ()>(keys).await {
                    self.record_error("DEL", &keys.join(","), &e);
                }
            }
        }
    }

    // ── Invalidation ──────────────────────────────────────────────────────

    /// Invalidate every response whose content can change when `match_id`
    /// changes state.  Called by the poller for each ingested contract event.
    ///
    /// This covers three key families:
    /// 1. the match's own summary (`/match/:id`),
    /// 2. every match list (`/matches`, including the pending list) — a
    ///    transition moves the match from one list to another,
    /// 3. analytics, whose counters are derived from the event table.
    pub async fn invalidate_match(&self, match_id: u64) {
        if !self.is_enabled() {
            return;
        }

        let mut keys = vec![match_key(match_id)];
        keys.extend(all_match_list_keys());
        keys.extend(all_analytics_keys());

        self.delete(&keys).await;
        self.counters.invalidations.fetch_add(1, Ordering::Relaxed);
        debug!(match_id, "invalidated cached responses after contract event");
    }

    /// Invalidate all list and analytics responses without touching individual
    /// match entries.  Used after bulk changes such as a reorg rollback.
    pub async fn invalidate_lists_and_analytics(&self) {
        if !self.is_enabled() {
            return;
        }
        let mut keys = all_match_list_keys();
        keys.extend(all_analytics_keys());
        self.delete(&keys).await;
        self.counters.invalidations.fetch_add(1, Ordering::Relaxed);
    }

    /// Drop every key this service owns.  Only the in-memory backend is cleared
    /// wholesale; for Redis the known key families are deleted so foreign keys
    /// in a shared instance survive.
    pub async fn clear(&self) {
        match &self.backend {
            Backend::Disabled => {}
            Backend::Memory(store) => store.clear(),
            Backend::Redis(_) => self.invalidate_lists_and_analytics().await,
        }
        self.counters.invalidations.fetch_add(1, Ordering::Relaxed);
    }

    // ── Internal ──────────────────────────────────────────────────────────

    fn record_error(&self, op: &str, key: &str, e: &redis::RedisError) {
        self.counters.errors.fetch_add(1, Ordering::Relaxed);
        warn!(
            "Redis {} failed for key {} ({}) — serving uncached",
            op, key, e
        );
    }
}

impl Default for ApiCache {
    fn default() -> Self {
        Self::in_memory()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Payload {
        id: u64,
        label: String,
    }

    fn payload(id: u64) -> Payload {
        Payload {
            id,
            label: format!("match-{}", id),
        }
    }

    // ── Keys ──────────────────────────────────────────────────────────────

    #[test]
    fn keys_are_namespaced_and_distinct() {
        assert_eq!(match_key(7), "cm:api:match:7");
        assert_eq!(matches_key(Some(&MatchStatus::Pending)), "cm:api:matches:pending");
        assert_eq!(matches_key(None), "cm:api:matches:all");
        assert_eq!(analytics_key(ANALYTICS_STATS), "cm:api:analytics:stats");
        assert_ne!(match_key(7), match_key(8));
    }

    #[test]
    fn every_status_has_its_own_list_key() {
        let keys = all_match_list_keys();
        let unique: std::collections::HashSet<_> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len(), "list keys must not collide");
        assert_eq!(keys.len(), 6, "5 statuses + the unfiltered list");
    }

    #[test]
    fn documented_ttls_are_the_configured_ttls() {
        assert_eq!(pending_matches_ttl().as_secs(), 10);
        assert_eq!(match_ttl().as_secs(), 5);
        assert_eq!(analytics_ttl().as_secs(), 60);
    }

    // ── Round-trip ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn set_then_get_returns_the_value() {
        let cache = ApiCache::in_memory();
        cache.set_json(&match_key(1), &payload(1), match_ttl()).await;

        let got: Option<Payload> = cache.get_json(&match_key(1)).await;
        assert_eq!(got, Some(payload(1)));
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().misses, 0);
    }

    #[tokio::test]
    async fn missing_key_is_a_miss() {
        let cache = ApiCache::in_memory();
        let got: Option<Payload> = cache.get_json(&match_key(404)).await;
        assert!(got.is_none());
        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hit_rate(), 0.0);
    }

    #[tokio::test]
    async fn disabled_cache_never_stores() {
        let cache = ApiCache::disabled();
        cache.set_json(&match_key(1), &payload(1), match_ttl()).await;
        let got: Option<Payload> = cache.get_json(&match_key(1)).await;
        assert!(got.is_none(), "disabled cache must always miss");
        assert!(!cache.is_enabled());
    }

    #[tokio::test]
    async fn expired_entry_is_not_served() {
        let cache = ApiCache::in_memory();
        // 1 s is the smallest TTL the cache will install.
        cache
            .set_json(&match_key(2), &payload(2), Duration::from_secs(1))
            .await;
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let got: Option<Payload> = cache.get_json(&match_key(2)).await;
        assert!(got.is_none(), "entry must expire once its TTL elapses");
    }

    #[tokio::test]
    async fn zero_ttl_is_clamped_rather_than_stored_forever() {
        let cache = ApiCache::in_memory();
        cache
            .set_json(&match_key(3), &payload(3), Duration::from_secs(0))
            .await;
        // Clamped to 1 s: readable now …
        let got: Option<Payload> = cache.get_json(&match_key(3)).await;
        assert_eq!(got, Some(payload(3)));
        // … and gone afterwards.
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let got: Option<Payload> = cache.get_json(&match_key(3)).await;
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn undecodable_payload_is_treated_as_a_miss_and_dropped() {
        let cache = ApiCache::in_memory();
        // Store a differently-shaped value under the same key.
        cache.set_json(&match_key(4), &"not-a-payload", match_ttl()).await;

        let got: Option<Payload> = cache.get_json(&match_key(4)).await;
        assert!(got.is_none(), "shape mismatch must not be served");
        assert_eq!(cache.stats().errors, 1);

        // The poisoned key must have been evicted, so a second read is a plain miss.
        let got: Option<Payload> = cache.get_json(&match_key(4)).await;
        assert!(got.is_none());
        assert_eq!(cache.stats().errors, 1, "no second decode error");
    }

    // ── Invalidation ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn invalidate_match_drops_that_match() {
        let cache = ApiCache::in_memory();
        cache.set_json(&match_key(9), &payload(9), match_ttl()).await;

        cache.invalidate_match(9).await;

        let got: Option<Payload> = cache.get_json(&match_key(9)).await;
        assert!(got.is_none(), "match entry must be invalidated");
        assert_eq!(cache.stats().invalidations, 1);
    }

    #[tokio::test]
    async fn invalidate_match_drops_every_match_list() {
        let cache = ApiCache::in_memory();
        for key in all_match_list_keys() {
            cache.set_json(&key, &payload(1), pending_matches_ttl()).await;
        }

        cache.invalidate_match(1).await;

        for key in all_match_list_keys() {
            let got: Option<Payload> = cache.get_json(&key).await;
            assert!(got.is_none(), "{} must be invalidated", key);
        }
    }

    #[tokio::test]
    async fn invalidate_match_drops_analytics() {
        let cache = ApiCache::in_memory();
        let key = analytics_key(ANALYTICS_STATS);
        cache.set_json(&key, &payload(1), analytics_ttl()).await;

        cache.invalidate_match(42).await;

        let got: Option<Payload> = cache.get_json(&key).await;
        assert!(got.is_none(), "analytics are derived from events");
    }

    #[tokio::test]
    async fn invalidating_one_match_leaves_other_matches_cached() {
        let cache = ApiCache::in_memory();
        cache.set_json(&match_key(1), &payload(1), match_ttl()).await;
        cache.set_json(&match_key(2), &payload(2), match_ttl()).await;

        cache.invalidate_match(1).await;

        let one: Option<Payload> = cache.get_json(&match_key(1)).await;
        let two: Option<Payload> = cache.get_json(&match_key(2)).await;
        assert!(one.is_none(), "match 1 was invalidated");
        assert_eq!(two, Some(payload(2)), "match 2 must be untouched");
    }

    #[tokio::test]
    async fn clear_removes_everything() {
        let cache = ApiCache::in_memory();
        cache.set_json(&match_key(1), &payload(1), match_ttl()).await;
        cache
            .set_json(&matches_key(Some(&MatchStatus::Pending)), &payload(2), pending_matches_ttl())
            .await;

        cache.clear().await;

        assert_eq!(cache.local_len(), Some(0));
    }

    #[tokio::test]
    async fn invalidation_on_a_disabled_cache_is_a_noop() {
        let cache = ApiCache::disabled();
        cache.invalidate_match(1).await;
        assert_eq!(cache.stats().invalidations, 0);
    }

    // ── Metrics ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn hit_rate_reflects_hits_and_misses() {
        let cache = ApiCache::in_memory();
        cache.set_json(&match_key(1), &payload(1), match_ttl()).await;

        let _: Option<Payload> = cache.get_json(&match_key(1)).await; // hit
        let _: Option<Payload> = cache.get_json(&match_key(1)).await; // hit
        let _: Option<Payload> = cache.get_json(&match_key(2)).await; // miss

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn from_config_without_url_falls_back_to_memory() {
        let cache = ApiCache::from_config(None).await;
        assert_eq!(cache.backend_name(), "memory");
        assert!(cache.is_enabled());
        assert!(!cache.is_shared());
    }

    #[tokio::test]
    async fn from_config_with_blank_url_falls_back_to_memory() {
        let cache = ApiCache::from_config(Some("   ")).await;
        assert_eq!(cache.backend_name(), "memory");
    }
}
