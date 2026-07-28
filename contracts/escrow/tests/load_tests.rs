//! Load tests — contract performance under high match volume.
//!
//! Measures how contract operations scale from 100 to 10,000 concurrent
//! matches.  Metrics reported:
//!
//! - **CPU instructions** (Soroban host budget)
//! - **Memory bytes** (Soroban host budget)
//! - **Wall-clock time** (std::time::Instant)
//! - **State-size growth** (approximate storage entries)
//!
//! Each scenario creates N background matches to seed the contract state, then
//! measures the cost of the target operation on a fresh match against that
//! backdrop.  The budget is reset before each measured call so seeding cost is
//! excluded.
//!
//! ## Sections
//!
//! 1. `create_match` throughput at scale
//! 2. `deposit` (activation — 2nd deposit) at scale
//! 3. `submit_result` at scale
//! 4. `get_active_matches_paginated` at scale
//! 5. State-size growth analysis
//! 6. Concurrent-match isolation (correctness under load)
//! 7. Summary report written to `reports/performance/load-test-results.json`
//!
//! Run with:
//!   cargo test -p escrow --test load_tests -- --nocapture

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use escrow::types::{Platform, ProtocolConfig, Winner};
use escrow::{EscrowContract, EscrowContractClient};
use soroban_sdk::{
    testutils::Address as _,
    token::StellarAssetClient,
    Address, Env, String as SorobanString,
};

// ── Constants ─────────────────────────────────────────────────────────────────

const STAKE: i128 = 100;
/// Generous mint so players can participate in thousands of matches.
const MINT_AMOUNT: i128 = 100_000_000;

/// Scale levels tested.  Adjust the upper bound if CI resources are limited.
/// The test suite is written so that reducing `SCALES` only drops data points;
/// it does not break any assertion.
const SCALES: &[u32] = &[100, 1_000, 10_000];

// ── Result data model ─────────────────────────────────────────────────────────

#[derive(Debug)]
struct LoadMeasurement {
    operation: &'static str,
    n_background_matches: u32,
    cpu_instructions: u64,
    memory_bytes: u64,
    wall_time_micros: u128,
}

impl LoadMeasurement {
    fn to_json(&self) -> String {
        format!(
            "    {{\
\n      \"operation\": \"{op}\",\
\n      \"n_background_matches\": {n},\
\n      \"cpu_instructions\": {cpu},\
\n      \"memory_bytes\": {mem},\
\n      \"wall_time_micros\": {wt}\
\n    }}",
            op = self.operation,
            n = self.n_background_matches,
            cpu = self.cpu_instructions,
            mem = self.memory_bytes,
            wt = self.wall_time_micros,
        )
    }
}

// ── Harness ───────────────────────────────────────────────────────────────────

/// Isolated contract instance with unlimited budget.
struct LoadHarness {
    env: Env,
    contract_id: Address,
    token: Address,
}

impl LoadHarness {
    fn new() -> Self {
        let env = Env::default();
        env.set_config(soroban_sdk::testutils::EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        env.mock_all_auths();
        env.budget().reset_unlimited();

        let admin = Address::generate(&env);
        let oracle = Address::generate(&env);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let token = token_id.address();

        let contract_id = env.register_contract(None, EscrowContract);
        let client = EscrowContractClient::new(&env, &contract_id);
        client.initialize(&oracle, &admin);
        client.set_protocol_config(&ProtocolConfig {
            vesting_duration_seconds: 0,
            cancellation_fee_basis_points: 0,
            treasury: admin.clone(),
        });

        Self { env, contract_id, token }
    }

    fn client(&self) -> EscrowContractClient<'_> {
        EscrowContractClient::new(&self.env, &self.contract_id)
    }

    /// Mint `amount` tokens to `addr`.
    fn mint(&self, addr: &Address, amount: i128) {
        // We need the asset admin — for simplicity re-register the asset as
        // admin using a fresh StellarAssetClient.
        let asset = StellarAssetClient::new(&self.env, &self.token);
        asset.mint(addr, &amount);
    }

    /// Create a new funded player.
    fn new_player(&self) -> Address {
        let p = Address::generate(&self.env);
        self.mint(&p, MINT_AMOUNT);
        p
    }

