#!/usr/bin/env bash
# smoke_test_oracle.sh — Oracle post-deploy sanity check for Checkmate-Escrow
#
# Verifies that the oracle service is correctly connected to the escrow contract
# by checking three things:
#   1. The oracle health endpoint returns HTTP 200.
#   2. The oracle's configured contract address matches $CONTRACT_ESCROW.
#   3. The escrow contract's on-chain oracle address matches the oracle's signing key.
#
# Usage:
#   ./scripts/smoke_test_oracle.sh [CONTRACT_ESCROW] [ORACLE_URL] [NETWORK]
#
# Arguments (all optional — fall back to environment / .env):
#   CONTRACT_ESCROW   Escrow contract ID (C…)
#   ORACLE_URL        Oracle service base URL (e.g. http://localhost:8080)
#   NETWORK           Stellar network name from environments.toml (default: testnet)
#
# Required tools: curl, jq, stellar
#
# Exit codes:
#   0 — all checks passed (PASS)
#   1 — one or more checks failed (FAIL)
#
# Examples:
#   # Use environment / .env defaults
#   ./scripts/smoke_test_oracle.sh
#
#   # Override inline
#   CONTRACT_ESCROW=CABC... ORACLE_URL=http://oracle:8080 ./scripts/smoke_test_oracle.sh
#
#   # Positional arguments
#   ./scripts/smoke_test_oracle.sh CABC... http://oracle:8080 testnet

set -euo pipefail

# ── Load .env if present ───────────────────────────────────────────────────────
if [[ -f ".env" ]]; then
    set -o allexport
    # shellcheck source=/dev/null
    source .env
    set +o allexport
fi

# ── Resolve arguments / environment ──────────────────────────────────────────
CONTRACT_ESCROW="${1:-${CONTRACT_ESCROW:-}}"
ORACLE_URL="${2:-${ORACLE_URL:-http://localhost:8080}}"
NETWORK="${3:-${STELLAR_NETWORK:-testnet}}"

# ── Colour helpers ────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Colour

pass() { echo -e "  ${GREEN}✅ PASS${NC}  $*"; }
fail() { echo -e "  ${RED}❌ FAIL${NC}  $*"; OVERALL_FAIL=1; }
info() { echo -e "  ${YELLOW}ℹ️  INFO${NC}  $*"; }

OVERALL_FAIL=0

echo ""
echo "══════════════════════════════════════════════════════"
echo "  Checkmate-Escrow — Oracle Smoke Test"
echo "══════════════════════════════════════════════════════"
echo "  Contract : ${CONTRACT_ESCROW:-(not set)}"
echo "  Oracle   : ${ORACLE_URL}"
echo "  Network  : ${NETWORK}"
echo ""

# ── Pre-flight: required tools ────────────────────────────────────────────────
echo "▶ Pre-flight checks"
for tool in curl jq stellar; do
    if command -v "$tool" &>/dev/null; then
        pass "$tool is available"
    else
        fail "$tool not found — install it before running this script"
        OVERALL_FAIL=1
    fi
done

if [[ $OVERALL_FAIL -ne 0 ]]; then
    echo ""
    echo -e "${RED}Pre-flight failed — cannot continue.${NC}"
    exit 1
fi

# ── Pre-flight: CONTRACT_ESCROW must be set ───────────────────────────────────
if [[ -z "$CONTRACT_ESCROW" ]]; then
    echo ""
    echo -e "${RED}❌ CONTRACT_ESCROW is not set.${NC}"
    echo "   Set it in .env, export it, or pass it as the first argument:"
    echo "   ./scripts/smoke_test_oracle.sh <CONTRACT_ESCROW> [ORACLE_URL] [NETWORK]"
    exit 1
fi

# ── Check 1: Oracle health endpoint ─────────────────────────────────────────
echo ""
echo "▶ Check 1 — Oracle health endpoint"

HTTP_STATUS=$(curl --silent --output /dev/null --write-out "%{http_code}" \
    --max-time 10 "${ORACLE_URL}/health" 2>/dev/null || echo "000")

if [[ "$HTTP_STATUS" == "200" ]]; then
    pass "GET ${ORACLE_URL}/health → HTTP ${HTTP_STATUS}"
else
    fail "GET ${ORACLE_URL}/health returned HTTP ${HTTP_STATUS} (expected 200)"
    info "Is the oracle service running?  Check: docker ps  or  systemctl status oracle"
