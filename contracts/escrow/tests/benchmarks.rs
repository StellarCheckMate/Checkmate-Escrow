//! Performance benchmarking suite for the escrow contract.
//!
//! Measures CPU instructions, memory bytes, and wall-clock time for the core
//! escrow operations (`deposit`, `submit_result`, `cancel_match`,
//! `get_active_matches`) and how those costs scale with the number of
//! active/historical matches already stored on-chain.
//!
//! Run via `scripts/benchmark.sh`, or directly with:
//!
//!   cargo test -p escrow --test benchmarks -- --nocapture
//!
//! A JSON report is written to `reports/performance/benchmark-results.json`
//! at the repository root.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use escrow::types::{Platform, Winner};
use escrow::{EscrowContract, EscrowContractClient};
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, String as SorobanString};

const STAKE: i128 = 100;
const MINT_AMOUNT: i128 = 1_000_000;

/// Sample sizes used for every scaling benchmark below. `100+` per the
/// issue's requirement is covered by the `100` case.
const SCALES: [u32; 3] = [1, 10, 100];

struct Measurement {
    name: &'static str,
    sample_size: u32,
    cpu_instructions: u64,
    memory_bytes: u64,
    wall_time_micros: u128,
}

impl Measurement {
    fn to_json(&self) -> String {
        format!(
            "    {{\n      \"name\": \"{name}\",\n      \"sample_size\": {sample_size},\n      \"cpu_instructions\": {cpu},\n      \"memory_bytes\": {mem},\n      \"wall_time_micros\": {wt}\n    }}",
            name = self.name,
            sample_size = self.sample_size,
            cpu = self.cpu_instructions,
            mem = self.memory_bytes,
            wt = self.wall_time_micros,
        )
    }
}

/// A freshly initialized contract plus a funded token, isolated per benchmark
/// so that one scenario's storage growth never leaks into another's.
struct Harness {
    env: Env,
    contract_id: Address,
    token: Address,
}

impl Harness {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.budget().reset_unlimited();

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token_id.address();

        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(&oracle, &admin);

        Self { env, contract_id, token }
    }

    fn client(&self) -> EscrowContractClient<'_> {
        EscrowContractClient::new(&self.env, &self.contract_id)
    }

    fn new_player(&self) -> Address {
        let player = Address::generate(&self.env);
        StellarAssetClient::new(&self.env, &self.token).mint(&player, &MINT_AMOUNT);
        player
    }

    /// Create a brand-new `Pending` match between two fresh players.
    fn new_match(&self, game_id: &str) -> (u64, Address, Address) {
        let p1 = self.new_player();
        let p2 = self.new_player();
        let id = self.client().create_match(
            &p1,
            &p2,
            &STAKE,
            &self.token,
            &SorobanString::from_str(&self.env, game_id),
            &Platform::Lichess,
        );
        (id, p1, p2)
    }

    /// Create `n` matches and fully fund them so each reaches `Active` state,
    /// populating the on-chain `ActiveMatches` index with `n` entries.
    fn create_active_matches(&self, n: u32, tag: &str) {
        for i in 0..n {
            let (id, p1, p2) = self.new_match(&format!("{tag}-{i:06}"));
            self.client().deposit(&id, &p1);
            self.client().deposit(&id, &p2);
        }
    }
}

/// Measure a single contract call, isolating its cost from setup work.
///
/// `setup` runs under an unlimited budget so it never trips resource limits.
/// The budget is reset to the standard mainnet-equivalent default immediately
/// before `op` runs, so the reported cost reflects only `op`.
fn measure<S: FnOnce(), O: FnOnce()>(
    env: &Env,
    name: &'static str,
    sample_size: u32,
    setup: S,
    op: O,
) -> Measurement {
    env.budget().reset_unlimited();
    setup();

    env.budget().reset_default();
    let start = Instant::now();
    op();
    let wall_time_micros = start.elapsed().as_micros();

    Measurement {
        name,
        sample_size,
        cpu_instructions: env.budget().cpu_instruction_cost(),
        memory_bytes: env.budget().memory_bytes_cost(),
        wall_time_micros,
    }
}

