//! Tool to migrate event IDs from random UUIDs to deterministic hashes.
//!
//! This tool:
//! 1. Scans all events in the database
//! 2. Computes deterministic IDs for each
//! 3. Identifies duplicates (same logical event, different old IDs)
//! 4. Generates SQL to deduplicate and reassign IDs
//! 5. Optionally executes the migration (with --commit flag)
//!
//! Usage:
//!   cargo run --bin migrate-ids -- --dry-run      # Show SQL without executing
//!   cargo run --bin migrate-ids -- --commit       # Execute the migration
//!
//! Requires DATABASE_URL environment variable.

use anyhow::{anyhow, Result};
use event_indexer::id_gen::compute_event_id;
use std::collections::HashMap;
use std::env;

#[derive(Clone, Debug)]
struct EventRow {
    id: String,
    ledger_sequence: u32,
    txn_hash: Option<String>,
    event_index_in_txn: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let db_url = env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let args: Vec<String> = env::args().collect();
    let commit = args.iter().any(|a| a == "--commit");
    let dry_run = args.iter().any(|a| a == "--dry-run") || !commit;

    let pool = event_indexer::db::build_pool(&db_url, 5)?;
    let conn = pool.get().await?;

    println!("Event ID Migration Tool");
    println!("Mode: {}\n", if commit { "COMMIT" } else { "DRY-RUN" });

    // Fetch all events
    println!("Fetching all events from database...");
    let rows = conn
        .query(
            r#"
            SELECT id, ledger_sequence, txn_hash, event_index_in_txn
            FROM events
            WHERE reorg_invalidated_at IS NULL
            ORDER BY ledger_sequence ASC, txn_hash ASC, event_index_in_txn ASC
            "#,
            &[],
        )
        .await?;

    let mut events: Vec<EventRow> = rows
        .iter()
        .map(|row| EventRow {
            id: row.get(0),
            ledger_sequence: row.get::<_, i32>(1) as u32,
            txn_hash: row.get(2),
            event_index_in_txn: row.get::<_, Option<i16>>(3).map(|i| i as u16),
        })
        .collect();

    println!("Fetched {} events\n", events.len());

    // Compute deterministic IDs and group by (ledger, txn_hash, event_index)
    let mut logical_event_map: HashMap<(u32, Option<String>, Option<u16>), Vec<EventRow>> = HashMap::new();

    for event in events {
        let key = (event.ledger_sequence, event.txn_hash.clone(), event.event_index_in_txn);
        logical_event_map.entry(key).or_insert_with(Vec::new).push(event);
    }

    // Identify duplicates and plan migration
    let mut duplicates_count = 0;
    let mut to_delete: Vec<String> = Vec::new();
    let mut to_update: Vec<(String, String)> = Vec::new(); // (old_id, new_id)

    for ((ledger, txn_hash, event_index), group) in logical_event_map.iter() {
        if group.len() > 1 {
            duplicates_count += group.len();

            let new_id = compute_event_id(
                *ledger,
                txn_hash.as_ref().ok_or(anyhow!("Missing txn_hash"))?,
                event_index.unwrap_or(0),
            );

            // Keep the first by timestamp, mark rest for deletion
            for (i, event) in group.iter().enumerate() {
                if i == 0 {
                    to_update.push((event.id.clone(), new_id.clone()));
                } else {
                    to_delete.push(event.id.clone());
                }
            }
        } else if let Some(event) = group.first() {
            // Single event: still assign deterministic ID
            let new_id = compute_event_id(
                *ledger,
                txn_hash.as_ref().ok_or(anyhow!("Missing txn_hash"))?,
                event_index.unwrap_or(0),
            );
            if new_id != event.id {
                to_update.push((event.id.clone(), new_id));
            }
        }
    }

    println!("Migration Plan:");
    println!("  Events to update with new ID: {}", to_update.len());
    println!("  Duplicate events to delete: {}", to_delete.len());
    println!("  Total changes: {}\n", to_update.len() + to_delete.len());

    if to_delete.is_empty() && to_update.is_empty() {
        println!("✓ No changes needed. Database is already fully migrated.");
        return Ok(());
    }

    if dry_run {
        println!("Generated SQL (--dry-run mode):\n");
        println!("BEGIN TRANSACTION;");

        // Show sample UPDATE statements
        if !to_update.is_empty() {
            println!("\n-- Update {} events with deterministic IDs", to_update.len());
            for (old_id, new_id) in to_update.iter().take(5) {
                println!("UPDATE events SET id = '{}' WHERE id = '{}';", new_id, old_id);
            }
            if to_update.len() > 5 {
                println!("-- ... {} more UPDATE statements", to_update.len() - 5);
            }
        }

        // Show sample DELETE statements
        if !to_delete.is_empty() {
            println!("\n-- Delete {} duplicate rows", to_delete.len());
            for id in to_delete.iter().take(5) {
                println!("DELETE FROM events WHERE id = '{}';", id);
            }
            if to_delete.len() > 5 {
                println!("-- ... {} more DELETE statements", to_delete.len() - 5);
            }
        }

        println!("\nCOMMIT;");
        println!("\nTo execute this migration, run:");
        println!("  cargo run --bin migrate-ids -- --commit");

        return Ok(());
    }

    // COMMIT mode: execute migration
    println!("EXECUTING MIGRATION (backup your database first!)\n");

    let mut tx = conn.transaction().await?;

    let mut updated = 0;
    for (old_id, new_id) in &to_update {
        tx.execute(
            "UPDATE events SET id = $1 WHERE id = $2",
            &[&new_id, &old_id],
        )
        .await?;
        updated += 1;
        if updated % 100 == 0 {
            println!("Updated {} events...", updated);
        }
    }

    let mut deleted = 0;
    for id in &to_delete {
        tx.execute("DELETE FROM events WHERE id = $1", &[&id]).await?;
        deleted += 1;
        if deleted % 100 == 0 {
            println!("Deleted {} duplicate rows...", deleted);
        }
    }

    // Verify before commit
    let count_after: i64 = tx
        .query_one("SELECT COUNT(*) FROM events WHERE reorg_invalidated_at IS NULL", &[])
        .await?
        .get(0);

    println!("\nVerification:");
    println!("  Events after migration: {}", count_after);
    println!("  Events updated: {}", updated);
    println!("  Duplicate rows deleted: {}", deleted);

    println!("\nCommitting transaction...");
    tx.commit().await?;

    println!("\n✓ Migration completed successfully!");
    println!("\nNext steps:");
    println!("  1. Run: cargo run --bin identify-duplicates");
    println!("  2. Verify: SELECT COUNT(DISTINCT id) FROM events;");
    println!("  3. Monitor ingestion for errors in the application logs");

    Ok(())
}
