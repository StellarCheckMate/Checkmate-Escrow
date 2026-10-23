use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use crate::state::AppState;

/// Current WebSocket protocol version advertised in the `hello` frame.
pub const PROTOCOL_VERSION: u32 = 1;

/// Server version advertised in the `hello` frame.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// First frame sent to every client on connection.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HelloFrame {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub protocol: u32,
    pub server_version: &'static str,
}

/// Build the `hello` frame sent on connection.
pub fn hello_frame() -> HelloFrame {
    HelloFrame {
        kind: "hello",
        protocol: PROTOCOL_VERSION,
        server_version: SERVER_VERSION,
    }
}

/// Message sent by a client to subscribe to a specific match's events.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Subscribe { match_id: String },
    Unsubscribe { match_id: String },
}

/// Event broadcast to subscribers of a match.
#[derive(Debug, Clone, Serialize)]
pub struct MatchEvent {
    pub match_id: String,
    pub event: String,
    pub payload: serde_json::Value,
}

/// Tracks which clients are subscribed to which match.
///
/// `match_id -> Set<client_id>` so that an event is only dispatched to the
/// clients that actually subscribed to that match.
pub type SubscriptionMap = Arc<RwLock<HashMap<String, HashSet<u64>>>>;

/// Create a fresh, empty subscription map.
pub fn new_subscription_map() -> SubscriptionMap {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Register `client_id` as a subscriber of `match_id`.
pub async fn subscribe(map: &SubscriptionMap, match_id: &str, client_id: u64) {
    let mut guard = map.write().await;
    guard
        .entry(match_id.to_string())
        .or_default()
        .insert(client_id);
}

/// Remove `client_id` from the subscribers of `match_id`.
pub async fn unsubscribe(map: &SubscriptionMap, match_id: &str, client_id: u64) {
    let mut guard = map.write().await;
    if let Some(clients) = guard.get_mut(match_id) {
        clients.remove(&client_id);
        if clients.is_empty() {
            guard.remove(match_id);
        }
    }
}

/// Remove `client_id` from every match it was subscribed to (on disconnect).
pub async fn unsubscribe_all(map: &SubscriptionMap, client_id: u64) {
    let mut guard = map.write().await;
    guard.retain(|_, clients| {
        clients.remove(&client_id);
        !clients.is_empty()
    });
}

/// Return the set of client ids subscribed to `match_id`.
pub async fn subscribers_for(map: &SubscriptionMap, match_id: &str) -> HashSet<u64> {
    let guard = map.read().await;
    guard.get(match_id).cloned().unwrap_or_default()
}

/// Dispatch an event only to the clients subscribed to its `match_id`.
///
/// Returns the number of clients the event was delivered to.
pub async fn dispatch_event(
    map: &SubscriptionMap,
    event: &MatchEvent,
    clients: &HashMap<u64, broadcast::Sender<MatchEvent>>,
) -> usize {
    let targets = subscribers_for(map, &event.match_id).await;
    let mut delivered = 0;
    for client_id in targets {
        if let Some(tx) = clients.get(&client_id) {
            // Ignore send errors: a client with no active receiver is simply skipped.
            if tx.send(event.clone()).is_ok() {
                delivered += 1;
            }
        }
    }
    delivered
}

/// Axum handler that upgrades an HTTP request to a WebSocket connection.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let client_id = state.next_client_id();
    let (mut sender, mut receiver) = socket.split();

    // Send the hello frame before any other traffic.
    match serde_json::to_string(&hello_frame()) {
        Ok(text) => {
            if sender.send(Message::Text(text)).await.is_err() {
                return;
            }
        }
        Err(e) => error!(?e, "failed to serialize hello frame"),
    }

    let (tx, mut rx) = broadcast::channel::<MatchEvent>(256);
    state.register_client(client_id, tx.clone()).await;
    info!(client_id, "websocket client connected");

    // Forward broadcast events for this client to the socket.
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            match serde_json::to_string(&event) {
                Ok(text) => {
                    if sender.send(Message::Text(text)).await.is_err() {
                        break;
                    }
                }
                Err(e) => error!(?e, "failed to serialize match event"),
            }
        }
    });

    // Handle inbound subscription messages from this client.
    let subscriptions = state.subscriptions.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Subscribe { match_id }) => {
                        subscribe(&subscriptions, &match_id, client_id).await;
                        debug!(client_id, %match_id, "client subscribed");
                    }
                    Ok(ClientMessage::Unsubscribe { match_id }) => {
                        unsubscribe(&subscriptions, &match_id, client_id).await;
                        debug!(client_id, %match_id, "client unsubscribed");
                    }
                    Err(e) => warn!(client_id, ?e, "invalid client message"),
                },
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    unsubscribe_all(&state.subscriptions, client_id).await;
    state.unregister_client(client_id).await;
    info!(client_id, "websocket client disconnected");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_frame_includes_protocol_and_server_version() {
        let frame = hello_frame();
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();

        assert_eq!(json["type"], "hello");
        assert_eq!(json["protocol"], PROTOCOL_VERSION);
        assert_eq!(json["server_version"], SERVER_VERSION);
        assert!(!SERVER_VERSION.is_empty());
    }

    #[tokio::test]
    async fn dispatch_only_reaches_subscribed_clients() {
        let map = new_subscription_map();

        // Two clients subscribed to different matches.
        subscribe(&map, "match-a", 1).await;
        subscribe(&map, "match-b", 2).await;

        let (tx1, mut rx1) = broadcast::channel::<MatchEvent>(16);
        let (tx2, mut rx2) = broadcast::channel::<MatchEvent>(16);
        let mut clients = HashMap::new();
        clients.insert(1u64, tx1);
        clients.insert(2u64, tx2);

        let event = MatchEvent {
            match_id: "match-a".to_string(),
            event: "updated".to_string(),
            payload: serde_json::json!({ "score": 1 }),
        };

        let delivered = dispatch_event(&map, &event, &clients).await;
        assert_eq!(delivered, 1, "event should reach exactly one subscriber");

        // Client 1 (subscribed to match-a) receives the event.
        let received = rx1.try_recv().expect("client 1 should receive event");
        assert_eq!(received.match_id, "match-a");

        // Client 2 (subscribed to match-b) must NOT receive it.
        assert!(
            rx2.try_recv().is_err(),
            "client 2 must not receive events for match-a"
        );
    }

    #[tokio::test]
    async fn unsubscribe_stops_delivery() {
        let map = new_subscription_map();
        subscribe(&map, "match-a", 1).await;
        unsubscribe(&map, "match-a", 1).await;

        let (tx, mut rx) = broadcast::channel::<MatchEvent>(16);
        let mut clients = HashMap::new();
        clients.insert(1u64, tx);

        let event = MatchEvent {
            match_id: "match-a".to_string(),
            event: "updated".to_string(),
            payload: serde_json::json!({}),
        };

        let delivered = dispatch_event(&map, &event, &clients).await;
        assert_eq!(delivered, 0);
        assert!(rx.try_recv().is_err());
    }
}
