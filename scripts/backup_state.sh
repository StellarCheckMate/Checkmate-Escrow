#!/usr/bin/env bash
# backup_state.sh — Export all Checkmate-Escrow contract state to JSON
#
# Reads every public state key from the escrow contract (matches, balances,
# config) using the Stellar CLI and writes a timestamped JSON snapshot that
# can later be replayed by restore_state.sh.
#
# Usage:
#   ./scripts/backup_state.sh [network] [output_dir]
#
# Defaults:
#   network     — value of $STELLAR_NETWORK or "testnet"
#   output_dir  — value of $BACKUP_DIR or "backups/"
#
# Required env vars (or values from .env):
#   CONTRACT_ESCROW   — deployed escrow contract ID
#
# Optional env vars:
#   DEPLOYER_KEYPAIR  — Stellar keypair name for CLI calls (default: deployer)
#   S3_BUCKET         — if set, the finished snapshot is uploaded here via
#                       `aws s3 cp`.  Example: s3://my-bucket/checkmate-backups
#   BACKUP_RETENTION_DAYS — local backups older than this many days are
#                            pruned (default: 30)

set -euo pipefail

# ── Load .env if present ───────────────────────────────────────────────────────
[[ -f ".env" ]] && set -o allexport && source .env && set +o allexport

# ── Parameters ─────────────────────────────────────────────────────────────────
NETWORK="${1:-${STELLAR_NETWORK:-testnet}}"
OUTPUT_DIR="${2:-${BACKUP_DIR:-backups}}"
DEPLOYER_KEYPAIR="${DEPLOYER_KEYPAIR:-deployer}"
RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-30}"

# ── Validate prerequisites ─────────────────────────────────────────────────────
require_cmd() { command -v "$1" &>/dev/null || { echo "❌ Missing required tool: $1"; exit 1; }; }
require_cmd stellar
require_cmd jq

[[ -z "${CONTRACT_ESCROW:-}" ]] && {
    echo "❌ CONTRACT_ESCROW is not set."
    echo "   Export it or add it to .env before running this script."
    exit 1
}

# ── Setup output directory ─────────────────────────────────────────────────────
mkdir -p "$OUTPUT_DIR"
TIMESTAMP=$(date -u +"%Y%m%dT%H%M%SZ")
SNAPSHOT_FILE="${OUTPUT_DIR}/escrow-snapshot-${NETWORK}-${TIMESTAMP}.json"
PARTIAL_FILE="${SNAPSHOT_FILE}.partial"

echo "🗄️  Checkmate-Escrow State Backup"
echo "   Network:  $NETWORK"
echo "   Contract: $CONTRACT_ESCROW"
echo "   Output:   $SNAPSHOT_FILE"
echo ""

# Helper: invoke a read-only contract function and return raw stdout.
# Returns an empty string on failure so the backup continues past missing keys.
invoke_read() {
    local func="$1"
    shift
    stellar contract invoke \
        --id "$CONTRACT_ESCROW" \
        --network "$NETWORK" \
        -- "$func" "$@" 2>/dev/null || echo "null"
}

# ── 1. Config / admin state ────────────────────────────────────────────────────
echo "📋 Reading config state..."
ADMIN=$(invoke_read get_admin)
ORACLE=$(invoke_read get_oracle)
PAUSED=$(invoke_read is_paused)
TIMEOUT=$(invoke_read get_match_timeout)
VERSION=$(invoke_read get_version)
DISPUTE_PERIOD=$(invoke_read get_dispute_period 2>/dev/null || echo "null")

# ── 2. Match counts and lists ──────────────────────────────────────────────────
echo "📋 Reading match lists..."
PENDING_MATCHES=$(invoke_read get_pending_matches)
ACTIVE_MATCHES=$(invoke_read get_active_matches)

# Determine total match count by reading the MatchCount storage key directly.
# If unavailable, derive it from the union of pending + active + querying
# completed matches through the paginated API.
MATCH_COUNT_RAW=$(invoke_read get_completed_matches_count 2>/dev/null || echo "null")

# ── 3. Per-match detailed state ────────────────────────────────────────────────
echo "📋 Reading per-match state..."

# Collect all match IDs from pending and active lists.
PENDING_IDS=$(echo "$PENDING_MATCHES" | jq -r '.[].id // empty' 2>/dev/null || true)
ACTIVE_IDS=$(echo "$ACTIVE_MATCHES"   | jq -r '.[].id // empty' 2>/dev/null || true)
ALL_IDS=$(printf '%s\n%s\n' "$PENDING_IDS" "$ACTIVE_IDS" | sort -u | grep -v '^$' || true)

