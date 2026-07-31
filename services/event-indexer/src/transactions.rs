//! Player transaction history: types, event→transaction mapping, and filters.
//!
//! The indexer does not store a separate `transactions` table — every financial
//! movement is already recorded as a contract event.  A "transaction" is
//! therefore a **projection of the events table**: each event whose type maps to
//! a [`TransactionType`] becomes one history row.
//!
//! ## Event → transaction mapping
//! | Transaction type | Matching event names (substring)          |
//! |------------------|-------------------------------------------|
//! | `deposit`        | `deposit`, `funded`                       |
//! | `payout`         | `completed`, `finalized`, `claim`, `payout`, `resolved` |
//! | `fee`            | `fee`, `cancelled`, `expired`             |
//!
//! Purely informational lifecycle events (`match:created`, `match:paused`,
//! `match:resumed`, `match:pending_result`, admin events, …) move no funds and
//! are therefore **excluded** from the history.  The exclusion happens in SQL so
//! that `total` and the `limit`/`offset` window stay consistent with the rows
//! returned.
//!
//! The mapping lives in exactly one place — [`TransactionType::from_event_type`]
//! for in-process classification and [`TransactionType::sql_patterns`] for the
//! equivalent SQL predicate.  The unit tests assert the two agree.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::validation::{MAX_PAGE_LIMIT, MIN_PAGE_LIMIT};

/// Default page size when the caller does not pass `limit`.
pub const DEFAULT_PAGE_LIMIT: i64 = 100;

// ── Transaction type ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    /// Stake moving from a player into escrow.
    Deposit,
    /// Escrow paying out to a player (win, draw split, refund, vested claim).
    Payout,
    /// Protocol fee taken from escrow (cancellation fee, expiry fee).
    Fee,
}

impl TransactionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransactionType::Deposit => "deposit",
            TransactionType::Payout => "payout",
            TransactionType::Fee => "fee",
        }
    }

    /// All types, in a stable order (used for the "no type filter" predicate).
    pub const ALL: [TransactionType; 3] = [
        TransactionType::Deposit,
        TransactionType::Payout,
        TransactionType::Fee,
    ];

    /// Parse a `type` / `tx_type` query value.  Case-insensitive.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "deposit" | "deposits" => Some(TransactionType::Deposit),
            "payout" | "payouts" => Some(TransactionType::Payout),
            "fee" | "fees" => Some(TransactionType::Fee),
            _ => None,
        }
    }

    /// Substrings that identify this transaction type inside an event name.
    ///
    /// Kept in one place so the Rust classifier and the SQL predicate cannot
    /// drift apart.
    pub fn event_name_markers(&self) -> &'static [&'static str] {
        match self {
            TransactionType::Deposit => &["deposit", "funded"],
            TransactionType::Payout => &["completed", "finalized", "claim", "payout", "resolved"],
            TransactionType::Fee => &["fee", "cancelled", "expired"],
        }
    }

    /// `ILIKE` patterns equivalent to [`Self::event_name_markers`].
    pub fn sql_patterns(&self) -> Vec<String> {
        self.event_name_markers()
            .iter()
            .map(|m| format!("%{}%", m))
            .collect()
    }

    /// Classify an indexed event type such as `match:deposit`.
    ///
    /// Returns `None` for events that move no funds.  Ordering matters: the
    /// first matching type wins, and the marker sets are disjoint by
    /// construction (asserted in the unit tests).
    pub fn from_event_type(event_type: &str) -> Option<Self> {
        let lowered = event_type.to_ascii_lowercase();
        Self::ALL
            .iter()
            .find(|t| t.event_name_markers().iter().any(|m| lowered.contains(m)))
            .copied()
    }

    /// `ILIKE` patterns covering every financial event type.
    pub fn all_sql_patterns() -> Vec<String> {
        Self::ALL.iter().flat_map(|t| t.sql_patterns()).collect()
    }
}

