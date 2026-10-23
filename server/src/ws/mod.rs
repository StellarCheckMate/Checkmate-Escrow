use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const PROTOCOL_VERSION: u32 = 1;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, warn};

use crate::state::AppState;

/// A single event emitted by the indexer / oracle pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsEvent {
    pub match_id: u64,
    pub payload: serde_json::Value,
}

/// Tracks which clients are subscribed to which match.
///
/// `match_id -> Set<client_id>` so that an event is only dispatched to the
/// clients that actually subscribed to that match.
#[derive(Debug, Default)]
pub struct SubscriptionMap {
    inner: HashMap<u64, HashSet<u64>>,
}

impl SubscriptionMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe `client_id` to `match_id`.
    pub fn subscribe(&mut self, match_id: u64, client_id: u64) {
        self.inner.entry(match_id).or_default().insert(client_id);
    }

    /// Remove `client_id` from every match it was subscribed to.
    pub fn unsubscribe_all(&mut self, client_id: u64) {
        self.inner.retain(|_, clients| {
            clients.remove(&client_id);
            !clients.is_empty()
        });
    }

    /// Return the set of clients subscribed to `match_id`.
    pub fn subscribers(&self, match_id: u64) -> Option<&HashSet<u64>> {
        self.inner.get(&match_id)
    }
}

/// Shared subscription state used by the broadcast loop.
pub type SharedSubscriptions = Arc<RwLock<SubscriptionMap>>;

/// Broadcast `event` only to clients subscribed to `event.match_id`.
pub async fn broadcast_event(
    subscriptions: &SharedSubscriptions,
    clients: &HashMap<u64, broadcast::Sender<WsEvent>>,
    event: WsEvent,
) {
    let subs = subscriptions.read().await;
    let Some(subscribers) = subs.subscribers(event.match_id) else {
        debug!(match_id = event.match_id, "no subscribers for event");
        return;
    };

    for client_id in subscribers {
        if let Some(tx) = clients.get(client_id) {
            if tx.send(event.clone()).is_err() {
                debug!(client_id, "client channel closed, skipping");
            }
        }
    }
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let client_id = state.next_client_id();
    let (mut sender, mut receiver) = socket.split();

    let hello = serde_json::json!({
        "type": "hello",
        "protocol": PROTOCOL_VERSION,
        "server_version": env!("CARGO_PKG_VERSION"),
    });
    let _ = sender.send(Message::Text(hello.to_string())).await;

    let (tx, mut rx) = broadcast::channel::<WsEvent>(256);
    state.register_client(client_id, tx).await;

    let subscriptions = state.subscriptions.clone();

    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let msg = match serde_json::to_string(&event) {
                Ok(m) => m,
                Err(e) => {
                    warn!(error = %e, "failed to serialize event");
                    continue;
                }
            };
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let subs = subscriptions.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Subscribe { match_id }) => {
                        subs.write().await.subscribe(match_id, client_id);
                    }
                    Ok(ClientMessage::Unsubscribe { match_id }) => {
                        let mut guard = subs.write().await;
                        if let Some(clients) = guard.inner.get_mut(&match_id) {
                            clients.remove(&client_id);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "invalid client message");
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    subscriptions.write().await.unsubscribe_all(client_id);
    state.unregister_client(client_id).await;
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Subscribe { match_id: u64 },
    Unsubscribe { match_id: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_frame_contains_version_fields() {
        let hello = serde_json::json!({
            "type": "hello",
            "protocol": PROTOCOL_VERSION,
            "server_version": env!("CARGO_PKG_VERSION"),
        });
        assert_eq!(hello["type"], "hello");
        assert_eq!(hello["protocol"], 1);
        assert!(hello["server_version"].is_string());
    }

    #[tokio::test]
    async fn broadcast_only_reaches_subscribed_clients() {
        let subscriptions: SharedSubscriptions = Arc::new(RwLock::new(SubscriptionMap::new()));

        // Two clients subscribed to different matches.
        subscriptions.write().await.subscribe(1, 100);
        subscriptions.write().await.subscribe(2, 200);

        let (tx1, mut rx1) = broadcast::channel::<WsEvent>(16);
        let (tx2, mut rx2) = broadcast::channel::<WsEvent>(16);

        let mut clients = HashMap::new();
        clients.insert(100u64, tx1);
        clients.insert(200u64, tx2);

        let event = WsEvent {
            match_id: 1,
            payload: serde_json::json!({ "score": 42 }),
        };
        broadcast_event(&subscriptions, &clients, event).await;

        // Client 100 (subscribed to match 1) receives the event.
        let received = rx1.try_recv().expect("client 100 should receive event");
        assert_eq!(received.match_id, 1);

        // Client 200 (subscribed to match 2) must NOT receive it.
        assert!(rx2.try_recv().is_err(), "client 200 should not receive match 1 event");
    }

    #[tokio::test]
    async fn unsubscribe_all_removes_client() {
        let mut map = SubscriptionMap::new();
        map.subscribe(1, 100);
        map.subscribe(2, 100);
        map.subscribe(1, 200);

        map.unsubscribe_all(100);

        assert_eq!(map.subscribers(1).map(|s| s.len()), Some(1));
        assert!(map.subscribers(2).is_none());
    }
}
