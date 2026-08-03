#!/usr/bin/env bash
# Checkmate-Escrow post-deployment verification
set -euo pipefail

usage() {
    echo "Usage: $0 <network> <escrow_contract_id> <oracle_contract_id>"
    echo ""
    echo "Verifies that deployed contracts are properly initialized and responding."
    echo ""
    echo "Arguments:"
    echo "  network                Contract network (testnet, mainnet, standalone)"
    echo "  escrow_contract_id     Escrow contract ID"
    echo "  oracle_contract_id     Oracle contract ID"
    echo ""
    exit 1
}

NETWORK="${1:-}"
ESCROW_CONTRACT_ID="${2:-}"
ORACLE_CONTRACT_ID="${3:-}"

[[ -z "$NETWORK" || -z "$ESCROW_CONTRACT_ID" || -z "$ORACLE_CONTRACT_ID" ]] && usage

PASS=0
FAIL=0

check() {
    local label="$1"
    local cmd="$2"
    if eval "$cmd" &>/dev/null; then
        echo "   ✅ $label"
        (( PASS++ )) || true
    else
        echo "   ❌ $label"
        (( FAIL++ )) || true
    fi
}

check_value() {
    local label="$1"
    local cmd="$2"
    local output
    output=$(eval "$cmd" 2>/dev/null || echo "")
    if [[ -n "$output" && "$output" != "null" && "$output" != '""' ]]; then
        echo "   ✅ $label: $output"
        (( PASS++ )) || true
    else
        echo "   ❌ $label: not set or invalid"
        (( FAIL++ )) || true
    fi
}

echo "🔍 Verifying $NETWORK deployment..."
echo "   Escrow:  $ESCROW_CONTRACT_ID"
echo "   Oracle:  $ORACLE_CONTRACT_ID"
echo ""

# Escrow contract checks
echo "📋 Escrow Contract Checks:"
check "Escrow: contract is deployed" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_admin"

check_value "Escrow: admin address is set" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_admin"

check_value "Escrow: oracle address is set" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_oracle"

check "Escrow: get_match_timeout responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_match_timeout"

check "Escrow: get_pending_matches responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_pending_matches"

check "Escrow: get_active_matches responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_active_matches"

# Oracle contract checks
echo ""
echo "📋 Oracle Contract Checks:"
check "Oracle: contract is deployed" \
    "stellar contract invoke --id '$ORACLE_CONTRACT_ID' --network '$NETWORK' -- get_admin"

check_value "Oracle: admin address is set" \
    "stellar contract invoke --id '$ORACLE_CONTRACT_ID' --network '$NETWORK' -- get_admin"

echo ""
echo "📊 Result: $PASS passed, $FAIL failed"

if [[ "$FAIL" -gt 0 ]]; then
    echo ""
    echo "❌ Verification failed. Check the contract IDs and network."
    echo "   See docs/error-codes.md for error code reference."
    echo "   See docs/deployment.md for troubleshooting steps."
    exit 1
fi

echo "✅ All checks passed. Deployment is ready."
exit 0
