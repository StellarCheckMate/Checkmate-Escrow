/// Tests for issue #1944: get_oracle_rotation_history view.
use super::*;

/// When no rotations have been performed the history is empty.
#[test]
fn oracle_rotation_history_is_empty_initially() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let history = client.get_oracle_rotation_history();
    assert_eq!(history.len(), 0, "history must be empty before any rotation");
}

/// A direct oracle update via update_oracle is recorded in history.
#[test]
fn update_oracle_is_recorded_in_history() {
    let (env, contract_id, old_oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_oracle = Address::generate(&env);
    client.update_oracle(&new_oracle);

    let history = client.get_oracle_rotation_history();
    assert_eq!(history.len(), 1, "one entry must be recorded");

    let entry = history.get(0).unwrap();
    assert_eq!(entry.old_oracle, old_oracle, "old_oracle must match");
    assert_eq!(entry.new_oracle, new_oracle, "new_oracle must match");
    assert_eq!(
        entry.kind,
        String::from_str(&env, "direct"),
        "kind must be 'direct'"
    );
}

/// A temporary oracle rotation is recorded with kind 'temporary'.
#[test]
fn temporary_oracle_rotation_is_recorded_in_history() {
    let (env, contract_id, old_oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let temp_oracle = Address::generate(&env);
    // duration: 1 hour (minimum allowed)
    client.rotate_oracle_temporary(&old_oracle, &temp_oracle, &3600u64);

    let history = client.get_oracle_rotation_history();
    assert_eq!(history.len(), 1, "one entry must be recorded");

    let entry = history.get(0).unwrap();
    assert_eq!(entry.old_oracle, old_oracle);
    assert_eq!(entry.new_oracle, temp_oracle);
    assert_eq!(
        entry.kind,
        String::from_str(&env, "temporary"),
        "kind must be 'temporary'"
    );
}

/// A permanent oracle rotation is recorded with kind 'permanent'.
#[test]
fn permanent_oracle_rotation_is_recorded_in_history() {
    let (env, contract_id, old_oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let new_oracle = Address::generate(&env);
    client.propose_oracle_rotation(&old_oracle, &new_oracle);
    client.rotate_oracle_permanent(&old_oracle, &new_oracle);

    let history = client.get_oracle_rotation_history();
    assert_eq!(history.len(), 1, "one entry must be recorded");

    let entry = history.get(0).unwrap();
    assert_eq!(entry.old_oracle, old_oracle);
    assert_eq!(entry.new_oracle, new_oracle);
    assert_eq!(
        entry.kind,
        String::from_str(&env, "permanent"),
        "kind must be 'permanent'"
    );
}

/// Multiple rotations accumulate in history, oldest first.
#[test]
fn multiple_rotations_accumulate_in_history_oldest_first() {
    let (env, contract_id, oracle1, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let oracle2 = Address::generate(&env);
    let oracle3 = Address::generate(&env);

    client.update_oracle(&oracle2);
    client.update_oracle(&oracle3);

    let history = client.get_oracle_rotation_history();
    assert_eq!(history.len(), 2, "two entries must be recorded");

    let first = history.get(0).unwrap();
    assert_eq!(first.old_oracle, oracle1, "first entry old oracle");
    assert_eq!(first.new_oracle, oracle2, "first entry new oracle");

    let second = history.get(1).unwrap();
    assert_eq!(second.old_oracle, oracle2, "second entry old oracle");
    assert_eq!(second.new_oracle, oracle3, "second entry new oracle");
}