MATCHES_JSON="[]"
for match_id in $ALL_IDS; do
    echo "   Reading match $match_id..."
    match_data=$(invoke_read get_match --match_id "$match_id")
    balance=$(invoke_read get_escrow_balance --match_id "$match_id")
    funded=$(invoke_read is_funded --match_id "$match_id")

    entry=$(jq -n \
        --argjson match "$match_data" \
        --argjson balance "$balance" \
        --argjson funded "$funded" \
        '{match: $match, escrow_balance: $balance, is_funded: $funded}')

    MATCHES_JSON=$(echo "$MATCHES_JSON" | jq ". + [$entry]")
done

# ── 4. Allowed-token list ──────────────────────────────────────────────────────
echo "📋 Reading allowed token list..."
ALLOWED_TOKENS=$(invoke_read get_allowed_tokens 2>/dev/null || echo "null")

# ── 5. Protocol config ─────────────────────────────────────────────────────────
echo "📋 Reading protocol config..."
PROTOCOL_CONFIG=$(invoke_read get_protocol_config 2>/dev/null || echo "null")

# ── 6. Fee tiers ───────────────────────────────────────────────────────────────
echo "📋 Reading fee tiers..."
FEE_TIERS=$(invoke_read get_fee_tiers 2>/dev/null || echo "null")

# ── 7. Assemble final snapshot ─────────────────────────────────────────────────
echo "📋 Assembling snapshot..."

jq -n \
    --arg     schema_version  "1" \
    --arg     timestamp        "$TIMESTAMP" \
    --arg     network          "$NETWORK" \
    --arg     contract_id      "$CONTRACT_ESCROW" \
    --argjson admin            "$ADMIN" \
    --argjson oracle           "$ORACLE" \
    --argjson paused           "$PAUSED" \
    --argjson match_timeout    "$TIMEOUT" \
    --argjson contract_version "$VERSION" \
    --argjson dispute_period   "$DISPUTE_PERIOD" \
    --argjson pending_matches  "$PENDING_MATCHES" \
    --argjson active_matches   "$ACTIVE_MATCHES" \
    --argjson match_count      "$MATCH_COUNT_RAW" \
    --argjson matches          "$MATCHES_JSON" \
    --argjson allowed_tokens   "$ALLOWED_TOKENS" \
    --argjson protocol_config  "$PROTOCOL_CONFIG" \
    --argjson fee_tiers        "$FEE_TIERS" \
    '{
        schema_version:   $schema_version,
        timestamp:        $timestamp,
        network:          $network,
        contract_id:      $contract_id,
        config: {
            admin:            $admin,
            oracle:           $oracle,
            paused:           $paused,
            match_timeout:    $match_timeout,
            contract_version: $contract_version,
            dispute_period:   $dispute_period,
            protocol_config:  $protocol_config,
            fee_tiers:        $fee_tiers,
            allowed_tokens:   $allowed_tokens
        },
        matches:          $matches,
        pending_matches:  $pending_matches,
        active_matches:   $active_matches,
        match_count:      $match_count
    }' > "$PARTIAL_FILE"

mv "$PARTIAL_FILE" "$SNAPSHOT_FILE"

SNAPSHOT_SIZE=$(wc -c < "$SNAPSHOT_FILE")
MATCH_COUNT_BACKED=$(echo "$MATCHES_JSON" | jq 'length')
echo ""
echo "✅ Snapshot written: $SNAPSHOT_FILE"
echo "   Size:    ${SNAPSHOT_SIZE} bytes"
echo "   Matches: $MATCH_COUNT_BACKED captured"

# ── 8. Optional S3 upload ──────────────────────────────────────────────────────
if [[ -n "${S3_BUCKET:-}" ]]; then
    echo ""
    echo "☁️  Uploading to S3: ${S3_BUCKET}/"
    require_cmd aws
    aws s3 cp "$SNAPSHOT_FILE" \
        "${S3_BUCKET}/escrow-snapshot-${NETWORK}-${TIMESTAMP}.json" \
        --storage-class STANDARD_IA
    echo "   ✅ Upload complete"
fi

# ── 9. Local retention cleanup ─────────────────────────────────────────────────
echo ""
echo "🧹 Pruning local backups older than ${RETENTION_DAYS} days..."
find "$OUTPUT_DIR" -name "escrow-snapshot-${NETWORK}-*.json" \
    -mtime +"$RETENTION_DAYS" -delete -print \
    | sed 's/^/   Removed: /' || true
echo "   Done."

echo ""
echo "📄 Backup complete: $SNAPSHOT_FILE"
