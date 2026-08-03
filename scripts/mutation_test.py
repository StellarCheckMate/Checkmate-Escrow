#!/usr/bin/env python3
"""Manual mutation testing script for the escrow Soroban contract.

Applies each mutation to lib.rs, runs passing tests, records results,
and restores the original.  Builds a final report
"""

import json
import os
import subprocess
import sys
import time
import re

ESCROW_SRC = os.path.join(os.path.dirname(__file__), "..", "contracts", "escrow", "src", "lib.rs")
BACKUP = ESCROW_SRC + ".bak"
WORKDIR = os.path.join(os.path.dirname(__file__), "..")

# Known-failing test modules to exclude
SKIP_MODULES = [
    "kani_harness",
    "dispute",
    "dispute_rollback",
    "lifecycle",
    "player_balance_history",
    "balance_history_edge_cases",
    "oracle_validation",
    "pagination",
    "security",
    "tier::",
    "ttl",
    "integration",
    "multi_token",
    "index",
    "fee_calculation_scenarios",
    "invariants",
]

def build_skip_args():
    result = []
    for m in SKIP_MODULES:
        result.extend(["--skip", m])
    return result

def apply_mutation(old_str, new_str):
    """Apply mutation by replacing text in lib.rs"""
    with open(ESCROW_SRC) as f:
        content = f.read()
    if old_str not in content:
        print(f"  ERROR: old_string not found: {old_str[:60]}")
        return False
    count = content.count(old_str)
    if count > 1:
        print(f"  WARNING: old_string appears {count} times, replacing only first")
    content = content.replace(old_str, new_str, 1)
    with open(ESCROW_SRC, "w") as f:
        f.write(content)
    return True

def restore_original():
    """Restore original lib.rs from backup"""
    if os.path.exists(BACKUP):
        with open(BACKUP) as f:
            original = f.read()
        with open(ESCROW_SRC, "w") as f:
            f.write(original)

def run_tests():
    """Run the lib tests with known-failing modules excluded"""
    skip_args = build_skip_args()
    cmd = ["cargo", "test", "-p", "escrow", "--lib", "--"] + skip_args
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=300,
        cwd=WORKDIR,
    )
    return result.returncode, result.stdout, result.stderr

# Define mutations: list of (name, old_string, new_string)
# Most are operator swaps from cargo-mutants output
MUTATIONS = [
    # Line 176: == -> != in initialize (checks oracle != contract)
    ("init_eq_to_neq", "if oracle == env.current_contract_address()", "if oracle != env.current_contract_address()"),
    # Line 261: > -> < in set_protocol_config (checks fee_bps > 10000)
    ("proto_fee_gt_to_lt", "if config.protocol_fee_bps > 10_000", "if config.protocol_fee_bps < 10_000"),
    # Line 850: == -> != in validate_game_id_format (checks len == 0)
    ("gameid_eq_to_neq", "if len == 0 || len > MAX_GAME_ID_LEN", "if len != 0 || len > MAX_GAME_ID_LEN"),
    # Line 850: > -> < in validate_game_id_format (checks len > MAX)
    ("gameid_gt_to_lt", "if len == 0 || len > MAX_GAME_ID_LEN", "if len == 0 || len < MAX_GAME_ID_LEN"),
    # Line 850: || -> && in validate_game_id_format
    ("gameid_or_to_and", "if len == 0 || len > MAX_GAME_ID_LEN", "if len == 0 && len > MAX_GAME_ID_LEN"),
    # Line 860: || -> && in Lichess check
    ("lichess_or_to_and", "if len != LICHESS_GAME_ID_LEN || !slice.iter()", "if len != LICHESS_GAME_ID_LEN && !slice.iter()"),
    # Line 865-867: similar || -> && for Chess.com check
    ("chess_or_to_and", "if len < CHESS_COM_GAME_ID_MIN_LEN\n                    || len > CHESS_COM_GAME_ID_MAX_LEN\n                    || !slice.iter()", "if len < CHESS_COM_GAME_ID_MIN_LEN\n                    && len > CHESS_COM_GAME_ID_MAX_LEN\n                    && !slice.iter()"),
    # Line 933: <= -> > in create_match (stake check)
    ("stake_le_to_gt", "if stake_amount <= 0 || stake_amount < protocol_cfg.minimum_stake", "if stake_amount > 0 || stake_amount < protocol_cfg.minimum_stake"),
    # Line 933: < -> > in create_match (minimum stake)
    ("minstake_lt_to_gt", "if stake_amount <= 0 || stake_amount < protocol_cfg.minimum_stake", "if stake_amount <= 0 || stake_amount > protocol_cfg.minimum_stake"),
    # Line 933: || -> && in create_match (stake checks)
    ("stake_or_to_and", "if stake_amount <= 0 || stake_amount < protocol_cfg.minimum_stake", "if stake_amount <= 0 && stake_amount < protocol_cfg.minimum_stake"),
    # Line 946: == -> != in create_match (player1 == player2)
    ("players_eq_to_neq", "if player1 == player2", "if player1 != player2"),
    # Line 2443: <= -> > in set_minimum_stake
    ("minstake_le_to_gt_2", "if amount <= 0", "if amount > 0"),
    # Line 386: == -> != in add_allowed_token (count == 0)
    ("tokencnt_eq_to_neq", "if count == 0", "if count != 0"),
    # Line 430: == -> != in remove_allowed_token
    ("tokencnt_rm_eq_to_neq", "if next_count == 0", "if next_count != 0"),
    # Line 763: <= -> > in set_fee_tiers (max_stake ordering)
    ("feetier_le_to_gt", "if tier.max_stake <= prev_max", "if tier.max_stake > prev_max"),
    # Line 818: <= -> > in compute_tiered_fee
    ("tierfee_le_to_gt", "if stake_amount <= tier.max_stake", "if stake_amount > tier.max_stake"),
]

