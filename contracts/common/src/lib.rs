//! Shared validation helpers for the Checkmate-Escrow contracts.
//!
//! This crate hosts validation logic that is reused across contract
//! entrypoints (payments, monitor records) and off-chain UI links, so the
//! rules live in exactly one place.

pub mod tx_hash;

pub use tx_hash::{validate_tx_hash, TxHashError, TX_HASH_LEN};
