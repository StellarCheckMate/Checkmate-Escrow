#!/usr/bin/env bash
# Checkmate-Escrow post-deployment verification
set -euo pipefail

usage() {
    echo "Usage: $0 <network> <escrow_contract_id> <oracle_contract_id>"
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

echo "🔍 Verifying $NETWORK deployment..."
echo "   Escrow:  $ESCROW_CONTRACT_ID"
echo "   Oracle:  $ORACLE_CONTRACT_ID"
echo ""

# Escrow checks
check "Escrow: get_admin responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_admin"

check "Escrow: get_match_timeout responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_match_timeout"

check "Escrow: get_pending_matches responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_pending_matches"

check "Escrow: get_active_matches responds" \
    "stellar contract invoke --id '$ESCROW_CONTRACT_ID' --network '$NETWORK' -- get_active_matches"

# Oracle checks
check "Oracle: get_admin responds" \
    "stellar contract invoke --id '$ORACLE_CONTRACT_ID' --network '$NETWORK' -- get_admin"

echo ""
echo "Result: $PASS passed, $FAIL failed"

if [[ "$FAIL" -gt 0 ]]; then
    echo ""
    echo "❌ Verification failed. Check the contract IDs and network."
    echo "   See docs/error-codes.md for error code reference."
    exit 1
fi

echo "✅ All checks passed."
