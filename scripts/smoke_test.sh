#!/usr/bin/env bash
# Checkmate-Escrow smoke test: end-to-end match lifecycle verification
# Tests: create_match → deposit → submit_result → verify payout on testnet
set -euo pipefail

# ── Usage ─────────────────────────────────────────────────────────────────────
usage() {
    echo "Usage: $0 <network>"
    echo ""
    echo "Runs an end-to-end smoke test of the match lifecycle on testnet/mainnet."
    echo ""
    echo "Arguments:"
    echo "  network    testnet or mainnet"
    echo ""
    echo "Required environment variables:"
    echo "  STELLAR_RPC_URL           RPC endpoint (default: testnet)"
    echo "  CONTRACT_ESCROW           Escrow contract ID"
    echo "  CONTRACT_ORACLE           Oracle contract ID"
    echo "  PLAYER1_KEYPAIR           Player 1 stellar keypair name (with funds)"
    echo "  PLAYER2_KEYPAIR           Player 2 stellar keypair name (with funds)"
    echo "  ORACLE_ADMIN_KEYPAIR      Oracle admin keypair name (can submit results)"
    echo "  TEST_TOKEN                Token contract ID (e.g. native XLM)"
    echo ""
    echo "Example:"
    echo "  export PLAYER1_KEYPAIR=alice"
    echo "  export PLAYER2_KEYPAIR=bob"
    echo "  export ORACLE_ADMIN_KEYPAIR=oracle_admin"
    echo "  export TEST_TOKEN=CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"
    echo "  $0 testnet"
    echo ""
    exit 1
}

NETWORK="${1:-}"
[[ -z "$NETWORK" ]] && usage
[[ "$NETWORK" != "testnet" && "$NETWORK" != "mainnet" ]] && {
    echo "❌ Network must be 'testnet' or 'mainnet'"; exit 1
}

# ── Pre-flight checks ─────────────────────────────────────────────────────────
echo "🔍 Smoke Test: Full Match Lifecycle"
echo ""
echo "Validating environment..."

# Load .env if present
[[ -f ".env" ]] && set -o allexport && source .env && set +o allexport

# Verify required tools
command -v stellar &>/dev/null || { echo "❌ stellar CLI not found"; exit 1; }
command -v jq &>/dev/null || { echo "❌ jq not found"; exit 1; }

# Verify required environment variables
check_env() {
    local var="$1"
    if [[ -z "${!var:-}" ]]; then
        echo "❌ Missing required environment variable: $var"
        exit 1
    fi
}

check_env "CONTRACT_ESCROW"
check_env "CONTRACT_ORACLE"
check_env "PLAYER1_KEYPAIR"
check_env "PLAYER2_KEYPAIR"
check_env "ORACLE_ADMIN_KEYPAIR"
check_env "TEST_TOKEN"

# Get stellar addresses from keypair names
echo "   ✅ Required env vars present"
echo ""

get_address() {
    stellar keys address "$1" 2>/dev/null || {
        echo "❌ Cannot access keypair: $1"
        exit 1
    }
}

PLAYER1_ADDR=$(get_address "$PLAYER1_KEYPAIR")
PLAYER2_ADDR=$(get_address "$PLAYER2_KEYPAIR")
ORACLE_ADMIN_ADDR=$(get_address "$ORACLE_ADMIN_KEYPAIR")

echo "   Player 1: $PLAYER1_ADDR"
echo "   Player 2: $PLAYER2_ADDR"
echo "   Oracle Admin: $ORACLE_ADMIN_ADDR"
echo ""

# ── Test Configuration ────────────────────────────────────────────────────────
STAKE=100
GAME_ID="smoke_test_$(date +%s)"
PLATFORM="Lichess"

echo "🎮 Match Configuration:"
echo "   Network: $NETWORK"
echo "   Escrow: $CONTRACT_ESCROW"
echo "   Oracle: $CONTRACT_ORACLE"
echo "   Stake: $STAKE"
echo "   Game ID: $GAME_ID"
echo ""

# ── Helper functions ──────────────────────────────────────────────────────────

step_status() {
    local step="$1"
    local status="$2"
    if [[ "$status" == "✅" ]]; then
        echo "   $status $step"
    else
        echo "   $status $step"
        exit 1
    fi
}

invoke_contract() {
    local contract="$1"
    local keypair="$2"
    local method="$3"
    shift 3
    local args=("$@")

    stellar contract invoke \
        --id "$contract" \
        --source "$keypair" \
        --network "$NETWORK" \
        -- "$method" "${args[@]}" 2>&1
}

# ── Step 1: Create Match ──────────────────────────────────────────────────────
echo "📋 Step 1: Create Match"

MATCH_ID_OUTPUT=$(invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "create_match" \
    --player1 "$PLAYER1_ADDR" \
    --player2 "$PLAYER2_ADDR" \
    --stake_amount "$STAKE" \
    --token "$TEST_TOKEN" \
    --game_id "$GAME_ID" \
    --platform "$PLATFORM") || {
    step_status "create_match" "❌"
}

# Extract match ID from output (it's typically the last numeric value returned)
MATCH_ID=$(echo "$MATCH_ID_OUTPUT" | tail -1 | tr -d ' \n')

# Verify match ID is a number
if ! [[ "$MATCH_ID" =~ ^[0-9]+$ ]]; then
    echo "   ❌ Failed to extract match ID. Output:"
    echo "$MATCH_ID_OUTPUT"
    exit 1
fi

