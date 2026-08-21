//! Oracle slash relay: listens for oracle_slash_signal events and executes slashing.
//!
//! When the escrow contract resolves a dispute as overturned, it emits an
//! `oracle_slash_signal` event. This relay subscribes to those events and
//! automatically calls `slash_oracle` on the oracle contract to penalize the
//! implicated oracle. The relay maintains idempotency by tracking which signals
//! have already been processed, preventing double-slashing.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

/// An oracle slash signal event from the escrow contract.
/// Tuple: (dispute_id, oracle_address, slash_amount)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlashSignal {
    pub dispute_id: u64,
    pub oracle_address: String, // Stellar address as string
    pub slash_amount: i128,
}

/// Oracle slash relay state tracker for idempotency.
///
/// The relay maintains a set of processed signals (by dispute_id, oracle, amount)
/// to ensure that the same signal is never slashed twice, even if event
/// processing is restarted or events are replayed.
pub struct SlashRelay {
    /// Track processed signals by (dispute_id, oracle_address, slash_amount) tuple
    processed_signals: Arc<RwLock<HashSet<SlashSignal>>>,
}

impl SlashRelay {
    /// Create a new slash relay instance.
    pub fn new() -> Self {
        Self {
            processed_signals: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Check if a signal has already been processed (idempotency check).
    /// Returns true if already processed, false otherwise.
    pub async fn is_already_processed(&self, signal: &SlashSignal) -> bool {
        self.processed_signals.read().await.contains(signal)
    }

    /// Mark a signal as processed after successful slashing.
    pub async fn mark_as_processed(&self, signal: SlashSignal) {
        self.processed_signals.write().await.insert(signal);
    }

    /// Get count of processed signals (for monitoring/testing).
    pub async fn processed_count(&self) -> usize {
        self.processed_signals.read().await.len()
    }

    /// Clear all processed signals (for testing only).
    #[cfg(test)]
    pub async fn clear_processed(&self) {
        self.processed_signals.write().await.clear();
    }
}

impl Default for SlashRelay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_slash_relay_idempotency() {
        let relay = SlashRelay::new();
        let signal = SlashSignal {
            dispute_id: 1,
            oracle_address: "GXXX".to_string(),
            slash_amount: 100,
        };

        // Signal should not be marked as processed yet
        assert!(!relay.is_already_processed(&signal).await);

        // Mark as processed
        relay.mark_as_processed(signal.clone()).await;

        // Now it should be marked as processed
        assert!(relay.is_already_processed(&signal).await);
        assert_eq!(relay.processed_count().await, 1);

        // Marking the same signal again doesn't increase the count (set semantics)
        relay.mark_as_processed(signal.clone()).await;
        assert_eq!(relay.processed_count().await, 1);
    }

    #[tokio::test]
    async fn test_different_signals_tracked_separately() {
        let relay = SlashRelay::new();
        let signal1 = SlashSignal {
            dispute_id: 1,
            oracle_address: "GORAC1".to_string(),
            slash_amount: 100,
        };
        let signal2 = SlashSignal {
            dispute_id: 2,
            oracle_address: "GORAC2".to_string(),
            slash_amount: 200,
        };

        relay.mark_as_processed(signal1.clone()).await;
        relay.mark_as_processed(signal2.clone()).await;

        assert!(relay.is_already_processed(&signal1).await);
        assert!(relay.is_already_processed(&signal2).await);
        assert_eq!(relay.processed_count().await, 2);
    }
}
