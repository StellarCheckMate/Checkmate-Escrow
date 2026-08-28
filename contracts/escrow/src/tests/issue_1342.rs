//! Tests for issue #1342: get_allowed_tokens_paginated
use super::*;

#[test]
fn test_get_allowed_tokens_paginated_with_25_tokens() {
    let (env, contract_id, _oracle, _p1, _p2, _token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let mut tokens = std::vec::Vec::new();
    for _ in 0..25 {
        let addr = Address::generate(&env);
        client.add_allowed_token(&addr);
        tokens.push(addr);
    }

    // First page: offset=0, limit=10
    let page0 = client.get_allowed_tokens_paginated(&0, &10);
    assert_eq!(page0.len(), 10);
    for i in 0..10u32 {
        assert_eq!(page0.get(i).unwrap(), tokens[i as usize]);
    }

    // Second page: offset=10, limit=10
    let page1 = client.get_allowed_tokens_paginated(&10, &10);
    assert_eq!(page1.len(), 10);
    for i in 0..10u32 {
        assert_eq!(page1.get(i).unwrap(), tokens[10 + i as usize]);
    }

    // Third page: offset=20, limit=10 — only 5 remain
    let page2 = client.get_allowed_tokens_paginated(&20, &10);
    assert_eq!(page2.len(), 5);

    // Offset beyond end returns empty
    let empty = client.get_allowed_tokens_paginated(&30, &10);
    assert_eq!(empty.len(), 0);

    // Zero limit returns empty
    let zero = client.get_allowed_tokens_paginated(&0, &0);
    assert_eq!(zero.len(), 0);
}
