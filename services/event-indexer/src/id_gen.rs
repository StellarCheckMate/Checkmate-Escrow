//! Deterministic event ID generation.
//!
//! Event IDs are derived from (ledger, txn_hash, event_index_in_txn) using SHA-256.
//! This ensures idempotent re-ingestion: the same event always produces the same ID.

use sha2::{Digest, Sha256};

/// Compute a deterministic event ID from (ledger, txn_hash, event_index).
///
/// # Arguments
///
/// * `ledger` – ledger sequence number (u32)
/// * `txn_hash` – transaction hash or identifier (string)
/// * `event_index` – position of this event within the transaction (u16)
///
/// # Returns
///
/// A 64-character hex string (SHA-256 output).
///
/// # Example
///
/// ```ignore
/// let id = compute_event_id(100, "tx123", 0);
/// assert_eq!(id.len(), 64); // 32 bytes × 2 (hex)
/// ```
pub fn compute_event_id(ledger: u32, txn_hash: &str, event_index: u16) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ledger.to_be_bytes());
    hasher.update(txn_hash.as_bytes());
    hasher.update(event_index.to_be_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_inputs_same_output() {
        let id1 = compute_event_id(100, "txhash123", 0);
        let id2 = compute_event_id(100, "txhash123", 0);
        assert_eq!(id1, id2, "same inputs must produce identical IDs");
    }

    #[test]
    fn different_ledger_different_id() {
        let id1 = compute_event_id(100, "txhash123", 0);
        let id2 = compute_event_id(101, "txhash123", 0);
        assert_ne!(id1, id2, "different ledgers must produce different IDs");
    }

    #[test]
    fn different_txn_hash_different_id() {
        let id1 = compute_event_id(100, "txhash123", 0);
        let id2 = compute_event_id(100, "txhash456", 0);
        assert_ne!(id1, id2, "different txn_hashes must produce different IDs");
    }

    #[test]
    fn different_event_index_different_id() {
        let id1 = compute_event_id(100, "txhash123", 0);
        let id2 = compute_event_id(100, "txhash123", 1);
        assert_ne!(id1, id2, "different event indices must produce different IDs");
    }

    #[test]
    fn id_is_valid_hex_64_chars() {
        let id = compute_event_id(100, "txhash123", 0);
        assert_eq!(id.len(), 64, "SHA-256 hex must be 64 characters");
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()), "ID must be valid hex");
    }
}
