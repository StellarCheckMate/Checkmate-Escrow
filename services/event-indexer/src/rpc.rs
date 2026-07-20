//! Soroban RPC client and event-poller loop.
//!
//! ## Leader-election integration
//! `event_poller` accepts a `LeaderElection` handle.  At the start of every
//! poll cycle it calls `leader.try_acquire()`.  Only the instance that wins
//! the distributed lease proceeds to fetch and persist events.  Followers skip
//! ingestion but stay alive so they can serve read traffic immediately on
//! failover.
//!
//! ## Idempotency
//! Because `db::Database::insert_event` uses `ON CONFLICT DO NOTHING`, it is
//! safe for a newly-elected leader to re-poll the last few ledgers without
//! creating duplicates.  The idempotency guarantee in the DB is the correctness
//! backstop; leader election is purely a performance optimisation that avoids
//! redundant RPC calls.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, info_span, warn, Instrument};
use uuid::Uuid;

use crate::cache::EventCache;
use crate::db::Database;
use crate::leader::LeaderElection;
use crate::models::IndexedEvent;
use chrono::Utc;

// ── RPC client ────────────────────────────────────────────────────────────────

pub struct SorobanRpcClient {
    client: Client,
    rpc_url: String,
}

impl SorobanRpcClient {
    pub fn new(rpc_url: &str) -> Result<Self> {
        Ok(SorobanRpcClient {
            client: Client::new(),
            rpc_url: rpc_url.to_string(),
        })
    }