impl std::fmt::Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Sorting ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransactionSortField {
    Timestamp,
    Amount,
    MatchId,
    Type,
}

impl TransactionSortField {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "timestamp" | "date" | "time" => Some(TransactionSortField::Timestamp),
            "amount" => Some(TransactionSortField::Amount),
            "match_id" | "match" => Some(TransactionSortField::MatchId),
            "type" | "tx_type" => Some(TransactionSortField::Type),
            _ => None,
        }
    }

    /// The SQL expression to sort by.
    ///
    /// This is a **fixed, whitelisted** fragment — user input only ever selects
    /// one of these variants, it is never interpolated into the query.
    pub fn sql_expr(&self) -> &'static str {
        match self {
            TransactionSortField::Timestamp => "timestamp",
            // `stake_amount` is stored as TEXT; cast so 100 sorts below 1000.
            TransactionSortField::Amount => "NULLIF(stake_amount, '')::NUMERIC",
            TransactionSortField::MatchId => "match_id",
            TransactionSortField::Type => "event_type",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl SortOrder {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "asc" | "ascending" => Some(SortOrder::Asc),
            "desc" | "descending" => Some(SortOrder::Desc),
            _ => None,
        }
    }

    pub fn sql_keyword(&self) -> &'static str {
        match self {
            SortOrder::Asc => "ASC",
            SortOrder::Desc => "DESC",
        }
    }
}

// ── Filters ───────────────────────────────────────────────────────────────────

/// A fully validated history query.  Construction is the responsibility of the
/// API layer, which runs every field through [`crate::validation`] first.
#[derive(Clone, Debug)]
pub struct TransactionHistoryFilters {
    pub player_address: String,
    pub tx_type: Option<TransactionType>,
    pub token: Option<String>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub limit: i64,
    pub offset: i64,
    pub sort_by: TransactionSortField,
    pub sort_order: SortOrder,
}

impl TransactionHistoryFilters {
    /// Filters for a player with all defaults: newest first, first page.
    pub fn new(player_address: impl Into<String>) -> Self {
        TransactionHistoryFilters {
            player_address: player_address.into(),
            tx_type: None,
            token: None,
            from_date: None,
            to_date: None,
            limit: DEFAULT_PAGE_LIMIT,
            offset: 0,
            sort_by: TransactionSortField::Timestamp,
            sort_order: SortOrder::Desc,
        }
    }

    /// Clamp the page window into the supported range.  The API layer rejects
    /// out-of-range values with a 400 before this is reached; this is the
    /// belt-and-braces path for internal callers.
    pub fn clamped(mut self) -> Self {
        self.limit = self.limit.clamp(MIN_PAGE_LIMIT, MAX_PAGE_LIMIT);
        self.offset = self.offset.max(0);
        self
    }

    /// The `ILIKE` patterns this query should match.
    pub fn event_type_patterns(&self) -> Vec<String> {
        match self.tx_type {
            Some(t) => t.sql_patterns(),
            None => TransactionType::all_sql_patterns(),
        }
    }
}

// ── Records ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionRecord {
    pub match_id: u64,
    pub timestamp: DateTime<Utc>,
    /// Serialized as `type` — the field name required by the API contract.
    #[serde(rename = "type")]
    pub tx_type: TransactionType,
    /// Stroop amount as a decimal string (values exceed JSON's safe integer range).
    pub amount: String,
    pub token: String,
    /// Underlying event identity, so a caller can correlate with `/events`.
    pub event_id: String,
    pub event_type: String,
    pub ledger_sequence: u32,
}

/// A page of history rows plus the metadata needed to walk the rest.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TransactionPage {
    pub transactions: Vec<TransactionRecord>,
    /// Total rows matching the filters, ignoring `limit`/`offset`.
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub has_more: bool,
}

