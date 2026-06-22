#!/usr/bin/env bash
# Runs the escrow contract's performance benchmarking suite and writes a JSON
# report to reports/performance/benchmark-results.json.
#
# See contracts/escrow/tests/benchmarks.rs for the measured scenarios and
# docs/performance-report.md for the latest committed results and analysis.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Running escrow contract benchmarks..."
cargo test -p escrow --test benchmarks -- --nocapture --test-threads=1

REPORT="$REPO_ROOT/reports/performance/benchmark-results.json"
if [[ ! -f "$REPORT" ]]; then
  echo "Benchmark report was not generated at $REPORT" >&2
  exit 1
fi

echo "Benchmark report written to $REPORT"
