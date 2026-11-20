//! Oracle slash relay: listens for oracle_slash_signal events and executes slashing.
//!
//! When the escrow contract resolves a dispute as overturned, it emits an
//! `oracle_slash_signal` event. This relay subscribes to those events and
//! automatically calls `slash_oracle` on the oracle contract to penalize the
//! implicated oracle. The relay maintains idempotency by tracking which signals
//! have already been processed, preventing double-slashing.
//!
//! ## Authentication
//!
//! To prevent an unauthenticated caller from triggering arbitrary slashes, every
//! [`SlashSignal`] must carry an Ed25519 signature over its canonical payload
//! (see [`SlashSignal::signing_payload`]).  The relay validates the signature
//! against a configured trusted public key before processing.  Signals whose
//! signature is absent or invalid are rejected with
//! [`SlashRelayError::InvalidSignature`] before any state is modified.

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::metrics;

/// Errors returned by the slash relay when a signal is rejected.
#[derive(Debug, thiserror::Error)]
pub enum SlashRelayError {
    #[error("slash signal signature is missing or invalid")]
    InvalidSignature,
}

/// An oracle slash signal event from the escrow contract.
/// Tuple: (dispute_id, oracle_address, slash_amount)
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SlashSignal {
    pub dispute_id: u64,
    pub oracle_address: String, // Stellar address as string
    pub slash_amount: i128,
    /// Ed25519 signature over [`Self::signing_payload`], produced by the
    /// trusted signatory (e.g. the escrow contract event emitter key).
    /// Must be exactly 64 bytes.
    pub signature: Vec<u8>,
}

impl SlashSignal {
    /// Canonical byte payload that must be signed by the trusted authority.
    ///
    /// Layout (all big-endian):
    /// ```text
    /// "SLASH:"         (6 bytes, ASCII prefix — domain separator)
    /// dispute_id       (8 bytes, u64 big-endian)
    /// oracle_address   (UTF-8, variable length)
    /// ":"              (1 byte separator)
    /// slash_amount     (16 bytes, i128 big-endian)
    /// ```
    pub fn signing_payload(&self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(6 + 8 + self.oracle_address.len() + 1 + 16);
        payload.extend_from_slice(b"SLASH:");
        payload.extend_from_slice(&self.dispute_id.to_be_bytes());
        payload.extend_from_slice(self.oracle_address.as_bytes());
        payload.push(b':');
        payload.extend_from_slice(&self.slash_amount.to_be_bytes());
        payload
    }
}

/// Oracle slash relay state tracker for idempotency.
///
/// The relay maintains a set of processed signals (by dispute_id, oracle, amount)
/// to ensure that the same signal is never slashed twice, even if event
/// processing is restarted or events are replayed.
pub struct SlashRelay {
    /// Track processed signals by (dispute_id, oracle_address, slash_amount) tuple
    processed_signals: Arc<RwLock<HashSet<SlashSignal>>>,
    /// Trusted Ed25519 public key (32 bytes). Only signals signed by this key
    /// are accepted.
    trusted_pubkey: [u8; 32],
}

impl SlashRelay {
    /// Create a new slash relay instance.
    ///
    /// `trusted_pubkey` is the raw 32-byte Ed25519 public key whose
    /// corresponding private key must sign every [`SlashSignal`].
    pub fn new(trusted_pubkey: [u8; 32]) -> Self {
        Self {
            processed_signals: Arc::new(RwLock::new(HashSet::new())),
            trusted_pubkey,
        }
    }

    /// Verify the signature on a slash signal.
    ///
    /// Returns `Ok(())` if the signature is valid, or
    /// `Err(SlashRelayError::InvalidSignature)` otherwise.
    pub fn verify_signature(&self, signal: &SlashSignal) -> Result<(), SlashRelayError> {
        use ed25519_dalek::{Signature, VerifyingKey};

        let sig_bytes: [u8; 64] = signal
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| SlashRelayError::InvalidSignature)?;

        let verifying_key = VerifyingKey::from_bytes(&self.trusted_pubkey)
            .map_err(|_| SlashRelayError::InvalidSignature)?;

        let signature = Signature::from_bytes(&sig_bytes);
        let payload = signal.signing_payload();

