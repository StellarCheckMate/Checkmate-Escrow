//! Tests for issue #1921 — `get_frozen_players` deprecation.
//!
//! Verifies that the deprecated `get_frozen_players` delegates to the same
//! paginated accessor as `admin_list_frozen_players`, so both functions return
//! identical results for the same `offset` / `limit` arguments.
#![cfg(test)]
extern crate std;

use super::*;

// ── #1921: get_frozen_players delegates to admin_list_frozen_players ──────────

/// `get_frozen_players` and `admin_list_frozen_players` must return the same
/// frozen-player list for identical pagination parameters.
#[test]
fn test_get_frozen_players_delegates_to_paginated_accessor() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Freeze three distinct players.
    let frozen: std::vec::Vec<Address> = (0..3).map(|_| Address::generate(&env)).collect();
    for player in &frozen {
        client.admin_freeze_player(player, &String::from_str(&env, "test freeze"));
    }

    // Both accessors should return the same first page.
    let deprecated = client.get_frozen_players(&0, &10);
    let paginated = client.admin_list_frozen_players(&admin, &0, &10).unwrap();

    assert_eq!(
        deprecated.len(),
        paginated.len(),
        "deprecated get_frozen_players must return the same length as admin_list_frozen_players"
    );
    for i in 0..deprecated.len() {
        assert_eq!(
            deprecated.get(i).unwrap(),
            paginated.get(i).unwrap(),
            "entries at index {} must match",
            i
        );
    }
}

/// Pagination parameters must be honoured identically by both accessors.
#[test]
fn test_get_frozen_players_pagination_matches_admin_list() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Freeze five players.
    for _ in 0..5 {
        let p = Address::generate(&env);
        client.admin_freeze_player(&p, &String::from_str(&env, "bulk freeze"));
    }

    // Page 1: skip the first 2, take up to 3.
    let dep_page = client.get_frozen_players(&2, &3);
    let pag_page = client.admin_list_frozen_players(&admin, &2, &3).unwrap();

    assert_eq!(dep_page.len(), pag_page.len());
    for i in 0..dep_page.len() {
        assert_eq!(dep_page.get(i).unwrap(), pag_page.get(i).unwrap());
    }
}

/// An offset beyond the list end must return an empty vector from both accessors.
#[test]
fn test_get_frozen_players_offset_beyond_end_returns_empty() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let p = Address::generate(&env);
    client.admin_freeze_player(&p, &String::from_str(&env, "single"));

    let result = client.get_frozen_players(&100, &10);
    assert_eq!(result.len(), 0, "offset beyond list end must return empty vec");
}

/// The limit must be silently clamped to 100 to prevent unbounded reads.
#[test]
fn test_get_frozen_players_limit_clamped_to_100() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Freeze 5 players — fewer than the limit cap so the full list is returned.
    for _ in 0..5 {
        let p = Address::generate(&env);
        client.admin_freeze_player(&p, &String::from_str(&env, "cap test"));
    }

    // Requesting 200 should behave identically to requesting 100.
    let clamped = client.get_frozen_players(&0, &200);
    let normal = client.get_frozen_players(&0, &100);

    assert_eq!(
        clamped.len(),
        normal.len(),
        "limit > 100 must be silently clamped to 100"
    );
}
