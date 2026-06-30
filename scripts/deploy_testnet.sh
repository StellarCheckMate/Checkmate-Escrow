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
echo "✅ Deployment complete!"
echo ""
echo "📋 Contract Addresses:"
echo "   Oracle Contract:  $ORACLE_CONTRACT_ID"
echo "   Escrow Contract:  $ESCROW_CONTRACT_ID"
echo ""
echo "🔧 Update your .env file with:"
echo "   CONTRACT_ESCROW=$ESCROW_CONTRACT_ID"
echo "   CONTRACT_ORACLE=$ORACLE_CONTRACT_ID"
echo ""
echo "🧪 Test the deployment:"
echo "   stellar contract invoke --id $ESCROW_CONTRACT_ID --network $NETWORK -- get_admin"
echo "   stellar contract invoke --id $ORACLE_CONTRACT_ID --network $NETWORK -- get_admin"
<parameter name="filePath">/home/farouq/Desktop/Checkmate-Escrow/scripts/deploy_testnet.sh