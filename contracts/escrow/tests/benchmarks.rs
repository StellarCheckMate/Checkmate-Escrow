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
use oracle::{OracleContract, OracleContractClient};
use soroban_sdk::{
    testutils::{Address as _, EnvTestConfig},
    token::StellarAssetClient,
    Address, Env, String as SorobanString,
};

const STAKE: i128 = 100;
const MINT_AMOUNT: i128 = 1_000_000;

/// Sample sizes used for scaling benchmarks. Covers the range from single match
/// to 1,000 concurrent active matches, demonstrating O(log n) or O(1) cost growth.
const SCALES: [u32; 4] = [1, 10, 100, 1000];

struct Measurement {
    name: &'static str,
    sample_size: u32,
    cpu_instructions: u64,
    memory_bytes: u64,
    wall_time_micros: u128,
    /// True when `op` blew the mainnet-equivalent budget instead of
    /// completing -- the cpu/mem figures are then whatever was consumed up
    /// to the point of the abort, not the operation's real cost.
    exceeded_budget: bool,
}

impl Measurement {
    fn to_json(&self) -> String {
        format!(
            "    {{\n      \"name\": \"{name}\",\n      \"sample_size\": {sample_size},\n      \"cpu_instructions\": {cpu},\n      \"memory_bytes\": {mem},\n      \"wall_time_micros\": {wt},\n      \"exceeded_budget\": {exceeded}\n    }}",
            name = self.name,
            sample_size = self.sample_size,
            cpu = self.cpu_instructions,
            mem = self.memory_bytes,
            wt = self.wall_time_micros,
            exceeded = self.exceeded_budget,
        )
    }
}

/// A freshly initialized contract plus a funded token, isolated per benchmark
/// so that one scenario's storage growth never leaks into another's.
struct Harness {
    env: Env,
    contract_id: Address,
    token: Address,
    oracle: Address,
    /// Monotonic counter used to derive Lichess-compliant (exactly 8 ASCII
    /// alphanumeric chars) game IDs, since call sites pass free-form
    /// descriptive labels ("baseline-deposit", "scale-cancel-history-000042")
    /// that don't fit that format on their own.
    game_id_counter: std::cell::Cell<u32>,
}

impl Harness {
    fn new() -> Self {
        let mut env = Env::default();
        // The SDK's default test-snapshot-on-drop dumps the full ledger
        // storage to JSON, which for the largest scale benchmarks (up to
        // 1,000 active matches) walks enough entries to exceed even
        // budget().reset_unlimited()'s ceiling and aborts the process.
        // The benchmark suite writes its own JSON report, so this
        // diagnostic snapshot isn't needed here.
        env.set_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        // Multi-token payouts route through the oracle's `swap`, which
        // requires escrow to require_auth() itself in a call that isn't the
        // root invocation (escrow -> oracle.swap -> token.transfer). Plain
        // mock_all_auths only mocks auths tied to the root invocation, so
        // this (a strict superset) is used everywhere instead.
        env.mock_all_auths_allowing_non_root_auth();
        env.budget().reset_unlimited();

        let admin = Address::generate(&env);
        let oracle_admin = Address::generate(&env);

        // A real deployed oracle, not a bare address: create_match_with_conversion
        // calls oracle.get_rate cross-contract, which panics if there's no
        // contract at that address.
        let oracle_id = env.register_contract(None, OracleContract);
        OracleContractClient::new(&env, &oracle_id).initialize(&oracle_admin);
        let oracle = oracle_id;

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token_id.address();

        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(&oracle, &admin);

        Self {
            env,
            contract_id,
            token,
            oracle,
            game_id_counter: std::cell::Cell::new(0),
        }
    }

    fn client(&self) -> EscrowContractClient<'_> {
        EscrowContractClient::new(&self.env, &self.contract_id)
    }

    fn new_player(&self) -> Address {
        let player = Address::generate(&self.env);
        StellarAssetClient::new(&self.env, &self.token).mint(&player, &MINT_AMOUNT);
        player
    }

    /// Create a brand-new `Pending` match between two fresh players. `label`
    /// is purely descriptive (unused past this call) — the on-chain game ID
    /// is derived from an internal counter to guarantee both uniqueness and
    /// Lichess's exactly-8-alphanumeric-char format.
    fn new_match(&self, _label: &str) -> (u64, Address, Address) {
        let n = self.game_id_counter.get();
        self.game_id_counter.set(n + 1);
        let game_id = format!("{n:08x}");

        let p1 = self.new_player();
        let p2 = self.new_player();
        let id = self.client().create_match(
            &p1,
            &p2,
            &STAKE,
            &self.token,
            &SorobanString::from_str(&self.env, &game_id),
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
///
/// At the largest scales, `op` can legitimately blow that real-network
/// budget (this is exactly the DoS-relevant growth this suite exists to
/// catch -- see `get_active_matches`, whose cost scales with total
/// historical match count). That's a finding to report, not a reason to
/// abort the whole benchmark run, so the panic the host raises on
/// exceeding budget is caught here and recorded as `exceeded_budget`.
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
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(op));
    let wall_time_micros = start.elapsed().as_micros();
    let exceeded_budget = outcome.is_err();
    if exceeded_budget {
        eprintln!("warning: {name} (n={sample_size}) exceeded the mainnet-equivalent budget");
    }

    Measurement {
        name,
        sample_size,
        cpu_instructions: env.budget().cpu_instruction_cost(),
        memory_bytes: env.budget().memory_bytes_cost(),
        wall_time_micros,
        exceeded_budget,
    }
}

