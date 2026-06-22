use super::*;
use soroban_sdk::testutils::Ledger as _;

#[test]
fn test_create_match_records_created_snapshot() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_created"),
        &Platform::Lichess,
    );

    let snaps = client.get_balance_snapshots(&admin, &id);
    assert_eq!(snaps.len(), 1);
    let snap = snaps.get(0).unwrap();
    assert_eq!(snap.match_id, id);
    assert_eq!(snap.index, 0);
    assert_eq!(snap.reason, SnapshotReason::Created);
    assert_eq!(snap.token, token);
    assert_eq!(snap.stake_amount, 100);
    assert_eq!(snap.escrow_balance, 0);
    assert!(!snap.player1_deposited);
    assert!(!snap.player2_deposited);

    let token_client = TokenClient::new(&env, &token);
    assert_eq!(snap.token_symbol, token_client.symbol());
}

#[test]
fn test_deposit_records_a_snapshot_per_deposit() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_deposit"),
        &Platform::Lichess,
    );

    client.deposit(&id, &player1);
    let snaps = client.get_balance_snapshots(&admin, &id);
    assert_eq!(snaps.len(), 2);
    let after_p1 = snaps.get(1).unwrap();
    assert_eq!(after_p1.reason, SnapshotReason::Deposit);
    assert_eq!(after_p1.escrow_balance, 100);
    assert!(after_p1.player1_deposited);
    assert!(!after_p1.player2_deposited);

    client.deposit(&id, &player2);
    let snaps = client.get_balance_snapshots(&admin, &id);
    assert_eq!(snaps.len(), 3);
    let after_p2 = snaps.get(2).unwrap();
    assert_eq!(after_p2.reason, SnapshotReason::Deposit);
    assert_eq!(after_p2.escrow_balance, 200);
    assert!(after_p2.player1_deposited);
    assert!(after_p2.player2_deposited);
}

#[test]
fn test_submit_result_records_completed_snapshot_with_zero_balance() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_completed"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player1);

    let latest = client.get_latest_snapshot(&admin, &id);
    assert_eq!(latest.reason, SnapshotReason::Completed);
    assert_eq!(latest.escrow_balance, 0);
    assert_eq!(latest.index, 3);
}

#[test]
fn test_cancel_match_records_cancelled_snapshot_with_zero_balance() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_cancelled"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.cancel_match(&id, &player1);

    let latest = client.get_latest_snapshot(&admin, &id);
    assert_eq!(latest.reason, SnapshotReason::Cancelled);
    assert_eq!(latest.escrow_balance, 0);
    assert_eq!(latest.index, 2);
}

#[test]
fn test_expire_match_records_cancelled_snapshot() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_match_timeout(&17_280);
    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_expired"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    env.ledger().set_sequence_number(100 + 17_280);
    client.expire_match(&id);

    let latest = client.get_latest_snapshot(&admin, &id);
    assert_eq!(latest.reason, SnapshotReason::Cancelled);
    assert_eq!(latest.escrow_balance, 0);
}

#[test]
fn test_full_lifecycle_snapshot_sequence_is_chronological() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_sequence"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);
    client.deposit(&id, &player2);
    client.submit_result(&id, &Winner::Player2);

    let snaps = client.get_balance_snapshots(&admin, &id);
    assert_eq!(snaps.len(), 4);

    let reasons: std::vec::Vec<SnapshotReason> = snaps.iter().map(|s| s.reason.clone()).collect();
    assert_eq!(
        reasons,
        std::vec![
            SnapshotReason::Created,
            SnapshotReason::Deposit,
            SnapshotReason::Deposit,
            SnapshotReason::Completed,
        ]
    );

    for (i, snap) in snaps.iter().enumerate() {
        assert_eq!(snap.index, i as u32);
    }
}

#[test]
fn test_admin_sees_exact_amounts_in_snapshots() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &250,
        &token,
        &String::from_str(&env, "snap_admin_view"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    let latest = client.get_latest_snapshot(&admin, &id);
    assert_eq!(latest.stake_amount, 250);
    assert_eq!(latest.escrow_balance, 250);
}

