//! Gas / resource benchmarks for the m-of-n oracle consensus path.
//!
//! Measures CPU instructions, memory bytes, and wall-clock time for
//! `submit_oracle_result` at 3, 5, 10, and 25 registered oracles, isolating
//! two distinct costs that scale with the size of the registered oracle set:
//!
//!  - **pending vote**: a submission that does *not* yet reach the consensus
//!    threshold. This is the more expensive steady-state case — it runs the
//!    O(n) deadlock-detection scan (`remaining_eligible_oracles`) over every
//!    registered oracle to decide whether the match should flip to disputed.
//!  - **finalizing vote**: the submission that crosses the threshold. This
//!    additionally walks every losing candidate's submitter list to
//!    auto-slash the minority, so its cost scales with how many oracles
//!    already voted for a different result, not with the full registry size.
//!
//! Run via:
//!
//!   cargo test -p oracle --test benchmarks -- --nocapture
//!
//! A JSON report is written to `reports/performance/oracle-consensus-benchmark-results.json`
//! at the repository root.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use oracle::types::{Platform, Winner};
use oracle::{OracleContract, OracleContractClient};
use soroban_sdk::{
    testutils::Address as _, token::StellarAssetClient, Address, Env, String as SorobanString,
};

const STAKE: i128 = 1_000;

/// Registered-oracle-set sizes benchmarked, per the task's required scale points.
const SCALES: [u32; 4] = [3, 5, 10, 25];

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

/// A freshly initialized oracle contract plus a funded token, isolated per
/// benchmark so that one scenario's registered-oracle set never leaks into
/// another's.
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
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token_id.address();

        let contract_id = env.register_contract(None, OracleContract);
        let client = OracleContractClient::new(&env, &contract_id);
        client.initialize(&admin);

        Self {
            env,
            contract_id,
            token,
        }
    }

    fn client(&self) -> OracleContractClient<'_> {
        OracleContractClient::new(&self.env, &self.contract_id)
    }

    /// Register `n` freshly-generated oracle addresses, each staking
    /// [`STAKE`] of the harness token, and return them in registration order.
    fn register_oracles(&self, n: u32) -> std::vec::Vec<Address> {
        let asset_client = StellarAssetClient::new(&self.env, &self.token);
        let mut oracles = std::vec::Vec::new();
        for _ in 0..n {
            let oracle = Address::generate(&self.env);
            asset_client.mint(&oracle, &STAKE);
            self.client()
                .register_oracle_with_stake(&oracle, &STAKE, &self.token);
            oracles.push(oracle);
        }
        oracles
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
    let mut results = std::vec::Vec::new();

    // ── Pending vote: threshold = n (unanimous), so every vote except the
    // last one hits the O(n) deadlock-detection scan without finalizing. ───
    for n in SCALES {
        let h = Harness::new();
        let oracles = h.register_oracles(n);
        h.client().set_consensus_threshold(&n);

        results.push(measure(
            &h.env,
            "submit_oracle_result (pending vote, scans full registered set)",
            n,
            || {},
            || {
                h.client().submit_oracle_result(
                    &oracles[0],
                    &0u64,
                    &SorobanString::from_str(&h.env, "bench_game"),
                    &Platform::Lichess,
                    &Winner::Player1,
                );
            },
        ));
    }

    // ── Finalizing vote: threshold = n/2 + 1 (simple majority). One
    // dissenting minority oracle votes first, then the majority votes in
    // agreement; the final vote crosses the threshold and must walk the
    // minority candidate's submitter list to auto-slash it. ────────────────
    for n in SCALES {
        let h = Harness::new();
        let oracles = h.register_oracles(n);
        let threshold = n / 2 + 1;
        h.client().set_consensus_threshold(&threshold);

        // One minority vote for a different result.
        h.client().submit_oracle_result(
            &oracles[0],
            &0u64,
            &SorobanString::from_str(&h.env, "bench_game"),
            &Platform::Lichess,
            &Winner::Player2,
        );

        // Majority votes agree, up to (but not including) the finalizing one.
        for oracle in oracles.iter().take(threshold as usize).skip(1) {
            h.client().submit_oracle_result(
                oracle,
                &0u64,
                &SorobanString::from_str(&h.env, "bench_game"),
                &Platform::Lichess,
                &Winner::Player1,
            );
        }

        let finalizing_oracle = &oracles[threshold as usize];
        results.push(measure(
            &h.env,
            "submit_oracle_result (finalizing vote, slashes minority)",
            n,
            || {},
            || {
                h.client().submit_oracle_result(
                    finalizing_oracle,
                    &0u64,
                    &SorobanString::from_str(&h.env, "bench_game"),
                    &Platform::Lichess,
                    &Winner::Player1,
                );
            },
        ));
    }

    // ── Degenerate n=1 baseline: single registered oracle, threshold=1,
    // finalizes on its first vote. Contrast point for the m-of-n numbers
    // above — this reproduces the original single-admin-oracle cost. ───────
    {
        let h = Harness::new();
        let oracles = h.register_oracles(1);

        results.push(measure(
            &h.env,
            "submit_oracle_result (degenerate n=1, finalizes immediately)",
            1,
            || {},
            || {
                h.client().submit_oracle_result(
                    &oracles[0],
                    &0u64,
                    &SorobanString::from_str(&h.env, "bench_game"),
                    &Platform::Lichess,
                    &Winner::Player1,
                );
            },
        ));
    }

    print_report(&results);
    write_report(&results);
}

fn print_report(results: &[Measurement]) {
    println!();
    println!(
        "{:<62} {:>5} {:>14} {:>12} {:>10}",
        "operation", "n", "cpu_insns", "mem_bytes", "wall_us"
    );
    for r in results {
        println!(
            "{:<62} {:>5} {:>14} {:>12} {:>10}",
            r.name, r.sample_size, r.cpu_instructions, r.memory_bytes, r.wall_time_micros
        );
    }
    println!();
}

fn write_report(results: &[Measurement]) {
    let entries: std::vec::Vec<String> = results.iter().map(Measurement::to_json).collect();
    let json = format!(
        "{{\n  \"generated_by\": \"contracts/oracle/tests/benchmarks.rs\",\n  \"results\": [\n{}\n  ]\n}}\n",
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
        .join("oracle-consensus-benchmark-results.json")
}
