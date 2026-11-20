/// #1936 — get_blacklist_paginated mirrors the allowlist pagination signature.
use super::*;

/// Returns an empty vec when the blacklist is empty.
#[test]
fn test_get_blacklist_paginated_empty_blacklist() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.get_blacklist_paginated(&0, &10);
    assert_eq!(result.len(), 0);
}

/// Returns up to `limit` entries starting at `offset`.
#[test]
fn test_get_blacklist_paginated_basic_pagination() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Register three extra token addresses on the blacklist.
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);
    let token_c = Address::generate(&env);
    let reason = String::from_str(&env, "test");

    client.add_token_to_blacklist(&token_a, &reason);
    client.add_token_to_blacklist(&token_b, &reason);
    client.add_token_to_blacklist(&token_c, &reason);

    // Page 0: first 2 entries.
    let page0 = client.get_blacklist_paginated(&0, &2);
    assert_eq!(page0.len(), 2);
    assert_eq!(page0.get(0).unwrap(), token_a);
    assert_eq!(page0.get(1).unwrap(), token_b);

    // Page 1: remaining entry.
    let page1 = client.get_blacklist_paginated(&2, &2);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap(), token_c);

    // Offset beyond length → empty.
    let page2 = client.get_blacklist_paginated(&3, &10);
    assert_eq!(page2.len(), 0);

    let _ = admin; // suppress unused warning
}

/// limit=0 returns an empty vec.
#[test]
fn test_get_blacklist_paginated_zero_limit() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let token_a = Address::generate(&env);
    client.add_token_to_blacklist(&token_a, &String::from_str(&env, "reason"));

    let result = client.get_blacklist_paginated(&0, &0);
    assert_eq!(result.len(), 0);
}

/// limit is silently clamped to 100.
#[test]
fn test_get_blacklist_paginated_limit_clamped_to_100() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Add a few entries; a huge limit should not return more than 100 and must not panic.
    for _ in 0..5 {
        let tok = Address::generate(&env);
        client.add_token_to_blacklist(&tok, &String::from_str(&env, "r"));
    }

    let result = client.get_blacklist_paginated(&0, &u32::MAX);
    assert!(
        result.len() <= 100,
        "result must not exceed the 100-entry cap"
    );
    assert_eq!(result.len(), 5, "all 5 entries should be returned (below cap)");
}

/// Results match get_blacklist when offset=0 and limit≥len.
#[test]
fn test_get_blacklist_paginated_full_page_matches_unbounded() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let tokens: Vec<Address> = (0..4).map(|_| Address::generate(&env)).collect();
    for tok in &tokens {
        client.add_token_to_blacklist(tok, &String::from_str(&env, "r"));
    }

    let full = client.get_blacklist();
    let paginated = client.get_blacklist_paginated(&0, &100);

    assert_eq!(full.len(), paginated.len());
    for i in 0..full.len() {
        assert_eq!(full.get(i).unwrap(), paginated.get(i).unwrap());
    }
}
