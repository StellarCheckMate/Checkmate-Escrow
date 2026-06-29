/// Integration tests: full match lifecycle (create → deposit → result → payout)
use super::*;

/// Full lifecycle — player1 wins: both deposit, oracle submits Player1 win,
/// winner receives the full pot and escrow balance drops to zero.
#[test]
fn test_full_lifecycle_winner_payout() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = token_client(&env, &token);

    let stake: i128 = 100;
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "integration_winner_game"),
        &Platform::Lichess,
    );

    // Both players deposit — match becomes Active
    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    assert!(client.is_funded(&match_id));
    assert_eq!(client.get_escrow_balance(&match_id), stake * 2);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    // Oracle submits result — player1 wins
    client.submit_result(&match_id, &Winner::Player1);

    // Escrow must be zero after payout
    assert_eq!(client.get_escrow_balance(&match_id), 0);

    // Winner received the full pot; loser balance unchanged
    assert_eq!(tc.balance(&player1), p1_before + stake * 2);
    assert_eq!(tc.balance(&player2), p2_before);

    // Match is in terminal Completed state
    let m = client.get_match(&match_id);
    assert_eq!(m.state, MatchState::Completed);
}

/// Full lifecycle — draw: both deposit, oracle submits Draw, each player gets
/// their stake back and escrow balance drops to zero.
#[test]
fn test_full_lifecycle_draw_refund() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let tc = token_client(&env, &token);

    let stake: i128 = 100;
    let match_id = client.create_match(
        &player1,
        &player2,
        &stake,
        &token,
        &String::from_str(&env, "integration_draw_game"),
        &Platform::Lichess,
    );

    client.deposit(&match_id, &player1);
    client.deposit(&match_id, &player2);

    let p1_before = tc.balance(&player1);
    let p2_before = tc.balance(&player2);

    // Oracle submits draw
    client.submit_result(&match_id, &Winner::Draw);

    // Each player gets their stake back
    assert_eq!(tc.balance(&player1), p1_before + stake);
    assert_eq!(tc.balance(&player2), p2_before + stake);

    // Escrow is empty and match is Completed
    assert_eq!(client.get_escrow_balance(&match_id), 0);
    assert_eq!(client.get_match(&match_id).state, MatchState::Completed);
}
