/// Tests for issue #1941: get_total_escrowed global view.
use super::*;

/// When no matches exist the total should be zero.
#[test]
fn get_total_escrowed_returns_zero_with_no_matches() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    assert_eq!(client.get_total_escrowed(), 0);
}

/// After one player deposits the escrowed total should equal one stake.
#[test]
fn get_total_escrowed_one_deposit_equals_one_stake() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "aabbccdd"),
        &Platform::Lichess,
    );
    client.deposit(&match_id, &player1);

    // Only player1 deposited; Pending match with one deposit → 100 tokens escrowed.
    assert_eq!(client.get_total_escrowed(), 100);
}

/// The total across two fully-funded active matches equals the sum of their
/// per-match escrow balances.
#[test]
fn get_total_escrowed_sums_across_multiple_active_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    // Mint extra tokens so both players can fund two matches.
    let asset = StellarAssetClient::new(&env, &token);
    asset.mint(&player1, &1000);
    asset.mint(&player2, &1000);

    let mid1 = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "11111111"),
        &Platform::Lichess,
    );
    client.deposit(&mid1, &player1);
    client.deposit(&mid1, &player2);

    let mid2 = client.create_match(
        &player1,
        &player2,
        &200,
        &token,
        &String::from_str(&env, "22222222"),
        &Platform::Lichess,
    );
    client.deposit(&mid2, &player1);
    client.deposit(&mid2, &player2);

    // match 1 contributes 200 (2 × 100), match 2 contributes 400 (2 × 200).
    assert_eq!(client.get_total_escrowed(), 600);

    // After settling match 1 the total should drop to 400.
    client.submit_result(&mid1, &Winner::Player1, &oracle);
    client.claim_vested_payout(&mid1, &player1);
    assert_eq!(client.get_total_escrowed(), 400);
}

/// Once a match is completed (payout executed) it no longer contributes to
/// the total — the escrow for that match is zero.
#[test]
fn get_total_escrowed_excludes_completed_matches() {
    let (env, contract_id, oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1,
        &player2,
        &50,
        &token,
        &String::from_str(&env, "33333333"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    client.deposit(&mid, &player2);

    // Before settlement: 100 tokens locked.
    assert_eq!(client.get_total_escrowed(), 100);

    client.submit_result(&mid, &Winner::Draw, &oracle);
    client.claim_vested_payout(&mid, &player1);
    client.claim_vested_payout(&mid, &player2);

    // After settlement: 0 tokens locked.
    assert_eq!(client.get_total_escrowed(), 0);
}

/// Cancelled matches are excluded from the total.
#[test]
fn get_total_escrowed_excludes_cancelled_matches() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mid = client.create_match(
        &player1,
        &player2,
        &75,
        &token,
        &String::from_str(&env, "44444444"),
        &Platform::Lichess,
    );
    client.deposit(&mid, &player1);
    // Before cancellation player1's stake is in escrow.
    assert_eq!(client.get_total_escrowed(), 75);

    client.cancel_match(&mid, &player1);
    assert_eq!(client.get_total_escrowed(), 0);
}
