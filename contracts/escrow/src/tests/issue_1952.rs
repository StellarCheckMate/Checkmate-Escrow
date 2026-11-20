//! Tests for issue #1952: dispute_oracle_result rejects a second open dispute
//! on the same match.
//!
//! The single-open-dispute cap (the griefing guard) was not covered by tests
//! for the case where a second dispute is explicitly opened, nor for the case
//! where a dispute can be opened on a fresh match after the first one resolves.
#![cfg(test)]

use super::*;
use soroban_sdk::testutils::Ledger as _;

const TEST_EVIDENCE_HASH: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn evidence(env: &Env) -> String {
    String::from_str(env, TEST_EVIDENCE_HASH)
}

/// Create a match and have both players deposit so it is in `Active` state.
fn funded_match(
    client: &EscrowContractClient,
    env: &Env,
    player1: &Address,
    player2: &Address,
    token: &Address,
    game_id: &str,
) -> u64 {
    let id = client.create_match(
        player1,
        player2,
        &100,
        token,
        &String::from_str(env, game_id),
        &Platform::Lichess,
    );
    client.deposit(&id, player1);
    client.deposit(&id, player2);
    id
}

// ── second open dispute rejected ──────────────────────────────────────────────

/// When a match already has an open (Active) dispute, a second call to
/// `dispute_oracle_result` on the same match must be rejected with
/// `DisputeAlreadyRaised`, regardless of which player makes the call.
#[test]
fn test_dispute_oracle_result_rejects_second_open_dispute_on_same_match() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    let match_id = funded_match(&client, &env, &player1, &player2, &token, "disc1111");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match_id, &Winner::Player1, &oracle);

    // First dispute — succeeds.
    client.dispute_oracle_result(&match_id, &player1, &evidence(&env));

    // Second dispute on the same match, from the other player — must be rejected.
    let result = client.try_dispute_oracle_result(&match_id, &player2, &evidence(&env));
    assert_eq!(
        result,
        Err(Ok(Error::DisputeAlreadyRaised)),
        "a second open dispute on the same match must be rejected"
    );

    // Also rejected if the same player tries again.
    let result2 = client.try_dispute_oracle_result(&match_id, &player1, &evidence(&env));
    assert_eq!(
        result2,
        Err(Ok(Error::DisputeAlreadyRaised)),
        "the original disputer must also be blocked from opening a second dispute"
    );
}

// ── dispute on a fresh match after first resolves ─────────────────────────────

/// After the first match's dispute is resolved and that match is complete,
/// a brand-new match can have its own dispute opened without interference.
/// This verifies the dispute guard is per-match and not a global flag.
#[test]
fn test_dispute_can_be_opened_on_new_match_after_first_dispute_resolves() {
    let (env, contract_id, oracle, player1, player2, token, _admin) =
        setup_with_dispute_period(200);
    let client = EscrowContractClient::new(&env, &contract_id);

    // ── Match 1: open a dispute and resolve it ────────────────────────────
    let match1 = funded_match(&client, &env, &player1, &player2, &token, "disc2222");

    env.ledger().set_sequence_number(1000);
    client.submit_result(&match1, &Winner::Player1, &oracle);

    let dispute1 = client.dispute_oracle_result(&match1, &player2, &evidence(&env));

    // Vote to overturn so the match completes via the dispute path.
    client.vote_on_dispute(&dispute1, &player2, &true);

    // Advance past the voting deadline to allow resolution.
    env.ledger()
        .set_sequence_number(1000 + VOTING_PERIOD_LEDGERS);
    client.resolve_dispute_by_vote(&dispute1);

    // Match 1 is now Completed.
    assert_eq!(
        client.get_match(&match1).state,
        MatchState::Completed,
        "match 1 should be completed after dispute resolution"
    );

    // ── Match 2: opening a dispute must succeed ───────────────────────────
    // The StellarAssetClient needs to mint tokens for both players again since
    // their balances were consumed by match 1.
    let asset_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
    asset_client.mint(&player1, &200);
    asset_client.mint(&player2, &200);

    // Create and fund a second, independent match.
    let match2 = funded_match(&client, &env, &player1, &player2, &token, "disc3333");

    // Advance ledger so the dispute period is still open.
    let submit_ledger = 1000 + VOTING_PERIOD_LEDGERS + 1;
    env.ledger().set_sequence_number(submit_ledger);
    client.submit_result(&match2, &Winner::Player1, &oracle);

    // The dispute window on match 2 is `submit_ledger` to `submit_ledger + dispute_period`.
    // We stay inside that window.
    let result = client.try_dispute_oracle_result(&match2, &player2, &evidence(&env));
    assert!(
        result.is_ok(),
        "a dispute on a fresh match must succeed after the previous match's dispute was resolved"
    );
}
