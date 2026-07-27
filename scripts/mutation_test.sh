#!/usr/bin/env bash
# Run cargo-mutants against Soroban contract packages.
#
# Usage:
#   ./scripts/mutation_test.sh              # escrow (default)
#   ./scripts/mutation_test.sh escrow
#   ./scripts/mutation_test.sh oracle
#   ./scripts/mutation_test.sh escrow --list # list mutants only
#   ./scripts/mutation_test.sh escrow --re 'deposit|submit_result|cancel_match'
#
# Requires: cargo install --locked cargo-mutants
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PACKAGE="${1:-escrow}"
shift || true

case "$PACKAGE" in
  escrow|oracle) ;;
  -h|--help|help)
    sed -n '2,12p' "$0"
    exit 0
    ;;
  *)
    echo "Unknown package '$PACKAGE'. Expected 'escrow' or 'oracle'." >&2
    exit 1
    ;;
esac

if ! command -v cargo-mutants >/dev/null 2>&1 && ! cargo mutants --version >/dev/null 2>&1; then
  echo "cargo-mutants is not installed. Install with:" >&2
  echo "  cargo install --locked cargo-mutants" >&2
  exit 1
fi

PKG_DIR="contracts/${PACKAGE}"
CONFIG="${PKG_DIR}/.cargo/mutants.toml"
OUT_DIR="${ROOT}/mutants-out/${PACKAGE}"
mkdir -p "$OUT_DIR"

JOBS="${CARGO_MUTANTS_JOBS:-2}"

# --list / --list-files should print to stdout; don't pass --output for those.
LIST_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --list|--list-files) LIST_ONLY=1 ;;
  esac
done

echo "Running cargo-mutants for package '${PACKAGE}'..." >&2
echo "  dir:    ${PKG_DIR}" >&2
echo "  config: ${CONFIG}" >&2
echo "  jobs:   ${JOBS}" >&2
if [[ "$LIST_ONLY" -eq 0 ]]; then
  echo "  output: ${OUT_DIR}" >&2
fi

ARGS=(--config .cargo/mutants.toml --jobs "$JOBS")
if [[ "$LIST_ONLY" -eq 0 ]]; then
  ARGS+=(--output "$OUT_DIR")
fi
ARGS+=("$@")

# Run from the package directory; cargo-mutants still resolves workspace paths.
(
  cd "$PKG_DIR"
  cargo mutants "${ARGS[@]}"
)

if [[ "$LIST_ONLY" -eq 0 ]]; then
  echo "" >&2
  echo "Done. Review missed mutants under ${OUT_DIR}/ and see docs/mutation-test-report.md." >&2
fi
