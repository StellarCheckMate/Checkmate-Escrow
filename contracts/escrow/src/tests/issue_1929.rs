//! #1929 — `set_maximum_stake(None)` removes the cap without checking against
//! `minimum_stake`.
//!
//! When `set_maximum_stake` is called with `Some(max)` the new cap must be
//! ≥ `minimum_stake`; otherwise the \[min, max\] range would be empty and no
//! stake would ever be accepted.  Calling it with `None` is always valid —
//! removing the cap gives an unlimited upper bound that is always ≥ any
//! finite minimum.

use super::*;

/// A `maximum_stake` below `minimum_stake` creates an empty valid range and
/// must be rejected with [`Error::InvalidAmount`].
#[test]
fn test_set_maximum_stake_below_minimum_stake_is_rejected() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Set minimum to 100 first.
    client.set_minimum_stake(&100_i128);

    // Any maximum below that minimum must be rejected.
    let result = client.try_set_maximum_stake(&Some(50_i128));
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "set_maximum_stake(Some(50)) with minimum_stake=100 must return InvalidAmount"
    );
}

/// A `maximum_stake` equal to `minimum_stake` is the degenerate single-value
/// range; it must be accepted (only that exact stake is valid).
#[test]
fn test_set_maximum_stake_equal_to_minimum_stake_is_accepted() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_minimum_stake(&50_i128);

    // max == min is a valid (if narrow) range.
    client.set_maximum_stake(&Some(50_i128));

    let cfg = client.get_protocol_config().unwrap();
    assert_eq!(cfg.stakes.maximum_stake, Some(50_i128));
}

/// A `maximum_stake` strictly above `minimum_stake` is the normal case and
/// must always be accepted.
#[test]
fn test_set_maximum_stake_above_minimum_stake_is_accepted() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    client.set_minimum_stake(&10_i128);
    client.set_maximum_stake(&Some(200_i128));

    let cfg = client.get_protocol_config().unwrap();
    assert_eq!(cfg.stakes.maximum_stake, Some(200_i128));
}

/// Calling `set_maximum_stake(None)` with a non-trivial `minimum_stake` must
/// succeed because an unlimited cap is always consistent with any floor.
/// This is the core regression for issue #1929.
#[test]
fn test_set_maximum_stake_none_is_always_valid_regardless_of_minimum_stake() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Raise minimum so the config is non-trivial.
    client.set_minimum_stake(&500_i128);

    // Removing the cap (None) must still succeed: unlimited upper bound ≥ 500.
    client.set_maximum_stake(&None);

    let cfg = client.get_protocol_config().unwrap();
    assert_eq!(
        cfg.stakes.maximum_stake, None,
        "maximum_stake must be None after set_maximum_stake(None)"
    );
    assert_eq!(
        cfg.stakes.minimum_stake, 500_i128,
        "minimum_stake must be unchanged after removing the cap"
    );
}

/// Raising `minimum_stake` above an already-set `maximum_stake` through
/// `set_minimum_stake` is separately guarded, but attempting the same via
/// `set_maximum_stake` with a cap below the floor must still be rejected.
#[test]
fn test_maximum_stake_validation_uses_current_minimum_stake() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Start with max > min.
    client.set_minimum_stake(&100_i128);
    client.set_maximum_stake(&Some(500_i128));

    // Now try to lower the cap below the existing minimum — must fail.
    let result = client.try_set_maximum_stake(&Some(99_i128));
    assert_eq!(
        result,
        Err(Ok(Error::InvalidAmount)),
        "maximum_stake of 99 must be rejected when minimum_stake is 100"
    );

    // Verify the config was not modified.
    let cfg = client.get_protocol_config().unwrap();
    assert_eq!(cfg.stakes.maximum_stake, Some(500_i128), "cap must be unchanged after rejected call");
}