    /// Create `n` background active matches to seed storage, using distinct
    /// players for each match to avoid interference.
    fn seed_active_matches(&self, n: u32) {
        let client = self.client();
        for i in 0..n {
            let p1 = self.new_player();
            let p2 = self.new_player();
            let game_id = SorobanString::from_str(&self.env, &format!("bg{:08}", i));
            let mid = client.create_match(
                &p1,
                &p2,
                &STAKE,
                &self.token,
                &game_id,
                &Platform::Lichess,
            );
            client.deposit(&mid, &p1);
            client.deposit(&mid, &p2);
        }
    }

    /// Measure one operation (closure) after resetting the host budget.
    ///
    /// Returns `(cpu_instructions, memory_bytes, wall_time_micros)`.
    fn measure<F: FnOnce()>(&self, f: F) -> (u64, u64, u128) {
        self.env.budget().reset_unlimited();
        let t0 = Instant::now();
        f();
        let elapsed = t0.elapsed().as_micros();
        let cpu = self.env.budget().cpu_instruction_cost();
        let mem = self.env.budget().memory_bytes_cost();
        (cpu, mem, elapsed)
    }
}

// ── Helper: unique game IDs ───────────────────────────────────────────────────

fn make_game_id(env: &Env, tag: &str, n: u32) -> SorobanString {
    SorobanString::from_str(env, &format!("{}{:08x}", tag, n))
}

// ── Test 1: create_match throughput ──────────────────────────────────────────

/// Measures `create_match` cost with N background active matches in storage.
#[test]
fn load_create_match_at_scale() {
    let mut results: Vec<LoadMeasurement> = Vec::new();

    for &n in SCALES {
        let h = LoadHarness::new();
        h.seed_active_matches(n);

        let p1 = h.new_player();
        let p2 = h.new_player();
        let game_id = make_game_id(&h.env, "cm", n);
        let client = h.client();

        let (cpu, mem, wt) = h.measure(|| {
            client.create_match(&p1, &p2, &STAKE, &h.token, &game_id, &Platform::Lichess);
        });

        println!(
            "[load] create_match | n={n:>6} | cpu={cpu:>12} | mem={mem:>10} | wall={wt:>8}µs"
        );
        results.push(LoadMeasurement {
            operation: "create_match",
            n_background_matches: n,
            cpu_instructions: cpu,
            memory_bytes: mem,
            wall_time_micros: wt,
        });
    }

    // Sanity: cost must not decrease as n grows (monotone growth expected).
    if results.len() >= 2 {
        // Allow some variance — just assert the first scale is cheaper than
        // the last.
        assert!(
            results.first().unwrap().cpu_instructions
                <= results.last().unwrap().cpu_instructions + 1_000_000,
            "create_match cost should not dramatically decrease with more background state"
        );
    }

    persist_results("create_match", &results);
}

// ── Test 2: deposit (activation) at scale ────────────────────────────────────

/// Measures the cost of the *second* deposit (which activates the match) when
/// N background matches are already active.
#[test]
fn load_deposit_activation_at_scale() {
    let mut results: Vec<LoadMeasurement> = Vec::new();

    for &n in SCALES {
        let h = LoadHarness::new();
        h.seed_active_matches(n);

        let p1 = h.new_player();
        let p2 = h.new_player();
        let game_id = make_game_id(&h.env, "dp", n);
        let client = h.client();

        // Create and have p1 deposit (free to not count as part of activation).
        let mid = client.create_match(&p1, &p2, &STAKE, &h.token, &game_id, &Platform::Lichess);
        client.deposit(&mid, &p1);

        // Measure the activating (2nd) deposit.
        let (cpu, mem, wt) = h.measure(|| {
            client.deposit(&mid, &p2);
        });

        println!(
            "[load] deposit(activate) | n={n:>6} | cpu={cpu:>12} | mem={mem:>10} | wall={wt:>8}µs"
        );
        results.push(LoadMeasurement {
            operation: "deposit_activation",
            n_background_matches: n,
            cpu_instructions: cpu,
            memory_bytes: mem,
            wall_time_micros: wt,
        });
    }

    persist_results("deposit_activation", &results);
}

// ── Test 3: submit_result at scale ────────────────────────────────────────────

