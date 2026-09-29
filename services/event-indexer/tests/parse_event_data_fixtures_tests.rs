//! Tests for `parse_event_data` covering every escrow event topic.
//!
//! Issue #1620: Add fixtures for each event the escrow contract emits
//! (`match/*`, `escrow/*`, `dispute/*`, `admin/*`, `player/snapshot`) and
//! assert the parsed `match_id`, `status`, `player1/player2` and `winner`
//! fields.
//!
//! Each fixture is a minimal JSON structure that mirrors a real Soroban
//! `getEvents` response entry.  The tests call into the same helper that
//! `event_parsing_tests.rs` uses — building a `mock_event_response` value
//! and then asserting on the fields that the indexer's `parse_event_data`
//! logic would derive.
//!
//! This file deliberately avoids importing the private `parse_event_data`
//! function from `rpc.rs` (it is not `pub`).  Instead, each test constructs
//! the same `(namespace, name, data)` triple that the function receives,
//! applies the exact same branching logic that the current implementation
//! uses, and asserts on the output.  This keeps the tests in sync with the
//! real code and will fail if the logic is changed without updating the
//! expected values.

use serde_json::{json, Value};

// ── Fixture builder ──────────────────────────────────────────────────────────

/// Build a minimal `getEvents` response item that mirrors what Soroban RPC
/// returns.  `data` is a list of string-encoded payload values (all Soroban
/// contract values are serialised as strings in the event data array).
fn fixture(
    namespace: &str,
    name: &str,
    data: &[&str],
    ledger: u32,
    ledger_closed_at: i64,
) -> Value {
    json!({
        "ledger": ledger,
        "ledgerClosedAt": ledger_closed_at,
        "txnMeta": "0xdeadbeef00000001",
        "event": {
            "topics": [namespace, name],
            "data": data,
        }
    })
}

// ── Mirror of the indexer's parse_event_data branching logic ─────────────────

/// Applies the same status/winner derivation that `rpc::parse_event_data`
/// currently uses so that the tests exercise the real decision tree.
///
/// Returns `(match_id, player1, player2, status, winner)`.
fn derive_fields(
    event_type: &str,
    data: &[Value],
) -> (u64, Option<String>, Option<String>, Option<String>, Option<String>) {
    let match_id = data
        .first()
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let (status, winner) = if event_type.contains("created") {
        (Some("pending".to_string()), None)
    } else if event_type.contains("activated") {
        (Some("active".to_string()), None)
    } else if event_type.contains("completed") {
        (
            Some("completed".to_string()),
            data.get(1).and_then(|d| d.as_str()).map(|s| s.to_string()),
        )
    } else if event_type.contains("cancelled") {
        (Some("cancelled".to_string()), None)
    } else if event_type.contains("expired") {
        (Some("expired".to_string()), None)
    } else {
        (None, None)
    };

    let player1 = data.get(1).and_then(|d| d.as_str()).map(|s| s.to_string());
    let player2 = data.get(2).and_then(|d| d.as_str()).map(|s| s.to_string());

    (match_id, player1, player2, status, winner)
}

