/// Tests for issues #1335 (deposit_batch) and #1337 (max_protocol_fee cap).
use super::*;

// ── Issue #1335: deposit_batch ────────────────────────────────────────────────

#[test]
fn test_deposit_batch_all_valid() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_a = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_aa1b"),
        &Platform::Lichess,
    );
    let match_b = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_bb2c"),
        &Platform::Lichess,
    );

    let entries: soroban_sdk::Vec<(u64, Address)> = soroban_sdk::vec![
        &env,
        (match_a, player1.clone()),
        (match_b, player1.clone()),
    ];

    let results = client.deposit_batch(&entries);
    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap(), None);
    assert_eq!(results.get(1).unwrap(), None);

    assert!(client.get_match(&match_a).player1_deposited);
    assert!(client.get_match(&match_b).player1_deposited);
}

#[test]
fn test_deposit_batch_mixed_valid_and_invalid() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_mix1"),
        &Platform::Lichess,
    );

    // Second entry references a non-existent match.
    let entries: soroban_sdk::Vec<(u64, Address)> = soroban_sdk::vec![
        &env,
        (match_id, player1.clone()),
        (9999u64, player1.clone()),
    ];

    let results = client.deposit_batch(&entries);
    assert_eq!(results.len(), 2);
    assert_eq!(results.get(0).unwrap(), None);
    assert_eq!(results.get(1).unwrap(), Some(Error::MatchNotFound));

    // Valid entry was still processed.
    assert!(client.get_match(&match_id).player1_deposited);
}

#[test]
fn test_deposit_batch_already_funded_entry_fails_independently() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_dup1"),
        &Platform::Lichess,
    );
    let match_id2 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_dup2"),
        &Platform::Lichess,
    );

    // Pre-deposit player1 into match_id so the batch entry is a duplicate.
    client.deposit(&match_id, &player1);

    let entries: soroban_sdk::Vec<(u64, Address)> = soroban_sdk::vec![
        &env,
        (match_id, player1.clone()),   // duplicate — should fail
        (match_id2, player1.clone()),  // fresh — should succeed
    ];

    let results = client.deposit_batch(&entries);
    assert_eq!(results.get(0).unwrap(), Some(Error::AlreadyFunded));
    assert_eq!(results.get(1).unwrap(), None);
}

#[test]
fn test_deposit_batch_returns_contract_paused_when_paused() {
    let (env, contract_id, _oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "batch_pause"),
        &Platform::Lichess,
    );

    client.pause(&admin);

    let entries: soroban_sdk::Vec<(u64, Address)> = soroban_sdk::vec![
        &env,
        (match_id, player1.clone()),
    ];

    let result = client.try_deposit_batch(&entries);
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

// ── Issue #1337: max_protocol_fee cap ────────────────────────────────────────

fn setup_with_fee(fee_bps: u32, max_fee: Option<i128>) -> (
    Env, Address, Address, Address, Address, Address, Address,
) {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: fee_bps,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: max_fee,
        dispute_bond_tier_schedule: soroban_sdk::vec![&env],
    });
    (env, contract_id, oracle, player1, player2, token, admin)
}

#[test]
fn test_fee_cap_limits_large_fee() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup_with_fee(1000, Some(50));
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &900);
    asset.mint(&player2, &900);

    let match_id = client.create_match(
        &player1, &player2, &1000, &token,
        &String::from_str(&env, "cap_test1"), &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let tc = TokenClient::new(&env, &token);
    let p1_before = tc.balance(&player1);

    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    // pot=2000, calculated_fee=200 (10%), capped at 50 → winner gets 1950
    assert_eq!(tc.balance(&player1) - p1_before, 1950);
    assert_eq!(tc.balance(&admin), 50);
}

#[test]
fn test_fee_no_cap_full_percentage_deducted() {
    let (env, contract_id, oracle, player1, player2, token, admin) = setup_with_fee(500, None);
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &900);
    asset.mint(&player2, &900);

    let match_id = client.create_match(
        &player1, &player2, &1000, &token,
        &String::from_str(&env, "nocap_test1"), &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let tc = TokenClient::new(&env, &token);
    let p1_before = tc.balance(&player1);

    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    // pot=2000, fee=100 (5%), no cap → winner gets 1900
    assert_eq!(tc.balance(&player1) - p1_before, 1900);
    assert_eq!(tc.balance(&admin), 100);
}

#[test]
fn test_fee_cap_larger_than_calculated_has_no_effect() {
    // When the cap is higher than the calculated fee, the calculated fee is used.
    let (env, contract_id, oracle, player1, player2, token, admin) = setup_with_fee(100, Some(1000));
    let client = EscrowContractClient::new(&env, &contract_id);
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &900);
    asset.mint(&player2, &900);

    let match_id = client.create_match(
        &player1, &player2, &1000, &token,
        &String::from_str(&env, "highcap_test"), &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let tc = TokenClient::new(&env, &token);
    let p1_before = tc.balance(&player1);

    client.submit_result(&match_id, &Winner::Player1, &oracle);
    client.claim_vested_payout(&match_id, &player1);

    // pot=2000, fee=20 (1%), cap=1000 → cap doesn't bind, winner gets 1980
    assert_eq!(tc.balance(&player1) - p1_before, 1980);
    assert_eq!(tc.balance(&admin), 20);
}

#[test]
fn test_protocol_config_stores_and_retrieves_max_protocol_fee() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_protocol_config(&ProtocolConfig {
        vesting_duration_seconds: 0,
        cancellation_fee_basis_points: 0,
        treasury: admin.clone(),
        stablecoin_only_mode: false,
        maximum_stake: None,
        match_timeout_seconds: DEFAULT_MATCH_TIMEOUT_SECONDS,
        protocol_fee_bps: 200,
        fee_recipient: admin.clone(),
        minimum_stake: DEFAULT_MINIMUM_STAKE,
        max_protocol_fee: Some(75),
        dispute_bond_tier_schedule: soroban_sdk::vec![&env],
    });

    let config = client.get_protocol_config();
    assert_eq!(config.max_protocol_fee, Some(75));
    assert_eq!(config.protocol_fee_bps, 200);
}