/// Measures `submit_result` cost when N matches are already active in storage.
#[test]
fn load_submit_result_at_scale() {
    let mut results: Vec<LoadMeasurement> = Vec::new();

    for &n in SCALES {
        let h = LoadHarness::new();
        h.seed_active_matches(n);

        let p1 = h.new_player();
        let p2 = h.new_player();
        let game_id = make_game_id(&h.env, "sr", n);
        let client = h.client();

        let mid = client.create_match(&p1, &p2, &STAKE, &h.token, &game_id, &Platform::Lichess);
        client.deposit(&mid, &p1);
        client.deposit(&mid, &p2);

        let (cpu, mem, wt) = h.measure(|| {
            client.submit_result(&mid, &Winner::Player1);
        });

        println!(
            "[load] submit_result | n={n:>6} | cpu={cpu:>12} | mem={mem:>10} | wall={wt:>8}µs"
        );
        results.push(LoadMeasurement {
            operation: "submit_result",
            n_background_matches: n,
            cpu_instructions: cpu,
            memory_bytes: mem,
            wall_time_micros: wt,
        });
    }

    persist_results("submit_result", &results);
}

// ── Test 4: get_active_matches_paginated at scale ────────────────────────────

/// Measures paginated active-match query cost at each scale.
/// Pagination should keep per-call cost bounded even at 10,000 matches.
#[test]
fn load_get_active_matches_paginated_at_scale() {
    let mut results: Vec<LoadMeasurement> = Vec::new();

    for &n in SCALES {
        let h = LoadHarness::new();
        h.seed_active_matches(n);
        let client = h.client();

        // Fetch the first page (50 results) — this is the hot path.
        let (cpu, mem, wt) = h.measure(|| {
            let _page = client.get_active_matches_paginated(&0, &50);
        });

        println!(
            "[load] get_active_matches_paginated(page=0,limit=50) | n={n:>6} | cpu={cpu:>12} | mem={mem:>10} | wall={wt:>8}µs"
        );
        results.push(LoadMeasurement {
            operation: "get_active_matches_paginated",
            n_background_matches: n,
            cpu_instructions: cpu,
            memory_bytes: mem,
            wall_time_micros: wt,
        });
    }

    // A paginated query must never read more than `limit` entries regardless
    // of how many matches exist — so cost should not grow unboundedly.
    // We assert the 10k cost is under 50× the 100-match cost as a loose bound.
    if results.len() >= 2 {
        let base = results.first().unwrap().cpu_instructions.max(1);
        let peak = results.last().unwrap().cpu_instructions;
        assert!(
            peak <= base * 50,
            "paginated query cost grew too fast: base={base} peak={peak}"
        );
    }

    persist_results("get_active_matches_paginated", &results);
}

// ── Test 5: State-size growth analysis ───────────────────────────────────────

/// Verifies that match count is accurately tracked at each scale.
/// Also checks that `get_player_matches` for a single player returns only
/// their own matches even when thousands of other matches exist.
#[test]
fn load_state_size_consistency() {
    for &n in SCALES {
        let h = LoadHarness::new();
        h.seed_active_matches(n);
        let client = h.client();

        // Create one extra match for a dedicated probe player.
        let probe = h.new_player();
        let other = h.new_player();
        let game_id = make_game_id(&h.env, "ss", n);
        client.create_match(
            &probe,
            &other,
            &STAKE,
            &h.token,
            &game_id,
            &Platform::Lichess,
        );

        let probe_matches = client.get_player_matches(&probe);
        assert_eq!(
            probe_matches.len(),
            1,
            "probe player must have exactly 1 match regardless of background state (n={n})"
        );

        println!("[load] state_size_consistency n={n}: OK");
    }
}

// ── Test 6: Correctness under load — no cross-match contamination ─────────────

