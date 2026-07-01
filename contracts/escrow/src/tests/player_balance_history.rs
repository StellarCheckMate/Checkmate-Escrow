use super::*;
use soroban_sdk::testutils::Ledger as _;

// ── player_escrow_balance / record_player_snapshot behavior ─────────────────

#[test]
fn test_get_balance_at_timestamp_returns_zero_for_player_with_no_history() {
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let _client = EscrowContractClient::new(&env, &contract_id);
    let outsider = Address::generate(&env);

    // Players that never interacted with the contract have no recorded
    // snapshots — every historical lookup must return 0.
    assert_eq!(env.as_contract(&contract_id, || EscrowContract::get_balance_at_timestamp(env.clone(), outsider.clone(), 0)), 0);
    assert_eq!(
        env.as_contract(&contract_id, || EscrowContract::get_balance_at_timestamp(
            env.clone(),
            outsider,
            u64::MAX,
        )),
        0,
    );

    // Players that touched the contract but never deposited are also empty.
    let _ = _client.create_match(
        &player1,
        &player2,
        &100,
        &_token,
        &String::from_str(&env, "no_history_match"),
        &Platform::Lichess,
    );
    assert_eq!(
        env.as_contract(&contract_id, || EscrowContract::get_balance_at_timestamp(
            env.clone(),
            player1,
            u64::MAX,
        )),
        0
    );
}

#[test]
fn test_deposit_records_player_snapshot_increasing_balance() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "deposit_player_snap"),
        &Platform::Lichess,
    );

    env.ledger().set_sequence_number(10);
    client.deposit(&id, &player1);

    // After player1 deposits at ledger 10, querying at any timestamp >= 10
    // returns their stake (100). Before that timestamp, balance is still 0.
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 0),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 9),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 10),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 1000),
            100
        );
    });

    // player2 has not deposited — their snapshot still reports 0.
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 10),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), u64::MAX),
            0
        );
    });
}

#[test]
fn test_two_deposits_cumulate_balance_in_history() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "two_deposits"),
        &Platform::Lichess,
    );

    env.ledger().set_sequence_number(5);
    client.deposit(&id, &player1);
    env.ledger().set_sequence_number(7);
    client.deposit(&id, &player2);

    env.as_contract(&contract_id, || {
        // Before any deposit: 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 4),
            0
        );
        // After player1 deposits at ledger 5: player1 = 100, player2 still 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 5),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 6),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 6),
            0
        );
        // After player2 deposits at ledger 7: player1 still 100, player2 = 100.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 7),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 7),
            100
        );
        // player2's first non-zero reading must be at ledger 7 (when they deposited),
        // not before.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 5),
            0
        );
    });
}

#[test]
fn test_submit_result_zeroes_player_balance_in_history() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "payout_zero"),
        &Platform::Lichess,
    );

    env.ledger().set_sequence_number(1);
    client.deposit(&id, &player1);
    env.ledger().set_sequence_number(2);
    client.deposit(&id, &player2);
    env.ledger().set_sequence_number(3);
    client.submit_result(&id, &Winner::Player1);

    env.as_contract(&contract_id, || {
        // Before the payout, both players had 100.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 2),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 2),
            100
        );
        // After the payout at ledger 3, both are 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 3),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 3),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), u64::MAX),
            0
        );
    });
}

#[test]
fn test_cancel_match_zeroes_player_balance_in_history() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "cancel_zero"),
        &Platform::Lichess,
    );

    env.ledger().set_sequence_number(10);
    client.deposit(&id, &player1);
    env.ledger().set_sequence_number(20);
    client.cancel_match(&id, &player1);

    env.as_contract(&contract_id, || {
        // After player1 deposits, balance is 100.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 10),
            100
        );
        // After cancel at ledger 20, balance is 0 again.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 20),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), u64::MAX),
            0
        );
        // player2 never deposited — always 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 20),
            0
        );
    });
}