#[test]
#[ignore = "slow (up to 1k-match scale); run explicitly with `cargo test -- --ignored`"]
fn run_all_benchmarks() {
    let mut results = Vec::new();

    // ── Baseline single-call costs (no contention, no scaling) ─────────────
    {
        let h = Harness::new();
        let (id, p1, _p2) = h.new_match("baseline-deposit");
        results.push(measure(
            &h.env,
            "deposit (1st, Pending -> Pending)",
            1,
            || {},
            || {
                h.client().deposit(&id, &p1);
            },
        ));
    }
    {
        let h = Harness::new();
        let (id, p1, _p2) = h.new_match("baseline-cancel");
        h.client().deposit(&id, &p1);
        results.push(measure(
            &h.env,
            "cancel_match (Pending, 1 deposit refunded)",
            1,
            || {},
            || {
                h.client().cancel_match(&id, &p1);
            },
        ));
    }
    {
        let h = Harness::new();
        let (id, p1, p2) = h.new_match("baseline-submit");
        h.client().deposit(&id, &p1);
        h.client().deposit(&id, &p2);
        results.push(measure(
            &h.env,
            "submit_result (Active -> Completed, 1 active match)",
            1,
            || {},
            || {
                h.client().submit_result(&id, &Winner::Player1, &h.oracle);
            },
        ));
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
                h.client()
                    .submit_result(&last_id, &Winner::Player1, &h.oracle);
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
            h.client().submit_result(&id, &Winner::Player1, &h.oracle);
        }
        let (target_id, target_p1, _) = h.new_match("scale-cancel-target");

        results.push(measure(
            &h.env,
            "cancel_match (Pending, no deposits)",
            n,
            || {},
            || {
                h.client().cancel_match(&target_id, &target_p1);
            },
        ));
    }

    // ── Scaling: get_active_matches vs. total historical match count ────────
    // get_active_matches scans every match ID ever issued (0..match_count),
    // not just the currently active ones -- this is the main unbounded-growth
    // / DoS-relevant read path in the contract.
    for n in SCALES {
        let h = Harness::new();
        h.create_active_matches(n, "scale-readindex-filler");

        results.push(measure(
            &h.env,
            "get_active_matches (scans 0..match_count)",
            n,
            || {},
            || {
                let _ = h.client().get_active_matches();
            },
        ));
    }

    // ── Multi-token settlement benchmarks ────────────────────────────────────
    {
        let h = Harness::new();
        let admin = Address::generate(&h.env);
        let token_b_id = h.env.register_stellar_asset_contract_v2(admin.clone());
        let token_b = token_b_id.address();
        let asset_b = StellarAssetClient::new(&h.env, &token_b);

        let p1 = h.new_player();
        let p2 = h.new_player();
        asset_b.mint(&p2, &(MINT_AMOUNT * 50)); // Player2 needs enough token_b

        // Oracle needs a rate for this pair (create_match_with_conversion
        // checks the requested rate against it) and a funded pool of both
        // tokens to actually perform the swap during payout.
        OracleContractClient::new(&h.env, &h.oracle).set_rate(&h.token, &token_b, &50_000_000);
        StellarAssetClient::new(&h.env, &h.token).mint(&h.oracle, &(MINT_AMOUNT * 50));
        asset_b.mint(&h.oracle, &(MINT_AMOUNT * 50));

        let id = h.client().create_match_with_conversion(
            &p1,
            &p2,
            &STAKE,
            &h.token,
            &token_b,
            &50_000_000_i128,
            &SorobanString::from_str(&h.env, "multi-token-submit"),
            &Platform::Lichess,
        );
        h.client().deposit(&id, &p1);
        h.client().deposit(&id, &p2);

        results.push(measure(
            &h.env,
            "submit_result (multi-token, Player1 wins)",
            1,
            || {},
            || {
                h.client().submit_result(&id, &Winner::Player1, &h.oracle);
            },
        ));
    }

    {
        let h = Harness::new();
        let admin = Address::generate(&h.env);
        let token_b_id = h.env.register_stellar_asset_contract_v2(admin.clone());
        let token_b = token_b_id.address();
        let asset_b = StellarAssetClient::new(&h.env, &token_b);

        let p1 = h.new_player();
        let p2 = h.new_player();
        asset_b.mint(&p2, &(MINT_AMOUNT * 50));

        OracleContractClient::new(&h.env, &h.oracle).set_rate(&h.token, &token_b, &50_000_000);
        StellarAssetClient::new(&h.env, &h.token).mint(&h.oracle, &(MINT_AMOUNT * 50));
        asset_b.mint(&h.oracle, &(MINT_AMOUNT * 50));

        let id = h.client().create_match_with_conversion(
            &p1,
            &p2,
            &STAKE,
            &h.token,
            &token_b,
            &50_000_000_i128,
            &SorobanString::from_str(&h.env, "multi-token-draw"),
            &Platform::Lichess,
        );
        h.client().deposit(&id, &p1);
        h.client().deposit(&id, &p2);

        results.push(measure(
            &h.env,
            "submit_result (multi-token, Draw)",
            1,
            || {},
            || {
                h.client().submit_result(&id, &Winner::Draw, &h.oracle);
            },
        ));
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
        let flag = if r.exceeded_budget {
            " EXCEEDED BUDGET"
        } else {
            ""
        };
        println!(
            "{:<58} {:>5} {:>14} {:>12} {:>10}{flag}",
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
