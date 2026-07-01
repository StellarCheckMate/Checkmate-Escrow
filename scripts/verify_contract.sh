#!/usr/bin/env bash
# Checkmate-Escrow — Contract verification helper
#
# Reads CONTRACT_ESCROW (and optionally CONTRACT_ORACLE) from .env, prints
# Stellar Expert explorer URLs, and optionally fetches the WASM hash for each
# contract via `stellar contract info`.
#
# Usage:
#   ./scripts/verify_contract.sh [--network <testnet|mainnet|futurenet>] [--no-wasm]
#
# Options:
#   --network <name>   Override the network (default: value of STELLAR_NETWORK in
#                      .env, or "testnet" if unset)
#   --no-wasm          Skip the `stellar contract info` WASM hash lookup
#   --help             Show this help message and exit
#
# Environment variables (read from .env if present):
#   CONTRACT_ESCROW    Escrow contract ID  (required)
#   CONTRACT_ORACLE    Oracle contract ID  (optional)
#   STELLAR_NETWORK    Default network     (optional, default: testnet)

set -euo pipefail

# ── Resolve the repo root so the script works from any directory ───────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Defaults ───────────────────────────────────────────────────────────────────
NETWORK=""
FETCH_WASM=true

# ── Argument parsing ───────────────────────────────────────────────────────────
usage() {
    # Print only the leading comment block (up to the first non-comment line
    # after the shebang), stripping the leading "# " or "#".
    awk '/^#!/{next} /^#/{sub(/^# ?/,""); print; next} NF{exit}' "$0"
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --network)
            [[ -z "${2:-}" ]] && { echo "❌ --network requires a value"; exit 1; }
            NETWORK="$2"; shift 2 ;;
        --no-wasm)
            FETCH_WASM=false; shift ;;
        --help|-h)
            usage ;;
        *)
            echo "❌ Unknown option: $1"
            echo "   Run with --help for usage."
            exit 1 ;;
    esac
done

# ── Load .env ──────────────────────────────────────────────────────────────────
ENV_FILE="$REPO_ROOT/.env"
if [[ -f "$ENV_FILE" ]]; then
    # Source without exporting, so we don't pollute the caller's environment with
    # everything in .env — only pull what we need below.
    set -o allexport
    # shellcheck source=/dev/null
    source "$ENV_FILE"
    set +o allexport
fi

# ── Resolve network ────────────────────────────────────────────────────────────
NETWORK="${NETWORK:-${STELLAR_NETWORK:-testnet}}"

case "$NETWORK" in
    testnet|mainnet|futurenet|standalone) ;;
    *)
        echo "❌ Unsupported network: '$NETWORK'"
        echo "   Supported values: testnet, mainnet, futurenet, standalone"
        exit 1 ;;
esac

# ── Stellar Expert base URL ────────────────────────────────────────────────────
# Stellar Expert only indexes testnet and mainnet.
case "$NETWORK" in
    testnet)    EXPERT_BASE="https://stellar.expert/explorer/testnet/contract" ;;
    mainnet)    EXPERT_BASE="https://stellar.expert/explorer/public/contract" ;;
    futurenet)  EXPERT_BASE="" ;;   # not indexed
    standalone) EXPERT_BASE="" ;;   # not indexed
esac

# ── Helper: print Stellar Expert URL or a note if not indexed ─────────────────
expert_url() {
    local contract_id="$1"
    if [[ -n "$EXPERT_BASE" ]]; then
        echo "${EXPERT_BASE}/${contract_id}"
    else
        echo "(Stellar Expert does not index $NETWORK)"
    fi
}

# ── Helper: fetch WASM hash via stellar contract info ─────────────────────────
WASM_FETCH_AVAILABLE=false
if $FETCH_WASM && command -v stellar &>/dev/null; then
    WASM_FETCH_AVAILABLE=true
fi