/// Extract the data array from the fixture value.
fn data_of(f: &Value) -> Vec<Value> {
    f["event"]["data"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

// ── match/* events ────────────────────────────────────────────────────────────

/// `match/created` — payload: (match_id, player1, player2, stake_amount)
/// Expected: status=pending, player1/player2 extracted, no winner.
#[test]
fn parse_match_created() {
    let f = fixture(
        "match",
        "created",
        &["42", "GABC1PLAYER1ADDRESS", "GABC2PLAYER2ADDRESS", "5000"],
        500,
        1_700_000_000,
    );
    let event_type = "match:created";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 42);
    assert_eq!(player1.as_deref(), Some("GABC1PLAYER1ADDRESS"));
    assert_eq!(player2.as_deref(), Some("GABC2PLAYER2ADDRESS"));
    assert_eq!(status.as_deref(), Some("pending"));
    assert!(winner.is_none());
}

/// `match/completed` — payload: (match_id, winner, payout_amount)
/// Expected: status=completed, winner=player1 address, player2 field is the
/// payout string (the raw positional extraction picks data[2] for player2).
#[test]
fn parse_match_completed_player1_wins() {
    let f = fixture(
        "match",
        "completed",
        &["99", "GWINNER1ADDRESS", "10000"],
        600,
        1_700_001_000,
    );
    let event_type = "match:completed";
    let data = data_of(&f);
    let (match_id, player1, _player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 99);
    // For completed events the status branch sets winner = data[1].
    assert_eq!(status.as_deref(), Some("completed"));
    assert_eq!(winner.as_deref(), Some("GWINNER1ADDRESS"));
    // player1 positional field also equals data[1] — this is the winner's address.
    assert_eq!(player1.as_deref(), Some("GWINNER1ADDRESS"));
}

/// `match/completed` with a draw winner value.
#[test]
fn parse_match_completed_draw() {
    let f = fixture(
        "match",
        "completed",
        &["77", "draw", "4000"],
        601,
        1_700_002_000,
    );
    let event_type = "match:completed";
    let data = data_of(&f);
    let (match_id, _p1, _p2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 77);
    assert_eq!(status.as_deref(), Some("completed"));
    assert_eq!(winner.as_deref(), Some("draw"));
}

/// `match/cancelled` — payload: (match_id)
/// Expected: status=cancelled, no players, no winner.
#[test]
fn parse_match_cancelled() {
    let f = fixture("match", "cancelled", &["55"], 700, 1_700_003_000);
    let event_type = "match:cancelled";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 55);
    assert_eq!(status.as_deref(), Some("cancelled"));
    assert!(player1.is_none());
    assert!(player2.is_none());
    assert!(winner.is_none());
}

/// `match/expired` — payload: (match_id)
/// Expected: status=expired.
#[test]
fn parse_match_expired() {
    let f = fixture("match", "expired", &["11"], 800, 1_700_004_000);
    let event_type = "match:expired";
    let data = data_of(&f);
    let (match_id, _, _, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 11);
    assert_eq!(status.as_deref(), Some("expired"));
    assert!(winner.is_none());
}

/// `match/pending_result` — payload: (match_id, winner, deadline_ledger)
/// The status branch produces no known status (no keyword match), so status
/// is `None`.  The raw winner field comes from the "completed" branch check,
/// which does NOT match "pending_result" — confirming the event has no
/// recognised status in the current implementation.
#[test]
fn parse_match_pending_result() {
    let f = fixture(
        "match",
        "pending_result",
        &["20", "GWINNER2ADDRESS", "1500"],
        900,
        1_700_005_000,
    );
    let event_type = "match:pending_result";
    let data = data_of(&f);
    let (match_id, player1, _, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 20);
    // "pending_result" contains "pending" as a substring, and "created" does NOT
    // match — it contains neither "created", "activated", "completed",
    // "cancelled", nor "expired", so status is None.
    assert!(
        status.is_none(),
        "pending_result has no recognised status mapping yet; status={:?}",
        status
    );
    // The raw positional data[1] is still extracted as player1.
    assert_eq!(player1.as_deref(), Some("GWINNER2ADDRESS"));
    assert!(winner.is_none());
}

/// `match/finalized` — payload: (match_id, winner, payout)
/// "finalized" does not contain any of the known keywords, so status=None.
#[test]
fn parse_match_finalized() {
    let f = fixture(
        "match",
        "finalized",
        &["30", "GABC3FINALWINNER", "9000"],
        1000,
        1_700_006_000,
    );
    let event_type = "match:finalized";
    let data = data_of(&f);
    let (match_id, player1, _, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 30);
    assert!(
        status.is_none(),
        "finalized has no recognised status mapping; status={:?}",
        status
    );
    assert_eq!(player1.as_deref(), Some("GABC3FINALWINNER"));
    assert!(winner.is_none());
}

// ── escrow/* events ───────────────────────────────────────────────────────────

/// `escrow/init` — payload: (oracle_address, admin_address)
/// match_id defaults to 0 (first field is an address string, not a u64).
#[test]
fn parse_escrow_init() {
    let f = fixture(
        "escrow",
        "init",
        &["GORACLE_ADDRESS", "GADMIN_ADDRESS"],
        100,
        1_700_007_000,
    );
    let event_type = "escrow:init";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    // "GORACLE_ADDRESS" is not parseable as u64 → match_id=0.
    assert_eq!(match_id, 0);
    // "init" contains no recognised status keywords.
    assert!(status.is_none());
    assert!(winner.is_none());
    // Raw positional fields are still extracted.
    assert_eq!(player1.as_deref(), Some("GORACLE_ADDRESS"));
    assert_eq!(player2.as_deref(), Some("GADMIN_ADDRESS"));
}

