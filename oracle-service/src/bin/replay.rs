//! `oracle-replay` — manual dead-letter replay CLI.
//!
//! Re-enqueues entries from the dead-letter store back into the live pending
//! queue so the pipeline poller will retry them on its next tick.
//!
//! ## Usage
//!
//! ```text
//! oracle-replay --list                         # list all dead-letter entries
//! oracle-replay --match-id <ID>                # dry-run preview for a single match
//! oracle-replay --match-id <ID> --confirm      # replay a single match (writes to queue)
//! oracle-replay --all                          # dry-run preview for all entries
//! oracle-replay --all --confirm                # replay all dead-letter entries (writes to queue)
//! oracle-replay --all --max-retries <N>        # preview only entries with total_attempts < N
//! oracle-replay --all --confirm --max-retries <N>  # re-enqueue only entries with total_attempts < N
//! ```
//!
//! ## Flags
//!
//! | Flag                   | Description                                                         |
//! |------------------------|---------------------------------------------------------------------|
//! | `--list`               | Print a summary table of all dead-letter entries.                   |
//! | `--all`                | Re-enqueue every entry in the dead-letter store (requires          |
//! |                        | `--confirm` to actually write; without it, performs a dry-run).    |
//! | `--match-id <ID>`      | Re-enqueue a single entry by match ID (requires `--confirm` to     |
//! |                        | actually write; without it, performs a dry-run).                   |
//! | `--confirm`            | Required to write any changes. Without this flag the tool logs     |
//! |                        | what *would* happen but makes no changes.  This prevents a stray  |
//! |                        | invocation from double-settling already-submitted matches.         |
//! | `--max-retries <N>`    | Skip entries whose `total_attempts` is ≥ N (i.e. too many failed   |
//! |                        | attempts to be worth replaying automatically).                      |
//!
//! ## Environment
//!
//! Uses the same `ORACLE_QUEUE_DIR` environment variable as the main service
//! (default: `./oracle-queue`).

use oracle_service::{dead_letter::DeadLetterStore, queue::PendingQueue};

fn queue_dir() -> String {
    std::env::var("ORACLE_QUEUE_DIR").unwrap_or_else(|_| "./oracle-queue".to_string())
}