fi

# ── Check 2: Oracle's configured contract address matches CONTRACT_ESCROW ──
echo ""
echo "▶ Check 2 — Oracle reports correct escrow contract address"

HEALTH_BODY=$(curl --silent --max-time 10 "${ORACLE_URL}/health" 2>/dev/null || echo "{}")

# The oracle health endpoint returns JSON with a `contract_id` (or
# `escrow_contract`) field. Try both field names for forward-compatibility.
ORACLE_CONTRACT=$(echo "$HEALTH_BODY" \
    | jq -r '.contract_id // .escrow_contract // .escrow_contract_id // empty' 2>/dev/null \
    || echo "")

if [[ -z "$ORACLE_CONTRACT" ]]; then
    fail "Could not read contract address from oracle health response."
    info "Raw response: ${HEALTH_BODY}"
    info "Expected JSON field: contract_id  (or escrow_contract / escrow_contract_id)"
else
    if [[ "$ORACLE_CONTRACT" == "$CONTRACT_ESCROW" ]]; then
        pass "Oracle contract_id matches CONTRACT_ESCROW (${CONTRACT_ESCROW})"
    else
        fail "Contract address mismatch!"
        info "  Oracle reports : ${ORACLE_CONTRACT}"
        info "  CONTRACT_ESCROW: ${CONTRACT_ESCROW}"
        info "Fix: update the oracle's CONTRACT_ESCROW / ESCROW_CONTRACT_ID env var and restart."
    fi
fi

# ── Check 3: On-chain oracle address matches the oracle's signing key ────────
echo ""
echo "▶ Check 3 — Escrow contract's on-chain oracle address"

# Query get_oracle_address from the escrow contract.
ON_CHAIN_ORACLE=$(stellar contract invoke \
    --id "$CONTRACT_ESCROW" \
    --network "$NETWORK" \
    -- get_oracle_address 2>/dev/null || echo "")

if [[ -z "$ON_CHAIN_ORACLE" ]]; then
    fail "Could not query get_oracle_address on escrow contract ${CONTRACT_ESCROW}."
    info "Ensure CONTRACT_ESCROW is correct and the contract is initialized."
    info "Debug: stellar contract invoke --id $CONTRACT_ESCROW --network $NETWORK -- get_oracle_address"
else
    # Strip surrounding quotes if the CLI wraps the address in them.
    ON_CHAIN_ORACLE="${ON_CHAIN_ORACLE//\"/}"

    # The oracle health endpoint also exposes the signer's public key.
    ORACLE_SIGNER=$(echo "$HEALTH_BODY" \
        | jq -r '.oracle_address // .signer_public_key // .signing_key // empty' 2>/dev/null \
        || echo "")

    if [[ -z "$ORACLE_SIGNER" ]]; then
        # Cannot compare — report what's on-chain and let the operator verify.
        info "On-chain oracle address: ${ON_CHAIN_ORACLE}"
        info "Oracle health did not expose a signer key field."
        info "Manually verify the on-chain address matches the oracle's signing key."
        info "Expected JSON field: oracle_address  (or signer_public_key / signing_key)"
    else
        if [[ "$ON_CHAIN_ORACLE" == "$ORACLE_SIGNER" ]]; then
            pass "On-chain oracle address matches oracle signing key (${ON_CHAIN_ORACLE})"
        else
            fail "Oracle address mismatch!"
            info "  On-chain oracle : ${ON_CHAIN_ORACLE}"
            info "  Oracle signer   : ${ORACLE_SIGNER}"
            info "Fix: call update_oracle on the escrow contract with the correct oracle address,"
            info "     or reconfigure the oracle service to use the registered signing key."
        fi
    fi
fi

# ── Summary ───────────────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════════"
if [[ $OVERALL_FAIL -eq 0 ]]; then
    echo -e "  ${GREEN}✅ PASS — All oracle smoke tests passed.${NC}"
else
    echo -e "  ${RED}❌ FAIL — One or more oracle smoke tests failed.${NC}"
    echo ""
    echo "  Consult the FAIL lines above for actionable next steps."
    echo "  See also: docs/deployment.md § Post-Deploy Verification"
fi
echo "══════════════════════════════════════════════════════"
echo ""

exit $OVERALL_FAIL
