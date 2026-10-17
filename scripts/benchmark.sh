#!/usr/bin/env bash
#
# scripts/benchmark.sh
#
# Runs the escrow contract's performance benchmarking suite and writes a JSON
# report to reports/performance/benchmark-results.json.
#
# What it measures:
#   The benchmark scenarios defined in contracts/escrow/tests/benchmarks.rs
#   (e.g. escrow creation, funding, release, and refund flows). These tests are
#   marked `#[ignore]` so they do not run as part of the normal test suite.
#
# When to run it:
#   - Before opening a PR that touches performance-sensitive code paths in the
#     escrow contract (storage access, token transfers, state transitions).
#   - When investigating a suspected performance regression.
#   - When refreshing the committed results in docs/performance-report.md.
#
# Prerequisites:
#   - A Rust toolchain with `cargo` available on PATH.
#   - Run from anywhere; the script resolves the repository root itself.
#   - The benchmark suite must be able to write to reports/performance/.
#
# Usage:
#   ./scripts/benchmark.sh
#
# See contracts/escrow/tests/benchmarks.rs for the measured scenarios and
# docs/performance-report.md for the latest committed results and analysis.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Running escrow contract benchmarks..."
cargo test -p escrow --test benchmarks -- --ignored --nocapture --test-threads=1

REPORT="$REPO_ROOT/reports/performance/benchmark-results.json"
if [[ ! -f "$REPORT" ]]; then
  echo "Benchmark report was not generated at $REPORT" >&2
  exit 1
fi

echo "Benchmark report written to $REPORT"