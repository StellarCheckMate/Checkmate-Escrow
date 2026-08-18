/// Platform statistics tests for on-chain aggregated analytics.
///
/// Tests the PlatformStats counters (total_matches, total_volume, total_payouts)
/// to ensure analytics data is correctly tracked without off-chain indexing.
use super::*;
use crate::tests::helpers::*;

#[test]
fn test_get_platform_stats_returns_default_when_empty() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stats = client.get_platform_stats();
    assert_eq!(stats.total_matches, 0, "total_matches should start at 0");
    assert_eq!(stats.total_volume, 0, "total_volume should start at 0");
    assert_eq!(stats.total_payouts, 0, "total_payouts should start at 0");
}

#[test]
fn test_platform_stats_increments_on_create_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let stats_before = client.get_platform_stats();
    assert_eq!(stats_before.total_matches, 0);
    assert_eq!(stats_before.total_volume, 0);

    // Create a match with stake 100
    let _match_id = create_default_match(&client, &env, &player1, &player2, &token, "ac39d856");

    let stats_after = client.get_platform_stats();
    assert_eq!(
        stats_after.total_matches, 1,
        "total_matches should increment to 1"
    );
    assert_eq!(
        stats_after.total_volume, 100,
        "total_volume should increment by stake (100)"
    );
    assert_eq!(
        stats_after.total_payouts, 0,
        "total_payouts should remain 0 (no result yet)"
    );
}

#[test]
fn test_platform_stats_accumulate_volume_across_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create first match with stake 100
    let _id1 = create_match_with_stake(&client, &env, &player1, &player2, &token, "4cb35dd9", 100);
    let stats1 = client.get_platform_stats();
    assert_eq!(stats1.total_matches, 1);
    assert_eq!(stats1.total_volume, 100);

    // Create second match with stake 50
    let _id2 = create_match_with_stake(&client, &env, &player1, &player2, &token, "cf30f135", 50);
    let stats2 = client.get_platform_stats();
    assert_eq!(stats2.total_matches, 2);
    assert_eq!(stats2.total_volume, 150, "total_volume should be 100 + 50");

    // Create third match with stake 75 (Bronze tier caps stakes at 100)
    let _id3 = create_match_with_stake(&client, &env, &player1, &player2, &token, "bb8a3c23", 75);
    let stats3 = client.get_platform_stats();
    assert_eq!(stats3.total_matches, 3);
    assert_eq!(
        stats3.total_volume, 225,
        "total_volume should be 100 + 50 + 75"
    );
}

#[test]
fn test_platform_stats_increments_payouts_on_submit_result() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_default_match(&client, &env, &player1, &player2, &token, "95e4f220");
    let stats_before_result = client.get_platform_stats();
    assert_eq!(stats_before_result.total_payouts, 0);

    // Fund and submit result
    fund_match(&client, match_id, &player1, &player2);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    let stats_after_result = client.get_platform_stats();
    assert_eq!(
        stats_after_result.total_payouts, 1,
        "total_payouts should increment on submit_result"
    );
    assert_eq!(
        stats_after_result.total_matches, 1,
        "total_matches should remain unchanged"
    );
}

#[test]
fn test_platform_stats_handles_draw_payout() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = create_default_match(&client, &env, &player1, &player2, &token, "e7b51174");
    fund_match(&client, match_id, &player1, &player2);
    client.submit_result(&match_id, &Winner::Draw, &oracle);

    let stats = client.get_platform_stats();
    assert_eq!(
        stats.total_payouts, 1,
        "total_payouts should count draw as a payout"
    );
}

#[test]
fn test_platform_stats_multiple_matches_and_payouts() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create and complete match 1
    let id1 = create_match_with_stake(&client, &env, &player1, &player2, &token, "2863ed2e", 100);
    fund_match(&client, id1, &player1, &player2);
    client.submit_result(&id1, &Winner::Player1, &oracle);

    let stats1 = client.get_platform_stats();
    assert_eq!(stats1.total_matches, 1);
    assert_eq!(stats1.total_volume, 100);
    assert_eq!(stats1.total_payouts, 1);

    // Create and complete match 2
    let id2 = create_match_with_stake(&client, &env, &player1, &player2, &token, "e6feee3c", 75);
    fund_match(&client, id2, &player1, &player2);
    client.submit_result(&id2, &Winner::Player2, &oracle);

    let stats2 = client.get_platform_stats();
    assert_eq!(stats2.total_matches, 2);
    assert_eq!(stats2.total_volume, 175);
    assert_eq!(stats2.total_payouts, 2);

    // Create match 3 but don't complete it
    let _id3 = create_match_with_stake(&client, &env, &player1, &player2, &token, "b5083a27", 50);

    let stats3 = client.get_platform_stats();
    assert_eq!(stats3.total_matches, 3);
    assert_eq!(
        stats3.total_volume, 225,
        "volume includes all matches even if incomplete"
    );
    assert_eq!(
        stats3.total_payouts, 2,
        "payouts only count completed matches"
    );
}

#[test]
fn test_platform_stats_with_different_stakes() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Create matches with various stakes
    // Bronze tier caps stakes at 100, so every value here stays in range.
    let stakes: [i128; 5] = [10, 25, 50, 75, 100];
    let mut total_expected_volume = 0i128;

    for (idx, &stake) in stakes.iter().enumerate() {
        let game_id = format!("{:08x}", idx);
        let _id =
            create_match_with_stake(&client, &env, &player1, &player2, &token, &game_id, stake);
        total_expected_volume = total_expected_volume.saturating_add(stake);
    }

    let stats = client.get_platform_stats();
    assert_eq!(stats.total_matches, 5);
    assert_eq!(stats.total_volume, total_expected_volume);
    assert_eq!(stats.total_payouts, 0, "no matches completed yet");
}