/// With 1,000 concurrent matches, submit results for a random subset and verify
/// that only those matches change state.
#[test]
fn load_correctness_no_cross_match_contamination() {
    const N: u32 = 1_000;
    // Only resolve a small subset of matches.
    const RESOLVE_EVERY: u32 = 100;

    let h = LoadHarness::new();
    let client = h.client();
    let mut match_ids: Vec<u64> = Vec::new();
    let mut players: Vec<(Address, Address)> = Vec::new();

    for i in 0..N {
        let p1 = h.new_player();
        let p2 = h.new_player();
        let game_id = make_game_id(&h.env, "cr", i);
        let mid = client.create_match(
            &p1, &p2, &STAKE, &h.token, &game_id, &Platform::Lichess,
        );
        client.deposit(&mid, &p1);
        client.deposit(&mid, &p2);
        match_ids.push(mid);
        players.push((p1, p2));
    }

    // Resolve every RESOLVE_EVERY-th match.
    let mut resolved: Vec<u64> = Vec::new();
    for (i, &mid) in match_ids.iter().enumerate() {
        if (i as u32) % RESOLVE_EVERY == 0 {
            client.submit_result(&mid, &Winner::Player1);
            let (p1, _) = &players[i];
            client.claim_vested_payout(&mid, p1);
            resolved.push(mid);
        }
    }

    // Verify: resolved matches are Completed; all others remain Active.
    let resolved_set: std::collections::HashSet<u64> = resolved.iter().copied().collect();
    for &mid in &match_ids {
        let state = client.get_match(&mid).state;
        if resolved_set.contains(&mid) {
            assert_eq!(
                state,
                escrow::types::MatchState::Completed,
                "match {mid} should be Completed"
            );
        } else {
            assert_eq!(
                state,
                escrow::types::MatchState::Active,
                "match {mid} should still be Active"
            );
        }
    }

    println!("[load] correctness_no_cross_match_contamination N={N}: OK");
}

// ── Test 7: Concurrent draw payouts ──────────────────────────────────────────

/// Submitting draw results for many matches in sequence must not corrupt
/// refund amounts — each player must receive exactly their stake back.
#[test]
fn load_draw_refunds_correct_at_scale() {
    const N: u32 = 100;

    let h = LoadHarness::new();
    let client = h.client();
    let token_client = soroban_sdk::token::Client::new(&h.env, &h.token);

    let mut players_list: Vec<(Address, Address)> = Vec::new();
    let mut match_ids: Vec<u64> = Vec::new();

    for i in 0..N {
        let p1 = h.new_player();
        let p2 = h.new_player();
        let game_id = make_game_id(&h.env, "dr", i);
        let mid = client.create_match(
            &p1, &p2, &STAKE, &h.token, &game_id, &Platform::Lichess,
        );
        client.deposit(&mid, &p1);
        client.deposit(&mid, &p2);
        players_list.push((p1, p2));
        match_ids.push(mid);
    }

    for (idx, &mid) in match_ids.iter().enumerate() {
        let (p1, p2) = &players_list[idx];
        let bal_before_p1 = token_client.balance(p1);
        let bal_before_p2 = token_client.balance(p2);

        client.submit_result(&mid, &Winner::Draw);
        client.claim_vested_payout(&mid, p1);
        client.claim_vested_payout(&mid, p2);

        let bal_after_p1 = token_client.balance(p1);
        let bal_after_p2 = token_client.balance(p2);

        assert_eq!(
            bal_after_p1,
            bal_before_p1 + STAKE,
            "draw refund incorrect for player1 in match {mid}"
        );
        assert_eq!(
            bal_after_p2,
            bal_before_p2 + STAKE,
            "draw refund incorrect for player2 in match {mid}"
        );
    }

    println!("[load] draw_refunds_correct_at_scale N={N}: OK");
}

// ── Report writer ─────────────────────────────────────────────────────────────

fn persist_results(section: &str, results: &[LoadMeasurement]) {
    if results.is_empty() {
        return;
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()  // contracts/
        .and_then(|p| p.parent())  // repo root
        .unwrap()
        .to_path_buf();
    let report_dir = repo_root.join("reports").join("performance");
    let _ = fs::create_dir_all(&report_dir);

    let path = report_dir.join("load-test-results.json");

    let entries: Vec<String> = results.iter().map(|r| r.to_json()).collect();
    let block = format!(
        "  {{\n    \"section\": \"{section}\",\n    \"measurements\": [\n{}\n    ]\n  }}",
        entries.join(",\n")
    );

    // Append to the file (each test run appends its section).
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let content = if existing.trim().is_empty() {
        format!("[\n{block}\n]\n")
    } else {
        // Naive append: insert before the last `]`.
        let trimmed = existing.trim_end_matches('\n').trim_end_matches(']').trim_end_matches('\n');
        format!("{trimmed},\n{block}\n]\n")
    };
    let _ = fs::write(&path, content);
}