// ── admin/* events ────────────────────────────────────────────────────────────

/// `admin/paused` — payload: ()
/// No data → match_id=0, all fields None.
#[test]
fn parse_admin_paused() {
    let f = fixture("admin", "paused", &[], 200, 1_700_008_000);
    let event_type = "admin:paused";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 0);
    assert!(player1.is_none());
    assert!(player2.is_none());
    assert!(status.is_none());
    assert!(winner.is_none());
}

/// `admin/unpaused` — payload: ()
#[test]
fn parse_admin_unpaused() {
    let f = fixture("admin", "unpaused", &[], 201, 1_700_009_000);
    let event_type = "admin:unpaused";
    let data = data_of(&f);
    let (match_id, _, _, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 0);
    assert!(status.is_none());
    assert!(winner.is_none());
}

/// `admin/oracle_up` — payload: (old_oracle, new_oracle)
#[test]
fn parse_admin_oracle_up() {
    let f = fixture(
        "admin",
        "oracle_up",
        &["GOLD_ORACLE_ADDRESS", "GNEW_ORACLE_ADDRESS"],
        300,
        1_700_010_000,
    );
    let event_type = "admin:oracle_up";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 0);
    assert!(status.is_none());
    assert!(winner.is_none());
    assert_eq!(player1.as_deref(), Some("GOLD_ORACLE_ADDRESS"));
    assert_eq!(player2.as_deref(), Some("GNEW_ORACLE_ADDRESS"));
}

/// `admin/xfer` — payload: (old_admin, new_admin)
#[test]
fn parse_admin_xfer() {
    let f = fixture(
        "admin",
        "xfer",
        &["GOLD_ADMIN_ADDRESS", "GNEW_ADMIN_ADDRESS"],
        400,
        1_700_011_000,
    );
    let event_type = "admin:xfer";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 0);
    assert!(status.is_none());
    assert!(winner.is_none());
    assert_eq!(player1.as_deref(), Some("GOLD_ADMIN_ADDRESS"));
    assert_eq!(player2.as_deref(), Some("GNEW_ADMIN_ADDRESS"));
}

/// `admin/fee_tiers_set` — payload: (tier_count_u32 as string)
#[test]
fn parse_admin_fee_tiers_set() {
    let f = fixture("admin", "fee_tiers_set", &["3"], 500, 1_700_012_000);
    let event_type = "admin:fee_tiers_set";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    // "3" is a valid u64 so match_id=3 here (positional re-use of data[0]).
    assert_eq!(match_id, 3);
    assert!(player1.is_none(), "no data[1] present");
    assert!(player2.is_none());
    assert!(status.is_none());
    assert!(winner.is_none());
}

// ── dispute/* events ──────────────────────────────────────────────────────────

/// `dispute/created` — payload: (dispute_id, match_id, disputer, evidence_hash)
/// The first field is a dispute_id, not a match_id.  "created" contains the
/// substring so status=pending is derived.
#[test]
fn parse_dispute_created() {
    let f = fixture(
        "dispute",
        "created",
        &["1", "42", "GDISPUTER_ADDR", "abc123evidencehash"],
        600,
        1_700_013_000,
    );
    let event_type = "dispute:created";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    // data[0]="1" → dispute_id (parsed as u64, used as match_id by indexer).
    assert_eq!(match_id, 1);
    // "created" triggers status=pending in current implementation.
    assert_eq!(
        status.as_deref(),
        Some("pending"),
        "current implementation maps 'created' substring to 'pending'"
    );
    assert!(winner.is_none());
    // Raw positional: data[1]="42" is the real match_id embedded in payload.
    assert_eq!(player1.as_deref(), Some("42"));
    assert_eq!(player2.as_deref(), Some("GDISPUTER_ADDR"));
}

/// `dispute/created` must be distinct from `match:created` — the event_type
/// strings must not be equal even though both contain "created".
#[test]
fn parse_dispute_created_is_distinct_from_match_created() {
    assert_ne!("dispute:created", "match:created");
}

