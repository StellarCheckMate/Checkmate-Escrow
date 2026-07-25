//! Tool to identify duplicate events in the database.
//!
//! Duplicates are identified by grouping events with identical
//! (ledger_sequence, txn_hash, event_type, match_id).
//!
//! Usage:
//!   cargo run --bin identify-duplicates
//!
//! Requires DATABASE_URL environment variable.

use anyhow::Result;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");

    let pool = event_indexer::db::build_pool(&db_url, 5)?;
    let conn = pool.get().await?;

    println!("Scanning for duplicate events...\n");

    let rows = conn
        .query(
            r#"
            SELECT
              ledger_sequence,
              txn_hash,
              event_type,
              match_id,
              COUNT(*) as duplicate_count,
              ARRAY_AGG(id ORDER BY timestamp ASC) as all_ids,
              ARRAY_AGG(timestamp ORDER BY timestamp ASC) as timestamps,
              MIN(timestamp) as oldest_timestamp
            FROM events
            WHERE reorg_invalidated_at IS NULL
            GROUP BY ledger_sequence, txn_hash, event_type, match_id
            HAVING COUNT(*) > 1
            ORDER BY duplicate_count DESC
            "#,
            &[],
        )
        .await?;

    if rows.is_empty() {
        println!("✓ No duplicates found! Database is clean.");
        return Ok(());
    }

    let mut total_duplicates = 0i64;
    let mut total_extra_rows = 0i64;

    println!("{:<6} {:<20} {:<40} {:<15} {:<35}", "Count", "Ledger", "Tx Hash", "Event Type", "Match ID");
    println!("{}", "─".repeat(135));

    for row in &rows {
        let dup_count: i64 = row.get(4);
        let ledger: i32 = row.get(0);
        let tx_hash: Option<String> = row.get(1);
        let event_type: String = row.get(2);
        let match_id: i64 = row.get(3);

        let tx_display = tx_hash
            .as_ref()
            .map(|h| {
                if h.len() > 40 {
                    format!("{}...", &h[..37])
                } else {
                    h.clone()
                }
            })
            .unwrap_or_else(|| "NULL".to_string());

        let event_display = if event_type.len() > 35 {
            format!("{}...", &event_type[..32])
        } else {
            event_type.clone()
        };

        println!("{:<6} {:<20} {:<40} {:<15} {:<35}", dup_count, ledger, tx_display, event_display, match_id);

        total_duplicates += dup_count;
        total_extra_rows += dup_count - 1;
    }

    println!("{}", "─".repeat(135));
    println!("\nSummary:");
    println!("  Total duplicate event groups: {}", rows.len());
    println!("  Total duplicate events: {}", total_duplicates);
    println!("  Extra rows to remove: {} (keeping 1 per group)", total_extra_rows);

    let total_events: i64 = conn
        .query_one("SELECT COUNT(*) FROM events WHERE reorg_invalidated_at IS NULL", &[])
        .await?
        .get(0);

    let inflation_pct = if total_events > 0 {
        (total_extra_rows as f64 / total_events as f64) * 100.0
    } else {
        0.0
    };

    println!("  Database inflation: {:.1}% ({} extra rows / {} total)", inflation_pct, total_extra_rows, total_events);
    println!("\nRecommendation:");
    if total_extra_rows == 0 {
        println!("  ✓ No migration needed.");
    } else {
        println!("  Run: cargo run --bin migrate-ids -- --dry-run");
        println!("  Then: cargo run --bin migrate-ids -- --commit");
    }

    Ok(())
}
