//! `oracle-export-csv` — export the oracle submission audit log as CSV.
//!
//! Reads `{ORACLE_QUEUE_DIR}/submissions.json` (written by the oracle poller
//! on every confirmed on-chain submission) and writes a CSV audit trail to
//! stdout or a specified output file.
//!
//! ## Usage
//!
//! ```text
//! oracle-export-csv                            # write CSV to stdout
//! oracle-export-csv --output report.csv        # write CSV to a file
//! oracle-export-csv --match-id 42              # filter to a single match
//! oracle-export-csv --platform lichess         # filter by platform
//! oracle-export-csv --winner player1           # filter by winner
//! ```
//!
//! ## Flags
//!
//! | Flag                   | Description                                              |
//! |------------------------|----------------------------------------------------------|
//! | `--output <file>`      | Write CSV to `<file>` instead of stdout.                 |
//! | `--match-id <ID>`      | Include only the entry for match `<ID>`.                 |
//! | `--platform <name>`    | Include only entries for `lichess` or `chess.com`.       |
//! | `--winner <result>`    | Include only entries where winner is `player1`,          |
//! |                        | `player2`, or `draw`.                                    |
//!
//! ## CSV format
//!
//! ```text
//! match_id,game_id,platform,winner,submitted_at,soroban_tx_hash
//! 1,abc123,lichess,player1,2024-01-15T12:00:00Z,deadbeef…
//! ```
//!
//! ## Environment
//!
//! Uses the same `ORACLE_QUEUE_DIR` environment variable as the main service
//! (default: `./oracle-queue`).

use std::io::Write;

use oracle_service::submission_log::SubmissionLog;

fn queue_dir() -> String {
    std::env::var("ORACLE_QUEUE_DIR").unwrap_or_else(|_| "./oracle-queue".to_string())
}

/// Parse `--output <file>` from args, returning `None` for stdout.
fn parse_output(args: &[String]) -> Option<String> {
    args.iter()
        .position(|a| a == "--output")
        .and_then(|pos| args.get(pos + 1))
        .map(|s| s.clone())
}

/// Parse `--match-id <ID>` from args.
fn parse_match_id(args: &[String]) -> Option<u64> {
    if let Some(pos) = args.iter().position(|a| a == "--match-id") {
        match args.get(pos + 1) {
            Some(val) => match val.parse::<u64>() {
                Ok(n) => Some(n),
                Err(_) => {
                    eprintln!("--match-id requires a non-negative integer, got: {}", val);
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("--match-id requires an argument");
                std::process::exit(1);
            }
        }
    } else {
        None
    }
}

/// Parse `--platform <name>` from args, returning the lowercased value.
fn parse_platform(args: &[String]) -> Option<String> {
    args.iter()
        .position(|a| a == "--platform")
        .and_then(|pos| args.get(pos + 1))
        .map(|s| s.to_lowercase())
}

/// Parse `--winner <result>` from args, returning the lowercased value.
fn parse_winner(args: &[String]) -> Option<String> {
    args.iter()
        .position(|a| a == "--winner")
        .and_then(|pos| args.get(pos + 1))
        .map(|s| s.to_lowercase())
}

/// Escape a CSV field: wrap in double quotes if it contains a comma, double
/// quote, or newline; escape internal double quotes by doubling them.
fn csv_field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[tokio::main]
async fn main() {
    // nosemgrep: rust.lang.security.args.args -- flag parsing only; no shell exec.
    let args: Vec<String> = std::env::args().collect();

    // Show usage on --help.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        return;
    }

    let dir = queue_dir();
    let log = SubmissionLog::new(&dir);

    let entries = match log.load().await {
        Ok(e) => e,
        Err(err) => {
            eprintln!("Error reading submission log: {}", err);
            std::process::exit(1);
        }
    };

    // ── Parse filters ─────────────────────────────────────────────────────
    let filter_match_id = parse_match_id(&args);
    let filter_platform = parse_platform(&args);
    let filter_winner = parse_winner(&args);

    // ── Apply filters ─────────────────────────────────────────────────────
    let filtered: Vec<_> = entries
        .iter()
        .filter(|r| {
            if let Some(id) = filter_match_id {
                if r.match_id != id {
                    return false;
                }
            }
            if let Some(ref p) = filter_platform {
                if r.platform.to_lowercase() != *p {
                    return false;
                }
            }
            if let Some(ref w) = filter_winner {
                if r.winner.to_lowercase() != *w {
                    return false;
                }
            }
            true
        })
        .collect();

    if filtered.is_empty() {
        eprintln!("No matching submission records found.");
        // Exit 0 — empty result is not an error.
        return;
    }

    // ── Build CSV ─────────────────────────────────────────────────────────
    let mut csv = String::new();
    csv.push_str("match_id,game_id,platform,winner,submitted_at,soroban_tx_hash\n");

    for r in &filtered {
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_field(&r.match_id.to_string()),
            csv_field(&r.game_id),
            csv_field(&r.platform),
            csv_field(&r.winner),
            csv_field(&r.submitted_at.to_rfc3339()),
            csv_field(&r.soroban_tx_hash),
        ));
    }

    // ── Write output ──────────────────────────────────────────────────────
    let output_path = parse_output(&args);
    match output_path {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &csv) {
                eprintln!("Error writing to {}: {}", path, e);
                std::process::exit(1);
            }
            eprintln!("Wrote {} record(s) to {}", filtered.len(), path);
        }
        None => {
            // Write to stdout.
            if let Err(e) = std::io::stdout().write_all(csv.as_bytes()) {
                eprintln!("Error writing to stdout: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn print_usage() {
    eprintln!(
        "oracle-export-csv: export oracle submission audit log as CSV\n\
         \n\
         Usage:\n\
         \n\
         oracle-export-csv                            write CSV to stdout\n\
         oracle-export-csv --output <file>            write CSV to a file\n\
         oracle-export-csv --match-id <ID>            filter to a single match\n\
         oracle-export-csv --platform <name>          filter by platform (lichess|chess.com)\n\
         oracle-export-csv --winner <result>          filter by winner (player1|player2|draw)\n\
         \n\
         CSV columns:\n\
           match_id, game_id, platform, winner, submitted_at, soroban_tx_hash\n\
         \n\
         Environment variables:\n\
           ORACLE_QUEUE_DIR   directory containing queue/log files (default: ./oracle-queue)"
    );
}
