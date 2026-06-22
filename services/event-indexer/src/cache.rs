use crate::models::IndexedEvent;
use std::collections::HashMap;

pub struct EventCache {
    events: HashMap<String, IndexedEvent>,
    match_index: HashMap<u64, Vec<String>>,
    max_size: usize,
}

impl EventCache {
    pub fn new(max_size: usize) -> Self {
        EventCache {
            events: HashMap::new(),
            match_index: HashMap::new(),
            max_size,
        }
    }

    pub fn insert(&mut self, event: IndexedEvent) {
        if self.events.len() >= self.max_size {
            if let Some((id, _)) = self.events.iter().next() {
                let id = id.clone();
                self.remove(&id);
            }
        }

        let event_id = event.id.clone();
        let match_id = event.match_id;

        self.events.insert(event_id.clone(), event);
        self.match_index
            .entry(match_id)
            .or_insert_with(Vec::new)
            .push(event_id);
    }

    pub fn get(&self, event_id: &str) -> Option<IndexedEvent> {
        self.events.get(event_id).cloned()
    }

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

    pub fn remove(&mut self, event_id: &str) {
        if let Some(event) = self.events.remove(event_id) {
            if let Some(ids) = self.match_index.get_mut(&event.match_id) {
                ids.retain(|id| id != event_id);
            }
        }
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.match_index.clear();
    }

    pub fn size(&self) -> usize {
        self.events.len()
    }
}