        verifying_key
            .verify_strict(&payload, &signature)
            .map_err(|_| SlashRelayError::InvalidSignature)
    }

    /// Check if a signal has already been processed (idempotency check).
    /// Returns true if already processed, false otherwise.
    pub async fn is_already_processed(&self, signal: &SlashSignal) -> bool {
        self.processed_signals.read().await.contains(signal)
    }

    /// Authenticate and mark a signal as processed after successful slashing.
    ///
    /// Rejects signals with an invalid or missing signature with
    /// [`SlashRelayError::InvalidSignature`] before touching any state.
    /// Also increments the `oracle_slash_total` Prometheus counter so the
    /// Grafana "Slash Events" panel reflects this new event.
    pub async fn mark_as_processed(
        &self,
        signal: SlashSignal,
    ) -> Result<(), SlashRelayError> {
        // Authenticate before any state mutation.
        self.verify_signature(&signal)?;

        let inserted = self.processed_signals.write().await.insert(signal);
        if inserted {
            // Only count genuinely new signals; do not double-count replays.
            metrics::increment_slash_total();
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// Generate a test keypair and sign a signal payload.
    fn sign_signal(signing_key: &SigningKey, signal: &SlashSignal) -> Vec<u8> {
        let payload = signal.signing_payload();
        signing_key.sign(&payload).to_bytes().to_vec()
    }

    /// Build a SlashSignal with a valid signature for the given key.
    fn signed_signal(
        signing_key: &SigningKey,
        dispute_id: u64,
        oracle_address: &str,
        slash_amount: i128,
    ) -> SlashSignal {
        let mut signal = SlashSignal {
            dispute_id,
            oracle_address: oracle_address.to_string(),
            slash_amount,
            signature: vec![],
        };
        signal.signature = sign_signal(signing_key, &signal);
        signal
    }

    #[tokio::test]
    async fn test_unauthenticated_slash_is_rejected() {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let trusted_pubkey = signing_key.verifying_key().to_bytes();
        let relay = SlashRelay::new(trusted_pubkey);

        // A signal with a garbage/empty signature must be rejected.
        let signal = SlashSignal {
            dispute_id: 1,
            oracle_address: "GXXX".to_string(),
            slash_amount: 100,
            signature: vec![0u8; 64], // wrong signature
        };

        let result = relay.mark_as_processed(signal).await;
        assert!(
            matches!(result, Err(SlashRelayError::InvalidSignature)),
            "expected InvalidSignature, got {:?}",
            result
        );
        // No side effects — nothing should have been recorded.
        assert_eq!(relay.processed_count().await, 0);
    }

    #[tokio::test]
    async fn test_signal_with_missing_signature_is_rejected() {
        let signing_key = SigningKey::from_bytes(&[42u8; 32]);
        let trusted_pubkey = signing_key.verifying_key().to_bytes();
        let relay = SlashRelay::new(trusted_pubkey);

        // Empty signature (wrong length) — must be rejected before any state change.
        let signal = SlashSignal {
            dispute_id: 2,
            oracle_address: "GORAC".to_string(),
            slash_amount: 50,
            signature: vec![], // empty — not 64 bytes
        };

        let result = relay.mark_as_processed(signal).await;
        assert!(
            matches!(result, Err(SlashRelayError::InvalidSignature)),
            "expected InvalidSignature for empty signature"
        );
        assert_eq!(relay.processed_count().await, 0);
    }

    #[tokio::test]
    async fn test_slash_relay_idempotency() {
        let signing_key = SigningKey::from_bytes(&[1u8; 32]);
        let trusted_pubkey = signing_key.verifying_key().to_bytes();
        let relay = SlashRelay::new(trusted_pubkey);

        let signal = signed_signal(&signing_key, 1, "GXXX", 100);

        // Signal should not be marked as processed yet
        assert!(!relay.is_already_processed(&signal).await);

        // Mark as processed — should succeed with valid signature
        relay.mark_as_processed(signal.clone()).await.unwrap();

        // Now it should be marked as processed
        assert!(relay.is_already_processed(&signal).await);
        assert_eq!(relay.processed_count().await, 1);

        // Marking the same signal again doesn't increase the count (set semantics)
        relay.mark_as_processed(signal.clone()).await.unwrap();
        assert_eq!(relay.processed_count().await, 1);
    }

    #[tokio::test]
    async fn test_different_signals_tracked_separately() {
        let signing_key = SigningKey::from_bytes(&[2u8; 32]);
        let trusted_pubkey = signing_key.verifying_key().to_bytes();
        let relay = SlashRelay::new(trusted_pubkey);

        let signal1 = signed_signal(&signing_key, 1, "GORAC1", 100);
        let signal2 = signed_signal(&signing_key, 2, "GORAC2", 200);

        relay.mark_as_processed(signal1.clone()).await.unwrap();
        relay.mark_as_processed(signal2.clone()).await.unwrap();

        assert!(relay.is_already_processed(&signal1).await);
        assert!(relay.is_already_processed(&signal2).await);
        assert_eq!(relay.processed_count().await, 2);
    }

    #[tokio::test]
    async fn test_wrong_key_is_rejected() {
        let signing_key = SigningKey::from_bytes(&[10u8; 32]);
        let other_key = SigningKey::from_bytes(&[20u8; 32]);
        // Relay trusts `signing_key` but signal is signed by `other_key`.
        let trusted_pubkey = signing_key.verifying_key().to_bytes();
        let relay = SlashRelay::new(trusted_pubkey);

        let signal = signed_signal(&other_key, 5, "GWRONG", 999);

        let result = relay.mark_as_processed(signal).await;
        assert!(
            matches!(result, Err(SlashRelayError::InvalidSignature)),
            "signal signed by untrusted key must be rejected"
        );
        assert_eq!(relay.processed_count().await, 0);
    }
}
