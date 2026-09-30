//! Test for player with many matches to verify PlayerEscrowBalance counter
//! efficiently handles the case without hitting read limits.
//!
//! The old implementation of `player_escrow_balance()` would iterate through
//! all matches in `PlayerMatches(player)`, causing read limits for active
//! players with many matches. This test verifies that the new counter-based
//! approach efficiently handles this case.

use super::*;
use soroban_sdk::testutils::Ledger as _;

#[test]
fn test_player_with_many_active_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create 50 matches between player1 and player2
    // This would cause significant read pressure with the old O(n) implementation
    const MATCH_COUNT: usize = 50;
    const STAKE_AMOUNT: i128 = 100;
    let mut match_ids = Vec::new();

    for i in 0..MATCH_COUNT {
        let game_id = format!("game_{}", i);
        let match_id = client.create_match(
            &player1,
            &player2,
            &STAKE_AMOUNT,
            &token,
            &String::from_str(&env, &game_id),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    // Player1 deposits in all matches
    for &match_id in &match_ids {
        client.deposit(&match_id, &player1);
    }

    // Verify player1's escrow balance reflects all deposits (MATCH_COUNT * STAKE_AMOUNT)
    // This call should be O(1) with the new counter-based approach
    env.as_contract(&contract_id, || {
        let expected_balance = (MATCH_COUNT as i128) * STAKE_AMOUNT;
        let actual_balance = EscrowContract::player_escrow_balance(&env, &player1);
        assert_eq!(
            actual_balance, expected_balance,
            "Player1 escrow balance should reflect all {} deposits",
            MATCH_COUNT
        );

        // Verify the snapshot records the correct balance
        let snapshot_balance =
            EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), u64::MAX);
        assert_eq!(
            snapshot_balance,
            BalanceAtTimestamp::Known(expected_balance),
            "Snapshot should record the full escrow balance"
        );
    });

    // Player2 deposits in all matches
    for &match_id in &match_ids {
        client.deposit(&match_id, &player2);
    }

    // Now both players should have MATCH_COUNT * STAKE_AMOUNT in escrow
    env.as_contract(&contract_id, || {
        let expected_balance = (MATCH_COUNT as i128) * STAKE_AMOUNT;
        let balance_p1 = EscrowContract::player_escrow_balance(&env, &player1);
        let balance_p2 = EscrowContract::player_escrow_balance(&env, &player2);
        assert_eq!(balance_p1, expected_balance);
        assert_eq!(balance_p2, expected_balance);
    });

    // Complete some matches and verify balances decrease correctly
    // Complete first 10 matches (half with player1 winning, half with player2 winning)
    for i in 0..10 {
        let winner = if i % 2 == 0 {
            Winner::Player1
        } else {
            Winner::Player2
        };
        client.submit_result(&match_ids[i], &winner, &oracle);
    }

    // Verify balances: both players should have reduced by 10 * STAKE_AMOUNT
    env.as_contract(&contract_id, || {
        let expected_balance = ((MATCH_COUNT - 10) as i128) * STAKE_AMOUNT;
        let balance_p1 = EscrowContract::player_escrow_balance(&env, &player1);
        let balance_p2 = EscrowContract::player_escrow_balance(&env, &player2);
        assert_eq!(
            balance_p1, expected_balance,
            "Player1 balance should decrease as matches complete"
        );
        assert_eq!(
            balance_p2, expected_balance,
            "Player2 balance should decrease as matches complete"
        );
    });

    // Cancel some pending matches and verify balance updates
    // Remaining 40 matches: cancel first 20
    for i in 10..30 {
        client.cancel_match(&match_ids[i], &player1);
    }

    // Verify final balances: both have (50-10-20) * 100 = 20 * 100 = 2000
    env.as_contract(&contract_id, || {
        let expected_balance = ((MATCH_COUNT - 10 - 20) as i128) * STAKE_AMOUNT;
        let balance_p1 = EscrowContract::player_escrow_balance(&env, &player1);
        let balance_p2 = EscrowContract::player_escrow_balance(&env, &player2);
        assert_eq!(balance_p1, expected_balance);
        assert_eq!(balance_p2, expected_balance);
    });
}

