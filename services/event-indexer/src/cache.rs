//! In-process LRU cache for recently-seen events.
//!
//! ## Eviction policy
//! We maintain a **strict LRU** order using an [`indexmap::IndexMap`].  Every
//! `insert` of an already-present key moves it to the back (most-recently-used
//! position).  When capacity is exceeded the *front* entry (least-recently-used)
//! is removed.  This gives fully deterministic, insertion/access-order eviction
//! that can be verified in tests without any reliance on hash-map iteration
//! order.
//!
//! ## Thread safety
//! `EventCache` is intentionally **not** `Sync`.  Callers are expected to wrap
//! it in `Arc<tokio::sync::RwLock<EventCache>>`, which is how `main.rs` and the
//! API server already use it.

use crate::models::IndexedEvent;
use indexmap::IndexMap;

pub struct EventCache {
    /// Ordered map: key = event ID, value = event. Front = LRU, back = MRU.
    events: IndexMap<String, IndexedEvent>,
    /// Secondary index: match_id → ordered list of event IDs in this cache.
    match_index: IndexMap<u64, Vec<String>>,
    max_size: usize,
}

impl EventCache {
    pub fn new(max_size: usize) -> Self {
        assert!(max_size > 0, "EventCache max_size must be > 0");
        EventCache {
            events: IndexMap::new(),
            match_index: IndexMap::new(),
            max_size,
        }
    }

    /// Insert or refresh an event.
    ///
    /// - If the event is already present it is *moved to the MRU position*.
    /// - When the cache is at capacity the LRU entry (front of the map) is
    ///   evicted before the new entry is inserted.
    pub fn insert(&mut self, event: IndexedEvent) {
        let event_id = event.id.clone();
        let match_id = event.match_id;

        // If it already exists, remove first so we can re-insert at the back
        // (most-recently-used position).
        if self.events.contains_key(&event_id) {
            self.events.shift_remove(&event_id);
        } else if self.events.len() >= self.max_size {
            // Evict the LRU entry (index 0 = front).
            if let Some((evicted_id, _)) = self.events.shift_remove_index(0) {
                // Also remove from match_index.
                self.remove_from_match_index(&evicted_id);
            }
        }

        self.events.insert(event_id.clone(), event);
        self.match_index
            .entry(match_id)
            .or_default()
            .push(event_id);
    }

    /// Retrieve an event by ID.
    ///
    /// Note: this is a read-only lookup and does **not** update LRU order.
    /// Promoting on read would require `&mut self`; for a read-heavy workload
    /// (the common case in `api.rs`) the simpler semantics are preferable and
    /// still give very good hit rates on recently-ingested events.
    pub fn get(&self, event_id: &str) -> Option<IndexedEvent> {
        self.events.get(event_id).cloned()
    }

