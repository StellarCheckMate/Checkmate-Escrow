//! #1722 — `set_minimum_stake` / `set_maximum_stake` bound `create_match`.
//!
//! Bounds sit inside the Bronze tier range (1..=100) so the tier check never
//! masks the bounds check for fresh players.
use super::*;

const MIN_STAKE: i128 = 10;
const MAX_STAKE: i128 = 50;

fn setup_with_bounds() -> (Env, Address, Address, Address, Address) {
    let (env, contract_id, _oracle, player1, player2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.set_minimum_stake(&MIN_STAKE);
    client.set_maximum_stake(&Some(MAX_STAKE));
    (env, contract_id, player1, player2, token)
}

fn try_create(
    env: &Env,
    contract_id: &Address,
    player1: &Address,
    player2: &Address,
    token: &Address,
    stake: i128,
    game_id: &str,
) -> Result<u64, Error> {
    EscrowContractClient::new(env, contract_id)
        .try_create_match(
            player1,
            player2,
            &stake,
            token,
            &String::from_str(env, game_id),
            &Platform::Lichess,
        )
        .map(|id| id.unwrap())
        .map_err(|e| e.unwrap())
}

#[test]
fn test_stake_below_minimum_returns_invalid_amount() {
    let (env, contract_id, player1, player2, token) = setup_with_bounds();

    assert_eq!(
        try_create(&env, &contract_id, &player1, &player2, &token, MIN_STAKE - 1, "1722aa01"),
        Err(Error::InvalidAmount)
    );
}

#[test]
fn test_stake_above_maximum_returns_invalid_amount() {
    let (env, contract_id, player1, player2, token) = setup_with_bounds();

    assert_eq!(
        try_create(&env, &contract_id, &player1, &player2, &token, MAX_STAKE + 1, "1722aa02"),
        Err(Error::InvalidAmount)
    );
}

#[test]
fn test_stake_exactly_at_minimum_is_accepted() {
    let (env, contract_id, player1, player2, token) = setup_with_bounds();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = try_create(&env, &contract_id, &player1, &player2, &token, MIN_STAKE, "1722aa03")
        .expect("stake equal to minimum_stake must be accepted");

    assert_eq!(client.get_match(&id).stake_amount, MIN_STAKE);
}

#[test]
fn test_stake_exactly_at_maximum_is_accepted() {
    let (env, contract_id, player1, player2, token) = setup_with_bounds();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = try_create(&env, &contract_id, &player1, &player2, &token, MAX_STAKE, "1722aa04")
        .expect("stake equal to maximum_stake must be accepted");

    assert_eq!(client.get_match(&id).stake_amount, MAX_STAKE);
}

#[test]
fn test_single_value_range_accepts_only_that_stake() {
    let (env, contract_id, player1, player2, token) = setup_with_bounds();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.set_maximum_stake(&Some(MIN_STAKE));

    assert!(try_create(&env, &contract_id, &player1, &player2, &token, MIN_STAKE, "1722aa05").is_ok());
    assert_eq!(
        try_create(&env, &contract_id, &player1, &player2, &token, MIN_STAKE + 1, "1722aa06"),
        Err(Error::InvalidAmount)
    );
}

#[test]
fn test_clearing_maximum_stake_removes_upper_bound() {
    let (env, contract_id, player1, player2, token) = setup_with_bounds();
    let client = EscrowContractClient::new(&env, &contract_id);
    client.set_maximum_stake(&None);

    assert!(
        try_create(&env, &contract_id, &player1, &player2, &token, MAX_STAKE + 1, "1722aa07").is_ok(),
        "with no maximum, only the tier cap applies"
    );
}
