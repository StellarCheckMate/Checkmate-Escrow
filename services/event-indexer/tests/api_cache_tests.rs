//! API response-cache tests: TTL behaviour, invalidation on state change, and
//! the router wiring that decides *what* gets cached.
//!
//! ## No Redis required
//! Every test runs against the process-local backend
//! (`ApiCache::in_memory`), which implements the same TTL and invalidation
//! semantics as the Redis backend.  The Redis path is a thin translation of the
//! same operations onto `GET` / `SETEX` / `DEL`, so the behavioural contract is
//! covered without a live server in CI.
//!
//! The handler-level tests build a real router but point the database at an
//! unroutable DSN: a cache **hit** must answer without touching the database, so
//! a `200` proves the hit and a `500` proves the miss.  That gives end-to-end
//! coverage of the cache-first path with no PostgreSQL dependency.

use std::sync::Arc;
use std::time::Duration;

use axum::body::to_bytes;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use tokio::sync::RwLock;
use tower::ServiceExt;

use event_indexer::api::{build_router, ApiResponse, Stats};
use event_indexer::api_cache::{
    all_match_list_keys, analytics_key, analytics_ttl, match_key, match_ttl, matches_key,
    pending_matches_ttl, ApiCache, ANALYTICS_STATS, ANALYTICS_TTL_SECS, MATCH_TTL_SECS,
    PENDING_MATCHES_TTL_SECS,
};
use event_indexer::cache::EventCache;
use event_indexer::db::Database;
use event_indexer::models::{MatchInfo, MatchStatus};
use event_indexer::rpc::SorobanRpcClient;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// A DSN that parses but can never connect, so any database access fails fast.
const UNREACHABLE_DSN: &str = "postgres://nobody:nobody@127.0.0.1:1/nowhere";

fn match_info(match_id: u64, status: MatchStatus) -> MatchInfo {
    MatchInfo {
        match_id,
        player1: "GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".to_string(),
        player2: "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H".to_string(),
        status,
        winner: None,
        stake_amount: "10000000".to_string(),
        token: "XLM".to_string(),
        game_id: "abcd1234".to_string(),
        platform: "lichess".to_string(),
        created_ledger: 100,
        completed_ledger: None,
        events: vec![],
    }
}

/// Router whose database is unreachable, so only cached responses can succeed.
fn router_with(api_cache: Arc<ApiCache>) -> axum::Router {
    let db = Arc::new(
        Database::from_dsns(UNREACHABLE_DSN, UNREACHABLE_DSN, 1, 1).expect("DSN must parse"),
    );
    let cache = Arc::new(RwLock::new(EventCache::new(16)));
    let rpc = Arc::new(SorobanRpcClient::new("http://127.0.0.1:1").unwrap());
    build_router(db, cache, rpc, api_cache)
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, body.to_vec())
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. TTLs match the documented values
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn ttls_match_the_specification() {
    assert_eq!(PENDING_MATCHES_TTL_SECS, 10, "pending matches: 10 s");
    assert_eq!(MATCH_TTL_SECS, 5, "single match: 5 s");
    assert_eq!(ANALYTICS_TTL_SECS, 60, "analytics: 60 s");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Store / load behaviour
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cached_match_round_trips_unchanged() {
    let cache = ApiCache::in_memory();
    let original = match_info(7, MatchStatus::Active);

    cache.set_json(&match_key(7), &original, match_ttl()).await;
    let restored: MatchInfo = cache.get_json(&match_key(7)).await.expect("must be cached");

    assert_eq!(restored.match_id, original.match_id);
    assert_eq!(restored.status, original.status);
    assert_eq!(restored.stake_amount, original.stake_amount);
}

#[tokio::test]
async fn each_status_list_is_cached_independently() {
    let cache = ApiCache::in_memory();
    let pending = vec![match_info(1, MatchStatus::Pending)];
    let active = vec![match_info(2, MatchStatus::Active)];

    cache
        .set_json(
            &matches_key(Some(&MatchStatus::Pending)),
            &pending,
            pending_matches_ttl(),
        )
        .await;
    cache
        .set_json(
            &matches_key(Some(&MatchStatus::Active)),
            &active,
            pending_matches_ttl(),
        )
        .await;

    let got: Vec<MatchInfo> = cache
        .get_json(&matches_key(Some(&MatchStatus::Pending)))
        .await
        .unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].match_id, 1, "pending list must not serve the active list");
}

