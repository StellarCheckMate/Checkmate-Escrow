//! Tests for Issue #1955 — submit_draw rejects a draw submitted by a non-depositor
//!
//! `submit_draw` is restricted to the registered oracle address. Any caller
//! that is not the oracle (i.e. a "non-depositor" — someone not authorized to
//! report results) must be rejected with `Unauthorized`.

use super::*;

// ── Rejection: non-oracle (non-depositor) caller ──────────────────────────────

/// A draw submitted by an arbitrary address that is not the registered oracle
/// must be rejected with `Unauthorized`.
#[test]
fn test_submit_draw_rejects_non_oracle_caller() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let non_oracle = Address::generate(&env);
    let result = client.try_submit_draw(&match_id, &non_oracle, &None);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "submit_draw must reject any caller that is not the registered oracle"
    );
}

/// A draw submitted by player1 (a match depositor but not the oracle) must
/// also be rejected — only the oracle address is authorised to report results.
#[test]
fn test_submit_draw_rejects_player_depositor_caller() {
    let (env, contract_id, _oracle, player1, _p2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_draw(&match_id, &player1, &None);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "submit_draw must reject player1 (a depositor) who is not the oracle"
    );
}

/// A draw submitted by the admin address must also be rejected — the admin
/// has no special authority to report match results.
#[test]
fn test_submit_draw_rejects_admin_caller() {
    let (env, contract_id, _oracle, _p1, _p2, _token, admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_draw(&match_id, &admin, &None);
    assert_eq!(
        result,
        Err(Ok(Error::Unauthorized)),
        "submit_draw must reject the admin address (not the oracle)"
    );
}

// ── Acceptance: oracle (authorized depositor) caller ─────────────────────────

/// A draw submitted by the registered oracle must succeed.
#[test]
fn test_submit_draw_accepts_registered_oracle() {
    let (env, contract_id, oracle, _p1, _p2, _token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_submit_draw(&match_id, &oracle, &None);
    assert!(
        result.is_ok(),
        "submit_draw must succeed when called by the registered oracle: {:?}",
        result
    );
}

/// After a successful `submit_draw`, the match transitions to `Completed`
/// and the escrow balance is zero (both players refunded their stake).
#[test]
fn test_submit_draw_completes_match_and_refunds_both_players() {
    let (env, contract_id, oracle, player1, player2, token, _admin, match_id) =
        setup_with_funded_match();
    let client = EscrowContractClient::new(&env, &contract_id);
    let token_client = TokenClient::new(&env, &token);

    client.submit_draw(&match_id, &oracle, &None);

    let m = client.get_match(&match_id);
    assert_eq!(
        m.state,
        MatchState::Completed,
        "match must be Completed after a draw"
    );
    assert_eq!(
        client.get_escrow_balance(&match_id),
        0,
        "escrow must be empty after a draw"
    );
    assert_eq!(
        token_client.balance(&player1),
        1000,
        "player1 must be fully refunded on a draw"
    );
    assert_eq!(
        token_client.balance(&player2),
        1000,
        "player2 must be fully refunded on a draw"
    );
}
