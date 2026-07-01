#!/usr/bin/env bash
# scripts/security-scan.sh — Local security scanning for Checkmate-Escrow
# Usage: bash scripts/security-scan.sh [--report]
#
# Runs: cargo-audit, clippy (deny warnings), and the property-based fuzz tests.
# Pass --report to write results to reports/security/.

set -euo pipefail

REPORT=false
if [[ "${1:-}" == "--report" ]]; then
    REPORT=true
fi

REPORT_DIR="reports/security"
TIMESTAMP="$(date -u +%Y%m%dT%H%M%SZ)"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "${GREEN}[PASS]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; }
info() { echo -e "${YELLOW}[INFO]${NC} $*"; }

OVERALL=0

# ── 1. cargo-audit ────────────────────────────────────────────────────────────
info "Running cargo-audit..."
if ! command -v cargo-audit &>/dev/null; then
    info "cargo-audit not installed — installing..."
    cargo install cargo-audit --locked --quiet
fi

AUDIT_ARGS=()
if $REPORT; then
    mkdir -p "$REPORT_DIR"
    AUDIT_ARGS+=(--json)
fi

if $REPORT; then
    if cargo audit "${AUDIT_ARGS[@]}" > "$REPORT_DIR/audit-${TIMESTAMP}.json" 2>&1; then
        pass "cargo-audit: no vulnerabilities found"
    else
        fail "cargo-audit: vulnerabilities detected (see $REPORT_DIR/audit-${TIMESTAMP}.json)"
        OVERALL=1
    fi
else
    if cargo audit; then
        pass "cargo-audit: no vulnerabilities found"
    else
        fail "cargo-audit: vulnerabilities detected"
        OVERALL=1
    fi
fi

# ── 2. Clippy (deny warnings) ─────────────────────────────────────────────────
info "Running clippy..."
CLIPPY_OUT=""
if CLIPPY_OUT=$(cargo clippy --all-targets --all-features -- -D warnings 2>&1); then
    pass "clippy: no warnings"
else
    fail "clippy: warnings or errors found"
    echo "$CLIPPY_OUT"
    OVERALL=1
fi

if $REPORT; then
    echo "$CLIPPY_OUT" > "$REPORT_DIR/clippy-${TIMESTAMP}.txt"
fi

# ── 3. Property-based fuzz tests ──────────────────────────────────────────────
info "Running property-based fuzz tests..."
FUZZ_OUT=""
if FUZZ_OUT=$(cargo test -p escrow fuzz -- --test-threads=1 2>&1); then
    pass "fuzz tests: all passed"
else
    fail "fuzz tests: failures detected"
    echo "$FUZZ_OUT"
    OVERALL=1
fi

if $REPORT; then
    echo "$FUZZ_OUT" > "$REPORT_DIR/fuzz-${TIMESTAMP}.txt"
fi

# ── 4. Full test suite ────────────────────────────────────────────────────────
info "Running full test suite..."
if cargo test -p escrow 2>&1 | tee ${REPORT:+"$REPORT_DIR/tests-${TIMESTAMP}.txt"} | grep -E "^test result"; then
    pass "test suite: all tests passed"
else
    fail "test suite: failures detected"
    OVERALL=1
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
if [[ $OVERALL -eq 0 ]]; then
    pass "All security checks passed."
    $REPORT && info "Reports written to $REPORT_DIR/"
else
    fail "One or more security checks failed."
    exit 1
fi
