use super::*;

/// Test #580: player-match pagination handles empty and partial pages
#[test]
fn test_player_match_pagination_handles_empty_and_partial_pages() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 25 matches for player1
    let mut match_ids = Vec::new();
    for i in 0..25 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("{:08x}", i)),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    // Query player1's matches with paginated API
    let player1_page_0 = client.get_player_matches_paginated(&player1, &0, &5);
    assert_eq!(player1_page_0.len(), 5);
    for (i, match_id) in player1_page_0.iter().enumerate() {
        assert_eq!(match_id, match_ids[i]);
    }

    let player1_page_1 = client.get_player_matches_paginated(&player1, &5, &10);
    assert_eq!(player1_page_1.len(), 10);
    for (i, match_id) in player1_page_1.iter().enumerate() {
        assert_eq!(match_id, match_ids[5 + i]);
    }

    let player1_page_2 = client.get_player_matches_paginated(&player1, &20, &10);
    assert_eq!(player1_page_2.len(), 5);
    for (i, match_id) in player1_page_2.iter().enumerate() {
        assert_eq!(match_id, match_ids[20 + i]);
    }

    let player1_page_3 = client.get_player_matches_paginated(&player1, &25, &10);
    assert_eq!(player1_page_3.len(), 0);

    // Verify the existing getter still returns the full list for compatibility.
    let player1_matches = client.get_player_matches(&player1);
    assert_eq!(player1_matches.len(), 25);

    // Query player2's matches (should have 25 as well)
    let player2_matches = client.get_player_matches(&player2);
    assert_eq!(player2_matches.len(), 25);

    // Query a player with no matches
    let player3 = Address::generate(&env);
    let player3_matches = client.get_player_matches(&player3);
    assert_eq!(player3_matches.len(), 0);
}

/// Test #581: player match pagination returns empty page for zero limit and offset beyond end
#[test]
fn test_player_match_pagination_zero_limit_and_offset_beyond_end() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 10 matches for player1
    let mut match_ids = Vec::new();
    for i in 0..10 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("{:08x}", i)),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    let zero_limit = client.get_player_matches_paginated(&player1, &0, &0);
    assert_eq!(zero_limit.len(), 0);

    let beyond_offset = client.get_player_matches_paginated(&player1, &15, &5);
    assert_eq!(beyond_offset.len(), 0);

    let partial_page = client.get_player_matches_paginated(&player1, &8, &5);
    assert_eq!(partial_page.len(), 2);
    assert_eq!(partial_page.get(0).unwrap(), match_ids[8]);
    assert_eq!(partial_page.get(1).unwrap(), match_ids[9]);
}

/// Test #1566: paginated getters cap `limit` at `MAX_PAGE_LIMIT` (100).
///
/// A caller requesting an unbounded (or absurdly large) `limit` must never
/// receive more than `MAX_PAGE_LIMIT` items, otherwise pagination is defeated.
#[test]
fn test_paginated_getters_cap_limit_at_max_page_limit() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Seed more matches than the cap so the cap is actually observable.
    let total: u64 = MAX_PAGE_LIMIT as u64 + 25;
    let mut match_ids = Vec::new();
    for i in 0..total {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("{:08x}", i)),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    // A huge limit is clamped to MAX_PAGE_LIMIT.
    let huge = client.get_player_matches_paginated(&player1, &0, &u32::MAX);
    assert_eq!(huge.len(), MAX_PAGE_LIMIT);
    for (i, match_id) in huge.iter().enumerate() {
        assert_eq!(match_id, match_ids[i]);
    }

    // Exactly the cap is allowed through unchanged.
    let at_cap = client.get_player_matches_paginated(&player1, &0, &(MAX_PAGE_LIMIT as u32));
    assert_eq!(at_cap.len(), MAX_PAGE_LIMIT);

    // A limit just over the cap is clamped down to the cap.
    let over_cap = client.get_player_matches_paginated(&player1, &0, &(MAX_PAGE_LIMIT as u32 + 1));
    assert_eq!(over_cap.len(), MAX_PAGE_LIMIT);

    // The cap also applies to the allowed-tokens paginated getter.
    let tokens = client.get_allowed_tokens_paginated(&0, &u32::MAX);
    assert!(tokens.len() <= MAX_PAGE_LIMIT);
}

