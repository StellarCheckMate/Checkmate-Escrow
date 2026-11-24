//! Integration tests for slash_relay deduplication and signal handling.
//!
//! Tests verify:
//! - Repeated slash signals for the same match are deduplicated
//! - Distinct slashes for different matches are both relayed and recorded

use ed25519_dalek::{Signer, SigningKey};
use oracle_service::slash_relay::{SlashRelay, SlashSignal};

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
    let payload = signal.signing_payload();
    signal.signature = signing_key.sign(&payload).to_bytes().to_vec();
    signal
}

/// A repeated slash signal for the same match must be deduplicated by slash_relay.
#[tokio::test]
async fn test_slash_relay_deduplicates_repeated_slash_signals_for_same_match() {
    let signing_key = SigningKey::from_bytes(&[88u8; 32]);
    let trusted_pubkey = signing_key.verifying_key().to_bytes();
    let relay = SlashRelay::new(trusted_pubkey);

    let match_id = 9001u64;
    let signal = signed_signal(&signing_key, match_id, "GORACLETEST", 2_500_000);

    // Initial signal processing
    relay.mark_as_processed(signal.clone()).await.expect("initial relay");
    assert!(relay.is_already_processed(&signal).await);
    assert_eq!(relay.processed_count().await, 1);

    // Repeated signal arrival for the same match
    relay.mark_as_processed(signal.clone()).await.expect("duplicate relay");
    assert_eq!(
        relay.processed_count().await,
        1,
        "processed count must not increment for repeated slash signal"
    );

    // Third arrival
    relay.mark_as_processed(signal.clone()).await.expect("triplicate relay");
    assert_eq!(relay.processed_count().await, 1);
}

/// Distinct slash signals for different matches must both be relayed.
#[tokio::test]
async fn test_slash_relay_relays_distinct_slashes() {
    let signing_key = SigningKey::from_bytes(&[99u8; 32]);
    let trusted_pubkey = signing_key.verifying_key().to_bytes();
    let relay = SlashRelay::new(trusted_pubkey);

    let signal1 = signed_signal(&signing_key, 1001, "GORACLE1", 1_000_000);
    let signal2 = signed_signal(&signing_key, 1002, "GORACLE2", 1_500_000);

    relay.mark_as_processed(signal1.clone()).await.expect("signal 1 relay");
    relay.mark_as_processed(signal2.clone()).await.expect("signal 2 relay");

    assert!(relay.is_already_processed(&signal1).await);
    assert!(relay.is_already_processed(&signal2).await);
    assert_eq!(
        relay.processed_count().await,
        2,
        "distinct slashes must both be relayed"
    );
}
