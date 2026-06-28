#!/usr/bin/env bash
# Checkmate-Escrow unified deployment script — testnet and mainnet
set -euo pipefail

# ── Usage ─────────────────────────────────────────────────────────────────────
usage() {
    echo "Usage: $0 <network> [options]"
    echo ""
    echo "  network       testnet | mainnet"
    echo ""
    echo "Options:"
    echo "  --skip-build  Skip contract compilation"
    echo "  --upgrade     Upgrade existing contracts (requires CONTRACT_ESCROW/CONTRACT_ORACLE)"
    echo ""
    echo "Required env vars:"
    echo "  DEPLOYER_KEYPAIR   Stellar keypair name (default: deployer)"
    echo "  ORACLE_ADMIN       Oracle admin Stellar address"
    echo "  ESCROW_ADMIN       Escrow admin Stellar address"
    echo ""
    echo "Upgrade env vars:"
    echo "  CONTRACT_ESCROW    Existing escrow contract ID"
    echo "  CONTRACT_ORACLE    Existing oracle contract ID"
    exit 1
}

# ── Argument parsing ───────────────────────────────────────────────────────────
NETWORK="${1:-}"
[[ -z "$NETWORK" ]] && usage

SKIP_BUILD=false
UPGRADE=false
for arg in "${@:2}"; do
    case "$arg" in
        --skip-build) SKIP_BUILD=true ;;
        --upgrade)    UPGRADE=true ;;
        *) echo "Unknown option: $arg"; usage ;;
    esac
done

[[ "$NETWORK" != "testnet" && "$NETWORK" != "mainnet" ]] && {
    echo "❌ Network must be 'testnet' or 'mainnet'"; exit 1
}

# ── Pre-flight checks ──────────────────────────────────────────────────────────
echo "🔍 Running pre-flight checks..."

check_tool() {
    command -v "$1" &>/dev/null || { echo "❌ Missing required tool: $1"; exit 1; }
}
check_tool stellar
check_tool cargo
check_tool rustc

RUST_VER=$(rustc --version | grep -oP '\d+\.\d+' | head -1)
RUST_MAJOR=$(echo "$RUST_VER" | cut -d. -f1)
RUST_MINOR=$(echo "$RUST_VER" | cut -d. -f2)
if [[ "$RUST_MAJOR" -lt 1 || ( "$RUST_MAJOR" -eq 1 && "$RUST_MINOR" -lt 70 ) ]]; then
    echo "❌ Rust 1.70+ required (found $RUST_VER)"; exit 1
fi

rustup target list --installed | grep -q "wasm32-unknown-unknown" || {
    echo "❌ Missing Rust target: wasm32-unknown-unknown"
    echo "   Run: rustup target add wasm32-unknown-unknown"
    exit 1
}

echo "   ✅ Rust $(rustc --version | awk '{print $2}')"
echo "   ✅ Stellar CLI $(stellar --version 2>&1 | head -1)"

# ── Load .env if present ───────────────────────────────────────────────────────
[[ -f ".env" ]] && set -o allexport && source .env && set +o allexport

DEPLOYER_KEYPAIR="${DEPLOYER_KEYPAIR:-deployer}"
ORACLE_ADMIN="${ORACLE_ADMIN:-}"
ESCROW_ADMIN="${ESCROW_ADMIN:-}"

[[ -z "$ORACLE_ADMIN" ]] && { echo "❌ ORACLE_ADMIN is required"; exit 1; }
[[ -z "$ESCROW_ADMIN" ]] && { echo "❌ ESCROW_ADMIN is required"; exit 1; }

# ── Mainnet safety gate ────────────────────────────────────────────────────────
if [[ "$NETWORK" == "mainnet" ]]; then
    echo ""
    echo "⚠️  MAINNET DEPLOYMENT"
    echo "   Oracle admin:  $ORACLE_ADMIN"
    echo "   Escrow admin:  $ESCROW_ADMIN"
    echo "   Deployer key:  $DEPLOYER_KEYPAIR"
    echo ""
    read -r -p "Type 'deploy mainnet' to confirm: " CONFIRM
    [[ "$CONFIRM" != "deploy mainnet" ]] && { echo "Aborted."; exit 1; }

    # Verify deployer key is accessible
    stellar keys address "$DEPLOYER_KEYPAIR" &>/dev/null || {
        echo "❌ Cannot access deployer keypair '$DEPLOYER_KEYPAIR'"; exit 1
    }
fi

if [[ "$NETWORK" == "testnet" ]]; then
    echo ""
    read -r -p "Deploy to TESTNET? [y/N] " CONFIRM
    [[ "$CONFIRM" != "y" && "$CONFIRM" != "Y" ]] && { echo "Aborted."; exit 1; }