/// Regression test: the deprecated unbounded `get_pending_matches()` must cap
/// its result at `MAX_UNBOUNDED_MATCH_RESULTS` and emit a truncation event so
/// callers have a signal that results were silently capped, instead of
/// looking like a complete result set.
#[test]
fn test_get_pending_matches_emits_truncation_event_when_capped() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Seed one real match to get a well-formed template, then clone it
    // directly into storage well past the cap — far cheaper than driving
    // `create_match` MAX_UNBOUNDED_MATCH_RESULTS+ times.
    let template_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "5f6a7b8c"),
        &Platform::Lichess,
    );

    let total: u64 = 10_005;
    env.as_contract(&contract_id, || {
        let template: Match = env
            .storage()
            .persistent()
            .get(&DataKey::Match(template_id))
            .unwrap();

        for id in 0..total {
            let mut m = template.clone();
            m.id = id;
            m.state = MatchState::Pending;
            env.storage().persistent().set(&DataKey::Match(id), &m);
        }
        env.storage().instance().set(&DataKey::MatchCount, &total);
    });

    let results = client.get_pending_matches();
    assert_eq!(
        results.len(),
        10_000,
        "unbounded getter must cap at MAX_UNBOUNDED_MATCH_RESULTS"
    );

    let events = env.events().all();
    let expected_topics = vec![
        &env,
        Symbol::new(&env, "match").into_val(&env),
        symbol_short!("truncated").into_val(&env),
    ];
    let matched = events
        .iter()
        .find(|(_, topics, _)| *topics == expected_topics);
    assert!(
        matched.is_some(),
        "truncation must emit a match/truncated event"
    );
}

/// Test #579: player history index excludes unrelated matches for other players
#[test]
fn test_player_history_index_excludes_unrelated_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);

    // Mint tokens for player3 and player4
    let asset_client = StellarAssetClient::new(&env, &token);
    asset_client.mint(&player3, &1000);
    asset_client.mint(&player4, &1000);

    // Create matches for player1 and player2
    let match_id_1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "404da6de"),
        &Platform::Lichess,
    );

    let match_id_2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "86b9ea0e"),
        &Platform::Lichess,
    );

    // Create matches for player3 and player4
    let match_id_3 = client.create_match(
        &player3,
        &player4,
        &100,
        &token,
        &String::from_str(&env, "fb595cfb"),
        &Platform::Lichess,
    );

    let match_id_4 = client.create_match(
        &player3,
        &player4,
        &100,
        &token,
        &String::from_str(&env, "24a3f6f9"),
        &Platform::Lichess,
    );

    // Assert player1 only receives their own match IDs
    let player1_matches = client.get_player_matches(&player1);
    assert_eq!(player1_matches.len(), 2);
    assert_eq!(player1_matches.get(0).unwrap(), match_id_1);
    assert_eq!(player1_matches.get(1).unwrap(), match_id_2);

    // Assert player2 only receives their own match IDs
    let player2_matches = client.get_player_matches(&player2);
    assert_eq!(player2_matches.len(), 2);
    assert_eq!(player2_matches.get(0).unwrap(), match_id_1);
    assert_eq!(player2_matches.get(1).unwrap(), match_id_2);

    // Assert player3 only receives their own match IDs
    let player3_matches = client.get_player_matches(&player3);
    assert_eq!(player3_matches.len(), 2);
    assert_eq!(player3_matches.get(0).unwrap(), match_id_3);
    assert_eq!(player3_matches.get(1).unwrap(), match_id_4);

    // Assert player4 only receives their own match IDs
    let player4_matches = client.get_player_matches(&player4);
    assert_eq!(player4_matches.len(), 2);
    assert_eq!(player4_matches.get(0).unwrap(), match_id_3);
    assert_eq!(player4_matches.get(1).unwrap(), match_id_4);
}

/// Test #578: get_player_matches preserves insertion order
#[test]
fn test_get_player_matches_preserves_insertion_order() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mut match_ids = Vec::new();
    for i in 0..5 {
        let match_id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("{:08x}", i)),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    let matches = client.get_player_matches(&player1);
    assert_eq!(matches.len(), 5);
    for (i, match_id) in matches.iter().enumerate() {
        assert_eq!(match_id, match_ids[i]);
    }
}

#[test]
fn test_get_match_history_filters_by_player_with_many_unrelated_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let player3 = Address::generate(&env);
    let player4 = Address::generate(&env);
    token_client(&env, &token).transfer(&player2, &player3, &500);
    token_client(&env, &token).transfer(&player2, &player4, &500);

    let mut player1_ids = Vec::new();
    let mut unrelated_ids = Vec::new();

    for i in 0..12 {
        let unrelated_id = client.create_match(
            &player3,
            &player4,
            &100,
            &token,
            &String::from_str(&env, &format!("unrelated_{}", i)),
            &Platform::Lichess,
        );
        client.cancel_match(&unrelated_id, &player3);
        unrelated_ids.push(unrelated_id);
    }

    for i in 0..3 {
        let id = client.create_match(
            &player1,
            &player2,
            &100,
            &token,
            &String::from_str(&env, &format!("player_history_{}", i)),
            &Platform::Lichess,
        );
        client.cancel_match(&id, &player1);
        player1_ids.push(id);
    }

    let history = client.get_match_history(&Some(player1.clone()), &10, &0);
    assert_eq!(history.len(), 3);
    assert_eq!(history.get(0).unwrap().id, player1_ids[2]);
    assert_eq!(history.get(1).unwrap().id, player1_ids[1]);
    assert_eq!(history.get(2).unwrap().id, player1_ids[0]);

    for match_obj in history.iter() {
        assert!(match_obj.player1 == player1 || match_obj.player2 == player1);
    }

    assert!(
        unrelated_ids
            .iter()
            .all(|id| !history.iter().any(|match_obj| match_obj.id == *id))
    );
}