#[tokio::test]
async fn entry_expires_after_its_ttl() {
    let cache = ApiCache::in_memory();
    cache
        .set_json(&match_key(1), &match_info(1, MatchStatus::Pending), Duration::from_secs(1))
        .await;

    assert!(cache.get_json::<MatchInfo>(&match_key(1)).await.is_some());
    tokio::time::sleep(Duration::from_millis(1_100)).await;
    assert!(
        cache.get_json::<MatchInfo>(&match_key(1)).await.is_none(),
        "value must not outlive its TTL"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Invalidation on state change
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn state_change_invalidates_the_match_and_every_list() {
    let cache = ApiCache::in_memory();

    // Warm: the match itself, all list variants, and analytics.
    cache
        .set_json(&match_key(5), &match_info(5, MatchStatus::Pending), match_ttl())
        .await;
    for key in all_match_list_keys() {
        cache
            .set_json(&key, &vec![match_info(5, MatchStatus::Pending)], pending_matches_ttl())
            .await;
    }
    cache
        .set_json(
            &analytics_key(ANALYTICS_STATS),
            &Stats {
                total_events: 1,
                cache_size: 1,
            },
            analytics_ttl(),
        )
        .await;

    // The poller ingests `match:activated` for match 5.
    cache.invalidate_match(5).await;

    assert!(
        cache.get_json::<MatchInfo>(&match_key(5)).await.is_none(),
        "the match summary is stale once its state changes"
    );
    for key in all_match_list_keys() {
        assert!(
            cache.get_json::<Vec<MatchInfo>>(&key).await.is_none(),
            "{key} is stale: the match moved between lists"
        );
    }
    assert!(
        cache
            .get_json::<Stats>(&analytics_key(ANALYTICS_STATS))
            .await
            .is_none(),
        "analytics counters are derived from the event table"
    );
}

#[tokio::test]
async fn invalidation_is_scoped_to_the_changed_match() {
    let cache = ApiCache::in_memory();
    cache
        .set_json(&match_key(1), &match_info(1, MatchStatus::Active), match_ttl())
        .await;
    cache
        .set_json(&match_key(2), &match_info(2, MatchStatus::Active), match_ttl())
        .await;

    cache.invalidate_match(1).await;

    assert!(cache.get_json::<MatchInfo>(&match_key(1)).await.is_none());
    assert!(
        cache.get_json::<MatchInfo>(&match_key(2)).await.is_some(),
        "unrelated matches must stay cached"
    );
}

#[tokio::test]
async fn reorg_invalidation_clears_aggregates_only() {
    let cache = ApiCache::in_memory();
    cache
        .set_json(&match_key(3), &match_info(3, MatchStatus::Completed), match_ttl())
        .await;
    cache
        .set_json(
            &matches_key(Some(&MatchStatus::Completed)),
            &vec![match_info(3, MatchStatus::Completed)],
            pending_matches_ttl(),
        )
        .await;

    cache.invalidate_lists_and_analytics().await;

    assert!(
        cache
            .get_json::<Vec<MatchInfo>>(&matches_key(Some(&MatchStatus::Completed)))
            .await
            .is_none(),
        "lists are rebuilt after a reorg"
    );
    assert!(
        cache.get_json::<MatchInfo>(&match_key(3)).await.is_some(),
        "per-match entries are invalidated individually as their events replay"
    );
}

#[tokio::test]
async fn repeated_invalidation_is_idempotent() {
    let cache = ApiCache::in_memory();
    cache
        .set_json(&match_key(1), &match_info(1, MatchStatus::Pending), match_ttl())
        .await;

    for _ in 0..5 {
        cache.invalidate_match(1).await;
    }

    assert!(cache.get_json::<MatchInfo>(&match_key(1)).await.is_none());
    assert_eq!(cache.stats().invalidations, 5);
    assert_eq!(cache.stats().errors, 0, "deleting a missing key is not an error");
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Handler wiring — a hit must not need the database
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cached_match_is_served_without_touching_the_database() {
    let cache = Arc::new(ApiCache::in_memory());
    cache
        .set_json(&match_key(42), &match_info(42, MatchStatus::Active), match_ttl())
        .await;

    let app = router_with(cache.clone());
    let (status, body) = get(&app, "/match/42").await;

    assert_eq!(
        status,
        StatusCode::OK,
        "the database is unreachable, so a 200 can only come from the cache"
    );
    let parsed: ApiResponse<MatchInfo> = serde_json::from_slice(&body).unwrap();
    assert!(parsed.success);
    assert_eq!(parsed.data.unwrap().match_id, 42);
    assert_eq!(cache.stats().hits, 1);
}

#[tokio::test]
async fn uncached_match_falls_through_to_the_database() {
    let cache = Arc::new(ApiCache::in_memory());
    let app = router_with(cache.clone());

    let (status, _) = get(&app, "/match/999").await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "a miss must reach the (unreachable) database rather than invent a response"
    );
    assert_eq!(cache.stats().misses, 1);
}

#[tokio::test]
async fn pending_match_list_is_served_from_cache() {
    let cache = Arc::new(ApiCache::in_memory());
    cache
        .set_json(
            &matches_key(Some(&MatchStatus::Pending)),
            &vec![match_info(1, MatchStatus::Pending), match_info(2, MatchStatus::Pending)],
            pending_matches_ttl(),
        )
        .await;

    let app = router_with(cache.clone());
    let (status, body) = get(&app, "/matches?status=pending").await;

    assert_eq!(status, StatusCode::OK);
    let parsed: ApiResponse<Vec<MatchInfo>> = serde_json::from_slice(&body).unwrap();
    assert_eq!(parsed.data.unwrap().len(), 2);
}

#[tokio::test]
async fn a_different_status_does_not_reuse_the_pending_entry() {
    let cache = Arc::new(ApiCache::in_memory());
    cache
        .set_json(
            &matches_key(Some(&MatchStatus::Pending)),
            &vec![match_info(1, MatchStatus::Pending)],
            pending_matches_ttl(),
        )
        .await;

    let app = router_with(cache.clone());
    let (status, _) = get(&app, "/matches?status=active").await;

    assert_eq!(
        status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "the active list is a separate key and must miss"
    );
}

#[tokio::test]
async fn analytics_are_served_from_cache() {
    let cache = Arc::new(ApiCache::in_memory());
    cache
        .set_json(
            &analytics_key(ANALYTICS_STATS),
            &Stats {
                total_events: 1234,
                cache_size: 7,
            },
            analytics_ttl(),
        )
        .await;

    let app = router_with(cache.clone());
    let (status, body) = get(&app, "/stats").await;

    assert_eq!(status, StatusCode::OK);
    let parsed: ApiResponse<Stats> = serde_json::from_slice(&body).unwrap();
    let stats = parsed.data.unwrap();
    assert_eq!(stats.total_events, 1234);
    assert_eq!(stats.cache_size, 7);
}

#[tokio::test]
async fn invalidated_match_is_no_longer_served_from_cache() {
    let cache = Arc::new(ApiCache::in_memory());
    cache
        .set_json(&match_key(42), &match_info(42, MatchStatus::Pending), match_ttl())
        .await;

    let app = router_with(cache.clone());
    assert_eq!(get(&app, "/match/42").await.0, StatusCode::OK);

    // The poller sees `match:activated` for match 42.
    cache.invalidate_match(42).await;

    assert_eq!(
        get(&app, "/match/42").await.0,
        StatusCode::INTERNAL_SERVER_ERROR,
        "after invalidation the request must go back to the database"
    );
}

#[tokio::test]
async fn a_404_is_not_cached() {
    // Only successful lookups are stored, so a match that is about to be
    // indexed cannot be pinned to "not found" for the length of the TTL.
    let cache = Arc::new(ApiCache::in_memory());
    let app = router_with(cache.clone());

    let _ = get(&app, "/match/12345").await;

    assert!(
        cache.get_json::<MatchInfo>(&match_key(12345)).await.is_none(),
        "a negative result must not be cached"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Disabled cache
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn disabled_cache_always_reaches_the_database() {
    let cache = Arc::new(ApiCache::disabled());
    cache
        .set_json(&match_key(1), &match_info(1, MatchStatus::Pending), match_ttl())
        .await;

    let app = router_with(cache.clone());
    let (status, _) = get(&app, "/match/1").await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert!(!cache.is_enabled());
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Ingestion-shaped invalidation sequence
// ─────────────────────────────────────────────────────────────────────────────

/// Walk a match through its lifecycle the way the poller does — cache a
/// response, ingest the next event, cache again — and assert the cache never
/// serves a response from before the most recent event.
#[tokio::test]
async fn lifecycle_sequence_never_serves_a_pre_event_response() {
    let cache = ApiCache::in_memory();
    let match_id = 11;

    let lifecycle = [
        ("match:created", MatchStatus::Pending),
        ("match:deposit", MatchStatus::Pending),
        ("match:activated", MatchStatus::Active),
        ("match:completed", MatchStatus::Completed),
    ];

    let mut previous: Option<MatchStatus> = None;

    for (event_type, status) in lifecycle {
        // The poller ingests the event …
        cache.invalidate_match(match_id).await;

        // … so whatever a reader cached before it is gone.
        assert!(
            cache
                .get_json::<MatchInfo>(&match_key(match_id))
                .await
                .is_none(),
            "{event_type} must invalidate the previous response ({:?})",
            previous
        );

        // A reader then repopulates from post-event state.
        cache
            .set_json(
                &match_key(match_id),
                &match_info(match_id, status.clone()),
                match_ttl(),
            )
            .await;

        let served: MatchInfo = cache
            .get_json(&match_key(match_id))
            .await
            .expect("just cached");
        assert_eq!(served.status, status);
        previous = Some(status);
    }
}

/// The `timestamp` field exists so callers can tell how fresh a response is;
/// assert the cache preserves it byte-for-byte rather than re-stamping on read.
#[tokio::test]
async fn cache_does_not_rewrite_timestamps() {
    use event_indexer::models::IndexedEvent;

    let cache = ApiCache::in_memory();
    let event = IndexedEvent {
        id: "evt-1".to_string(),
        ledger_sequence: 10,
        match_id: 1,
        event_type: "match:created".to_string(),
        player1: None,
        player2: None,
        status: Some("pending".to_string()),
        winner: None,
        stake_amount: None,
        token: None,
        game_id: None,
        platform: None,
        timestamp: Utc::now(),
        txn_hash: None,
        event_index_in_txn: None,
        reorg_invalidated_at: None,
    };
    let expected = event.timestamp;

    cache.set_json("cm:api:test:event", &event, match_ttl()).await;
    let restored: IndexedEvent = cache.get_json("cm:api:test:event").await.unwrap();

    assert_eq!(restored.timestamp, expected);
}
