//! Tests for issue #1925: `get_blacklist_paginated` respects pagination bounds.
//!
//! `get_blacklist` returns the entire blacklist with no limit, which on a
//! large blacklist can exhaust the ledger instruction budget (mirroring the
//! allowlist/matches pagination gap). The `get_blacklist_paginated` variant
//! mirrors the allowlist pagination signature and must respect `offset`/
//! `limit` bounds.
use super::*;

/// Seed `count` distinct blacklisted tokens. Insertion order is preserved by
/// the blacklist list, so the on-chain order matches the call order.
fn seed_blacklist(env: &Env, client: &EscrowContractClient, count: u32) {
    for i in 0..count {
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(env))
            .address();
        client.add_token_to_blacklist(&token, &String::from_str(env, &format!("reason {}", i)));
    }
}

#[test]
fn test_get_blacklist_paginated_respects_offset_and_limit() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    seed_blacklist(&env, &client, 7);
    let full = client.get_blacklist();
    assert_eq!(full.len(), 7);

    // First page of 3 matches the first three entries of the full list.
    let page0 = client.get_blacklist_paginated(&0, &3);
    assert_eq!(page0.len(), 3);
    for i in 0..3u32 {
        assert_eq!(page0.get(i).unwrap(), full.get(i).unwrap());
    }

    // offset 3, limit 5 -> only 4 remain (partial page), bounds respected.
    let page1 = client.get_blacklist_paginated(&3, &5);
    assert_eq!(page1.len(), 4);
    for i in 0..4u32 {
        assert_eq!(page1.get(i).unwrap(), full.get(3 + i).unwrap());
    }

    // Offset exactly at the end -> empty page.
    assert_eq!(client.get_blacklist_paginated(&7, &5).len(), 0);

    // Offset past the end -> empty page (start is clamped to total).
    assert_eq!(client.get_blacklist_paginated(&100, &5).len(), 0);

    // Zero limit -> empty page (nothing requested).
    assert_eq!(client.get_blacklist_paginated(&0, &0).len(), 0);

    // Partial last page: offset 5, limit 10 -> last 2.
    let page_last = client.get_blacklist_paginated(&5, &10);
    assert_eq!(page_last.len(), 2);
    assert_eq!(page_last.get(0).unwrap(), full.get(5).unwrap());
    assert_eq!(page_last.get(1).unwrap(), full.get(6).unwrap());
}

#[test]
fn test_get_blacklist_paginated_empty_by_default() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // No tokens blacklisted -> every page is empty regardless of bounds.
    assert_eq!(client.get_blacklist_paginated(&0, &10).len(), 0);
    assert_eq!(client.get_blacklist_paginated(&5, &10).len(), 0);
    assert_eq!(client.get_blacklist_paginated(&0, &0).len(), 0);
}

#[test]
fn test_get_blacklist_paginated_pages_reconstruct_full_list() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    seed_blacklist(&env, &client, 5);
    let full = client.get_blacklist();
    assert_eq!(full.len(), 5);

    // Page through the whole list with a small limit and reconstruct it; the
    // concatenation must equal the unbounded getter's result (same order, no
    // duplicates, no gaps) — i.e. pagination bounds are self-consistent.
    let mut rebuilt = soroban_sdk::vec![&env];
    let mut offset = 0u32;
    loop {
        let page = client.get_blacklist_paginated(&offset, &2);
        if page.is_empty() {
            break;
        }
        for token in page.iter() {
            rebuilt.push_back(token);
        }
        offset = offset.saturating_add(page.len());
    }

    assert_eq!(rebuilt.len(), 5);
    for i in 0..5u32 {
        assert_eq!(rebuilt.get(i).unwrap(), full.get(i).unwrap());
    }
}

#[test]
fn test_get_blacklist_paginated_signature_matches_allowlist() {
    // The blacklist pagination signature mirrors the allowlist one
    // (`offset: u32, limit: u32 -> Vec<Address>`). A round trip with the same
    // offset/limit against a same-sized list must return pages of equal length.
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Build a parallel allowlist and blacklist of the same size.
    for i in 0..6u32 {
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        client.add_token_to_blacklist(&token, &String::from_str(&env, &format!("b {}", i)));
        let allowed = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        client.add_allowed_token(&allowed);
    }

    let bl_page = client.get_blacklist_paginated(&2, &3);
    let al_page = client.get_allowed_tokens_paginated(&2, &3);
    assert_eq!(bl_page.len(), al_page.len());
    assert_eq!(bl_page.len(), 3);
}
