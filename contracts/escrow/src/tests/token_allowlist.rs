use super::*;

#[test]
fn test_is_token_allowed_returns_false_for_unknown_tokens() {
    let (env, contract_id, _oracle, _player1, _player2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let unknown_token = Address::generate(&env);
    let result = client.is_token_allowed(&unknown_token);
    assert!(!result, "unknown token should not be allowed");
}