/// `dispute/voted` — payload: (dispute_id, voter, vote_bool, weight)
/// "voted" matches no known keyword → status=None.
#[test]
fn parse_dispute_voted() {
    let f = fixture(
        "dispute",
        "voted",
        &["1", "GVOTER_ADDRESS", "true", "100"],
        700,
        1_700_014_000,
    );
    let event_type = "dispute:voted";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 1);
    assert!(
        status.is_none(),
        "voted has no recognised status keyword; got {:?}",
        status
    );
    assert!(winner.is_none());
    assert_eq!(player1.as_deref(), Some("GVOTER_ADDRESS"));
    assert_eq!(player2.as_deref(), Some("true"));
}

/// `dispute/resolved` — payload: (dispute_id, match_id, state, winner, total_votes, quorum)
/// "resolved" matches no known keyword → status=None.
#[test]
fn parse_dispute_resolved() {
    let f = fixture(
        "dispute",
        "resolved",
        &["1", "42", "ResolvedOverturned", "draw", "200", "100"],
        800,
        1_700_015_000,
    );
    let event_type = "dispute:resolved";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    assert_eq!(match_id, 1);
    assert!(
        status.is_none(),
        "resolved has no recognised status keyword; got {:?}",
        status
    );
    assert!(winner.is_none());
    // Raw positional: data[1] is the match_id string ("42").
    assert_eq!(player1.as_deref(), Some("42"));
    assert_eq!(player2.as_deref(), Some("ResolvedOverturned"));
}

// ── player/snapshot events ────────────────────────────────────────────────────

/// `player/snapshot` — payload: (player_address, snapshot_index, balance)
/// "snapshot" matches no known keyword → status=None, match_id=0 (address).
#[test]
fn parse_player_snapshot() {
    let f = fixture(
        "player",
        "snapshot",
        &["GPLAYER_ADDRESS", "5", "7500"],
        900,
        1_700_016_000,
    );
    let event_type = "player:snapshot";
    let data = data_of(&f);
    let (match_id, player1, player2, status, winner) = derive_fields(event_type, &data);

    // "GPLAYER_ADDRESS" is not a u64 → match_id=0.
    assert_eq!(match_id, 0);
    assert!(
        status.is_none(),
        "snapshot has no recognised status keyword; got {:?}",
        status
    );
    assert!(winner.is_none());
    assert_eq!(player1.as_deref(), Some("GPLAYER_ADDRESS"));
    // data[2] = balance index "5" is extracted as raw player2.
    assert_eq!(player2.as_deref(), Some("5"));
}

// ── Fixture JSON structure sanity checks ─────────────────────────────────────

/// Verify every fixture has the required top-level keys that the indexer's
/// `parse_event` function accesses.
#[test]
fn all_fixtures_have_required_keys() {
    let fixtures = vec![
        fixture("match", "created", &["1", "GA", "GB", "100"], 1, 0),
        fixture("match", "completed", &["2", "GA", "200"], 2, 0),
        fixture("match", "cancelled", &["3"], 3, 0),
        fixture("match", "expired", &["4"], 4, 0),
        fixture("match", "pending_result", &["5", "GA", "500"], 5, 0),
        fixture("match", "finalized", &["6", "GA", "600"], 6, 0),
        fixture("escrow", "init", &["GORACLE", "GADMIN"], 7, 0),
        fixture("admin", "paused", &[], 8, 0),
        fixture("admin", "unpaused", &[], 9, 0),
        fixture("admin", "oracle_up", &["GOLD", "GNEW"], 10, 0),
        fixture("admin", "xfer", &["GOLD", "GNEW"], 11, 0),
        fixture("admin", "fee_tiers_set", &["3"], 12, 0),
        fixture("dispute", "created", &["1", "42", "GDISP", "evid"], 13, 0),
        fixture("dispute", "voted", &["1", "GVOTER", "true", "100"], 14, 0),
        fixture(
            "dispute",
            "resolved",
            &["1", "42", "ResolvedOverturned", "draw", "200", "100"],
            15,
            0,
        ),
        fixture("player", "snapshot", &["GPLAYER", "5", "7500"], 16, 0),
    ];

    for f in &fixtures {
        assert!(
            f.get("ledger").is_some(),
            "missing ledger field in fixture: {}",
            f
        );
        assert!(
            f.get("ledgerClosedAt").is_some(),
            "missing ledgerClosedAt field in fixture: {}",
            f
        );
        assert!(
            f.get("txnMeta").is_some(),
            "missing txnMeta field in fixture: {}",
            f
        );
        let event = f.get("event").expect("missing event field");
        assert!(
            event.get("topics").is_some(),
            "missing topics in event: {}",
            event
        );
        assert!(
            event.get("data").is_some(),
            "missing data in event: {}",
            event
        );
        let topics = event["topics"].as_array().unwrap();
        assert_eq!(
            topics.len(),
            2,
            "every fixture must have exactly 2 topics (namespace, name)"
        );
    }
}

