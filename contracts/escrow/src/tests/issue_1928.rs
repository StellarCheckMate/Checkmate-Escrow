//! Tests for issue #1928: `set_match_timeout` rejects a timeout shorter than
//! the minimum heartbeat interval.
//!
//! The match expiration timeout must stay at or above the heartbeat/rollback
//! window (`MIN_HEARTBEAT_INTERVAL_SECONDS`); otherwise a match could expire
//! before a player's liveness window has even elapsed, which is inconsistent
//! with the heartbeat mechanism.
use super::*;

#[test]
fn test_set_match_timeout_rejects_timeout_below_heartbeat_interval() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // A timeout one second below the minimum heartbeat interval is
    // inconsistent — the match could expire before the heartbeat/rollback
    // window elapses — and must be rejected with InvalidTimeout.
    let too_short = MIN_HEARTBEAT_INTERVAL_SECONDS - 1;
    let result = client.try_set_match_timeout(&too_short);
    assert_eq!(
        result,
        Err(Ok(Error::InvalidTimeout)),
        "timeout shorter than the minimum heartbeat interval must be rejected"
    );

    // The heartbeat interval is the heartbeat/rollback window itself.
    assert_eq!(MIN_HEARTBEAT_INTERVAL_SECONDS, ROLLBACK_WINDOW_SECONDS);
}

#[test]
fn test_set_match_timeout_accepts_heartbeat_interval_boundary() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // A timeout exactly equal to the minimum heartbeat interval is the floor:
    // the interval must not be undercut, but equality is allowed.
    client.set_match_timeout(&MIN_HEARTBEAT_INTERVAL_SECONDS);
    assert_eq!(
        client.get_match_timeout(),
        MIN_HEARTBEAT_INTERVAL_SECONDS,
        "timeout equal to the heartbeat interval must be accepted and stored"
    );
}
