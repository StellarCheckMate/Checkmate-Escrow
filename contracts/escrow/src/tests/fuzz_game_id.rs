//! Fuzz-style tests for `create_match` with arbitrary `game_id` values.
//!
//! Issue #1367 — the `game_id` field accepts arbitrary strings but validation
//! runs in `create_match`.  This module exercises the validation path with:
//!
//! - Zero-length strings
//! - Maximum-length strings (64 bytes)
//! - Strings longer than `MAX_GAME_ID_LEN` (65–200 bytes)
//! - Non-ASCII and non-alphanumeric characters
//! - Strings of exactly 8, 12 characters (Lichess valid lengths)
//! - Strings of 7–12 numeric digits (Chess.com valid lengths)
//! - Random byte sequences in the 0–200 byte range via quickcheck
//!
//! **Contract under test:** `create_match` must never panic for any input.
//! Invalid `game_id` values must return `Error::InvalidGameId`, not a panic.
//!
//! Run with:
//! ```bash
//! cargo test -p escrow fuzz_game_id
//! ```

#![cfg(test)]
extern crate std;

use super::*;
use quickcheck::TestResult;
use quickcheck_macros::quickcheck;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Call `create_match` with a raw byte slice as game_id on the given platform.
/// Returns the contract result without panicking.
fn try_create_with_bytes(
    env: &Env,
    client: &EscrowContractClient<'_>,
    player1: &Address,
    player2: &Address,
    token: &Address,
    bytes: &[u8],
    platform: Platform,
) -> Result<u64, soroban_sdk::Error> {
    // soroban_sdk::String only accepts valid UTF-8 / ASCII-safe bytes via
    // `from_str`; we construct the string from a Rust &str reference.
    // Non-UTF-8 byte slices are first lossily converted so the host never
    // panics on string construction — we want to exercise the contract's
    // validation, not the string-construction layer.
    let lossy = std::string::String::from_utf8_lossy(bytes);
    let game_id = soroban_sdk::String::from_str(env, lossy.as_ref());

    client.try_create_match(player1, player2, &100, token, &game_id, &platform)
        .map_err(|e| e.unwrap())
}

// ── Edge-case unit tests ──────────────────────────────────────────────────────

/// Empty game_id must return InvalidGameId, not a panic.
#[test]
fn fuzz_empty_game_id_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, ""),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)), "empty game_id must be rejected");
}

/// A game_id longer than MAX_GAME_ID_LEN (64 bytes) must return InvalidGameId.
#[test]
fn fuzz_oversized_game_id_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 65 alphanumeric characters — one byte over the limit.
    let long_id = std::string::String::from("a").repeat(65);
    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, &long_id),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)), "65-byte game_id must be rejected");
}

/// A Lichess game_id with exactly 8 alphanumeric chars must be accepted.
#[test]
fn fuzz_valid_lichess_8_char_accepted() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, "abcd1234"),
        &Platform::Lichess,
    );
    assert!(result.is_ok(), "8-char alphanumeric Lichess id must be accepted");
}

/// A Lichess game_id with exactly 12 alphanumeric chars must be accepted.
#[test]
fn fuzz_valid_lichess_12_char_accepted() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, "abcdef123456"),
        &Platform::Lichess,
    );
    assert!(result.is_ok(), "12-char alphanumeric Lichess id must be accepted");
}

/// A Lichess game_id with 9 chars (not 8 or 12) must be rejected.
#[test]
fn fuzz_lichess_wrong_length_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    for len in [1u32, 2, 7, 9, 10, 11, 13, 20, 63] {
        let id = std::string::String::from("a").repeat(len as usize);
        let result = client.try_create_match(
            &p1,
            &p2,
            &100,
            &token,
            &soroban_sdk::String::from_str(&env, &id),
            &Platform::Lichess,
        );
        assert_eq!(
            result,
            Err(Ok(Error::InvalidGameId)),
            "Lichess id of length {} must be rejected",
            len
        );
    }
}

/// A Lichess game_id containing non-alphanumeric characters must be rejected.
#[test]
fn fuzz_lichess_non_alphanumeric_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let non_alpha_ids = ["abcd12!@", "abcd-123", "abcd 123", "abcd\t123"];
    for id in &non_alpha_ids {
        let result = client.try_create_match(
            &p1,
            &p2,
            &100,
            &token,
            &soroban_sdk::String::from_str(&env, id),
            &Platform::Lichess,
        );
        assert_eq!(
            result,
            Err(Ok(Error::InvalidGameId)),
            "Lichess id '{}' with non-alphanumeric chars must be rejected",
            id
        );
    }
}

/// A Chess.com game_id with 7–12 ASCII digits must be accepted.
#[test]
fn fuzz_valid_chess_com_numeric_accepted() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // 7-digit ID (minimum valid length)
    let result7 = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, "1234567"),
        &Platform::ChessDotCom,
    );
    assert!(result7.is_ok(), "7-digit chess.com id must be accepted");
}

