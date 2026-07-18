//! `oracle-replay` — manual dead-letter replay CLI.
//!
//! Re-enqueues entries from the dead-letter store back into the live pending
//! queue so the pipeline poller will retry them on its next tick.
//!
//! ## Usage
//!
//! ```text
//! oracle-replay --list                  # list all dead-letter entries
//! oracle-replay --match-id <ID>         # replay a single match
//! oracle-replay --all                   # replay all dead-letter entries
//! ```
//!
//! ## Environment
//!
//! Uses the same `ORACLE_QUEUE_DIR` environment variable as the main service
//! (default: `./oracle-queue`).

use oracle_service::{dead_letter::DeadLetterStore, queue::PendingQueue};

fn queue_dir() -> String {
    std::env::var("ORACLE_QUEUE_DIR").unwrap_or_else(|_| "./oracle-queue".to_string())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let dir = queue_dir();
    let store = DeadLetterStore::new(&dir);
    let queue = PendingQueue::new(&dir);

    // ── --list ────────────────────────────────────────────────────────────
    if args.iter().any(|a| a == "--list") {
        let entries = store.load().await.unwrap_or_default();
        if entries.is_empty() {
            println!("Dead-letter store is empty.");
        } else {
            println!(
                "{:<8}  {:<12}  {:<10}  {:<8}  {:<30}  {}",
                "match_id", "game_id", "platform", "attempts", "dead_lettered_at", "last_error"
            );
            println!("{}", "-".repeat(100));
            for dl in &entries {
                println!(
                    "{:<8}  {:<12}  {:<10}  {:<8}  {:<30}  {}",
                    dl.entry.match_id,
                    dl.entry.game_id,
                    dl.entry.platform,
                    dl.total_attempts,
                    dl.dead_lettered_at.to_rfc3339(),
                    dl.entry.last_error.as_deref().unwrap_or("-"),
                );
            }
        }
        return;
    }

    // ── --all ─────────────────────────────────────────────────────────────
    if args.iter().any(|a| a == "--all") {
        let entries = store.load().await.unwrap_or_default();
        if entries.is_empty() {
            println!("Nothing to replay.");
            return;
        }
        for dl in &entries {
            let match_id = dl.entry.match_id;
            match queue
                .enqueue(
                    match_id,
                    dl.entry.game_id.clone(),
                    dl.entry.platform,
                )
                .await
            {
                Ok(true) => {
                    store.remove(match_id).await.ok();
                    println!("Re-enqueued match_id={}", match_id);
                }
                Ok(false) => println!("match_id={} already in queue, skipping", match_id),
                Err(e) => eprintln!("ERROR re-enqueueing match_id={}: {}", match_id, e),
            }
        }
        return;
    }

    // ── --match-id <ID> ───────────────────────────────────────────────────
    if let Some(pos) = args.iter().position(|a| a == "--match-id") {
        if let Some(id_str) = args.get(pos + 1) {
            match id_str.parse::<u64>() {
                Ok(match_id) => {
                    let entries = store.load().await.unwrap_or_default();
                    match entries.iter().find(|e| e.entry.match_id == match_id) {
                        None => {
                            eprintln!("match_id={} not found in dead-letter store", match_id);
                            std::process::exit(1);
                        }
                        Some(dl) => {
                            match queue
                                .enqueue(match_id, dl.entry.game_id.clone(), dl.entry.platform)
                                .await
                            {
                                Ok(true) => {
                                    store.remove(match_id).await.ok();
                                    println!("Re-enqueued match_id={}", match_id);
                                }
                                Ok(false) => println!(
                                    "match_id={} already in pending queue",
                                    match_id
                                ),
                                Err(e) => {
                                    eprintln!("ERROR: {}", e);
                                    std::process::exit(1);
                                }
                            }
                        }
                    }
                }
                Err(_) => {
                    eprintln!("Invalid match_id: {}", id_str);
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!("--match-id requires an argument");
            std::process::exit(1);
        }
        return;
    }

    // ── Usage ─────────────────────────────────────────────────────────────
    eprintln!(
        "oracle-replay: manual dead-letter replay tool\n\
         \n\
         Usage:\n\
         \n\
         oracle-replay --list               list dead-letter entries\n\
         oracle-replay --match-id <ID>      re-enqueue a specific match\n\
         oracle-replay --all                re-enqueue all dead-letter entries\n\
         \n\
         Environment variables:\n\
         ORACLE_QUEUE_DIR   directory containing queue files (default: ./oracle-queue)"
    );
    std::process::exit(1);
}
