//! Transaction-history tests: filtering, pagination, sorting and response shape.
//!
//! ## Two layers
//! - **Filter construction** (always runs): the query-parameter → filter
//!   conversion, including every rejection, needs no I/O.
//! - **SQL behaviour** (needs `DATABASE_URL`): filtering, pagination and sorting
//!   are implemented in SQL, so they are exercised against a real PostgreSQL.
//!   These tests follow the convention in `integration_tests.rs` and skip
//!   themselves when `DATABASE_URL` is unset.
//!
//! Each database-backed test uses its **own player address** so the suite can
//! run in parallel against a shared database without tests seeing each other's
//! rows.

use chrono::{DateTime, Duration as ChronoDuration, Utc};

use event_indexer::api::{build_transaction_filters, TransactionHistoryQuery};
use event_indexer::db::Database;
use event_indexer::models::IndexedEvent;
use event_indexer::transactions::{
    SortOrder, TransactionHistoryFilters, TransactionPage, TransactionSortField, TransactionType,
    DEFAULT_PAGE_LIMIT,
};

// ── Test players (checksum-valid, one per database-backed test) ───────────────

const PLAYER_MIXED: &str = "GAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQDZ7H";
const PLAYER_TYPES: &str = "GABAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEJXA";
const PLAYER_TOKENS: &str = "GABQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQHGPC";
const PLAYER_DATES: &str = "GACAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAJJHP";
const PLAYER_PAGES: &str = "GACQKBIFAUCQKBIFAUCQKBIFAUCQKBIFAUCQKBIFAUCQKBIFAUCQKG7N";
const PLAYER_SORT: &str = "GADAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDANWXK";
const PLAYER_ISOLATED: &str = "GADQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOBYHA4DQOZPI";
const PLAYER_FIELDS: &str = "GAEQSCIJBEEQSCIJBEEQSCIJBEEQSCIJBEEQSCIJBEEQSCIJBEEQSH7S";
const PLAYER_REORG: &str = "GAFAUCQKBIFAUCQKBIFAUCQKBIFAUCQKBIFAUCQKBIFAUCQKBIFAVXXV";
const PLAYER_UNRELATED: &str = "GAFQWCYLBMFQWCYLBMFQWCYLBMFQWCYLBMFQWCYLBMFQWCYLBMFQWYPX";
/// Never seeded by any test — used to prove an empty history is a success.
const PLAYER_WITH_NO_HISTORY: &str =
    "GAGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYDAMBQGAYXH2";
const OPPONENT: &str = "GAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEARIHQ";

// ─────────────────────────────────────────────────────────────────────────────
// Part 1 — filter construction (no I/O)
// ─────────────────────────────────────────────────────────────────────────────

fn query() -> TransactionHistoryQuery {
    TransactionHistoryQuery::default()
}

#[test]
fn defaults_are_applied_when_no_parameters_are_given() {
    let filters = build_transaction_filters(PLAYER_MIXED, &query()).unwrap();

    assert_eq!(filters.player_address, PLAYER_MIXED);
    assert_eq!(filters.limit, DEFAULT_PAGE_LIMIT);
    assert_eq!(filters.offset, 0);
    assert_eq!(filters.sort_by, TransactionSortField::Timestamp);
    assert_eq!(filters.sort_order, SortOrder::Desc);
    assert!(filters.tx_type.is_none());
    assert!(filters.token.is_none());
    assert!(filters.from_date.is_none() && filters.to_date.is_none());
}

#[test]
fn every_supported_parameter_is_parsed() {
    let q = TransactionHistoryQuery {
        from_date: Some("2026-01-01".to_string()),
        to_date: Some("2026-12-31T23:59:59Z".to_string()),
        limit: Some("25".to_string()),
        offset: Some("50".to_string()),
        tx_type: Some("payout".to_string()),
        token: Some("XLM".to_string()),
        sort_by: Some("amount".to_string()),
        sort_order: Some("asc".to_string()),
    };

    let filters = build_transaction_filters(PLAYER_MIXED, &q).unwrap();

    assert_eq!(filters.limit, 25);
    assert_eq!(filters.offset, 50);
    assert_eq!(filters.tx_type, Some(TransactionType::Payout));
    assert_eq!(filters.token.as_deref(), Some("XLM"));
    assert_eq!(filters.sort_by, TransactionSortField::Amount);
    assert_eq!(filters.sort_order, SortOrder::Asc);
    assert_eq!(
        filters.from_date.unwrap().to_rfc3339(),
        "2026-01-01T00:00:00+00:00"
    );
    assert!(filters.to_date.is_some());
}

