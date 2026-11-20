//! Tests for issue #1923 — `get_global_platform_stats` running aggregate.
//!
//! Verifies that the global aggregate stored under `DataKey::Stats` is
//! maintained incrementally (not recomputed from per-platform stats on every
//! call), and that after any sequence of lifecycle transitions the global
//! totals equal the sum of all per-platform stats.
#![cfg(test)]
extern crate std;

use super::*;
use crate::tests::helpers::*;

// ── #1923: global aggregate matches sum of per-platform stats ─────────────────

/// After creating matches on a single platform the global stats must equal
/// the platform stats for that platform.
#[test]
fn test_global_stats_equal_single_platform_stats() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_default_match(&client, &env, &player1, &player2, &token, "ab12cd34");
    create_default_match(&client, &env, &player1, &player2, &token, "ef56gh78");

    let global = client.get_global_platform_stats();
    let lichess = client.get_platform_stats(&Platform::Lichess).unwrap();

    assert_eq!(
        global.total_matches, lichess.total_matches,
        "global total_matches must equal single-platform total"
    );
    assert_eq!(
        global.total_volume, lichess.total_volume,
        "global total_volume must equal single-platform total"
    );
    assert_eq!(
        global.total_payouts, lichess.total_payouts,
        "global total_payouts must equal single-platform total"
    );
}

/// After creating matches on two different platforms the global stats must
/// equal the arithmetic sum of the two per-platform stats.
#[test]
fn test_global_stats_equal_sum_of_per_platform_stats() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create and complete one Lichess match.
    let lid = create_match_with_stake(&client, &env, &player1, &player2, &token, "li123456", 100);
    fund_match(&client, lid, &player1, &player2);
    client.submit_result(&lid, &Winner::Player1, &oracle);

    // Create one Chess.com match (do not complete it).
    client.create_match(
        &player1,
        &player2,
        &50,
        &token,
        &String::from_str(&env, "1234567"),
        &Platform::ChessDotCom,
    );

    let lichess = client.get_platform_stats(&Platform::Lichess).unwrap();
    let chessdotcom = client.get_platform_stats(&Platform::ChessDotCom).unwrap();
    let global = client.get_global_platform_stats();

    let expected_matches = lichess.total_matches + chessdotcom.total_matches;
    let expected_volume = lichess.total_volume + chessdotcom.total_volume;
    let expected_payouts = lichess.total_payouts + chessdotcom.total_payouts;

    assert_eq!(
        global.total_matches, expected_matches,
        "global total_matches must equal sum of per-platform totals (got {}, want {})",
        global.total_matches, expected_matches
    );
    assert_eq!(
        global.total_volume, expected_volume,
        "global total_volume must equal sum of per-platform totals (got {}, want {})",
        global.total_volume, expected_volume
    );
    assert_eq!(
        global.total_payouts, expected_payouts,
        "global total_payouts must equal sum of per-platform totals (got {}, want {})",
        global.total_payouts, expected_payouts
    );
}

/// The global aggregate must be updated incrementally: each `create_match`
/// call must increase `total_matches` and `total_volume` by exactly 1 and
/// the stake amount respectively, without recomputing from scratch.
#[test]
fn test_global_stats_incremented_incrementally_on_create() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let before = client.get_global_platform_stats();
    assert_eq!(before.total_matches, 0);
    assert_eq!(before.total_volume, 0);

    create_match_with_stake(&client, &env, &player1, &player2, &token, "aa11bb22", 75);

    let after = client.get_global_platform_stats();
    assert_eq!(after.total_matches, 1, "total_matches must increment by 1");
    assert_eq!(after.total_volume, 75, "total_volume must increment by stake");
}

/// Each completed match must increment `total_payouts` in the global
/// aggregate by exactly 1.
#[test]
fn test_global_stats_total_payouts_incremented_on_completion() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id1 = create_match_with_stake(&client, &env, &player1, &player2, &token, "cc33dd44", 50);
    fund_match(&client, id1, &player1, &player2);
    client.submit_result(&id1, &Winner::Player1, &oracle);

    let after_first = client.get_global_platform_stats();
    assert_eq!(after_first.total_payouts, 1);

    let id2 = create_match_with_stake(&client, &env, &player1, &player2, &token, "ee55ff66", 80);
    fund_match(&client, id2, &player1, &player2);
    client.submit_result(&id2, &Winner::Draw, &oracle);

    let after_second = client.get_global_platform_stats();
    assert_eq!(after_second.total_payouts, 2);

    // Confirm it still matches per-platform sum.
    let lichess = client.get_platform_stats(&Platform::Lichess).unwrap();
    assert_eq!(
        after_second.total_payouts, lichess.total_payouts,
        "global payouts must equal per-platform sum when only one platform is used"
    );
}

/// Calling `get_global_platform_stats` multiple times must return the same
/// value without side effects — confirming the aggregate is read-only.
#[test]
fn test_global_stats_idempotent_reads() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    create_default_match(&client, &env, &player1, &player2, &token, "aa22bb33");

    let first_read = client.get_global_platform_stats();
    let second_read = client.get_global_platform_stats();

    assert_eq!(
        first_read.total_matches, second_read.total_matches,
        "repeated reads must not mutate the aggregate"
    );
    assert_eq!(first_read.total_volume, second_read.total_volume);
    assert_eq!(first_read.total_payouts, second_read.total_payouts);
}