#[test]
fn test_player_escrow_balance_accumulation_and_decrement() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Test incremental balance changes as matches are created and settled
    const STAKE: i128 = 250;

    // Create and track 3 matches
    let match_id_1 = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "game_1"),
        &Platform::Lichess,
    );
    let match_id_2 = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "game_2"),
        &Platform::Lichess,
    );
    let match_id_3 = client.create_match(
        &player1,
        &player2,
        &STAKE,
        &token,
        &String::from_str(&env, "game_3"),
        &Platform::Lichess,
    );

    // Player1 deposits in match 1 only
    client.deposit(&match_id_1, &player1);
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player1),
            STAKE,
            "After 1st deposit, balance should be 1 * STAKE"
        );
    });

    // Player1 deposits in match 2
    client.deposit(&match_id_2, &player1);
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player1),
            2 * STAKE,
            "After 2nd deposit, balance should be 2 * STAKE"
        );
    });

    // Player1 deposits in match 3
    client.deposit(&match_id_3, &player1);
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player1),
            3 * STAKE,
            "After 3rd deposit, balance should be 3 * STAKE"
        );
    });

    // Player2 also deposits in all three
    client.deposit(&match_id_1, &player2);
    client.deposit(&match_id_2, &player2);
    client.deposit(&match_id_3, &player2);

    // Both should have 3 * STAKE
    env.as_contract(&contract_id, || {
        assert_eq!(EscrowContract::player_escrow_balance(&env, &player1), 3 * STAKE);
        assert_eq!(EscrowContract::player_escrow_balance(&env, &player2), 3 * STAKE);
    });

    // Complete match 1 (player1 wins)
    client.submit_result(&match_id_1, &Winner::Player1, &oracle);
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player1),
            2 * STAKE,
            "After first match completes, balance should be 2 * STAKE"
        );
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player2),
            2 * STAKE,
            "Player2 balance should also be 2 * STAKE"
        );
    });

    // Cancel match 2
    client.cancel_match(&match_id_2, &player1);
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player1),
            STAKE,
            "After match 2 cancels, balance should be 1 * STAKE"
        );
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player2),
            STAKE,
            "Player2 balance should also be 1 * STAKE"
        );
    });

    // Complete match 3 (draw)
    client.submit_result(&match_id_3, &Winner::Draw, &oracle);
    env.as_contract(&contract_id, || {
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player1),
            0,
            "After all matches settle, balance should be 0"
        );
        assert_eq!(
            EscrowContract::player_escrow_balance(&env, &player2),
            0,
            "Player2 balance should also be 0"
        );
    });
}

#[test]
fn test_balance_history_reflects_counter_updates() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    const STAKE: i128 = 500;
    const MATCH_COUNT: usize = 25;

    // Create 25 matches and track snapshots through deposits
    let mut match_ids = Vec::new();
    for i in 0..MATCH_COUNT {
        let game_id = format!("snap_test_{}", i);
        let match_id = client.create_match(
            &player1,
            &player2,
            &STAKE,
            &token,
            &String::from_str(&env, &game_id),
            &Platform::Lichess,
        );
        match_ids.push(match_id);
    }

    // Deposit in batches of 5 and verify running balance at each step
    for batch_num in 0..5 {
        let start_idx = batch_num * 5;
        for idx in start_idx..start_idx + 5 {
            client.deposit(&match_ids[idx], &player1);
        }

        env.as_contract(&contract_id, || {
            let expected_balance = ((batch_num + 1) * 5) as i128 * STAKE;
            let actual_balance =
                EscrowContract::get_balance_at_timestamp(env.clone(), player1.clone(), u64::MAX);
            assert_eq!(
                actual_balance,
                BalanceAtTimestamp::Known(expected_balance),
                "After batch {}, balance should be {} * {} = {}",
                batch_num + 1,
                (batch_num + 1) * 5,
                STAKE,
                expected_balance
            );
        });
    }
}
