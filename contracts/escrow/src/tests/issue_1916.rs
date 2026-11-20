//! Tests for issue #1916: `submit_draw` must require both depositors'
//! agreement before settling, so a single party (or the oracle alone) cannot
//! force a draw payout.
//!
//! Flow:
//! 1. Both depositors call `agree_to_draw` while the match is `Active`.
//! 2. Only once both flags are set may the oracle call `submit_draw` to settle.
//!
//! Before the fix, `submit_draw` settled immediately on the oracle's call with
//! no depositor agreement, so a single party could force a draw payout.
use super::*;

// ── submit_draw requires both agreements ─────────────────────────────────────

#[test]
fn test_submit_draw_rejected_without_any_agreement() {
    let (env, contract_id, oracle, _player1, _player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Neither depositor has agreed — the oracle alone cannot force a draw.
    let result = client.try_submit_draw(&match_id, &oracle, &None);
    assert_eq!(
        result,
        Err(Ok(Error::DrawAgreementRequired)),
        "submit_draw must be rejected until both depositors agree"
    );
}

#[test]
fn test_submit_draw_rejected_with_only_one_agreement() {
    let (env, contract_id, oracle, player1, _player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Only player1 agrees — a unilateral draw submission is still rejected.
    client.agree_to_draw(&match_id, &player1);
    let result = client.try_submit_draw(&match_id, &oracle, &None);
    assert_eq!(
        result,
        Err(Ok(Error::DrawAgreementRequired)),
        "submit_draw must be rejected when only one depositor has agreed"
    );
}

#[test]
fn test_submit_draw_settles_after_both_depositors_agree() {
    let (env, contract_id, oracle, player1, player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Both depositors agree, then the oracle may settle the draw.
    client.agree_to_draw(&match_id, &player1);
    client.agree_to_draw(&match_id, &player2);

    let result = client.try_submit_draw(&match_id, &oracle, &None);
    assert!(
        result.is_ok(),
        "submit_draw must succeed once both depositors have agreed"
    );

    // Default dispute period is 0, so the match settles immediately.
    let m = client.get_match(&match_id);
    assert_eq!(m.winner, Winner::Draw, "match must be settled as a draw");
    assert_eq!(m.state, MatchState::Completed, "match must be completed");
}

// ── agree_to_draw access control ─────────────────────────────────────────────

#[test]
fn test_agree_to_draw_rejects_non_participant() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    // A stranger cannot agree to a draw on someone else's match.
    let stranger = Address::generate(&env);
    let result = client.try_agree_to_draw(&match_id, &stranger);
    assert!(
        result.is_err(),
        "only match participants may agree to a draw"
    );
}

#[test]
fn test_agree_to_draw_rejects_inactive_match() {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // A match that is still Pending (not yet funded) is not Active, so a draw
    // agreement is not meaningful yet.
    let match_id = client.create_match(
        &player1,
        &player2,
        &100,
        &token,
        &String::from_str(&env, "9a771e03"),
        &Platform::Lichess,
    );
    let result = client.try_agree_to_draw(&match_id, &player1);
    assert!(
        result.is_err(),
        "agree_to_draw must be rejected while the match is not yet Active"
    );
}