fetch_wasm_hash() {
    local label="$1"
    local contract_id="$2"

    if ! $WASM_FETCH_AVAILABLE; then
        echo "   WASM hash : (skipped — stellar CLI not found)"
        return
    fi

    local info_output
    # `stellar contract info` prints JSON or key=value pairs depending on CLI version;
    # we capture stdout+stderr and grep for the wasm_hash / hash field.
    if info_output=$(stellar contract info \
            --id "$contract_id" \
            --network "$NETWORK" 2>&1); then
        # Try to extract a 64-char hex string (the WASM hash)
        local wasm_hash
        wasm_hash=$(echo "$info_output" \
            | grep -oE '[0-9a-fA-F]{64}' \
            | head -1 || true)
        if [[ -n "$wasm_hash" ]]; then
            echo "   WASM hash : $wasm_hash"
        else
            # Surface whatever the CLI returned so the user can interpret it
            echo "   WASM info : $info_output"
        fi
    else
        echo "   WASM hash : ⚠️  Could not fetch (contract may not be on-chain yet, or CLI error)"
        echo "              → $info_output"
    fi
}

# ── Validate required variables ────────────────────────────────────────────────
CONTRACT_ESCROW="${CONTRACT_ESCROW:-}"
CONTRACT_ORACLE="${CONTRACT_ORACLE:-}"

if [[ -z "$CONTRACT_ESCROW" ]]; then
    echo "❌ CONTRACT_ESCROW is not set."
    echo ""
    echo "   Set it in your .env file:"
    echo "     CONTRACT_ESCROW=<your-escrow-contract-id>"
    echo ""
    echo "   Or export it before running this script:"
    echo "     export CONTRACT_ESCROW=<your-escrow-contract-id>"
    exit 1
fi

# ── Basic format sanity check (Stellar contract IDs are 56 uppercase base32 chars) ──
validate_contract_id() {
    local id="$1"
    local label="$2"
    if [[ ! "$id" =~ ^[A-Z2-7]{56}$ ]]; then
        echo "⚠️  Warning: $label does not look like a valid Stellar contract ID."
        echo "   Expected 56 uppercase base32 characters (A-Z, 2-7)."
        echo "   Got: $id"
    fi
}

validate_contract_id "$CONTRACT_ESCROW" "CONTRACT_ESCROW"
[[ -n "$CONTRACT_ORACLE" ]] && validate_contract_id "$CONTRACT_ORACLE" "CONTRACT_ORACLE"

# ── Main output ────────────────────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║          Checkmate-Escrow — Contract Verification            ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  Network : $NETWORK"
echo ""

OVERALL_PASS=true

# ── Escrow contract ────────────────────────────────────────────────────────────
echo "── Escrow Contract ──────────────────────────────────────────────"
echo "   Contract ID : $CONTRACT_ESCROW"
echo "   Explorer URL: $(expert_url "$CONTRACT_ESCROW")"
fetch_wasm_hash "Escrow" "$CONTRACT_ESCROW"
echo ""

# ── Oracle contract (optional) ────────────────────────────────────────────────
if [[ -n "$CONTRACT_ORACLE" ]]; then
    echo "── Oracle Contract ──────────────────────────────────────────────"
    echo "   Contract ID : $CONTRACT_ORACLE"
    echo "   Explorer URL: $(expert_url "$CONTRACT_ORACLE")"
    fetch_wasm_hash "Oracle" "$CONTRACT_ORACLE"
    echo ""
fi

# ── Quick liveness probe via stellar contract invoke ──────────────────────────
if command -v stellar &>/dev/null; then
    echo "── Liveness probe ───────────────────────────────────────────────"
    probe() {
        local label="$1"
        local contract_id="$2"
        local fn="$3"
        printf "   %-40s" "$label"
        local out
        if out=$(stellar contract invoke \
                --id "$contract_id" \
                --network "$NETWORK" \
                -- "$fn" 2>&1); then
            echo "✅"
        else
            echo "❌  (${out})"
            OVERALL_PASS=false
        fi
    }

    probe "Escrow get_admin" "$CONTRACT_ESCROW" "get_admin"
    [[ -n "$CONTRACT_ORACLE" ]] && probe "Oracle  get_admin" "$CONTRACT_ORACLE" "get_admin"
    echo ""
fi

# ── Result ─────────────────────────────────────────────────────────────────────
if $OVERALL_PASS; then
    echo "✅ Verification passed — contract is live and accessible."
else
    echo "❌ Verification failed — one or more checks did not pass."
    echo "   • Confirm the contract ID is correct."
    echo "   • Confirm the network matches where you deployed."
    echo "   • See docs/error-codes.md for error code reference."
    exit 1
fi
