//! Tests for #1957 — `get_blacklist_paginated` offset/limit bounds.
//!
//! Covers:
//! - A normal paginated slice returns exactly the expected items.
//! - An out-of-range `offset` (beyond the list length) returns an empty vec.
//! - A `limit` larger than the remaining items is clamped to the tail.
//! - A `limit` that exceeds `MAX_PAGE_LIMIT` is clamped to `MAX_PAGE_LIMIT`.

#![cfg(test)]
extern crate std;

use super::*;

fn reason(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

/// Build a blacklist of `n` freshly-generated token addresses and return them
/// in insertion order.
fn seed_blacklist(
    env: &Env,
    client: &EscrowContractClient,
    n: usize,
) -> std::vec::Vec<Address> {
    let mut tokens = std::vec::Vec::new();
    for i in 0..n {
        let addr = env
            .register_stellar_asset_contract_v2(Address::generate(env))
            .address();
        client.add_token_to_blacklist(&addr, &reason(env, &format!("reason_{}", i)));
        tokens.push(addr);
    }
    tokens
}

// ── Slice correctness ─────────────────────────────────────────────────────────

/// A mid-list page returns exactly the items at [offset, offset+limit).
#[test]
fn test_blacklist_paginated_returns_expected_slice() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let tokens = seed_blacklist(&env, &client, 5);

    // Request items at index 1 and 2 (limit=2, offset=1).
    let page = client.get_blacklist_paginated(&1, &2);
    assert_eq!(page.len(), 2, "expected exactly 2 items in the slice");
    assert_eq!(page.get(0).unwrap(), tokens[1]);
    assert_eq!(page.get(1).unwrap(), tokens[2]);
}

/// First page (offset=0) returns the leading items.
#[test]
fn test_blacklist_paginated_first_page() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let tokens = seed_blacklist(&env, &client, 4);

    let page = client.get_blacklist_paginated(&0, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), tokens[0]);
    assert_eq!(page.get(1).unwrap(), tokens[1]);
}

/// Last page returns fewer items than `limit` when the tail is shorter.
#[test]
fn test_blacklist_paginated_last_page_is_shorter() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let tokens = seed_blacklist(&env, &client, 5);

    // 5 items, offset=4, limit=3 → only 1 item left.
    let page = client.get_blacklist_paginated(&4, &3);
    assert_eq!(page.len(), 1, "tail page must be clamped to remaining items");
    assert_eq!(page.get(0).unwrap(), tokens[4]);
}

// ── Out-of-range offset ───────────────────────────────────────────────────────

/// An offset equal to the list length returns an empty vec (no panic).
#[test]
fn test_blacklist_paginated_offset_at_exact_end_returns_empty() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    seed_blacklist(&env, &client, 3);

    let page = client.get_blacklist_paginated(&3, &5);
    assert_eq!(page.len(), 0, "offset == len must return empty");
}

/// An offset beyond the list length also returns an empty vec.
#[test]
fn test_blacklist_paginated_offset_beyond_end_returns_empty() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    seed_blacklist(&env, &client, 3);

    let page = client.get_blacklist_paginated(&100, &10);
    assert_eq!(page.len(), 0, "offset beyond length must return empty");
}

/// An empty blacklist with any offset/limit returns an empty vec.
#[test]
fn test_blacklist_paginated_empty_list_returns_empty() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let page = client.get_blacklist_paginated(&0, &10);
    assert_eq!(page.len(), 0, "empty blacklist must yield empty page");
}

// ── Limit clamping ────────────────────────────────────────────────────────────

/// A `limit` of zero always returns an empty vec.
#[test]
fn test_blacklist_paginated_zero_limit_returns_empty() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    seed_blacklist(&env, &client, 3);

    let page = client.get_blacklist_paginated(&0, &0);
    assert_eq!(page.len(), 0, "limit=0 must return empty");
}

/// A `limit` larger than `MAX_PAGE_LIMIT` is clamped to `MAX_PAGE_LIMIT`.
#[test]
fn test_blacklist_paginated_limit_clamped_at_max_page_limit() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Seed more than MAX_PAGE_LIMIT items.
    let total = MAX_PAGE_LIMIT + 10;
    seed_blacklist(&env, &client, total);

    // Requesting u32::MAX items must not exceed MAX_PAGE_LIMIT.
    let page = client.get_blacklist_paginated(&0, &u32::MAX);
    assert_eq!(
        page.len() as usize,
        MAX_PAGE_LIMIT,
        "limit must be clamped to MAX_PAGE_LIMIT"
    );
}

/// Exactly `MAX_PAGE_LIMIT` as the `limit` is allowed through without clamping.
#[test]
fn test_blacklist_paginated_limit_exactly_at_max_page_limit() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let total = MAX_PAGE_LIMIT + 5;
    let tokens = seed_blacklist(&env, &client, total);

    let page = client.get_blacklist_paginated(&0, &(MAX_PAGE_LIMIT as u32));
    assert_eq!(page.len() as usize, MAX_PAGE_LIMIT);
    // First and last item in the page match the seeded order.
    assert_eq!(page.get(0).unwrap(), tokens[0]);
    assert_eq!(page.get((MAX_PAGE_LIMIT - 1) as u32).unwrap(), tokens[MAX_PAGE_LIMIT - 1]);
}