def main():
    results = []
    total = len(MUTATIONS)
    
    # First, verify baseline passes
    print("Running baseline test suite (known-failing modules excluded)...")
    rc, stdout, stderr = run_tests()
    if rc != 0:
        print("BASELINE TESTS FAIL. Cannot proceed with mutation testing.")
        print(stderr[-2000:] if len(stderr) > 2000 else stderr)
        sys.exit(1)
    
    # Parse baseline passing tests
    baseline_passed = 0
    for line in stdout.split("\n"):
        if "test result:" in line:
            import re
            m = re.search(r'(\d+)\s+passed', line)
            if m:
                baseline_passed = int(m.group(1))
                break
    print(f"Baseline: {baseline_passed} tests passing\n")
    
    for i, (name, old, new) in enumerate(MUTATIONS, 1):
        print(f"[{i}/{total}] Testing mutation: {name}")
        restore_original()
        if not apply_mutation(old, new):
            results.append({"name": name, "status": "ERROR", "detail": "old_string not found"})
            print(f"  -> ERROR: could not apply mutation\n")
            continue
        
        try:
            rc, stdout, stderr = run_tests()
        except subprocess.TimeoutExpired:
            results.append({"name": name, "status": "TIMEOUT", "detail": ""})
            print(f"  -> TIMEOUT\n")
            continue
        
        if rc == 0:
            results.append({"name": name, "status": "MISSED", "detail": "Tests still pass"})
            print(f"  -> MISSED - tests did not catch this mutation\n")
        else:
            # Parse which tests failed
            failed = []
            for line in stdout.split("\n"):
                if "FAILED" in line and "tests::" in line:
                    failed.append(line.strip())
            results.append({"name": name, "status": "CAUGHT", "detail": "; ".join(failed)})
            print(f"  -> CAUGHT by {len(failed)} test(s)\n")
    
    # Restore original
    restore_original()
    
    # Print report
    print("\n" + "=" * 70)
    print("MUTATION TESTING RESULTS")
    print("=" * 70)
    
    caught = [r for r in results if r["status"] == "CAUGHT"]
    missed = [r for r in results if r["status"] == "MISSED"]
    errors = [r for r in results if r["status"] not in ("CAUGHT", "MISSED")]
    
    print(f"\nTotal mutations attempted: {total}")
    print(f"Caught by tests:           {len(caught)}")
    print(f"Missed by tests:           {len(missed)}")
    print(f"Errors/timeouts:           {len(errors)}")
    print(f"\nMutation score:           {len(caught) / (len(caught) + len(missed)) * 100:.1f}%")
    
    if missed:
        print(f"\nMISSED MUTATIONS (false negatives):")
        for r in missed:
            print(f"  - {r['name']}")
    
    if caught:
        print(f"\nCAUGHT MUTATIONS:")
        for r in caught:
            print(f"  - {r['name']}: {r['detail']}")
    
    # Save results as JSON
    report = {
        "baseline_passing": baseline_passed,
        "total_mutations": total,
        "caught": len(caught),
        "missed": len(missed),
        "errors": len(errors),
        "mutation_score": len(caught) / (len(caught) + len(missed)) * 100 if (caught or missed) else 0,
        "details": results,
    }
    report_path = os.path.join(WORKDIR, "mutants.out", "mutation_results.json")
    os.makedirs(os.path.dirname(report_path), exist_ok=True)
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)
    print(f"\nFull results saved to {report_path}")

if __name__ == "__main__":
    main()