#[test]
fn blank_parameter_values_fall_back_to_defaults() {
    let q = TransactionHistoryQuery {
        limit: Some("   ".to_string()),
        tx_type: Some("".to_string()),
        ..Default::default()
    };

    let filters = build_transaction_filters(PLAYER_MIXED, &q).unwrap();
    assert_eq!(filters.limit, DEFAULT_PAGE_LIMIT);
    assert!(filters.tx_type.is_none());
}

#[test]
fn invalid_player_address_is_rejected() {
    let err = build_transaction_filters("PLAYER_ONE", &query()).unwrap_err();
    assert_eq!(err.field, "player_address");
}

#[test]
fn each_invalid_parameter_is_reported_against_its_own_field() {
    let cases: Vec<(TransactionHistoryQuery, &str)> = vec![
        (
            TransactionHistoryQuery {
                limit: Some("0".to_string()),
                ..Default::default()
            },
            "limit",
        ),
        (
            TransactionHistoryQuery {
                limit: Some("1001".to_string()),
                ..Default::default()
            },
            "limit",
        ),
        (
            TransactionHistoryQuery {
                offset: Some("-1".to_string()),
                ..Default::default()
            },
            "offset",
        ),
        (
            TransactionHistoryQuery {
                tx_type: Some("withdrawal".to_string()),
                ..Default::default()
            },
            "type",
        ),
        (
            TransactionHistoryQuery {
                token: Some("XLM'; --".to_string()),
                ..Default::default()
            },
            "token",
        ),
        (
            TransactionHistoryQuery {
                from_date: Some("yesterday".to_string()),
                ..Default::default()
            },
            "from_date",
        ),
        (
            TransactionHistoryQuery {
                sort_by: Some("stake_amount; DROP".to_string()),
                ..Default::default()
            },
            "sort_by",
        ),
        (
            TransactionHistoryQuery {
                sort_order: Some("random".to_string()),
                ..Default::default()
            },
            "sort_order",
        ),
    ];

    for (q, expected_field) in cases {
        let err = build_transaction_filters(PLAYER_MIXED, &q)
            .expect_err("must be rejected")
            .field;
        assert_eq!(err, expected_field);
    }
}

#[test]
fn inverted_date_range_is_rejected() {
    let q = TransactionHistoryQuery {
        from_date: Some("2026-12-31".to_string()),
        to_date: Some("2026-01-01".to_string()),
        ..Default::default()
    };
    let err = build_transaction_filters(PLAYER_MIXED, &q).unwrap_err();
    assert_eq!(err.field, "from_date");
    assert!(err.message.contains("to_date"));
}

#[test]
fn page_metadata_is_derived_from_the_total() {
    let page = TransactionPage::new(vec![], 250, 100, 0);
    assert_eq!(page.total, 250);
    assert_eq!(page.limit, 100);
    assert_eq!(page.offset, 0);
    assert!(page.has_more);

    let last = TransactionPage::new(vec![], 100, 100, 100);
    assert!(!last.has_more);
}

// ─────────────────────────────────────────────────────────────────────────────
// Part 2 — SQL behaviour (requires DATABASE_URL)
// ─────────────────────────────────────────────────────────────────────────────

/// Connect and ensure the schema exists, or return `None` when no database is
/// configured so the test can skip.
async fn database() -> Option<Database> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let db = Database::from_dsns(&url, &url, 2, 2).expect("DSN must parse");
    db.init_schema().await.expect("schema");
    Some(db)
}

fn event(
    id: &str,
    match_id: u64,
    event_type: &str,
    player: &str,
    amount: &str,
    token: &str,
    timestamp: DateTime<Utc>,
) -> IndexedEvent {
    IndexedEvent {
        id: id.to_string(),
        ledger_sequence: match_id as u32,
        match_id,
        event_type: event_type.to_string(),
        player1: Some(player.to_string()),
        player2: Some(OPPONENT.to_string()),
        status: None,
        winner: None,
        stake_amount: Some(amount.to_string()),
        token: Some(token.to_string()),
        game_id: Some("abcd1234".to_string()),
        platform: Some("lichess".to_string()),
        timestamp,
        txn_hash: Some(format!("tx-{}", id)),
        event_index_in_txn: Some(0),
        reorg_invalidated_at: None,
    }
}

async fn seed(db: &Database, events: &[IndexedEvent]) {
    for e in events {
        db.insert_event(e).await.expect("insert");
    }
}