#[test]
fn test_player_sees_redacted_amounts_in_snapshots() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &250,
        &token,
        &String::from_str(&env, "snap_player_view"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    // player1 is a participant — gets partial (redacted) data, not Unauthorized.
    let latest = client.get_latest_snapshot(&player1, &id);
    assert_eq!(
        latest.stake_amount, 0,
        "stake_amount must be redacted for non-admin callers"
    );
    assert_eq!(
        latest.escrow_balance, 0,
        "escrow_balance must be redacted for non-admin callers"
    );
    // Non-sensitive fields remain visible.
    assert_eq!(latest.reason, SnapshotReason::Deposit);
    assert!(latest.player1_deposited);

    // player2 (the other participant) also gets partial data.
    let latest_p2 = client.get_latest_snapshot(&player2, &id);
    assert_eq!(latest_p2.stake_amount, 0);
    assert_eq!(latest_p2.escrow_balance, 0);
}

#[test]
fn test_unrelated_caller_cannot_query_snapshots() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let outsider = Address::generate(&env);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_unauthorized"),
        &Platform::Lichess,
    );

    let result = client.try_get_balance_snapshots(&outsider, &id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));

    let result = client.try_get_latest_snapshot(&outsider, &id);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_get_latest_snapshot_on_nonexistent_match_returns_match_not_found() {
    let (env, contract_id, _oracle, _player1, _player2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_get_latest_snapshot(&admin, &9999u64);
    assert_eq!(result, Err(Ok(Error::MatchNotFound)));
}

#[test]
fn test_multi_token_matches_record_independent_symbols_and_amounts() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let player1 = Address::generate(&env);
    let player2 = Address::generate(&env);

    let token_a_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_a = token_a_id.address();
    let token_b_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_b = token_b_id.address();

    for token in [&token_a, &token_b] {
        let asset_client = StellarAssetClient::new(&env, token);
        asset_client.mint(&player1, &1000);
        asset_client.mint(&player2, &1000);
    }

    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);
    client.initialize(&oracle, &admin);

    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token_a,
        &String::from_str(&env, "multi_token_a"),
        &Platform::Lichess,
    );
    let match_b = client.create_match(
        &player1,
        &player2,
        &60,
        &token_b,
        &String::from_str(&env, "multi_token_b"),
        &Platform::ChessDotCom,
    );

    client.deposit(&match_a, &player1);
    client.deposit(&match_b, &player1);

    let snap_a = client.get_latest_snapshot(&admin, &match_a);
    let snap_b = client.get_latest_snapshot(&admin, &match_b);

    assert_eq!(snap_a.token, token_a);
    assert_eq!(snap_a.escrow_balance, 100);
    assert_eq!(snap_b.token, token_b);
    assert_eq!(snap_b.escrow_balance, 60);
}

#[test]
fn test_snapshot_ring_buffer_prunes_oldest_entries() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_pruning"),
        &Platform::Lichess,
    );

    // Drive far more snapshots than MAX_SNAPSHOTS_PER_MATCH by calling the
    // internal recorder directly — exercises the ring-buffer overwrite path
    // that a normal 4-event lifecycle never reaches.
    env.as_contract(&contract_id, || {
        let m: Match = env.storage().persistent().get(&DataKey::Match(id)).unwrap();
        for _ in 0..(MAX_SNAPSHOTS_PER_MATCH * 3) {
            EscrowContract::record_snapshot(&env, &m, SnapshotReason::Deposit);
        }
    });

    let snaps = client.get_balance_snapshots(&admin, &id);
    assert_eq!(
        snaps.len() as u32,
        MAX_SNAPSHOTS_PER_MATCH,
        "ring buffer must cap stored snapshots at MAX_SNAPSHOTS_PER_MATCH"
    );

    // Oldest surviving entries must be the most recently written ones —
    // older entries were overwritten, not appended past the cap.
    let total_written = 1 + MAX_SNAPSHOTS_PER_MATCH * 3; // +1 for the Created snapshot
    let first_surviving_index = total_written - MAX_SNAPSHOTS_PER_MATCH;
    assert_eq!(snaps.get(0).unwrap().index, first_surviving_index);
    assert_eq!(snaps.get(snaps.len() - 1).unwrap().index, total_written - 1);
}

#[test]
fn test_get_balance_snapshots_empty_for_match_with_no_recorded_history() {
    // Defensive case: if SnapshotCount were ever absent for an existing match,
    // queries should degrade to an empty list rather than panicking.
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "snap_wipe_history"),
        &Platform::Lichess,
    );

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .remove(&DataKey::SnapshotCount(id));
    });

    let snaps = client.get_balance_snapshots(&admin, &id);
    assert_eq!(snaps.len(), 0);

    let result = client.try_get_latest_snapshot(&admin, &id);
    assert_eq!(result, Err(Ok(Error::SnapshotNotFound)));
}
