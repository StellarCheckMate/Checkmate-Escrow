#!/usr/bin/env bash
set -euo pipefail

# Checkmate-Escrow Testnet Deployment Script
# Deploys and initializes both Oracle and Escrow contracts to Stellar testnet

echo "🚀 Starting Checkmate-Escrow testnet deployment..."

# Configuration - modify these as needed
NETWORK="testnet"
DEPLOYER_KEYPAIR=${DEPLOYER_KEYPAIR:-"deployer"}  # Default keypair name
ORACLE_ADMIN=${ORACLE_ADMIN:-""}  # Set this to your oracle admin address
ESCROW_ADMIN=${ESCROW_ADMIN:-""}  # Set this to your escrow admin address

# Validate required parameters
if [[ -z "$ORACLE_ADMIN" ]]; then
    echo "❌ Error: ORACLE_ADMIN environment variable must be set"
    echo "   Example: export ORACLE_ADMIN=GA..."
    exit 1
fi

if [[ -z "$ESCROW_ADMIN" ]]; then
    echo "❌ Error: ESCROW_ADMIN environment variable must be set"
    echo "   Example: export ESCROW_ADMIN=GA..."
    exit 1
fi

# Get deployer address
echo "🔑 Getting deployer address..."
DEPLOYER_ADDRESS=$(stellar keys address "$DEPLOYER_KEYPAIR")
echo "   Deployer: $DEPLOYER_ADDRESS"

# Build contracts if not already built
if [[ ! -f "target/wasm32-unknown-unknown/release/oracle.wasm" ]] || [[ ! -f "target/wasm32-unknown-unknown/release/escrow.wasm" ]]; then
    echo "🔨 Building contracts..."
    ./scripts/build.sh
fi

echo "📦 Deploying Oracle contract..."
ORACLE_CONTRACT_ID=$(stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/oracle.wasm \
    --source "$DEPLOYER_KEYPAIR" \
    --network "$NETWORK")

echo "   Oracle Contract ID: $ORACLE_CONTRACT_ID"

echo "⚙️  Initializing Oracle contract..."
stellar contract invoke \
    --id "$ORACLE_CONTRACT_ID" \
    --source "$DEPLOYER_KEYPAIR" \
    --network "$NETWORK" \
    -- \
    initialize \
    --admin "$ORACLE_ADMIN" \
    --deployer "$DEPLOYER_ADDRESS"

echo "📦 Deploying Escrow contract..."
ESCROW_CONTRACT_ID=$(stellar contract deploy \
    --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
    --source "$DEPLOYER_KEYPAIR" \
    --network "$NETWORK")

echo "   Escrow Contract ID: $ESCROW_CONTRACT_ID"

echo "⚙️  Initializing Escrow contract..."
stellar contract invoke \
    --id "$ESCROW_CONTRACT_ID" \
    --source "$DEPLOYER_KEYPAIR" \
    --network "$NETWORK" \
    -- \
    initialize \
    --oracle "$ORACLE_CONTRACT_ID" \
    --admin "$ESCROW_ADMIN" \
    --deployer "$DEPLOYER_ADDRESS"

echo ""
echo "🩺 Verifying deployment health..."

RPC_URL=${SOROBAN_RPC_URL:-"https://soroban-testnet.stellar.org"}

check_initialized() {
    local name="$1"
    local contract_id="$2"
    local result

    if ! result=$(stellar contract invoke \
        --id "$contract_id" \
        --source "$DEPLOYER_KEYPAIR" \
        --network "$NETWORK" \
        -- \
        is_initialized 2>&1); then
        echo "❌ Health check failed for $name contract ($contract_id):"
        echo "   $result"
        return 1
    fi

    if [[ "$result" != *"true"* ]]; then
        echo "❌ $name contract ($contract_id) reports not initialized: $result"
        return 1
    fi

    echo "   ✅ $name contract is initialized"
    return 0
}

HEALTH_OK=1
check_initialized "Oracle" "$ORACLE_CONTRACT_ID" || HEALTH_OK=0
check_initialized "Escrow" "$ESCROW_CONTRACT_ID" || HEALTH_OK=0

if [[ "$HEALTH_OK" -ne 1 ]]; then
    echo ""
    echo "❌ Deployment health check failed. One or more contracts are not initialized."
    exit 1
fi

echo ""
echo "✅ Deployment complete and verified healthy!"
echo ""
echo "📋 Contract Addresses:"
echo "   Oracle Contract:  $ORACLE_CONTRACT_ID"
echo "   Escrow Contract:  $ESCROW_CONTRACT_ID"
echo ""
echo "🌐 RPC URL: $RPC_URL"
echo ""
echo "🔧 Update your .env file with:"
echo "   CONTRACT_ESCROW=$ESCROW_CONTRACT_ID"
echo "   CONTRACT_ORACLE=$ORACLE_CONTRACT_ID"
echo ""
echo "🧪 Test the deployment:"
echo "   stellar contract invoke --id $ESCROW_CONTRACT_ID --network $NETWORK -- get_admin"
echo "   stellar contract invoke --id $ORACLE_CONTRACT_ID --network $NETWORK -- get_admin"
<parameter name="filePath">/home/farouq/Desktop/Checkmate-Escrow/scripts/deploy_testnet.sh