#[test]
fn test_expire_match_zeroes_player_balance_in_history() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_match_timeout(&17_280);
    env.ledger().set_sequence_number(100);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "expire_zero"),
        &Platform::Lichess,
    );

    env.ledger().set_sequence_number(200);
    client.deposit(&id, &player1);

    env.ledger().set_sequence_number(200 + 17_280);
    client.expire_match(&id);

    env.as_contract(&contract_id, || {
        // After deposit at ledger 200: 100.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 200),
            100
        );
        // After expire at ledger 200+17280: 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 200 + 17_280),
            0
        );
    });
}

#[test]
fn test_balance_history_across_multiple_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "multi_a"),
        &Platform::Lichess,
    );
    let b = client.create_match(
        &player1,
        &player2,
        &50,
        &token,
        &String::from_str(&env, "multi_b"),
        &Platform::ChessDotCom,
    );

    env.ledger().set_sequence_number(1);
    client.deposit(&a, &player1); // player1 has 100 in match a
    env.ledger().set_sequence_number(2);
    client.deposit(&a, &player2);
    env.ledger().set_sequence_number(3);
    client.submit_result(&a, &Winner::Player1); // both back to 0
    env.ledger().set_sequence_number(4);
    client.deposit(&b, &player1); // player1 has 50 in match b
    env.ledger().set_sequence_number(5);
    client.deposit(&b, &player2); // player2 has 50 in match b

    env.as_contract(&contract_id, || {
        // player1 timeline: 0 -> 100 -> 0 -> 50 -> 50
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 1),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 3),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 4),
            50
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 5),
            50
        );

        // player2 timeline: 0 -> 0 -> 100 (player2 deposit in match a at l2) -> 0 (payout) -> 0 -> 50
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 2),
            100
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 3),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player2.clone(), 5),
            50
        );
    });
}

#[test]
fn test_get_balance_at_timestamp_returns_zero_when_no_snapshot_before_query() {
    // The pruning-mechanism contract: if the ring buffer has cycled and all
    // surviving snapshots are newer than `timestamp`, the query returns 0.
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "no_history_before"),
        &Platform::Lichess,
    );

    env.ledger().set_sequence_number(500);
    client.deposit(&id, &player1);

    env.as_contract(&contract_id, || {
        // Querying before the recorded snapshot returns 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 0),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 100),
            0
        );
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 499),
            0
        );
        // At or after the recorded ledger, the snapshot is returned.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 500),
            100
        );
    });
}

#[test]
fn test_get_balance_at_timestamp_after_ring_buffer_overwrites_oldest() {
    // Drives more than MAX_PLAYER_SNAPSHOTS snapshots for one player to
    // exercise the ring-buffer overwrite path that production code never
    // reaches, and then verifies that the oldest queries return 0 (correctly
    // pruned) while the newest queries still find fresh balances.
    let (env, contract_id, _oracle, player1, player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = client.create_match(
        &player1,
        &player2,
        &100,
        &_token,
        &String::from_str(&env, "ring_prune"),
        &Platform::Lichess,
    );
    client.deposit(&id, &player1);

    // Manually pump (MAX_PLAYER_SNAPSHOTS * 2) additional snapshots to ensure
    // the oldest ones are pruned.
    env.as_contract(&contract_id, || {
        for i in 0..(MAX_PLAYER_SNAPSHOTS * 2) {
            env.ledger().set_sequence_number(1_000 + i as u32);
            EscrowContract::record_player_snapshot(&env, &player1);
        }
    });

    env.as_contract(&contract_id, || {
        let last_idx = MAX_PLAYER_SNAPSHOTS * 2 - 1;
        let final_ledger = 1_000u32 + last_idx;

        // 1) The freshly-recorded value at the final ledger must be retrievable.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), final_ledger as u64),
            100,
            "latest snapshot must be retrievable at its own ledger"
        );

        // 2) The very first manually-recorded snapshot (at ledger 1_000) sits
        //    inside the surviving window and must still resolve to 100.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 1_000),
            100,
            "first in-window snapshot must still be retrievable after overwrites"
        );

        // 3) Queries strictly before all surviving snapshots must return 0.
        assert_eq!(
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), 999),
            0
        );
    });
}