    /// Return all cached events for a match in insertion order.
    pub fn get_by_match(&self, match_id: u64) -> Vec<IndexedEvent> {
        self.match_index
            .get(&match_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.events.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Explicitly remove an event from the cache.
    pub fn remove(&mut self, event_id: &str) {
        if self.events.shift_remove(event_id).is_some() {
            self.remove_from_match_index(event_id);
        }
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.match_index.clear();
    }

    pub fn size(&self) -> usize {
        self.events.len()
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    fn remove_from_match_index(&mut self, event_id: &str) {
        // We need to know which match_id this event belonged to, but we've
        // already removed it from `self.events`.  Scan the match_index to find
        // and remove the stale reference.  The match_index stays small per match,
        // so this linear scan is acceptable.
        self.match_index
            .values_mut()
            .for_each(|ids| ids.retain(|id| id != event_id));
        // Prune empty vecs to keep the index tidy.
        self.match_index.retain(|_, ids| !ids.is_empty());
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_event(id: &str, match_id: u64) -> IndexedEvent {
        IndexedEvent {
            id: id.to_string(),
            ledger_sequence: 1,
            match_id,
            event_type: "test".to_string(),
            player1: None,
            player2: None,
            status: None,
            winner: None,
            stake_amount: None,
            token: None,
            game_id: None,
            platform: None,
            timestamp: Utc::now(),
            txn_hash: None,
        }
    }

    // ── Capacity and basic eviction ───────────────────────────────────────

    #[test]
    fn cache_never_exceeds_max_size() {
        let mut cache = EventCache::new(2);
        cache.insert(make_event("a", 1));
        cache.insert(make_event("b", 1));
        cache.insert(make_event("c", 1));
        assert_eq!(cache.size(), 2);
    }

    #[test]
    fn newest_entry_is_always_retained() {
        let mut cache = EventCache::new(2);
        cache.insert(make_event("a", 1));
        cache.insert(make_event("b", 1));
        cache.insert(make_event("c", 1));
        assert!(cache.get("c").is_some(), "most-recently inserted must be present");
    }

    // ── Deterministic LRU eviction order ─────────────────────────────────

    /// Insert A, B, C into a capacity-2 cache.  A is the LRU entry at the time
    /// C is inserted, so A must be the evicted one.  This test proves the
    /// eviction is **deterministic** and **FIFO among unaccessed entries**.
    #[test]
    fn lru_evicts_oldest_inserted_entry() {
        let mut cache = EventCache::new(2);
        cache.insert(make_event("a", 1)); // LRU after: [a]
        cache.insert(make_event("b", 1)); // LRU after: [a, b]
        // Inserting c must evict a (the front / LRU entry).
        cache.insert(make_event("c", 1)); // LRU after: [b, c]

        assert!(cache.get("a").is_none(), "a must have been evicted (LRU)");
        assert!(cache.get("b").is_some(), "b must survive");
        assert!(cache.get("c").is_some(), "c must survive");
    }

    /// Prove eviction order across four sequential insertions into a
    /// capacity-2 cache.
    #[test]
    fn lru_eviction_order_is_fifo_for_unaccessed_entries() {
        let mut cache = EventCache::new(2);
        cache.insert(make_event("1", 1));
        cache.insert(make_event("2", 1));
        // evicts "1"
        cache.insert(make_event("3", 1));
        assert!(cache.get("1").is_none());
        assert!(cache.get("2").is_some());
        assert!(cache.get("3").is_some());

        // evicts "2"
        cache.insert(make_event("4", 1));
        assert!(cache.get("2").is_none());
        assert!(cache.get("3").is_some());
        assert!(cache.get("4").is_some());
    }

    /// Re-inserting an existing key must *not* evict any other entry and
    /// must move the key to the MRU position.
    #[test]
    fn reinserting_existing_key_moves_it_to_mru() {
        let mut cache = EventCache::new(2);
        cache.insert(make_event("a", 1)); // [a]
        cache.insert(make_event("b", 1)); // [a, b]
        // Touch a → moves it to back: [b, a]
        cache.insert(make_event("a", 1));
        assert_eq!(cache.size(), 2, "size must not grow on re-insert");

        // Now inserting c must evict b (new LRU), not a.
        cache.insert(make_event("c", 1)); // evicts b → [a, c]
        assert!(cache.get("a").is_some(), "a must survive (was MRU)");
        assert!(cache.get("b").is_none(), "b must be evicted (was LRU)");
        assert!(cache.get("c").is_some(), "c must be present");
    }

    // ── Match index ───────────────────────────────────────────────────────

    #[test]
    fn get_by_match_returns_all_events_for_match() {
        let mut cache = EventCache::new(10);
        cache.insert(make_event("e1", 42));
        cache.insert(make_event("e2", 42));
        cache.insert(make_event("e3", 99));

        let m42 = cache.get_by_match(42);
        assert_eq!(m42.len(), 2);
        assert!(m42.iter().all(|e| e.match_id == 42));

        let m99 = cache.get_by_match(99);
        assert_eq!(m99.len(), 1);
    }

    #[test]
    fn match_index_cleaned_up_on_eviction() {
        let mut cache = EventCache::new(2);
        cache.insert(make_event("e1", 1)); // will be evicted
        cache.insert(make_event("e2", 2));
        cache.insert(make_event("e3", 3)); // evicts e1

        // e1 is gone; match 1 should have no cached events.
        assert_eq!(cache.get_by_match(1).len(), 0);
    }

    #[test]
    fn explicit_remove_cleans_match_index() {
        let mut cache = EventCache::new(10);
        cache.insert(make_event("e1", 7));
        cache.remove("e1");
        assert_eq!(cache.get_by_match(7).len(), 0);
        assert_eq!(cache.size(), 0);
    }

    // ── Edge cases ────────────────────────────────────────────────────────

    #[test]
    fn eviction_at_capacity_original_test() {
        let capacity = 2;
        let mut cache = EventCache::new(capacity);

        cache.insert(make_event("evt-1", 1));
        cache.insert(make_event("evt-2", 1));
        assert_eq!(cache.size(), 2);

        cache.insert(make_event("evt-3", 1));

        assert_eq!(cache.size(), capacity, "cache must not exceed max_size after eviction");
        assert!(cache.get("evt-3").is_some(), "newly inserted event must be present");

        let surviving_old = ["evt-1", "evt-2"]
            .iter()
            .filter(|id| cache.get(id).is_some())
            .count();
        assert_eq!(surviving_old, 1, "exactly one old entry must survive eviction");
        // With LRU the surviving one is always evt-2 (more recently inserted).
        assert!(
            cache.get("evt-2").is_some(),
            "evt-2 (MRU among old entries) must survive; evt-1 (LRU) must be evicted"
        );
        assert!(cache.get("evt-1").is_none(), "evt-1 (LRU) must be evicted");
    }
}
