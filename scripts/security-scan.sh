#!/usr/bin/env bash
# scripts/security-scan.sh — Local security scanning for Checkmate-Escrow
#
# Usage:
#   bash scripts/security-scan.sh [--report]
#
# Runs:
#   1. cargo-audit   — check for known vulnerable dependencies (#984)
#   2. cargo-deny    — license compliance and crate bans (#984)
#   3. semgrep       — SAST code pattern analysis (#984)
#   4. clippy        — Rust linting with deny-warnings
#   5. cargo test    — full test suite
#
# Pass --report to write machine-readable results to reports/security/.

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

pass()  { echo -e "${GREEN}[PASS]${NC} $*"; }
fail()  { echo -e "${RED}[FAIL]${NC} $*"; }
info()  { echo -e "${YELLOW}[INFO]${NC} $*"; }
skip()  { echo -e "${YELLOW}[SKIP]${NC} $*"; }

OVERALL=0

# ── 1. cargo-audit ────────────────────────────────────────────────────────────
info "Running cargo-audit..."
if ! command -v cargo-audit &>/dev/null; then
    info "cargo-audit not installed — installing..."
    cargo install cargo-audit --locked --quiet
fi

if $REPORT; then
    mkdir -p "$REPORT_DIR"
    AUDIT_OUT="$REPORT_DIR/cargo-audit-${TIMESTAMP}.json"
    if cargo audit --json > "$AUDIT_OUT" 2>&1; then
        pass "cargo-audit: no vulnerabilities found"
    else
        fail "cargo-audit: vulnerabilities detected (see $AUDIT_OUT)"
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

# ── 2. cargo-deny ─────────────────────────────────────────────────────────────
info "Running cargo-deny..."
if ! command -v cargo-deny &>/dev/null; then
    info "cargo-deny not installed — installing..."
    cargo install cargo-deny --locked --quiet
fi

if $REPORT; then
    mkdir -p "$REPORT_DIR"
    DENY_OUT="$REPORT_DIR/cargo-deny-${TIMESTAMP}.txt"
    if cargo deny check 2>&1 | tee "$DENY_OUT"; then
        pass "cargo-deny: all license and ban checks passed"
    else
        fail "cargo-deny: license or ban violations found (see $DENY_OUT)"
        OVERALL=1
    fi
else
    if cargo deny check; then
        pass "cargo-deny: all license and ban checks passed"
    else
        fail "cargo-deny: license or ban violations found"
        OVERALL=1
    fi
fi

# ── 3. semgrep SAST ───────────────────────────────────────────────────────────
info "Running semgrep SAST..."
if ! command -v semgrep &>/dev/null; then
    skip "semgrep not installed — install with: pip install semgrep"
    skip "Skipping SAST scan. Run 'semgrep --config p/rust .' manually."
else
    if $REPORT; then
        mkdir -p "$REPORT_DIR"
        SEMGREP_OUT="$REPORT_DIR/semgrep-${TIMESTAMP}.json"
        if semgrep \
            --config "p/rust" \
            --config "p/secrets" \
            --error \
            --json \
            --output "$SEMGREP_OUT" \
            --exclude "target/**" \
            --exclude "node_modules/**" \
            . 2>&1; then
            FINDING_COUNT=$(jq '.results | length' "$SEMGREP_OUT" 2>/dev/null || echo 0)
            pass "semgrep: no findings ($FINDING_COUNT results in $SEMGREP_OUT)"
        else
            FINDING_COUNT=$(jq '.results | length' "$SEMGREP_OUT" 2>/dev/null || echo "unknown")
            fail "semgrep: ${FINDING_COUNT} finding(s) — see $SEMGREP_OUT"
            OVERALL=1
        fi
    else
        if semgrep \
            --config "p/rust" \
            --config "p/secrets" \
            --error \
            --exclude "target/**" \
            --exclude "node_modules/**" \
            . 2>&1; then
            pass "semgrep: no SAST findings"
        else
            fail "semgrep: SAST findings detected"
            OVERALL=1
        fi
    fi
fi

# ── 4. Clippy (deny warnings) ─────────────────────────────────────────────────
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
    mkdir -p "$REPORT_DIR"
    echo "$CLIPPY_OUT" > "$REPORT_DIR/clippy-${TIMESTAMP}.txt"
fi

# ── 5. Full test suite ────────────────────────────────────────────────────────
info "Running full test suite (oracle-service)..."
if cargo test -p oracle-service 2>&1 | tee ${REPORT:+"$REPORT_DIR/tests-${TIMESTAMP}.txt"} | grep -E "^test result"; then
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