/// Remove the rows a test inserted, so repeated local runs stay deterministic.
async fn cleanup(db: &Database, ids: &[&str]) {
    let conn = db.write_pool().get().await.expect("pool");
    for id in ids {
        let _ = conn
            .execute("DELETE FROM events WHERE id = $1", &[id])
            .await;
    }
}

#[tokio::test]
async fn history_excludes_non_financial_events() {
    let Some(db) = database().await else {
        println!("Skipping history_excludes_non_financial_events: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = [
        "txh-mixed-created",
        "txh-mixed-deposit",
        "txh-mixed-completed",
        "txh-mixed-paused",
    ];
    seed(
        &db,
        &[
            event(ids[0], 7101, "match:created", PLAYER_MIXED, "1000", "XLM", now),
            event(ids[1], 7101, "match:deposit", PLAYER_MIXED, "1000", "XLM", now),
            event(ids[2], 7101, "match:completed", PLAYER_MIXED, "2000", "XLM", now),
            event(ids[3], 7101, "match:paused", PLAYER_MIXED, "1000", "XLM", now),
        ],
    )
    .await;

    let (rows, total) = db
        .query_player_transactions(&TransactionHistoryFilters::new(PLAYER_MIXED))
        .await
        .expect("query");

    assert_eq!(total, 2, "only the deposit and the payout move funds");
    assert_eq!(rows.len(), 2);
    let types: Vec<TransactionType> = rows.iter().map(|r| r.tx_type).collect();
    assert!(types.contains(&TransactionType::Deposit));
    assert!(types.contains(&TransactionType::Payout));

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_returns_the_documented_fields() {
    let Some(db) = database().await else {
        println!("Skipping history_returns_the_documented_fields: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = ["txh-fields-deposit"];
    seed(
        &db,
        &[event(
            ids[0], 7102, "match:deposit", PLAYER_FIELDS, "12345", "USDC", now,
        )],
    )
    .await;

    let filters = TransactionHistoryFilters::new(PLAYER_FIELDS);
    let (rows, _) = db.query_player_transactions(&filters).await.expect("query");

    let row = rows.first().expect("one row");
    assert_eq!(row.match_id, 7102);
    assert_eq!(row.tx_type, TransactionType::Deposit);
    assert_eq!(row.amount, "12345");
    assert_eq!(row.token, "USDC");
    assert_eq!(row.event_id, ids[0]);
    assert!(row.timestamp <= Utc::now());

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_filters_by_transaction_type() {
    let Some(db) = database().await else {
        println!("Skipping history_filters_by_transaction_type: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = [
        "txh-types-d1",
        "txh-types-d2",
        "txh-types-p1",
        "txh-types-f1",
    ];
    seed(
        &db,
        &[
            event(ids[0], 7111, "match:deposit", PLAYER_TYPES, "100", "XLM", now),
            event(ids[1], 7112, "match:deposit", PLAYER_TYPES, "200", "XLM", now),
            event(ids[2], 7113, "match:completed", PLAYER_TYPES, "300", "XLM", now),
            event(ids[3], 7114, "match:cancelled", PLAYER_TYPES, "400", "XLM", now),
        ],
    )
    .await;

    for (tx_type, expected) in [
        (TransactionType::Deposit, 2),
        (TransactionType::Payout, 1),
        (TransactionType::Fee, 1),
    ] {
        let mut filters = TransactionHistoryFilters::new(PLAYER_TYPES);
        filters.tx_type = Some(tx_type);
        let (rows, total) = db.query_player_transactions(&filters).await.expect("query");

        assert_eq!(total, expected, "{tx_type} count");
        assert!(
            rows.iter().all(|r| r.tx_type == tx_type),
            "{tx_type} filter leaked other types"
        );
    }

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_filters_by_token() {
    let Some(db) = database().await else {
        println!("Skipping history_filters_by_token: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = ["txh-token-xlm", "txh-token-usdc"];
    seed(
        &db,
        &[
            event(ids[0], 7121, "match:deposit", PLAYER_TOKENS, "100", "XLM", now),
            event(ids[1], 7122, "match:deposit", PLAYER_TOKENS, "100", "USDC", now),
        ],
    )
    .await;

    let mut filters = TransactionHistoryFilters::new(PLAYER_TOKENS);
    filters.token = Some("USDC".to_string());
    let (rows, total) = db.query_player_transactions(&filters).await.expect("query");

    assert_eq!(total, 1);
    assert_eq!(rows[0].token, "USDC");

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_filters_by_date_range() {
    let Some(db) = database().await else {
        println!("Skipping history_filters_by_date_range: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let old = now - ChronoDuration::days(30);
    let ids = ["txh-date-old", "txh-date-new"];
    seed(
        &db,
        &[
            event(ids[0], 7131, "match:deposit", PLAYER_DATES, "100", "XLM", old),
            event(ids[1], 7132, "match:deposit", PLAYER_DATES, "200", "XLM", now),
        ],
    )
    .await;

    // Only the recent one.
    let mut recent = TransactionHistoryFilters::new(PLAYER_DATES);
    recent.from_date = Some(now - ChronoDuration::days(1));
    let (rows, total) = db.query_player_transactions(&recent).await.expect("query");
    assert_eq!(total, 1);
    assert_eq!(rows[0].match_id, 7132);

    // Only the old one.
    let mut historical = TransactionHistoryFilters::new(PLAYER_DATES);
    historical.to_date = Some(now - ChronoDuration::days(1));
    let (rows, total) = db
        .query_player_transactions(&historical)
        .await
        .expect("query");
    assert_eq!(total, 1);
    assert_eq!(rows[0].match_id, 7131);

    // A window covering both.
    let mut both = TransactionHistoryFilters::new(PLAYER_DATES);
    both.from_date = Some(old - ChronoDuration::days(1));
    both.to_date = Some(now + ChronoDuration::days(1));
    let (_, total) = db.query_player_transactions(&both).await.expect("query");
    assert_eq!(total, 2);

    // A window covering neither.
    let mut empty = TransactionHistoryFilters::new(PLAYER_DATES);
    empty.from_date = Some(now + ChronoDuration::days(1));
    let (rows, total) = db.query_player_transactions(&empty).await.expect("query");
    assert_eq!(total, 0);
    assert!(rows.is_empty());

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_paginates_without_gaps_or_repeats() {
    let Some(db) = database().await else {
        println!("Skipping history_paginates_without_gaps_or_repeats: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let owned_ids: Vec<String> = (0..5).map(|i| format!("txh-page-{i}")).collect();
    let ids: Vec<&str> = owned_ids.iter().map(String::as_str).collect();

    let events: Vec<IndexedEvent> = (0..5)
        .map(|i| {
            event(
                ids[i],
                7140 + i as u64,
                "match:deposit",
                PLAYER_PAGES,
                &((i + 1) * 100).to_string(),
                "XLM",
                now - ChronoDuration::minutes(i as i64),
            )
        })
        .collect();
    seed(&db, &events).await;

    // Walk the whole history two rows at a time.
    let mut seen: Vec<u64> = Vec::new();
    let mut offset = 0i64;
    loop {
        let mut filters = TransactionHistoryFilters::new(PLAYER_PAGES);
        filters.limit = 2;
        filters.offset = offset;
        filters.sort_by = TransactionSortField::MatchId;
        filters.sort_order = SortOrder::Asc;

        let (rows, total) = db.query_player_transactions(&filters).await.expect("query");
        assert_eq!(total, 5, "total must not change while paging");

        let page = TransactionPage::new(rows, total, filters.limit, filters.offset);
        assert!(page.transactions.len() <= 2, "limit must be respected");
        seen.extend(page.transactions.iter().map(|r| r.match_id));

        if !page.has_more {
            break;
        }
        offset += 2;
    }

    assert_eq!(seen.len(), 5, "every row seen exactly once");
    let unique: std::collections::HashSet<u64> = seen.iter().copied().collect();
    assert_eq!(unique.len(), 5, "no row returned twice");
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "ordering held across pages");

    // An offset past the end is an empty page, not an error.
    let mut past_end = TransactionHistoryFilters::new(PLAYER_PAGES);
    past_end.offset = 500;
    let (rows, total) = db.query_player_transactions(&past_end).await.expect("query");
    assert!(rows.is_empty());
    assert_eq!(total, 5, "total still reports the full history");

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_sorts_by_amount_and_timestamp_in_both_directions() {
    let Some(db) = database().await else {
        println!("Skipping history_sorts_by_amount_and_timestamp: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = ["txh-sort-small", "txh-sort-large", "txh-sort-mid"];
    seed(
        &db,
        &[
            // Amounts chosen so a *text* sort would disagree with a numeric one:
            // "1000" < "90" as text, but 1000 > 90 numerically.
            event(ids[0], 7151, "match:deposit", PLAYER_SORT, "90", "XLM", now - ChronoDuration::minutes(2)),
            event(ids[1], 7152, "match:deposit", PLAYER_SORT, "1000", "XLM", now - ChronoDuration::minutes(1)),
            event(ids[2], 7153, "match:deposit", PLAYER_SORT, "500", "XLM", now),
        ],
    )
    .await;

    let amounts = |rows: &[event_indexer::transactions::TransactionRecord]| -> Vec<i64> {
        rows.iter().map(|r| r.amount.parse().unwrap()).collect()
    };

    let mut asc = TransactionHistoryFilters::new(PLAYER_SORT);
    asc.sort_by = TransactionSortField::Amount;
    asc.sort_order = SortOrder::Asc;
    let (rows, _) = db.query_player_transactions(&asc).await.expect("query");
    assert_eq!(
        amounts(&rows),
        vec![90, 500, 1000],
        "amounts must sort numerically, not lexicographically"
    );

    let mut desc = TransactionHistoryFilters::new(PLAYER_SORT);
    desc.sort_by = TransactionSortField::Amount;
    desc.sort_order = SortOrder::Desc;
    let (rows, _) = db.query_player_transactions(&desc).await.expect("query");
    assert_eq!(amounts(&rows), vec![1000, 500, 90]);

    // Default sort: newest first.
    let (rows, _) = db
        .query_player_transactions(&TransactionHistoryFilters::new(PLAYER_SORT))
        .await
        .expect("query");
    assert_eq!(rows[0].match_id, 7153, "newest first by default");

    let mut oldest = TransactionHistoryFilters::new(PLAYER_SORT);
    oldest.sort_order = SortOrder::Asc;
    let (rows, _) = db.query_player_transactions(&oldest).await.expect("query");
    assert_eq!(rows[0].match_id, 7151, "oldest first when asked");

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn history_matches_both_sides_of_a_match_but_not_other_players() {
    let Some(db) = database().await else {
        println!("Skipping history_matches_both_sides_of_a_match: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = ["txh-iso-as-p1", "txh-iso-as-p2", "txh-iso-other"];

    let mut as_player2 = event(
        ids[1], 7162, "match:deposit", OPPONENT, "100", "XLM", now,
    );
    as_player2.player2 = Some(PLAYER_ISOLATED.to_string());

    let mut unrelated = event(
        ids[2], 7163, "match:deposit", OPPONENT, "100", "XLM", now,
    );
    unrelated.player2 = Some(PLAYER_UNRELATED.to_string());

    seed(
        &db,
        &[
            event(ids[0], 7161, "match:deposit", PLAYER_ISOLATED, "100", "XLM", now),
            as_player2,
            unrelated,
        ],
    )
    .await;

    let (rows, total) = db
        .query_player_transactions(&TransactionHistoryFilters::new(PLAYER_ISOLATED))
        .await
        .expect("query");

    assert_eq!(total, 2, "the player counts as player1 and as player2");
    let match_ids: Vec<u64> = rows.iter().map(|r| r.match_id).collect();
    assert!(match_ids.contains(&7161) && match_ids.contains(&7162));
    assert!(
        !match_ids.contains(&7163),
        "another player's transaction must not leak"
    );

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn reorg_invalidated_events_are_excluded() {
    let Some(db) = database().await else {
        println!("Skipping reorg_invalidated_events_are_excluded: DATABASE_URL not set");
        return;
    };

    let now = Utc::now();
    let ids = ["txh-reorg-valid", "txh-reorg-rolled-back"];

    let mut rolled_back = event(
        ids[1], 7172, "match:deposit", PLAYER_REORG, "100", "XLM", now,
    );
    rolled_back.reorg_invalidated_at = Some(now);

    seed(
        &db,
        &[
            event(ids[0], 7171, "match:deposit", PLAYER_REORG, "100", "XLM", now),
            rolled_back,
        ],
    )
    .await;

    let (rows, total) = db
        .query_player_transactions(&TransactionHistoryFilters::new(PLAYER_REORG))
        .await
        .expect("query");

    assert_eq!(total, 1, "a rolled-back ledger never moved funds");
    assert_eq!(rows[0].match_id, 7171);

    cleanup(&db, &ids).await;
}

#[tokio::test]
async fn empty_history_is_a_successful_empty_page() {
    let Some(db) = database().await else {
        println!("Skipping empty_history_is_a_successful_empty_page: DATABASE_URL not set");
        return;
    };

    // A valid address that no test ever seeds.
    let (rows, total) = db
        .query_player_transactions(&TransactionHistoryFilters::new(PLAYER_WITH_NO_HISTORY))
        .await
        .expect("query must succeed, not error");

    assert!(rows.is_empty());
    assert_eq!(total, 0);
    let page = TransactionPage::new(rows, total, 100, 0);
    assert!(!page.has_more);
}