#[test]
fn run_all_benchmarks() {
    let mut results = Vec::new();

    // ── Baseline single-call costs (no contention, no scaling) ─────────────
    {
        let h = Harness::new();
        let (id, p1, _p2) = h.new_match("baseline-deposit");
        results.push(measure(&h.env, "deposit (1st, Pending -> Pending)", 1, || {}, || {
            h.client().deposit(&id, &p1);
        }));
    }
    {
        let h = Harness::new();
        let (id, p1, _p2) = h.new_match("baseline-cancel");
        h.client().deposit(&id, &p1);
        results.push(measure(&h.env, "cancel_match (Pending, 1 deposit refunded)", 1, || {}, || {
            h.client().cancel_match(&id, &p1);
        }));
    }
    {
        let h = Harness::new();
        let (id, p1, p2) = h.new_match("baseline-submit");
        h.client().deposit(&id, &p1);
        h.client().deposit(&id, &p2);
        results.push(measure(&h.env, "submit_result (Active -> Completed, 1 active match)", 1, || {}, || {
            h.client().submit_result(&id, &Winner::Player1);
        }));
    }

    // ── Scaling: deposit (activation path) vs. # of already-active matches ──
    // The 2nd deposit on a match flips it to Active and appends to the
    // ActiveMatches index, which is stored as a single vector re-read and
    // re-written in full on every mutation.
    for n in SCALES {
        let h = Harness::new();
        let (id, p1, p2) = h.new_match("scale-deposit-target");
        h.client().deposit(&id, &p1); // first deposit: cheap, not measured

        results.push(measure(
            &h.env,
            "deposit (activation: appends to ActiveMatches index)",
            n,
            || h.create_active_matches(n, "scale-deposit-filler"),
            || {
                h.client().deposit(&id, &p2);
            },
        ));
    }

    // ── Scaling: submit_result vs. # of active matches ──────────────────────
    // Completing a match removes it from the ActiveMatches index, which
    // rebuilds the entire vector regardless of where the entry lives.
    for n in SCALES {
        let h = Harness::new();
        results.push(measure(
            &h.env,
            "submit_result (removes from ActiveMatches index)",
            n,
            || h.create_active_matches(n, "scale-submit-filler"),
            || {
                let last_id = (n - 1) as u64;
                h.client().submit_result(&last_id, &Winner::Player1);
            },
        ));
    }

    // ── Scaling: cancel_match vs. # of historical (terminal) matches ────────
    // cancel_match only touches the single target match's storage entry, so
    // this is expected to stay flat -- included as a control / contrast.
    for n in SCALES {
        let h = Harness::new();
        for i in 0..n {
            let (id, p1, p2) = h.new_match(&format!("scale-cancel-history-{i:06}"));
            h.client().deposit(&id, &p1);
            h.client().deposit(&id, &p2);
            h.client().submit_result(&id, &Winner::Player1);
        }
        let (target_id, target_p1, _) = h.new_match("scale-cancel-target");

        results.push(measure(&h.env, "cancel_match (Pending, no deposits)", n, || {}, || {
            h.client().cancel_match(&target_id, &target_p1);
        }));
    }

    // ── Scaling: get_active_matches vs. total historical match count ────────
    // get_active_matches scans every match ID ever issued (0..match_count),
    // not just the currently active ones -- this is the main unbounded-growth
    // / DoS-relevant read path in the contract.
    for n in SCALES {
        let h = Harness::new();
        h.create_active_matches(n, "scale-readindex-filler");

        results.push(measure(&h.env, "get_active_matches (scans 0..match_count)", n, || {}, || {
            let _ = h.client().get_active_matches();
        }));
    }

    print_report(&results);
    write_report(&results);
}

fn print_report(results: &[Measurement]) {
    println!();
    println!(
        "{:<58} {:>5} {:>14} {:>12} {:>10}",
        "operation", "n", "cpu_insns", "mem_bytes", "wall_us"
    );
    for r in results {
        println!(
            "{:<58} {:>5} {:>14} {:>12} {:>10}",
            r.name, r.sample_size, r.cpu_instructions, r.memory_bytes, r.wall_time_micros
        );
    }
    println!();
}

fn write_report(results: &[Measurement]) {
    let entries: Vec<String> = results.iter().map(Measurement::to_json).collect();
    let json = format!(
        "{{\n  \"generated_by\": \"contracts/escrow/tests/benchmarks.rs\",\n  \"results\": [\n{}\n  ]\n}}\n",
        entries.join(",\n")
    );

    let path = report_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).expect("failed to create reports/performance directory");
    }
    fs::write(&path, json).expect("failed to write benchmark report");
    println!("Wrote benchmark report to {}", path.display());
}

fn report_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("reports")
        .join("performance")
        .join("benchmark-results.json")
}