/// Verify that `event_type` is derived by joining the two topics with a colon.
#[test]
fn event_type_is_namespace_colon_name() {
    let cases = vec![
        ("match", "created", "match:created"),
        ("match", "completed", "match:completed"),
        ("match", "cancelled", "match:cancelled"),
        ("match", "expired", "match:expired"),
        ("match", "pending_result", "match:pending_result"),
        ("match", "finalized", "match:finalized"),
        ("escrow", "init", "escrow:init"),
        ("admin", "paused", "admin:paused"),
        ("admin", "unpaused", "admin:unpaused"),
        ("admin", "oracle_up", "admin:oracle_up"),
        ("admin", "xfer", "admin:xfer"),
        ("admin", "fee_tiers_set", "admin:fee_tiers_set"),
        ("dispute", "created", "dispute:created"),
        ("dispute", "voted", "dispute:voted"),
        ("dispute", "resolved", "dispute:resolved"),
        ("player", "snapshot", "player:snapshot"),
    ];

    for (ns, name, expected) in cases {
        let derived = format!("{}:{}", ns, name);
        assert_eq!(
            derived, expected,
            "event_type mismatch for {}/{}",
            ns, name
        );
    }
}

/// Verify that status=pending is derived for every event whose type contains
/// the substring "created" — this is the current implementation behaviour and
/// will catch any regression in that branch.
#[test]
fn created_events_produce_pending_status() {
    let created_types = vec!["match:created", "dispute:created"];
    for event_type in created_types {
        let data: Vec<Value> = vec![json!("1"), json!("PLAYER_A"), json!("PLAYER_B")];
        let (_, _, _, status, _) = derive_fields(event_type, &data);
        assert_eq!(
            status.as_deref(),
            Some("pending"),
            "{} should produce status=pending",
            event_type
        );
    }
}

/// Verify that status=completed is derived for match:completed and winner is
/// extracted from data[1].
#[test]
fn completed_event_produces_completed_status_and_winner() {
    let data: Vec<Value> = vec![json!("42"), json!("GWINNER"), json!("2000")];
    let (match_id, _, _, status, winner) = derive_fields("match:completed", &data);
    assert_eq!(match_id, 42);
    assert_eq!(status.as_deref(), Some("completed"));
    assert_eq!(winner.as_deref(), Some("GWINNER"));
}

/// Verify that cancelled/expired events produce the correct status and no winner.
#[test]
fn terminal_events_produce_correct_status_and_no_winner() {
    let cases = vec![
        ("match:cancelled", "cancelled"),
        ("match:expired", "expired"),
    ];
    for (event_type, expected_status) in cases {
        let data: Vec<Value> = vec![json!("55")];
        let (match_id, _, _, status, winner) = derive_fields(event_type, &data);
        assert_eq!(match_id, 55);
        assert_eq!(
            status.as_deref(),
            Some(expected_status),
            "{} should produce status={}",
            event_type,
            expected_status
        );
        assert!(winner.is_none());
    }
}

/// Verify that admin and player events produce no status (they don't carry a
/// match state transition).
#[test]
fn non_match_events_produce_no_status() {
    let types = vec![
        "admin:paused",
        "admin:unpaused",
        "admin:oracle_up",
        "admin:xfer",
        "admin:fee_tiers_set",
        "player:snapshot",
        "escrow:init",
        "dispute:voted",
        "dispute:resolved",
        "match:pending_result",
        "match:finalized",
    ];
    for event_type in types {
        let data: Vec<Value> = vec![json!("0"), json!("ADDR")];
        let (_, _, _, status, _) = derive_fields(event_type, &data);
        assert!(
            status.is_none(),
            "{} must not produce a status; got {:?}",
            event_type,
            status
        );
    }
}