impl TransactionPage {
    pub fn new(transactions: Vec<TransactionRecord>, total: i64, limit: i64, offset: i64) -> Self {
        let has_more = offset.saturating_add(transactions.len() as i64) < total;
        TransactionPage {
            transactions,
            total,
            limit,
            offset,
            has_more,
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Event classification ──────────────────────────────────────────────

    #[test]
    fn deposit_events_are_classified_as_deposits() {
        assert_eq!(
            TransactionType::from_event_type("match:deposit"),
            Some(TransactionType::Deposit)
        );
        assert_eq!(
            TransactionType::from_event_type("match:funded"),
            Some(TransactionType::Deposit)
        );
    }

    #[test]
    fn payout_events_are_classified_as_payouts() {
        for name in [
            "match:completed",
            "match:finalized",
            "match:claim",
            "dispute:resolved",
        ] {
            assert_eq!(
                TransactionType::from_event_type(name),
                Some(TransactionType::Payout),
                "{} should be a payout",
                name
            );
        }
    }

    #[test]
    fn fee_events_are_classified_as_fees() {
        for name in ["match:cancelled", "match:expired", "match:cancel_fee"] {
            assert_eq!(
                TransactionType::from_event_type(name),
                Some(TransactionType::Fee),
                "{} should be a fee",
                name
            );
        }
    }

    #[test]
    fn non_financial_events_are_excluded() {
        for name in [
            "match:created",
            "match:paused",
            "match:resumed",
            "match:pending_result",
            "admin:init",
            "dispute:voted",
        ] {
            assert_eq!(
                TransactionType::from_event_type(name),
                None,
                "{} moves no funds and must be excluded",
                name
            );
        }
    }

    #[test]
    fn classification_is_case_insensitive() {
        assert_eq!(
            TransactionType::from_event_type("MATCH:DEPOSIT"),
            Some(TransactionType::Deposit)
        );
    }

    #[test]
    fn markers_are_disjoint_across_types() {
        // If two types shared a marker, `from_event_type` would silently prefer
        // whichever comes first in `ALL` — assert that can never happen.
        for (i, a) in TransactionType::ALL.iter().enumerate() {
            for b in TransactionType::ALL.iter().skip(i + 1) {
                for marker_a in a.event_name_markers() {
                    for marker_b in b.event_name_markers() {
                        assert!(
                            !marker_a.contains(marker_b) && !marker_b.contains(marker_a),
                            "markers {:?} and {:?} overlap",
                            marker_a,
                            marker_b
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn sql_patterns_agree_with_the_rust_classifier() {
        // Every marker must produce a pattern that matches the same event name,
        // and every classified name must be covered by `all_sql_patterns`.
        for t in TransactionType::ALL {
            for marker in t.event_name_markers() {
                let event_name = format!("match:{}", marker);
                assert_eq!(TransactionType::from_event_type(&event_name), Some(t));
                assert!(
                    t.sql_patterns().contains(&format!("%{}%", marker)),
                    "pattern for {} missing",
                    marker
                );
                assert!(TransactionType::all_sql_patterns()
                    .contains(&format!("%{}%", marker)));
            }
        }
    }

    // ── Query-value parsing ───────────────────────────────────────────────

    #[test]
    fn transaction_type_parsing_accepts_singular_and_plural() {
        assert_eq!(TransactionType::parse("deposit"), Some(TransactionType::Deposit));
        assert_eq!(TransactionType::parse("DEPOSITS"), Some(TransactionType::Deposit));
        assert_eq!(TransactionType::parse(" payout "), Some(TransactionType::Payout));
        assert_eq!(TransactionType::parse("fees"), Some(TransactionType::Fee));
    }

    #[test]
    fn unknown_transaction_type_is_rejected() {
        assert_eq!(TransactionType::parse("withdrawal"), None);
        assert_eq!(TransactionType::parse(""), None);
    }

    #[test]
    fn sort_field_and_order_parsing() {
        assert_eq!(
            TransactionSortField::parse("timestamp"),
            Some(TransactionSortField::Timestamp)
        );
        assert_eq!(
            TransactionSortField::parse("MATCH_ID"),
            Some(TransactionSortField::MatchId)
        );
        assert_eq!(TransactionSortField::parse("drop table"), None);
        assert_eq!(SortOrder::parse("ASC"), Some(SortOrder::Asc));
        assert_eq!(SortOrder::parse("desc"), Some(SortOrder::Desc));
        assert_eq!(SortOrder::parse("sideways"), None);
    }

    #[test]
    fn sort_expressions_are_fixed_fragments() {
        // Sanity: no variant can carry user input into SQL.
        for field in [
            TransactionSortField::Timestamp,
            TransactionSortField::Amount,
            TransactionSortField::MatchId,
            TransactionSortField::Type,
        ] {
            let expr = field.sql_expr();
            assert!(!expr.contains(';') && !expr.contains("--"));
        }
    }

    // ── Filters ───────────────────────────────────────────────────────────

    #[test]
    fn default_filters_are_newest_first_first_page() {
        let f = TransactionHistoryFilters::new("GABC");
        assert_eq!(f.limit, DEFAULT_PAGE_LIMIT);
        assert_eq!(f.offset, 0);
        assert_eq!(f.sort_by, TransactionSortField::Timestamp);
        assert_eq!(f.sort_order, SortOrder::Desc);
        assert!(f.tx_type.is_none());
    }

    #[test]
    fn clamping_bounds_limit_and_offset() {
        let mut f = TransactionHistoryFilters::new("GABC");
        f.limit = 10_000;
        f.offset = -5;
        let f = f.clamped();
        assert_eq!(f.limit, MAX_PAGE_LIMIT);
        assert_eq!(f.offset, 0);
    }

    #[test]
    fn unfiltered_query_still_excludes_non_financial_events() {
        let f = TransactionHistoryFilters::new("GABC");
        let patterns = f.event_type_patterns();
        assert!(patterns.contains(&"%deposit%".to_string()));
        assert!(patterns.contains(&"%completed%".to_string()));
        assert!(
            !patterns.contains(&"%created%".to_string()),
            "match:created must not be selected"
        );
    }

    #[test]
    fn type_filter_narrows_the_patterns() {
        let mut f = TransactionHistoryFilters::new("GABC");
        f.tx_type = Some(TransactionType::Deposit);
        let patterns = f.event_type_patterns();
        assert_eq!(patterns, vec!["%deposit%", "%funded%"]);
    }

    // ── Pagination metadata ───────────────────────────────────────────────

    #[test]
    fn has_more_is_true_when_rows_remain() {
        let page = TransactionPage::new(vec![], 250, 100, 0);
        assert!(page.has_more);
    }

    #[test]
    fn has_more_is_false_on_the_last_page() {
        let rows = vec![record(1), record(2)];
        let page = TransactionPage::new(rows, 202, 100, 200);
        assert!(!page.has_more, "offset 200 + 2 rows == total 202");
    }

    #[test]
    fn has_more_is_false_for_an_empty_result() {
        let page = TransactionPage::new(vec![], 0, 100, 0);
        assert!(!page.has_more);
        assert_eq!(page.total, 0);
    }

    // ── Serialization contract ────────────────────────────────────────────

    #[test]
    fn record_serializes_type_as_lowercase_type_field() {
        let json = serde_json::to_value(record(1)).unwrap();
        assert_eq!(json["type"], "deposit");
        assert_eq!(json["match_id"], 1);
        assert_eq!(json["amount"], "1000");
        assert!(json.get("tx_type").is_none(), "field must be named `type`");
    }

    fn record(match_id: u64) -> TransactionRecord {
        TransactionRecord {
            match_id,
            timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            tx_type: TransactionType::Deposit,
            amount: "1000".to_string(),
            token: "XLM".to_string(),
            event_id: format!("evt-{}", match_id),
            event_type: "match:deposit".to_string(),
            ledger_sequence: 100,
        }
    }
}