step_status "create_match (ID: $MATCH_ID)" "✅"

# ── Step 2: Verify Match is in Pending State ──────────────────────────────────
echo ""
echo "📋 Step 2: Verify Match State (Pending)"

MATCH_STATE=$(invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "get_match" \
    --match_id "$MATCH_ID") || {
    step_status "get_match" "❌"
}

if echo "$MATCH_STATE" | grep -q "Pending"; then
    step_status "match state is Pending" "✅"
else
    echo "   ❌ Match state is not Pending. Output:"
    echo "$MATCH_STATE"
    exit 1
fi

# ── Step 3: Player 1 Deposits ─────────────────────────────────────────────────
echo ""
echo "📋 Step 3: Player 1 Deposits"

invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "deposit" \
    --match_id "$MATCH_ID" \
    --player "$PLAYER1_ADDR" > /dev/null || {
    step_status "player 1 deposit" "❌"
}

step_status "player 1 deposit" "✅"

# ── Step 4: Player 2 Deposits ─────────────────────────────────────────────────
echo ""
echo "📋 Step 4: Player 2 Deposits"

invoke_contract "$CONTRACT_ESCROW" "$PLAYER2_KEYPAIR" "deposit" \
    --match_id "$MATCH_ID" \
    --player "$PLAYER2_ADDR" > /dev/null || {
    step_status "player 2 deposit" "❌"
}

step_status "player 2 deposit" "✅"

# ── Step 5: Verify Match is Active (Fully Funded) ─────────────────────────────
echo ""
echo "📋 Step 5: Verify Match State (Active)"

MATCH_STATE=$(invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "get_match" \
    --match_id "$MATCH_ID") || {
    step_status "get_match (active check)" "❌"
}

if echo "$MATCH_STATE" | grep -q "Active"; then
    step_status "match state is Active" "✅"
else
    echo "   ❌ Match state is not Active. Output:"
    echo "$MATCH_STATE"
    exit 1
fi

# ── Step 6: Verify Escrow Balance ─────────────────────────────────────────────
echo ""
echo "📋 Step 6: Verify Escrow Balance"

ESCROW_BALANCE=$(invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "get_escrow_balance" \
    --match_id "$MATCH_ID") || {
    step_status "get_escrow_balance" "❌"
}

EXPECTED_BALANCE=$((STAKE * 2))
ESCROW_BALANCE_NUM=$(echo "$ESCROW_BALANCE" | tail -1 | tr -d ' \n')

if [[ "$ESCROW_BALANCE_NUM" == "$EXPECTED_BALANCE" ]]; then
    step_status "escrow balance is $EXPECTED_BALANCE (correct)" "✅"
else
    echo "   ❌ Escrow balance mismatch. Expected: $EXPECTED_BALANCE, Got: $ESCROW_BALANCE_NUM"
    exit 1
fi

# ── Step 7: Submit Result (Player 1 Wins) ─────────────────────────────────────
echo ""
echo "📋 Step 7: Submit Result (Player 1 Wins)"

invoke_contract "$CONTRACT_ORACLE" "$ORACLE_ADMIN_KEYPAIR" "submit_result" \
    --match_id "$MATCH_ID" \
    --game_id "$GAME_ID" \
    --platform "$PLATFORM" \
    --result "Player1" \
    --response_time_ms 100 > /dev/null || {
    step_status "submit_result" "❌"
}

step_status "submit_result (Player1)" "✅"

# ── Step 8: Verify Payout (Escrow Balance Should be Zero) ──────────────────────
echo ""
echo "📋 Step 8: Verify Payout (Escrow Balance = 0)"

ESCROW_BALANCE_AFTER=$(invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "get_escrow_balance" \
    --match_id "$MATCH_ID") || {
    step_status "get_escrow_balance (after result)" "❌"
}

ESCROW_BALANCE_AFTER_NUM=$(echo "$ESCROW_BALANCE_AFTER" | tail -1 | tr -d ' \n')

if [[ "$ESCROW_BALANCE_AFTER_NUM" == "0" ]]; then
    step_status "escrow balance is 0 (payout complete)" "✅"
else
    echo "   ❌ Escrow balance should be 0 after payout. Got: $ESCROW_BALANCE_AFTER_NUM"
    exit 1
fi

# ── Step 9: Verify Match is Completed ─────────────────────────────────────────
echo ""
echo "📋 Step 9: Verify Match State (Completed)"

MATCH_STATE_FINAL=$(invoke_contract "$CONTRACT_ESCROW" "$PLAYER1_KEYPAIR" "get_match" \
    --match_id "$MATCH_ID") || {
    step_status "get_match (final state)" "❌"
}

if echo "$MATCH_STATE_FINAL" | grep -q "Completed"; then
    step_status "match state is Completed" "✅"
else
    echo "   ❌ Match state is not Completed. Output:"
    echo "$MATCH_STATE_FINAL"
    exit 1
fi

# ── Success ───────────────────────────────────────────────────────────────────
echo ""
echo "✅ Smoke Test Passed!"
echo ""
echo "📊 Summary:"
echo "   Network:         $NETWORK"
echo "   Match ID:        $MATCH_ID"
echo "   Game ID:         $GAME_ID"
echo "   Stake:           $STAKE"
echo "   Final State:     Completed"
echo "   Winner:          Player 1"
echo "   Escrow Balance:  0 (all funds disbursed)"
echo ""
echo "🎉 Full lifecycle verified: create → deposit → result → payout"
exit 0
