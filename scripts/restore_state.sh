#!/usr/bin/env bash
# restore_state.sh — Replay a Checkmate-Escrow state snapshot into a new contract
#
# Reads a JSON snapshot produced by backup_state.sh and replays the config
# and active/pending match state into a freshly deployed contract.
#
# IMPORTANT: This script replays *observable* contract state by re-invoking
# the public admin functions and re-creating matches.  It cannot replay
# in-progress deposits made by players (those require player signatures) — see
# the "Limitations" section in docs/disaster-recovery.md.
#
# Usage:
#   ./scripts/restore_state.sh <snapshot_file> [network]
#
# Examples:
#   ./scripts/restore_state.sh backups/escrow-snapshot-testnet-20260101T000000Z.json testnet
#   ./scripts/restore_state.sh backups/escrow-snapshot-mainnet-20260101T000000Z.json mainnet
#
# Required env vars (or values from .env):
#   CONTRACT_ESCROW       — the NEW (target) escrow contract ID
#   CONTRACT_ORACLE       — the oracle contract ID for the new deployment
#   DEPLOYER_KEYPAIR      — Stellar keypair name (default: deployer)
#
# The new contract must already be deployed and initialized (use deploy.sh first).

set -euo pipefail

# ── Load .env if present ───────────────────────────────────────────────────────
[[ -f ".env" ]] && set -o allexport && source .env && set +o allexport

# ── Parameters ─────────────────────────────────────────────────────────────────
SNAPSHOT_FILE="${1:-}"
NETWORK="${2:-${STELLAR_NETWORK:-testnet}}"
DEPLOYER_KEYPAIR="${DEPLOYER_KEYPAIR:-deployer}"

# ── Validate prerequisites ─────────────────────────────────────────────────────
require_cmd() { command -v "$1" &>/dev/null || { echo "❌ Missing required tool: $1"; exit 1; }; }
require_cmd stellar
require_cmd jq

[[ -z "$SNAPSHOT_FILE" ]] && {
    echo "Usage: $0 <snapshot_file> [network]"
    exit 1
}

[[ ! -f "$SNAPSHOT_FILE" ]] && {
    echo "❌ Snapshot file not found: $SNAPSHOT_FILE"
    exit 1
}

[[ -z "${CONTRACT_ESCROW:-}" ]] && {
    echo "❌ CONTRACT_ESCROW (target contract) is not set."
    exit 1
}

# ── Verify snapshot schema ─────────────────────────────────────────────────────
SCHEMA_VERSION=$(jq -r '.schema_version // empty' "$SNAPSHOT_FILE")
[[ "$SCHEMA_VERSION" != "1" ]] && {
    echo "❌ Unsupported snapshot schema version: ${SCHEMA_VERSION:-<missing>}"
    echo "   This script supports schema_version=1 only."
    exit 1
}

SNAP_NETWORK=$(jq -r '.network' "$SNAPSHOT_FILE")
SNAP_TIMESTAMP=$(jq -r '.timestamp' "$SNAPSHOT_FILE")
SNAP_CONTRACT=$(jq -r '.contract_id' "$SNAPSHOT_FILE")

echo "🔄 Checkmate-Escrow State Restore"
echo "   Snapshot:        $SNAPSHOT_FILE"
echo "   Snapshot time:   $SNAP_TIMESTAMP"
echo "   Source network:  $SNAP_NETWORK"
echo "   Source contract: $SNAP_CONTRACT"
echo "   Target network:  $NETWORK"
echo "   Target contract: $CONTRACT_ESCROW"
echo ""

if [[ "$SNAP_NETWORK" != "$NETWORK" ]]; then
    echo "⚠️  WARNING: snapshot is from '$SNAP_NETWORK' but target is '$NETWORK'."
    read -r -p "Continue anyway? [y/N] " CONFIRM
    [[ "$CONFIRM" != "y" && "$CONFIRM" != "Y" ]] && { echo "Aborted."; exit 1; }
fi

if [[ "$NETWORK" == "mainnet" ]]; then
    echo ""
    echo "⚠️  MAINNET RESTORE — this will modify production state."
    echo "   Target contract: $CONTRACT_ESCROW"
    read -r -p "Type 'restore mainnet' to confirm: " CONFIRM
    [[ "$CONFIRM" != "restore mainnet" ]] && { echo "Aborted."; exit 1; }
fi

# Helper: invoke a write function on the target contract.
invoke_write() {
    local func="$1"
    shift
    stellar contract invoke \
        --id "$CONTRACT_ESCROW" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK" \
        -- "$func" "$@"
}

# ── Step 1: Restore protocol config ───────────────────────────────────────────
echo "⚙️  Step 1/5 — Restoring protocol config..."