/// Parse `--max-retries <N>` from `args`, returning `None` if the flag is
/// absent and `Some(N)` when present. Exits with a usage error on bad input.
fn parse_max_retries(args: &[String]) -> Option<u32> {
    if let Some(pos) = args.iter().position(|a| a == "--max-retries") {
        match args.get(pos + 1) {
            Some(val) => match val.parse::<u32>() {
                Ok(n) => Some(n),
                Err(_) => {
                    eprintln!(
                        "--max-retries requires a non-negative integer, got: {}",
                        val
                    );
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("--max-retries requires an argument");
                std::process::exit(1);
            }
        }
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    // nosemgrep: rust.lang.security.args.args -- used only for --list/--match-id/--all/--dry-run/--max-retries flag parsing, not a security decision.
    let args: Vec<String> = std::env::args().collect();

    let dir = queue_dir();
    // The replay tool doesn't enforce a capacity limit — it always operates
    // on the full current contents of the dead-letter file as-is.
    let store = DeadLetterStore::new(&dir, 0);
    let queue = PendingQueue::new(&dir);

    // `--confirm` is required before any writes are made.  Without it the tool
    // runs in dry-run mode regardless of other flags.  This prevents a stray
    // invocation from double-settling matches that were already submitted.
    let confirmed = args.iter().any(|a| a == "--confirm");
    // Preserve backward-compat: an explicit `--dry-run` also activates dry-run
    // mode (useful in scripts that build on top of this tool).
    let dry_run = !confirmed || args.iter().any(|a| a == "--dry-run");
    let max_retries = parse_max_retries(&args);

    if dry_run {
        println!("[dry-run] No changes will be written. Pass --confirm to submit.");
    }

    // ── --list ────────────────────────────────────────────────────────────
    if args.iter().any(|a| a == "--list") {
        let entries = store.load().await.unwrap_or_default();
        if entries.is_empty() {
            println!("Dead-letter store is empty.");
        } else {
            println!(
                "{:<8}  {:<12}  {:<10}  {:<8}  {:<30}  last_error",
                "match_id", "game_id", "platform", "attempts", "dead_lettered_at"
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

        let mut succeeded = 0u32;
        let mut skipped = 0u32;
        let mut failed = 0u32;

        for dl in &entries {
            let match_id = dl.entry.match_id;

            // Skip entries that have exceeded --max-retries.
            if let Some(max) = max_retries {
                if dl.total_attempts >= max {
                    println!(
                        "SKIP    match_id={} (total_attempts={} >= max-retries={})",
                        match_id, dl.total_attempts, max
                    );
                    skipped += 1;
                    continue;
                }
            }

            if dry_run {
                println!(
                    "DRY-RUN match_id={} game_id={} platform={} attempts={}",
                    match_id, dl.entry.game_id, dl.entry.platform, dl.total_attempts
                );
                succeeded += 1;
                continue;
            }

            match queue
                .enqueue(match_id, dl.entry.game_id.clone(), dl.entry.platform)
                .await
            {
                Ok(true) => {
                    store.remove(match_id).await.ok();
                    println!("SUCCESS match_id={}", match_id);
                    succeeded += 1;
                }
                Ok(false) => {
                    println!("SKIP    match_id={} already in queue", match_id);
                    skipped += 1;
                }
                Err(e) => {
                    eprintln!("FAILURE match_id={}: {}", match_id, e);
                    failed += 1;
                }
            }
        }

        println!(
            "\nSummary: {} succeeded, {} skipped, {} failed",
            succeeded, skipped, failed
        );
        if failed > 0 {
            std::process::exit(1);
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
                            // Check --max-retries for single-entry replay too.
                            if let Some(max) = max_retries {
                                if dl.total_attempts >= max {
                                    println!(
                                        "SKIP match_id={} (total_attempts={} >= max-retries={})",
                                        match_id, dl.total_attempts, max
                                    );
                                    return;
                                }
                            }

                            if dry_run {
                                println!(
                                    "DRY-RUN match_id={} game_id={} platform={} attempts={}",
                                    match_id,
                                    dl.entry.game_id,
                                    dl.entry.platform,
                                    dl.total_attempts
                                );
                                return;
                            }

                            match queue
                                .enqueue(match_id, dl.entry.game_id.clone(), dl.entry.platform)
                                .await
                            {
                                Ok(true) => {
                                    store.remove(match_id).await.ok();
                                    println!("SUCCESS match_id={}", match_id);
                                }
                                Ok(false) => {
                                    println!("SKIP match_id={} already in pending queue", match_id)
                                }
                                Err(e) => {
                                    eprintln!("FAILURE match_id={}: {}", match_id, e);
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
         oracle-replay --list                              list dead-letter entries\n\
         oracle-replay --match-id <ID>                     preview re-enqueue for a specific match\n\
         oracle-replay --match-id <ID> --confirm           re-enqueue a specific match (writes)\n\
         oracle-replay --all                               preview re-enqueue for all entries\n\
         oracle-replay --all --confirm                     re-enqueue all dead-letter entries (writes)\n\
         oracle-replay --all --confirm --max-retries <N>   skip entries with attempts >= N\n\
         \n\
         Flags:\n\
         --confirm          required to write any changes (default is dry-run)\n\
         --dry-run          explicit dry-run; equivalent to omitting --confirm\n\
         --max-retries <N>  skip entries whose total_attempts is >= N\n\
         \n\
         Environment variables:\n\
         ORACLE_QUEUE_DIR   directory containing queue files (default: ./oracle-queue)"
    );
    std::process::exit(1);
}