fi

# ── Build ──────────────────────────────────────────────────────────────────────
ORACLE_WASM="target/wasm32-unknown-unknown/release/oracle.wasm"
ESCROW_WASM="target/wasm32-unknown-unknown/release/escrow.wasm"

if [[ "$SKIP_BUILD" == false ]]; then
    echo ""
    echo "🔨 Building contracts..."
    ./scripts/build.sh
fi

[[ -f "$ORACLE_WASM" ]] || { echo "❌ oracle.wasm not found; run without --skip-build"; exit 1; }
[[ -f "$ESCROW_WASM" ]] || { echo "❌ escrow.wasm not found; run without --skip-build"; exit 1; }

# ── Deploy or upgrade ──────────────────────────────────────────────────────────
echo ""
DEPLOYER_ADDRESS=$(stellar keys address "$DEPLOYER_KEYPAIR")
echo "🔑 Deployer: $DEPLOYER_ADDRESS"

if [[ "$UPGRADE" == true ]]; then
    echo ""
    echo "♻️  Upgrading existing contracts..."
    [[ -z "${CONTRACT_ORACLE:-}" ]] && { echo "❌ CONTRACT_ORACLE required for --upgrade"; exit 1; }
    [[ -z "${CONTRACT_ESCROW:-}" ]] && { echo "❌ CONTRACT_ESCROW required for --upgrade"; exit 1; }

    stellar contract upload \
        --wasm "$ORACLE_WASM" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK"

    stellar contract upload \
        --wasm "$ESCROW_WASM" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK"

    ORACLE_CONTRACT_ID="$CONTRACT_ORACLE"
    ESCROW_CONTRACT_ID="$CONTRACT_ESCROW"
    echo "   ✅ Wasm uploaded (contract IDs unchanged)"
else
    echo ""
    echo "📦 Deploying Oracle..."
    ORACLE_CONTRACT_ID=$(stellar contract deploy \
        --wasm "$ORACLE_WASM" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK")
    echo "   Oracle: $ORACLE_CONTRACT_ID"

    echo "⚙️  Initializing Oracle..."
    stellar contract invoke \
        --id "$ORACLE_CONTRACT_ID" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK" \
        -- initialize \
        --admin "$ORACLE_ADMIN" \
        --deployer "$DEPLOYER_ADDRESS"

    echo ""
    echo "📦 Deploying Escrow..."
    ESCROW_CONTRACT_ID=$(stellar contract deploy \
        --wasm "$ESCROW_WASM" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK")
    echo "   Escrow: $ESCROW_CONTRACT_ID"

    echo "⚙️  Initializing Escrow..."
    stellar contract invoke \
        --id "$ESCROW_CONTRACT_ID" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK" \
        -- initialize \
        --oracle "$ORACLE_CONTRACT_ID" \
        --admin "$ESCROW_ADMIN" \
        --deployer "$DEPLOYER_ADDRESS"
fi

# ── Post-deployment verification ───────────────────────────────────────────────
echo ""
echo "🔍 Verifying deployment..."
./scripts/verify-deployment.sh "$NETWORK" "$ESCROW_CONTRACT_ID" "$ORACLE_CONTRACT_ID"

# ── Generate deployment report ─────────────────────────────────────────────────
REPORT_DIR="reports"
mkdir -p "$REPORT_DIR"
TIMESTAMP=$(date -u +"%Y%m%dT%H%M%SZ")
REPORT_FILE="$REPORT_DIR/deployment-${NETWORK}-${TIMESTAMP}.txt"

cat > "$REPORT_FILE" <<EOF
Checkmate-Escrow Deployment Report
===================================
Timestamp:        $TIMESTAMP
Network:          $NETWORK
Mode:             $([ "$UPGRADE" == true ] && echo "upgrade" || echo "fresh")
Deployer address: $DEPLOYER_ADDRESS

Contract Addresses
------------------
Oracle:  $ORACLE_CONTRACT_ID
Escrow:  $ESCROW_CONTRACT_ID

Environment
-----------
Rust:         $(rustc --version)
Stellar CLI:  $(stellar --version 2>&1 | head -1)
EOF

echo ""
echo "✅ Deployment complete!"
echo ""
echo "📋 Contract Addresses:"
echo "   Oracle: $ORACLE_CONTRACT_ID"
echo "   Escrow: $ESCROW_CONTRACT_ID"
echo ""
echo "📄 Report saved: $REPORT_FILE"
echo ""
echo "🔧 Update your .env:"
echo "   CONTRACT_ORACLE=$ORACLE_CONTRACT_ID"
echo "   CONTRACT_ESCROW=$ESCROW_CONTRACT_ID"