    async fn make_request(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4().to_string(),
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "RPC request failed with status: {}",
                response.status()
            ));
        }

        let json = response.json::<Value>().await?;

        if let Some(error) = json.get("error") {
            return Err(anyhow!("RPC error: {}", error));
        }

        Ok(json.get("result").cloned().unwrap_or(Value::Null))
    }

    pub async fn get_events(
        &self,
        contract_id: &str,
        start_ledger: Option<u32>,
        limit: Option<u32>,
    ) -> Result<Vec<Value>> {
        let start = start_ledger.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let filters = vec![json!({
            "type": "contract",
            "contractIds": [contract_id]
        })];

        let params = json!({
            "startLedger": start,
            "limit": limit,
            "filters": filters,
        });

        let result = self.make_request("getEvents", params).await?;

        if let Some(events) = result.get("events").and_then(|e| e.as_array()) {
            Ok(events.clone())
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn get_ledger(&self) -> Result<u32> {
        let result = self.make_request("getLedger", json!({})).await?;

        if let Some(sequence) = result.get("sequence").and_then(|s| s.as_u64()) {
            Ok(sequence as u32)
        } else {
            Err(anyhow!("Failed to get ledger sequence"))
        }
    }
}

// ── Poller loop ───────────────────────────────────────────────────────────────

/// Main ingestion loop.
///
/// - `leader` – the distributed election handle for this instance.
/// - Only the current leader calls `poll_events`; followers sleep and retry.
pub async fn event_poller(
    rpc: Arc<SorobanRpcClient>,
    db: Arc<Database>,
    cache: Arc<RwLock<EventCache>>,
    mut leader: LeaderElection,
    contract_id: &str,
    poll_interval_secs: u64,
) -> Result<()> {
    // Bootstrap: read last persisted ledger without holding the leader lease.
    let mut last_ledger = db.get_latest_ledger().await?;

    loop {
        // ── Leader check ──────────────────────────────────────────────────
        let is_leader = leader.try_acquire().await;

        if !is_leader {
            debug!("Not the leader – skipping poll");
            sleep(Duration::from_secs(poll_interval_secs)).await;
            continue;
        }

        // ── Poll ──────────────────────────────────────────────────────────
        let span = info_span!("poll_iteration", contract_id, last_ledger = ?last_ledger);
        match poll_events(&rpc, &db, &cache, contract_id, last_ledger)
            .instrument(span)
            .await
        {
            Ok(new_ledger) => {
                if let Some(ledger) = new_ledger {
                    last_ledger = Some(ledger);
                    info!("Events polled up to ledger: {}", ledger);
                }
            }
            Err(e) => {
                error!("Error polling events: {}", e);
            }
        }

        sleep(Duration::from_secs(poll_interval_secs)).await;
    }
}

// ── Internal polling logic ────────────────────────────────────────────────────

async fn poll_events(
    rpc: &Arc<SorobanRpcClient>,
    db: &Arc<Database>,
    cache: &Arc<RwLock<EventCache>>,
    contract_id: &str,
    start_ledger: Option<u32>,
) -> Result<Option<u32>> {
    let events = rpc
        .get_events(contract_id, start_ledger, Some(100))
        .await?;

    if events.is_empty() {
        warn!("RPC returned no new events");
        return Ok(None);
    }

    let mut max_ledger = None;

    for event_value in events {
        if let Ok(indexed_event) = parse_event(&event_value) {
            debug!("Parsed event: {:?}", indexed_event.event_type);

            // Idempotent: `ON CONFLICT DO NOTHING` in the DB means this is
            // safe to call multiple times for the same event.
            db.insert_event(&indexed_event).await?;

            let mut cache_lock = cache.write().await;
            cache_lock.insert(indexed_event.clone());
            drop(cache_lock);

            max_ledger = Some(indexed_event.ledger_sequence);
        }
    }

    Ok(max_ledger)
}

// ── Event parsing ─────────────────────────────────────────────────────────────

fn parse_event(event_value: &Value) -> Result<IndexedEvent> {
    let ledger_sequence = event_value
        .get("ledger")
        .and_then(|l| l.as_u64())
        .ok_or(anyhow!("Missing ledger"))? as u32;

    let txn_meta = event_value
        .get("txnMeta")
        .and_then(|m| m.as_str())
        .unwrap_or("");

    let event_data = event_value
        .get("event")
        .ok_or(anyhow!("Missing event data"))?;

    let topics = event_data
        .get("topics")
        .and_then(|t| t.as_array())
        .ok_or(anyhow!("Missing topics"))?;

    if topics.len() < 2 {
        return Err(anyhow!("Invalid topics length"));
    }

    let event_namespace = topics.first().and_then(|t| t.as_str()).unwrap_or("unknown");
    let event_name = topics.get(1).and_then(|t| t.as_str()).unwrap_or("unknown");
    let event_type = format!("{}:{}", event_namespace, event_name);

    let empty = vec![];
    let data = event_data
        .get("data")
        .and_then(|d| d.as_array())
        .unwrap_or(&empty);

    let (match_id, player1, player2, status, winner, stake_amount, token, game_id, platform) =
        parse_event_data(&event_type, data);

    Ok(IndexedEvent {
        id: Uuid::new_v4().to_string(),
        ledger_sequence,
        match_id,
        event_type,
        player1,
        player2,
        status,
        winner,
        stake_amount,
        token,
        game_id,
        platform,
        timestamp: Utc::now(),
        txn_hash: Some(txn_meta.to_string()),
    })
}

#[allow(clippy::type_complexity)]
fn parse_event_data(
    event_type: &str,
    data: &[Value],
) -> (
    u64,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let match_id = data
        .first()
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let (status, winner) = if event_type.contains("created") {
        (Some("pending".to_string()), None)
    } else if event_type.contains("activated") {
        (Some("active".to_string()), None)
    } else if event_type.contains("completed") {
        (
            Some("completed".to_string()),
            data.get(1).and_then(|d| d.as_str()).map(|s| s.to_string()),
        )
    } else if event_type.contains("cancelled") {
        (Some("cancelled".to_string()), None)
    } else if event_type.contains("expired") {
        (Some("expired".to_string()), None)
    } else {
        (None, None)
    };

    (
        match_id,
        data.get(1).and_then(|d| d.as_str()).map(|s| s.to_string()),
        data.get(2).and_then(|d| d.as_str()).map(|s| s.to_string()),
        status,
        winner,
        data.get(3).and_then(|d| d.as_str()).map(|s| s.to_string()),
        data.get(4).and_then(|d| d.as_str()).map(|s| s.to_string()),
        data.get(5).and_then(|d| d.as_str()).map(|s| s.to_string()),
        data.get(6).and_then(|d| d.as_str()).map(|s| s.to_string()),
    )
}