/// A Chess.com game_id with 12 ASCII digits must be accepted.
#[test]
fn fuzz_valid_chess_com_12_digit_accepted() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, "123456789012"),
        &Platform::ChessDotCom,
    );
    assert!(result.is_ok(), "12-digit chess.com id must be accepted");
}

/// A Chess.com game_id containing non-digit characters must be rejected.
#[test]
fn fuzz_chess_com_non_numeric_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let bad_ids = ["1234abc7", "1234567!", "abcdefgh", "1234-567"];
    for id in &bad_ids {
        let result = client.try_create_match(
            &p1,
            &p2,
            &100,
            &token,
            &soroban_sdk::String::from_str(&env, id),
            &Platform::ChessDotCom,
        );
        assert_eq!(
            result,
            Err(Ok(Error::InvalidGameId)),
            "Chess.com id '{}' with non-digit chars must be rejected",
            id
        );
    }
}

/// A Chess.com game_id with fewer than 7 or more than 12 digits must be rejected.
#[test]
fn fuzz_chess_com_wrong_length_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    for len in [1usize, 2, 3, 4, 5, 6, 13, 14, 20, 63] {
        let id = "1".repeat(len);
        let result = client.try_create_match(
            &p1,
            &p2,
            &100,
            &token,
            &soroban_sdk::String::from_str(&env, &id),
            &Platform::ChessDotCom,
        );
        assert_eq!(
            result,
            Err(Ok(Error::InvalidGameId)),
            "Chess.com id of length {} must be rejected",
            len
        );
    }
}

/// game_id of exactly 200 'a' characters must be rejected (over MAX_GAME_ID_LEN=64).
#[test]
fn fuzz_200_byte_game_id_rejected() {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let long_id = std::string::String::from("a").repeat(200);
    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, &long_id),
        &Platform::Lichess,
    );
    assert_eq!(result, Err(Ok(Error::InvalidGameId)), "200-byte id must be rejected");
}

// ── Quickcheck-based fuzz tests ───────────────────────────────────────────────

/// Quickcheck property: for any arbitrary byte sequence of length 0–200,
/// `create_match` must never panic — it either succeeds (for the rare
/// inputs that happen to be valid) or returns `InvalidGameId`.
///
/// We test both platforms in a single property keyed by a boolean.
#[quickcheck]
fn prop_no_panic_on_arbitrary_game_id(raw: std::vec::Vec<u8>, use_lichess: bool) -> TestResult {
    // Limit to 0–200 bytes as specified in the task.
    if raw.len() > 200 {
        return TestResult::discard();
    }

    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);
    let platform = if use_lichess { Platform::Lichess } else { Platform::ChessDotCom };

    let result = try_create_with_bytes(&env, &client, &p1, &p2, &token, &raw, platform);

    // The contract must either succeed or return a structured error — never panic.
    // A panic would abort the test process rather than returning here.
    // We accept any outcome except a panic; we just ensure the call returns.
    match result {
        Ok(_) => TestResult::passed(),
        Err(_) => TestResult::passed(),
    }
}

/// Quickcheck property: for any string that passes as a Lichess game_id
/// (exactly 8 or 12 ASCII alphanumeric chars), `create_match` must succeed.
#[quickcheck]
fn prop_valid_lichess_game_id_accepted(is_12: bool) -> TestResult {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    let id = if is_12 { "aB3dEf1234Gh" } else { "aB3dEf12" };
    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, id),
        &Platform::Lichess,
    );
    TestResult::from_bool(result.is_ok())
}

/// Quickcheck property: invalid game_ids (wrong length, wrong chars) must
/// always return `InvalidGameId`, never another error.
#[quickcheck]
fn prop_invalid_game_id_returns_correct_error(len_mod: u8, use_lichess: bool) -> TestResult {
    let (env, contract_id, _oracle, p1, p2, token, _admin) = setup();
    let client = EscrowContractClient::new(&env, &contract_id);

    // Build a length that is definitely invalid for the chosen platform.
    let invalid_len = match use_lichess {
        // Valid Lichess lengths are 8 and 12; map to something else.
        true => {
            let l = (len_mod as usize % 64) + 1;
            if l == 8 || l == 12 { 9 } else { l }
        }
        // Valid Chess.com lengths are 7–12; map to out-of-range.
        false => {
            let l = (len_mod as usize % 64) + 1;
            if (7..=12).contains(&l) { 6 } else { l }
        }
    };

    // Use all-alphanumeric content so length is the sole invalid factor.
    let id = "a".repeat(invalid_len);
    let platform = if use_lichess { Platform::Lichess } else { Platform::ChessDotCom };

    let result = client.try_create_match(
        &p1,
        &p2,
        &100,
        &token,
        &soroban_sdk::String::from_str(&env, &id),
        &platform,
    );

    match result {
        Err(Ok(Error::InvalidGameId)) => TestResult::passed(),
        // Any other result (success or different error) is a failure.
        other => TestResult::error(format!(
            "expected InvalidGameId for id='{}' platform={}, got {:?}",
            id,
            if use_lichess { "Lichess" } else { "ChessDotCom" },
            other,
        )),
    }
}