PROTOCOL_CONFIG=$(jq -r '.config.protocol_config // empty' "$SNAPSHOT_FILE")
if [[ -n "$PROTOCOL_CONFIG" && "$PROTOCOL_CONFIG" != "null" ]]; then
    VESTING=$(echo "$PROTOCOL_CONFIG"  | jq -r '.vesting_duration_seconds // 0')
    CANCEL_FEE=$(echo "$PROTOCOL_CONFIG" | jq -r '.cancellation_fee_basis_points // 0')
    TREASURY=$(echo "$PROTOCOL_CONFIG"  | jq -r '.treasury')
    STABLECOIN=$(echo "$PROTOCOL_CONFIG" | jq -r '.stablecoin_only_mode // false')

    invoke_write set_protocol_config \
        --vesting_duration_seconds "$VESTING" \
        --cancellation_fee_basis_points "$CANCEL_FEE" \
        --treasury "$TREASURY" \
        --stablecoin_only_mode "$STABLECOIN" && echo "   ✅ Protocol config restored" || echo "   ⚠️  Protocol config: set_protocol_config failed (manual intervention may be needed)"
fi

# ── Step 2: Restore match timeout ─────────────────────────────────────────────
echo "⚙️  Step 2/5 — Restoring match timeout..."
TIMEOUT=$(jq -r '.config.match_timeout // empty' "$SNAPSHOT_FILE")
if [[ -n "$TIMEOUT" && "$TIMEOUT" != "null" ]]; then
    invoke_write set_match_timeout --ledgers "$TIMEOUT" \
        && echo "   ✅ Match timeout restored to $TIMEOUT ledgers" \
        || echo "   ⚠️  Match timeout restore failed"
fi

# ── Step 3: Restore allowed token list ────────────────────────────────────────
echo "⚙️  Step 3/5 — Restoring allowed token list..."
ALLOWED_TOKENS=$(jq -c '.config.allowed_tokens // []' "$SNAPSHOT_FILE")
TOKEN_COUNT=$(echo "$ALLOWED_TOKENS" | jq 'if type == "array" then length else 0 end')
if [[ "$TOKEN_COUNT" -gt 0 ]]; then
    echo "$ALLOWED_TOKENS" | jq -r '.[]' | while read -r token_addr; do
        invoke_write add_allowed_token --token "$token_addr" \
            && echo "   ✅ Added allowed token: $token_addr" \
            || echo "   ⚠️  add_allowed_token failed for $token_addr"
    done
else
    echo "   ℹ️  No allowed tokens to restore (open allowlist mode)."
fi

# ── Step 4: Restore fee tiers ─────────────────────────────────────────────────
echo "⚙️  Step 4/5 — Restoring fee tiers..."
FEE_TIERS=$(jq -c '.config.fee_tiers // []' "$SNAPSHOT_FILE")
FEE_COUNT=$(echo "$FEE_TIERS" | jq 'if type == "array" then length else 0 end')
if [[ "$FEE_COUNT" -gt 0 ]]; then
    invoke_write set_fee_tiers --tiers "$FEE_TIERS" \
        && echo "   ✅ Fee tiers restored ($FEE_COUNT tiers)" \
        || echo "   ⚠️  set_fee_tiers failed"
else
    echo "   ℹ️  No fee tiers to restore."
fi

# ── Step 5: Report active/pending matches that require manual intervention ─────
echo "⚙️  Step 5/5 — Reporting in-flight matches..."

MATCH_COUNT=$(jq '.matches | length' "$SNAPSHOT_FILE")
echo ""
echo "⚠️  In-flight matches in snapshot: $MATCH_COUNT"
echo "   These matches cannot be automatically replayed because they require"
echo "   original player signatures and on-chain token transfers."
echo "   Each match must be manually recreated or disputed. Details below:"
echo ""

jq -r '.matches[] | "   Match \(.match.id): state=\(.match.state) player1=\(.match.player1) player2=\(.match.player2) stake=\(.match.stake_amount) token=\(.match.token)"' \
    "$SNAPSHOT_FILE" 2>/dev/null | head -50 || true

if [[ "$MATCH_COUNT" -gt 50 ]]; then
    echo "   ... ($MATCH_COUNT total — showing first 50; see $SNAPSHOT_FILE for full list)"
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo "✅ Restore complete."
echo ""
echo "📋 Post-restore checklist:"
echo "   1. Verify admin:          stellar contract invoke --id $CONTRACT_ESCROW --network $NETWORK -- get_admin"
echo "   2. Verify oracle:         stellar contract invoke --id $CONTRACT_ESCROW --network $NETWORK -- get_oracle"
echo "   3. Verify config:         stellar contract invoke --id $CONTRACT_ESCROW --network $NETWORK -- get_protocol_config"
echo "   4. Verify token list:     stellar contract invoke --id $CONTRACT_ESCROW --network $NETWORK -- get_allowed_tokens"
echo "   5. Re-create in-flight matches manually (see docs/disaster-recovery.md)"
echo "   6. Notify players with active matches via out-of-band communication"
